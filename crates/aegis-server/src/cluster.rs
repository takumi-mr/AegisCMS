//! 複数台のサーバーで動かすときの連携。
//! - Admin Studio での変更（プラグインの有効・無効・実行順・設定値、通知のテンプレート）をイベントバス（`aegis_cache::EventBus`、
//!   `[cache]` と同じバックエンド）で知らせ、受け取ったサーバーが DB・キャッシュから読み直す。知らせは送った
//!   サーバー自身にも届くので、1 台（メモリ）でも複数台（Redis）でも同じ処理になる
//! - `[cache].backend = "memory"` はセッションも知らせもプロセスの中だけなので 1 台でしか使えない。
//!   [`SingleServerGuard`] が DB に生存の印を書き、同じ DB で 2 台目が動いていれば起動を断る

use std::sync::Arc;
use std::time::Duration;

use aegis_cache::{BusMessage, BusSubscription};
use aegis_db::DatabaseAdapter;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::state::AppState;

/// 有効なプラグインの一覧（実行順）が変わった。受け取ったら DB の一覧に揃える
pub const TOPIC_PLUGINS: &str = "plugins.enabled";
/// プラグインの設定値が変わった（payload はプラグイン名）。受け取ったらキャッシュから読み直す
pub const TOPIC_PLUGIN_SETTINGS: &str = "plugins.settings";
/// 通知のテンプレートが Admin Studio で編集された。受け取ったら DB から読み直す
pub const TOPIC_NOTIFICATION_TEMPLATES: &str = "notifications.templates";

/// ほかのサーバー（と自分）に知らせる。届かなくても操作は失敗にしない（ほかのサーバーは再接続・再起動で揃う）
pub async fn announce(state: &AppState, topic: &str, payload: &str) {
    if let Err(e) = state.bus.publish(topic, payload).await {
        tracing::warn!(
            "Could not tell the other servers about '{}' (they catch up when they reconnect or restart): {}",
            topic,
            e
        );
    }
}

/// 知らせを受け取って反映し続ける。`subscription` は起動の早いうちに作っておく（読み込みとの間の取りこぼしを防ぐ）
pub fn spawn_listener(state: Arc<AppState>, mut subscription: BusSubscription) {
    tokio::spawn(async move {
        while let Some(message) = subscription.recv().await {
            handle(&state, message).await;
        }
    });
}

pub async fn handle(state: &AppState, message: BusMessage) {
    match message {
        BusMessage::Event { topic, payload } => match topic.as_str() {
            TOPIC_PLUGINS => sync_plugins(state).await,
            TOPIC_PLUGIN_SETTINGS => {
                if aegis_core::is_valid_plugin_name(&payload) {
                    crate::handlers::plugin::refresh_plugin_settings(state, &payload).await;
                }
            }
            TOPIC_NOTIFICATION_TEMPLATES => {
                crate::handlers::notifications::reload_templates(state).await
            }
            #[cfg(feature = "search")]
            crate::search::TOPIC_SEARCH => crate::search::handle_event(state, &payload).await,
            _ => tracing::debug!("Ignoring event bus topic '{}'", topic),
        },
        BusMessage::Resync => {
            sync_plugins(state).await;
            crate::handlers::notifications::reload_templates(state).await;
            let mut names: Vec<String> = state
                .plugin_settings
                .read()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .keys()
                .cloned()
                .collect();
            for name in state.plugin_registry.enabled().await {
                if !names.contains(&name) {
                    names.push(name);
                }
            }
            for name in names {
                crate::handlers::plugin::refresh_plugin_settings(state, &name).await;
            }
            // 切断中に届かなかったコンテンツの変更を取り込む
            #[cfg(feature = "search")]
            crate::search::rebuild(state).await;
        }
    }
}

async fn sync_plugins(state: &AppState) {
    if let Err(e) = crate::plugin_registry::sync_from_db(state).await {
        tracing::error!(
            "Could not apply the plugin list switched on another server (keeping the current plugins): {}",
            e
        );
    }
}

/// `_aegis_settings` の名前（値は [`Heartbeat`] の JSON）
pub const INSTANCE_SETTING: &str = "server.memory_cache_instance";

/// 生存の印を書き直す間隔
pub const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(10);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct Heartbeat {
    /// 起動ごとの ID
    id: String,
    /// 書き直すたびに増やす（時計がずれていても、動いているかを値の変化で確かめられる）
    beat: u64,
    at: DateTime<Utc>,
}

/// `[cache].backend = "memory"` のサーバーが同じ DB で 1 台だけ動くようにする
pub struct SingleServerGuard {
    db: Arc<dyn DatabaseAdapter>,
    id: String,
    task: tokio::task::JoinHandle<()>,
}

