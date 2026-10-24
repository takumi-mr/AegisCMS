use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::HashMap;
use std::sync::Arc;

use crate::audit::AuditContext;
use crate::auth::authorize::RequireRole;
use crate::auth::AdminSession;
use crate::state::AppState;
use aegis_core::{AuditAction, AuditActor, AuditOutcome};
use aegis_runtime::{PluginInfo, WasmPluginLoader};

type ApiError = (StatusCode, Json<serde_json::Value>);

fn api_error(status: StatusCode, message: impl Into<String>) -> ApiError {
    (status, Json(json!({ "error": message.into() })))
}

pub(crate) async fn get_effective_plugin_settings(
    state: &AppState,
    plugin_name: &str,
) -> HashMap<String, String> {
    let mut map = HashMap::new();

    // 1. 静的設定（aegis.toml）
    if let Some(static_settings) = state.config.plugins.settings.get(plugin_name) {
        for (k, v) in static_settings {
            let val_str = match v {
                toml::Value::String(s) => match s.strip_prefix("env:") {
                    Some(name) => std::env::var(name.trim()).unwrap_or_default(),
                    None => s.clone(),
                },
                other => serde_json::to_string(other).unwrap_or_default(),
            };
            map.insert(k.clone(), val_str);
        }
    }

    // 2. メモリ内動的設定の確認
    let loaded = {
        let guard = state.plugin_settings.read().unwrap();
        guard.get(plugin_name).cloned()
    };

    if let Some(dyn_map) = loaded {
        map.extend(dyn_map);
    } else {
        // キャッシュからロード
        let key = format!("plugin_settings:{}", plugin_name);
        if let Ok(Some(json_str)) = state.cache.get(&key).await {
            if let Ok(dyn_map) = serde_json::from_str::<HashMap<String, String>>(&json_str) {
                if let Ok(mut guard) = state.plugin_settings.write() {
                    guard.insert(plugin_name.to_string(), dyn_map.clone());
                }
                map.extend(dyn_map);
            }
        }
    }

    map
}

/// `GET /api/v1/plugins` — インストール済み・有効化済みのプラグイン一覧（admin 権限必須）。
/// 有効なもの（有効化リストの順 = フックの実行順）→ 無効なもの（名前順）の順に返す
pub async fn list_plugins(
    State(state): State<Arc<AppState>>,
    RequireRole(_session): AdminSession,
) -> Result<Json<Vec<PluginInfo>>, ApiError> {
    let enabled = state.plugin_registry.enabled().await;
    plugin_list(&state, &enabled).await.map(Json)
}

async fn plugin_list(state: &AppState, enabled: &[String]) -> Result<Vec<PluginInfo>, ApiError> {
    let mut plugins = WasmPluginLoader::list(
        &state.config.paths.plugins_dir,
        enabled,
        &state.config.runtime.default_resource_limits(),
    )
    .map_err(|e| {
        tracing::error!("Failed to list plugins: {}", e);
        api_error(StatusCode::INTERNAL_SERVER_ERROR, "Failed to list plugins")
    })?;

    for p in &mut plugins {
        p.settings = get_effective_plugin_settings(state, &p.name).await;
    }

    Ok(plugins)
}

#[derive(Debug, Deserialize, Serialize)]
pub struct SetPluginEnabledRequest {
    pub enabled: bool,
}

