//! 有効なプラグインの一覧と、再起動なしの有効・無効の切り替え。
//! 一覧は `[plugins].enabled` を、Admin Studio で変更した値（DB の `_aegis_settings` の `plugins.enabled`）で
//! 上書きする。切り替えは新しい `PluginSupervisor` を組み立ててから差し替える（読み込み済みのプラグインは使い回す）。
//! 切り替えたサーバーはイベントバスで知らせ、受け取ったサーバー（自分を含む）は DB の一覧に揃える（[`sync_from_db`]）

use std::path::PathBuf;
use std::sync::Arc;

use aegis_core::{is_valid_plugin_name, AegisError, PluginResourceLimits, Result};
use aegis_db::DatabaseAdapter;
use aegis_runtime::{AegisPlugin, PluginServices, PluginSupervisor, WasmEngine, WasmPluginLoader};
use tokio::sync::Mutex;

use crate::state::AppState;

/// `_aegis_settings` の名前（値は JSON の文字列の配列。順序がフックの実行順）
pub const ENABLED_SETTING: &str = "plugins.enabled";

const MANIFEST_FILE: &str = "plugin.manifest.json";

/// 有効なプラグインの一覧（Admin Studio で保存した値 → `[plugins].enabled` の順）。
/// 保存した値が壊れていればエラーにする（アクセス制限などのプラグインが黙って外れることを防ぐため）
pub async fn resolve_enabled(
    db: &dyn DatabaseAdapter,
    configured: &[String],
) -> Result<Vec<String>> {
    let settings = db.list_settings().await?;
    let Some(saved) = settings.get(ENABLED_SETTING) else {
        return Ok(configured.to_vec());
    };
    let enabled: Vec<String> = serde_json::from_str(saved).map_err(|e| {
        AegisError::Config(format!(
            "_aegis_settings \"{}\" is not a JSON array of plugin names: {}",
            ENABLED_SETTING, e
        ))
    })?;
    if let Some(invalid) = enabled.iter().find(|name| !is_valid_plugin_name(name)) {
        return Err(AegisError::Config(format!(
            "_aegis_settings \"{}\" has an invalid plugin name {:?}",
            ENABLED_SETTING, invalid
        )));
    }
    tracing::info!(
        "Using the plugin list saved from Admin Studio (overrides [plugins].enabled): {:?}",
        enabled
    );
    Ok(enabled)
}

/// プラグインを読み込むための部品（Wasm エンジン・ホスト API・インストール先）
pub struct PluginLoader {
    engine: WasmEngine,
    services: PluginServices,
    plugins_dir: PathBuf,
    default_limits: PluginResourceLimits,
}

impl PluginLoader {
    pub fn new(
        engine: WasmEngine,
        services: PluginServices,
        plugins_dir: PathBuf,
        default_limits: PluginResourceLimits,
    ) -> Self {
        Self {
            engine,
            services,
            plugins_dir,
            default_limits,
        }
    }

    /// 起動時: `enabled` をその順に読み込む（インストール済みで無効なものはログに出す）
    pub fn load(&self, enabled: &[String]) -> Result<PluginSupervisor> {
        let mut supervisor = PluginSupervisor::new();
        WasmPluginLoader::load_enabled(
            &self.plugins_dir,
            enabled,
            self.engine.clone(),
            &mut supervisor,
            self.services.clone(),
            &self.default_limits,
        )?;
        Ok(supervisor)
    }

    /// 切り替え時: `enabled` の順に組み立てる。`loaded` にあるものは使い回し、Wasm をコンパイルし直さない
    pub fn rebuild(
        &self,
        enabled: &[String],
        loaded: &[Arc<dyn AegisPlugin>],
    ) -> Result<PluginSupervisor> {
        let mut supervisor = PluginSupervisor::new();
        for name in enabled {
            if let Some(plugin) = loaded.iter().find(|p| &p.manifest().name == name) {
                supervisor.register(plugin.clone());
                continue;
            }
            let plugin_dir = self.plugins_dir.join(name);
            if !plugin_dir.join(MANIFEST_FILE).is_file() {
                return Err(AegisError::Validation(format!(
                    "Plugin '{}' is not installed",
                    name
                )));
            }
            let instance = WasmPluginLoader::load_plugin(
                &plugin_dir,
                self.engine.clone(),
                self.services.clone(),
                &self.default_limits,
            )
            .map_err(|e| {
                AegisError::Validation(format!("Failed to load plugin '{}': {}", name, e))
            })?;
            supervisor.register(instance);
        }
        Ok(supervisor)
    }
}