impl SingleServerGuard {
    /// 同じ DB で別のサーバー（メモリキャッシュ）が動いていればエラー。前の印が新しければ、書き直されるかを
    /// `interval` の 2 倍だけ待って確かめる（異常終了した直後の再起動は少し待つだけで起動する）
    pub async fn acquire(db: Arc<dyn DatabaseAdapter>, interval: Duration) -> anyhow::Result<Self> {
        let id = uuid::Uuid::new_v4().to_string();
        if let Some(other) = read(db.as_ref()).await? {
            let age = (Utc::now() - other.at).to_std().unwrap_or_default();
            if age < interval * 3 {
                tracing::info!(
                    "Another server with [cache].backend = \"memory\" was running on this database {:?} ago; checking whether it still is",
                    age
                );
                tokio::time::sleep(interval * 2).await;
                if read(db.as_ref()).await?.is_some_and(|again| again != other) {
                    anyhow::bail!(
                        "Another AegisCMS server with [cache].backend = \"memory\" is running on this database. \
                         The in-memory cache (login sessions, plugin switches) is not shared between servers: \
                         set [cache].backend = \"redis\" to run several servers"
                    );
                }
            }
        }

        let mut heartbeat = Heartbeat {
            id: id.clone(),
            beat: 0,
            at: Utc::now(),
        };
        write(db.as_ref(), &heartbeat).await?;
        let task_db = db.clone();
        let task = tokio::spawn(async move {
            loop {
                tokio::time::sleep(interval).await;
                match read(task_db.as_ref()).await {
                    Ok(Some(other)) if other.id != heartbeat.id => tracing::error!(
                        "Another AegisCMS server with [cache].backend = \"memory\" is running on this database: \
                         sessions and plugin switches are not shared. Use [cache].backend = \"redis\" for several servers"
                    ),
                    Ok(_) => {}
                    Err(e) => tracing::warn!("Could not read {}: {}", INSTANCE_SETTING, e),
                }
                heartbeat.beat += 1;
                heartbeat.at = Utc::now();
                if let Err(e) = write(task_db.as_ref(), &heartbeat).await {
                    tracing::warn!("Could not write {}: {}", INSTANCE_SETTING, e);
                }
            }
        });
        Ok(Self { db, id, task })
    }

    /// 終了時: 印を消す（すぐに再起動しても待たない）
    pub async fn release(self) {
        self.task.abort();
        if let Ok(Some(current)) = read(self.db.as_ref()).await {
            if current.id == self.id {
                if let Err(e) = self.db.delete_setting(INSTANCE_SETTING).await {
                    tracing::warn!("Could not remove {}: {}", INSTANCE_SETTING, e);
                }
            }
        }
    }
}

async fn read(db: &dyn DatabaseAdapter) -> aegis_core::Result<Option<Heartbeat>> {
    let settings = db.list_settings().await?;
    // 読めない値は無いものとして上書きする
    Ok(settings
        .get(INSTANCE_SETTING)
        .and_then(|value| serde_json::from_str(value).ok()))
}

async fn write(db: &dyn DatabaseAdapter, heartbeat: &Heartbeat) -> aegis_core::Result<()> {
    let value = serde_json::to_string(heartbeat).unwrap_or_default();
    db.set_setting(INSTANCE_SETTING, &value).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use aegis_runtime::PluginSupervisor;

    const INTERVAL: Duration = Duration::from_millis(100);

    #[tokio::test]
    async fn test_second_memory_cache_server_is_refused() {
        let state = crate::test_support::test_state(PluginSupervisor::new()).await;
        let first = SingleServerGuard::acquire(state.db.clone(), INTERVAL)
            .await
            .unwrap();

        let err = SingleServerGuard::acquire(state.db.clone(), INTERVAL)
            .await
            .err()
            .expect("the first server is still running");
        assert!(err.to_string().contains("redis"), "{}", err);

        // After a clean shutdown the next server starts at once
        first.release().await;
        let started = std::time::Instant::now();
        let second = SingleServerGuard::acquire(state.db.clone(), INTERVAL)
            .await
            .unwrap();
        assert!(started.elapsed() < INTERVAL);
        second.release().await;
        assert!(!state
            .db
            .list_settings()
            .await
            .unwrap()
            .contains_key(INSTANCE_SETTING));
    }

    #[tokio::test]
    async fn test_server_restarting_after_a_crash_takes_over() {
        let state = crate::test_support::test_state(PluginSupervisor::new()).await;
        // A fresh mark that is no longer rewritten (the server was killed)
        let crashed = SingleServerGuard::acquire(state.db.clone(), INTERVAL)
            .await
            .unwrap();
        crashed.task.abort();

        let next = SingleServerGuard::acquire(state.db.clone(), INTERVAL)
            .await
            .unwrap();
        let saved = read(state.db.as_ref()).await.unwrap().unwrap();
        assert_eq!(saved.id, next.id);
        next.release().await;
    }
}
