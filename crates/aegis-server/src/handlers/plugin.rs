use axum::{extract::State, http::StatusCode, Json};
use serde_json::json;
use std::sync::Arc;

use crate::auth::authorize::RequireRole;
use crate::auth::AdminSession;
use crate::state::AppState;
use aegis_runtime::{PluginInfo, WasmPluginLoader};

/// `GET /api/v1/plugins` — インストール済み・有効化済みのプラグイン一覧（admin 権限必須）。
/// 有効なもの（`[plugins].enabled` の順 = フックの実行順）→ 無効なもの（名前順）の順に返す
pub async fn list_plugins(
    State(state): State<Arc<AppState>>,
    RequireRole(_session): AdminSession,
) -> Result<Json<Vec<PluginInfo>>, (StatusCode, Json<serde_json::Value>)> {
    WasmPluginLoader::list(
        &state.config.paths.plugins_dir,
        &state.config.plugins.enabled,
        &state.config.runtime.default_resource_limits(),
    )
    .map(Json)
    .map_err(|e| {
        tracing::error!("Failed to list plugins: {}", e);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": "Failed to list plugins" })),
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth::SessionData;
    use crate::test_support::test_state_with_config;
    use aegis_core::AegisConfig;
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
}