/// `PUT /api/v1/plugins/{name}/enabled` — プラグインを有効・無効にする（admin 権限必須）。
/// 有効にしたプラグインはフックの実行順の最後に加わる。一覧を DB（`_aegis_settings` の `plugins.enabled`）に
/// 保存し、再起動せずに反映する（以後 `[plugins].enabled` より優先）。応答は `GET /api/v1/plugins` と同じ
pub async fn set_plugin_enabled(
    State(state): State<Arc<AppState>>,
    RequireRole(session): AdminSession,
    audit: AuditContext,
    Path(name): Path<String>,
    Json(payload): Json<SetPluginEnabledRequest>,
) -> Result<Json<Vec<PluginInfo>>, ApiError> {
    if !aegis_core::is_valid_plugin_name(&name) {
        return Err(api_error(StatusCode::BAD_REQUEST, "Invalid plugin name"));
    }
    let Some(loader) = state.plugin_registry.loader().cloned() else {
        return Err(api_error(
            StatusCode::CONFLICT,
            "Wasm plugins are turned off on this server ([runtime].enable_wasm_plugins = false)",
        ));
    };

    // 同時の切り替えは順に処理する（一覧の読み書きと supervisor の差し替えの間ロックを持つ）
    let mut enabled = state.plugin_registry.lock().await;
    if enabled.contains(&name) == payload.enabled {
        return plugin_list(&state, &enabled).await.map(Json);
    }
    if !payload.enabled && is_notification_channel(&state, &name) {
        return Err(api_error(
            StatusCode::CONFLICT,
            format!(
                "Plugin '{}' is a notification channel in [notifications].channels; remove it there first",
                name
            ),
        ));
    }
    let next: Vec<String> = if payload.enabled {
        enabled.iter().cloned().chain([name.clone()]).collect()
    } else {
        enabled.iter().filter(|n| **n != name).cloned().collect()
    };

    let (action, verb) = if payload.enabled {
        (AuditAction::PluginEnable, "enabled")
    } else {
        (AuditAction::PluginDisable, "disabled")
    };
    let actor = Some(AuditActor {
        user_id: session.user_id,
        role: session.role,
    });
    let failure = |error: String| {
        audit
            .event(action, AuditOutcome::Failure)
            .with_actor(actor.clone())
            .with_resource("plugin", Some(name.clone()))
            .with_details(json!({ "error": error }))
    };

    if payload.enabled {
        // 管理画面で保存した設定値を、プラグインが読めるようにメモリに載せておく
        get_effective_plugin_settings(&state, &name).await;
    }

    // Wasm のコンパイルは重いのでブロッキング用のスレッドで行う（読み込み済みのものは使い回す）
    let loaded = state.supervisor.read().await.plugins().to_vec();
    let rebuild_list = next.clone();
    let supervisor =
        match tokio::task::spawn_blocking(move || loader.rebuild(&rebuild_list, &loaded)).await {
            Ok(Ok(supervisor)) => supervisor,
            Ok(Err(e)) => {
                state.audit.record(failure(e.to_string())).await;
                return Err(api_error(StatusCode::BAD_REQUEST, e.to_string()));
            }
            Err(e) => {
                tracing::error!("Plugin loading task failed: {}", e);
                state.audit.record(failure(e.to_string())).await;
                return Err(api_error(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "Failed to load the plugins",
                ));
            }
        };

    let json = serde_json::to_string(&next).unwrap_or_default();
    if let Err(e) = state
        .db
        .set_setting(crate::plugin_registry::ENABLED_SETTING, &json)
        .await
    {
        tracing::error!("Failed to save the enabled plugins: {}", e);
        state.audit.record(failure(e.to_string())).await;
        return Err(api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Failed to save the enabled plugins",
        ));
    }

    // プラグインが登録するテンプレートを入れ替えて、supervisor を差し替える
    let templates = supervisor.collect_plugin_templates();
    {
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
    let previous = std::mem::replace(&mut *enabled, next);
    tracing::info!("Plugin '{}' {} from Admin Studio", name, verb);

    state
        .audit
        .record(
            audit
                .event(action, AuditOutcome::Success)
                .with_actor(actor.clone())
                .with_resource("plugin", Some(name.clone()))
                .with_details(json!({ "from": previous, "to": *enabled })),
        )
        .await;

    plugin_list(&state, &enabled).await.map(Json)
}

