use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::path::Path as FilePath;
use std::sync::Arc;

use crate::audit::AuditContext;
use crate::auth::authorize::RequireRole;
use crate::auth::{AdminSession, ViewerSession};
use crate::state::AppState;
use aegis_core::{guess_content_type, AuditAction, AuditOutcome};
use aegis_theme::ThemeInfo;

#[derive(Debug, Deserialize, Serialize)]
pub struct SwitchThemePayload {
    pub theme_id: String,
}

/// `GET /api/v1/themes` — インストールされているテーマ一覧を取得
pub async fn list_themes(
    State(state): State<Arc<AppState>>,
    RequireRole(_session): ViewerSession,
) -> Json<Vec<ThemeInfo>> {
    let themes = state.theme.list_installed_themes();
    Json(themes)
}

/// `GET /api/v1/themes/active` — 現在アクティブなテーマ情報を取得
pub async fn get_active_theme(
    State(state): State<Arc<AppState>>,
    RequireRole(_session): ViewerSession,
) -> Result<Json<ThemeInfo>, (StatusCode, String)> {
    let active_id = state.theme.active_theme();
    match state.theme.get_theme_info(&active_id) {
        Some(info) => Ok(Json(info)),
        None => Err((
            StatusCode::NOT_FOUND,
            format!("Active theme '{}' not found in registry", active_id),
        )),
    }
}

/// `PUT /api/v1/themes/active` — アクティブなテーマを動的に切り替える（admin 権限必須）
pub async fn switch_active_theme(
    State(state): State<Arc<AppState>>,
    RequireRole(session): AdminSession,
    audit_ctx: AuditContext,
    Json(payload): Json<SwitchThemePayload>,
) -> Result<Json<ThemeInfo>, (StatusCode, String)> {
    let old_theme = state.theme.active_theme();
    let new_theme = payload.theme_id.trim();

    if new_theme.is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            "Theme ID cannot be empty".to_string(),
        ));
    }

    // テーマ切り替えの適用
    if let Err(e) = state.theme.set_active_theme(new_theme) {
        return Err((
            StatusCode::BAD_REQUEST,
            format!("Failed to activate theme: {}", e),
        ));
    }

    let event = audit_ctx
        .event(AuditAction::ThemeSwitch, AuditOutcome::Success)
        .with_actor(Some(aegis_core::AuditActor {
            user_id: session.user_id,
            role: session.role,
        }))
        .with_details(json!({
            "from_theme": old_theme,
            "to_theme": new_theme,
        }));
    state.audit.record(event).await;

    // 更新後のテーマ情報を取得して返却
    match state.theme.get_theme_info(new_theme) {
        Some(info) => Ok(Json(info)),
        None => Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Theme '{}' activated but failed to fetch info", new_theme),
        )),
    }
}

/// `GET /themes/{theme_id}/preview` — テーマのプレビュー画像を配信
pub async fn get_theme_preview(
    State(state): State<Arc<AppState>>,
    Path(theme_id): Path<String>,
) -> impl IntoResponse {
    let preview_file = state.theme.theme_preview_file(&theme_id);
    match preview_file {
        Some(path) if path.is_file() => match std::fs::read(&path) {
            Ok(bytes) => {
                let mime =
                    guess_content_type(path.file_name().and_then(|s| s.to_str()).unwrap_or(""));
                ([(axum::http::header::CONTENT_TYPE, mime)], bytes).into_response()
            }
            Err(_) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Failed to read preview image",
            )
                .into_response(),
        },
        _ => (StatusCode::NOT_FOUND, "Theme preview not found").into_response(),
    }
}

/// `GET /theme/assets/{*path}` — 現在アクティブなテーマのアセットを動的に解決して配信
pub async fn get_active_theme_asset(
    State(state): State<Arc<AppState>>,
    Path(path): Path<String>,
) -> impl IntoResponse {
    let assets_dir = state.theme.active_assets_dir();
    match assets_dir {
        Some(dir) => serve_file_safely(&dir, &path),
        None => (StatusCode::NOT_FOUND, "No assets found for active theme").into_response(),
    }
}

/// `GET /themes/{theme_id}/assets/{*path}` — 指定したテーマのアセットを配信
pub async fn get_specific_theme_asset(
    State(state): State<Arc<AppState>>,
    Path((theme_id, path)): Path<(String, String)>,
) -> impl IntoResponse {
    let assets_dir = state.theme.theme_assets_dir(&theme_id);
    match assets_dir {
        Some(dir) => serve_file_safely(&dir, &path),
        None => (
            StatusCode::NOT_FOUND,
            format!("Assets not found for theme '{}'", theme_id),
        )
            .into_response(),
    }
}

fn serve_file_safely(base_dir: &FilePath, rel_path: &str) -> axum::response::Response {
    let clean_path = FilePath::new(rel_path);
    // パストラバーサル脆弱性の防止
    for comp in clean_path.components() {
        if matches!(
            comp,
            std::path::Component::ParentDir
                | std::path::Component::Prefix(_)
                | std::path::Component::RootDir
        ) {
            return (StatusCode::BAD_REQUEST, "Invalid file path").into_response();
        }
    }

    let target = base_dir.join(clean_path);
    if !target.is_file() {
        return (StatusCode::NOT_FOUND, "Asset not found").into_response();
    }

    match std::fs::read(&target) {
        Ok(bytes) => {
            let mime =
                guess_content_type(target.file_name().and_then(|s| s.to_str()).unwrap_or(""));
            ([(axum::http::header::CONTENT_TYPE, mime)], bytes).into_response()
        }
        Err(_) => (StatusCode::INTERNAL_SERVER_ERROR, "Failed to read asset").into_response(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::test_state;
    use aegis_auth::SessionData;
    use aegis_runtime::PluginSupervisor;

    fn viewer() -> ViewerSession {
        RequireRole(SessionData {
            id: "ses_viewer".to_string(),
            user_id: "viewer1".to_string(),
            role: "viewer".to_string(),
            created_at: String::new(),
            expires_at: String::new(),
            metadata: Default::default(),
        })
    }

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
    async fn test_theme_list_and_active_theme() {
        let state = test_state(PluginSupervisor::new()).await;

        // 1. テーマ一覧取得
        let Json(themes) = list_themes(State(state.clone()), viewer()).await;
        assert!(!themes.is_empty());
        assert!(themes.iter().any(|t| t.manifest.id == "default"));
    }

    #[tokio::test]
    async fn test_theme_lifecycle_handlers() {
        let state = test_state(PluginSupervisor::new()).await;

        // 1. アクティブテーマ取得
        let Json(active) = get_active_theme(State(state.clone()), viewer())
            .await
            .expect("active theme should be found");
        assert_eq!(active.manifest.id, "default");
        assert!(active.is_active);

        // 2. Admin によるテーマ切り替え
        let Json(updated) = switch_active_theme(
            State(state.clone()),
            admin(),
            AuditContext::default(),
            Json(SwitchThemePayload {
                theme_id: "default".to_string(),
            }),
        )
        .await
        .expect("Switching to default should succeed");

        assert_eq!(updated.manifest.id, "default");

        // 3. 存在しないテーマへの切り替えは 400 Bad Request
        let err = switch_active_theme(
            State(state.clone()),
            admin(),
            AuditContext::default(),
            Json(SwitchThemePayload {
                theme_id: "non-existent-theme-xyz".to_string(),
            }),
        )
        .await;

        assert!(err.is_err());
        let (status, _) = err.unwrap_err();
        assert_eq!(status, StatusCode::BAD_REQUEST);
    }
}