/// 有効なプラグインの一覧と、切り替えに使うローダー
pub struct PluginRegistry {
    /// `[runtime].enable_wasm_plugins = false` なら `None`（切り替えられない）
    loader: Option<Arc<PluginLoader>>,
    /// 有効なプラグイン（フックの実行順）。切り替えの間はロックしたままにして、同時の切り替えを順に処理する
    enabled: Mutex<Vec<String>>,
}

impl PluginRegistry {
    pub fn new(loader: Option<PluginLoader>, enabled: Vec<String>) -> Self {
        Self {
            loader: loader.map(Arc::new),
            enabled: Mutex::new(enabled),
        }
    }

    pub fn loader(&self) -> Option<&Arc<PluginLoader>> {
        self.loader.as_ref()
    }

    pub async fn enabled(&self) -> Vec<String> {
        self.enabled.lock().await.clone()
    }

    /// 切り替えの間に持つロック（中身が一覧）
    pub async fn lock(&self) -> tokio::sync::MutexGuard<'_, Vec<String>> {
        self.enabled.lock().await
    }
}

/// `next` の順の supervisor を組み立てる（`current` は今の一覧）。Wasm のコンパイルは重いのでブロッキング用の
/// スレッドで行い、読み込み済みのものは使い回す。読み込めなければ `Validation` エラー
pub async fn prepare(
    state: &AppState,
    current: &[String],
    next: &[String],
) -> Result<PluginSupervisor> {
    let Some(loader) = state.plugin_registry.loader().cloned() else {
        return Err(AegisError::Config(
            "Wasm plugins are turned off on this server ([runtime].enable_wasm_plugins = false)"
                .to_string(),
        ));
    };
    // 管理画面で保存した設定値を、新しく有効にするプラグインが読めるようにメモリに載せておく
    for name in next.iter().filter(|n| !current.contains(n)) {
        crate::handlers::plugin::get_effective_plugin_settings(state, name).await;
    }
    let loaded = state.supervisor.read().await.plugins().to_vec();
    let next = next.to_vec();
    tokio::task::spawn_blocking(move || loader.rebuild(&next, &loaded))
        .await
        .map_err(|e| AegisError::Internal(format!("Plugin loading task failed: {}", e)))?
}

/// 組み立てた supervisor に差し替える（プラグインが登録するテンプレートも入れ替える）
pub async fn install(state: &AppState, supervisor: PluginSupervisor) {
    let templates = supervisor.collect_plugin_templates();
    let mut current = state.supervisor.write().await;
    for old in current.collect_plugin_templates().keys() {
        if !templates.contains_key(old) {
            state.theme.unregister_template(old);
        }
    }
    for (template, content) in &templates {
        state.theme.register_template(template, content);
    }
    *current = supervisor;
}

/// DB の一覧（保存した値が無ければ `[plugins].enabled`）に揃える。切り替えの知らせを受けたときに呼ぶ
/// （知らせたサーバー自身では一覧が同じなので何もしない）。差し替えたら `true`
pub async fn sync_from_db(state: &AppState) -> Result<bool> {
    let mut enabled = state.plugin_registry.lock().await;
    let next = resolve_enabled(state.db.as_ref(), &state.config.plugins.enabled).await?;
    if next == *enabled {
        return Ok(false);
    }
    if state.plugin_registry.loader().is_some() {
        let supervisor = prepare(state, &enabled, &next).await?;
        install(state, supervisor).await;
    }
    tracing::info!(
        "Plugins switched on another server: {:?} -> {:?}",
        *enabled,
        next
    );
    *enabled = next;
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_resolve_enabled_prefers_the_saved_list() {
        let state = crate::test_support::test_state(PluginSupervisor::new()).await;
        let db = state.db.as_ref();
        let configured = vec!["from-config".to_string()];

        assert_eq!(resolve_enabled(db, &configured).await.unwrap(), configured);

        db.set_setting(ENABLED_SETTING, r#"["b","a"]"#)
            .await
            .unwrap();
        assert_eq!(
            resolve_enabled(db, &configured).await.unwrap(),
            vec!["b", "a"]
        );

        // An empty saved list disables everything (it is not "unset")
        db.set_setting(ENABLED_SETTING, "[]").await.unwrap();
        assert!(resolve_enabled(db, &configured).await.unwrap().is_empty());

        for broken in [r#"{"a":1}"#, r#"["../etc"]"#] {
            db.set_setting(ENABLED_SETTING, broken).await.unwrap();
            assert!(
                resolve_enabled(db, &configured).await.is_err(),
                "{}",
                broken
            );
        }
    }
}