/// `[notifications].channels` の `plugin:<名前>` に使われているか（無効にすると通知が届かなくなる）
fn is_notification_channel(state: &AppState, name: &str) -> bool {
    let notifications = &state.config.notifications;
    notifications.enabled
        && notifications
            .channels
            .iter()
            .any(|c| matches!(c, aegis_notify::ChannelKind::Plugin(p) if p == name))
}

/// `GET /api/v1/plugins/{name}/settings` — プラグインの設定値を取得する（admin 権限必須）
pub async fn get_plugin_settings(
    State(state): State<Arc<AppState>>,
    RequireRole(_session): AdminSession,
    Path(name): Path<String>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    if !aegis_core::is_valid_plugin_name(&name) {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "Invalid plugin name" })),
        ));
    }
    let settings = get_effective_plugin_settings(&state, &name).await;
    Ok(Json(json!({ "name": name, "settings": settings })))
}

#[derive(Debug, Deserialize, Serialize)]
pub struct UpdatePluginSettingsRequest {
    pub settings: HashMap<String, String>,
}

/// `PUT /api/v1/plugins/{name}/settings` — プラグインの設定値を更新する（admin 権限必須）
pub async fn update_plugin_settings(
    State(state): State<Arc<AppState>>,
    RequireRole(_session): AdminSession,
    audit: AuditContext,
    Path(name): Path<String>,
    Json(payload): Json<UpdatePluginSettingsRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    if !aegis_core::is_valid_plugin_name(&name) {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "Invalid plugin name" })),
        ));
    }

    let key = format!("plugin_settings:{}", name);
    let json_str = serde_json::to_string(&payload.settings).map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": format!("Serialization failed: {}", e) })),
        )
    })?;

    state.cache.set(&key, &json_str, None).await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": format!("Cache set failed: {}", e) })),
        )
    })?;

    if let Ok(mut guard) = state.plugin_settings.write() {
        guard.insert(name.clone(), payload.settings.clone());
    }

    state
        .audit
        .record(
            audit
                .event(AuditAction::PluginSettingsUpdate, AuditOutcome::Success)
                .with_resource("plugin", Some(name.clone()))
                .with_details(json!({ "keys": payload.settings.keys().collect::<Vec<_>>() })),
        )
        .await;

    let effective = get_effective_plugin_settings(&state, &name).await;
    Ok(Json(json!({ "name": name, "settings": effective })))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::AegisConfig;
    use crate::test_support::test_state_with_config;
    use aegis_auth::SessionData;
    use aegis_runtime::PluginSupervisor;

    fn admin() -> AdminSession {
        RequireRole(SessionData {
            id: "ses_admin".to_string(),
            user_id: "admin1".to_string(),
            role: "admin".to_string(),
            created_at: String::new(),
            expires_at: String::new(),
            metadata: Default::default(),
        })
    }

    #[tokio::test]
    async fn test_list_plugins_reports_enabled_and_installed_state() {
        let plugins_dir =
            std::env::temp_dir().join(format!("aegis_plugins_api_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&plugins_dir);
        for name in ["seo-tools", "slack-notifier"] {
            let dir = plugins_dir.join(name);
            std::fs::create_dir_all(&dir).unwrap();
            std::fs::write(
                dir.join("plugin.manifest.json"),
                format!(
                    r#"{{"name": "{}", "version": "1.2.0", "entrypoint": "plugin.wasm",
                        "display_name": "{} plugin", "capabilities": {{"hooks": ["on-before-save"]}}}}"#,
                    name, name
                ),
            )
            .unwrap();
            std::fs::write(dir.join("plugin.wasm"), b"\0asm\x01\0\0\0").unwrap();
        }
        let mut config = AegisConfig::default();
        config.paths.plugins_dir = plugins_dir.clone();
        config.plugins.enabled = vec!["slack-notifier".to_string(), "removed".to_string()];
        let state = test_state_with_config(config, PluginSupervisor::new()).await;

        let Json(plugins) = list_plugins(State(state), admin()).await.unwrap();
        let summary: Vec<(&str, bool, bool)> = plugins
            .iter()
            .map(|p| (p.name.as_str(), p.is_enabled, p.is_installed))
            .collect();
        assert_eq!(
            summary,
            vec![
                ("slack-notifier", true, true),
                ("removed", true, false),
                ("seo-tools", false, true),
            ]
        );
        let manifest = plugins[0].manifest.as_ref().unwrap();
        assert_eq!(manifest.version, "1.2.0");
        assert_eq!(
            manifest.display_name.as_deref(),
            Some("slack-notifier plugin")
        );
        let _ = std::fs::remove_dir_all(&plugins_dir);
    }

    /// The repository's `plugins/` (official plugins with their built Wasm), switchable at runtime
    async fn switchable_state(
        enabled: &[&str],
        configure: impl FnOnce(&mut AegisConfig),
    ) -> Arc<AppState> {
        let plugins_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../plugins");
        let mut config = AegisConfig::default();
        config.paths.plugins_dir = plugins_dir.clone();
        configure(&mut config);
        let enabled: Vec<String> = enabled.iter().map(|n| n.to_string()).collect();
        let loader = crate::plugin_registry::PluginLoader::new(
            aegis_runtime::WasmEngine::new().unwrap(),
            Arc::new(std::sync::Mutex::new(HashMap::new())).into(),
            plugins_dir,
            config.runtime.default_resource_limits(),
        );
        let supervisor = loader.load(&enabled).unwrap();
        let mut state = crate::test_support::test_app_state(config, supervisor, Vec::new()).await;
        state.plugin_registry = Arc::new(crate::plugin_registry::PluginRegistry::new(
            Some(loader),
            enabled,
        ));
        Arc::new(state)
    }

    async fn set_enabled(
        state: &Arc<AppState>,
        name: &str,
        enabled: bool,
    ) -> Result<Vec<PluginInfo>, StatusCode> {
        set_plugin_enabled(
            State(state.clone()),
            admin(),
            AuditContext::default(),
            Path(name.to_string()),
            Json(SetPluginEnabledRequest { enabled }),
        )
        .await
        .map(|Json(list)| list)
        .map_err(|(status, _)| status)
    }

    async fn loaded_names(state: &AppState) -> Vec<String> {
        state
            .supervisor
            .read()
            .await
            .plugins()
            .iter()
            .map(|p| p.manifest().name.clone())
            .collect()
    }

    async fn saved_list(state: &AppState) -> Option<String> {
        state
            .db
            .list_settings()
            .await
            .unwrap()
            .remove(crate::plugin_registry::ENABLED_SETTING)
    }

    #[tokio::test]
    async fn test_enable_and_disable_plugins_without_restart() {
        let state = switchable_state(&["official-code-enhancer"], |_| {}).await;
        let enhancer = state.supervisor.read().await.plugins()[0].clone();

        // Enabling appends to the hook order and keeps the loaded instance
        let list = set_enabled(&state, "official-shop-rules", true)
            .await
            .unwrap();
        let enabled: Vec<&str> = list
            .iter()
            .filter(|p| p.is_enabled)
            .map(|p| p.name.as_str())
            .collect();
        assert_eq!(
            enabled,
            vec!["official-code-enhancer", "official-shop-rules"]
        );
        assert_eq!(
            loaded_names(&state).await,
            vec!["official-code-enhancer", "official-shop-rules"]
        );
        assert!(Arc::ptr_eq(
            &enhancer,
            &state.supervisor.read().await.plugins()[0]
        ));
        assert_eq!(
            saved_list(&state).await.as_deref(),
            Some(r#"["official-code-enhancer","official-shop-rules"]"#)
        );

        // Enabling again changes nothing
        set_enabled(&state, "official-shop-rules", true)
            .await
            .unwrap();
        assert_eq!(loaded_names(&state).await.len(), 2);

        let list = set_enabled(&state, "official-code-enhancer", false)
            .await
            .unwrap();
        assert!(
            !list
                .iter()
                .find(|p| p.name == "official-code-enhancer")
                .unwrap()
                .is_enabled
        );
        assert_eq!(loaded_names(&state).await, vec!["official-shop-rules"]);
        assert_eq!(
            state.plugin_registry.enabled().await,
            vec!["official-shop-rules"]
        );
        assert_eq!(
            saved_list(&state).await.as_deref(),
            Some(r#"["official-shop-rules"]"#)
        );

        // A plugin that is not installed is rejected and nothing changes
        assert_eq!(
            set_enabled(&state, "not-installed", true)
                .await
                .unwrap_err(),
            StatusCode::BAD_REQUEST
        );
        assert_eq!(
            set_enabled(&state, "../plugins", true).await.unwrap_err(),
            StatusCode::BAD_REQUEST
        );
        assert_eq!(loaded_names(&state).await, vec!["official-shop-rules"]);
        assert_eq!(
            saved_list(&state).await.as_deref(),
            Some(r#"["official-shop-rules"]"#)
        );
    }

    #[tokio::test]
    async fn test_plugin_switch_is_refused_when_it_cannot_apply() {
        // A plugin used as a notification channel cannot be disabled
        let state = switchable_state(&["official-order-notifier"], |config| {
            config.notifications.enabled = true;
            config.notifications.channels = vec![aegis_notify::ChannelKind::Plugin(
                "official-order-notifier".into(),
            )];
        })
        .await;
        assert_eq!(
            set_enabled(&state, "official-order-notifier", false)
                .await
                .unwrap_err(),
            StatusCode::CONFLICT
        );
        assert_eq!(loaded_names(&state).await, vec!["official-order-notifier"]);

        // Without the Wasm runtime ([runtime].enable_wasm_plugins = false) there is nothing to switch
        let state = test_state_with_config(AegisConfig::default(), PluginSupervisor::new()).await;
        assert_eq!(
            set_enabled(&state, "official-shop-rules", true)
                .await
                .unwrap_err(),
            StatusCode::CONFLICT
        );
        assert_eq!(saved_list(&state).await, None);
    }

    #[tokio::test]
    async fn test_plugin_settings_get_and_update() {
        let state = test_state_with_config(AegisConfig::default(), PluginSupervisor::new()).await;

        // 1. Initial settings (empty)
        let Json(res) = get_plugin_settings(
            State(state.clone()),
            admin(),
            Path("official-shop-rules".to_string()),
        )
        .await
        .unwrap();
        assert_eq!(res["name"], "official-shop-rules");
        assert_eq!(res["settings"], json!({}));

        // 2. Update settings
        let mut new_settings = HashMap::new();
        new_settings.insert("shipping_fee".to_string(), "700".to_string());
        new_settings.insert("coupons".to_string(), "SALE20:20%".to_string());

        let Json(updated) = update_plugin_settings(
            State(state.clone()),
            admin(),
            AuditContext::default(),
            Path("official-shop-rules".to_string()),
            Json(UpdatePluginSettingsRequest {
                settings: new_settings.clone(),
            }),
        )
        .await
        .unwrap();

        assert_eq!(updated["name"], "official-shop-rules");
        assert_eq!(updated["settings"]["shipping_fee"], "700");
        assert_eq!(updated["settings"]["coupons"], "SALE20:20%");

        // 3. Fetch again to verify persistence in cache & state
        let Json(fetched) = get_plugin_settings(
            State(state.clone()),
            admin(),
            Path("official-shop-rules".to_string()),
        )
        .await
        .unwrap();
        assert_eq!(fetched["settings"]["shipping_fee"], "700");

        // 4. Invalid name rejected
        assert_eq!(
            get_plugin_settings(
                State(state.clone()),
                admin(),
                Path("INVALID NAME!".to_string())
            )
            .await
            .unwrap_err()
            .0,
            StatusCode::BAD_REQUEST
        );
    }
}
