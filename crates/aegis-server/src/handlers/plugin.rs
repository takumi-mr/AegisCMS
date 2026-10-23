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
use aegis_core::{AuditAction, AuditOutcome};
use aegis_runtime::{PluginInfo, WasmPluginLoader};

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
/// 有効なもの（`[plugins].enabled` の順 = フックの実行順）→ 無効なもの（名前順）の順に返す
pub async fn list_plugins(
    State(state): State<Arc<AppState>>,
    RequireRole(_session): AdminSession,
) -> Result<Json<Vec<PluginInfo>>, (StatusCode, Json<serde_json::Value>)> {
    let mut plugins = WasmPluginLoader::list(
        &state.config.paths.plugins_dir,
        &state.config.plugins.enabled,
        &state.config.runtime.default_resource_limits(),
    )
    .map_err(|e| {
        tracing::error!("Failed to list plugins: {}", e);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": "Failed to list plugins" })),
        )
    })?;

    for p in &mut plugins {
        p.settings = get_effective_plugin_settings(&state, &p.name).await;
    }

    Ok(Json(plugins))
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
