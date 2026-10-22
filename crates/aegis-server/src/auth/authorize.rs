use axum::extract::FromRequestParts;
use axum::http::request::Parts;
use axum::http::StatusCode;
use axum::Json;
use serde_json::json;
use std::sync::Arc;

use super::{extract_session_id, SessionData};
use crate::state::AppState;

/// ロールの強さ（`admin` ⊃ `editor` ⊃ `viewer`）。`auth::ROLES` 以外のロールは権限なし
pub fn role_level(role: &str) -> u8 {
    match role {
        "admin" => ADMIN,
        "editor" => EDITOR,
        "viewer" => VIEWER,
        _ => 0,
    }
}

pub const VIEWER: u8 = 1;
pub const EDITOR: u8 = 2;
pub const ADMIN: u8 = 3;

fn role_name(level: u8) -> &'static str {
    match level {
        ADMIN => "admin",
        EDITOR => "editor",
        _ => "viewer",
    }
}

/// 有効なセッションと、`MIN_LEVEL` 以上のロールを要求する extractor。
/// セッションが無い・無効なら 401、ロールが足りなければ 403 で拒否する
#[derive(Debug, Clone)]
pub struct RequireRole<const MIN_LEVEL: u8>(pub SessionData);

/// 読み取り系 API（`viewer` 以上）
pub type ViewerSession = RequireRole<VIEWER>;
/// コンテンツ作成・アップロード（`editor` 以上）
pub type EditorSession = RequireRole<EDITOR>;
/// 管理系 API（`admin`）
pub type AdminSession = RequireRole<ADMIN>;

type Rejection = (StatusCode, Json<serde_json::Value>);

fn reject(status: StatusCode, message: impl Into<String>) -> Rejection {
    (status, Json(json!({ "error": message.into() })))
}

impl<const MIN_LEVEL: u8> FromRequestParts<Arc<AppState>> for RequireRole<MIN_LEVEL> {
    type Rejection = Rejection;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &Arc<AppState>,
    ) -> Result<Self, Self::Rejection> {
        let session_id = extract_session_id(&parts.headers).ok_or_else(|| {
            reject(
                StatusCode::UNAUTHORIZED,
                "Missing Authorization Bearer token or X-Session-Id header",
            )
        })?;
        let session = state
            .sessions
            .get_session(&session_id)
            .await
            .map_err(|e| {
                reject(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    format!("Session query error: {}", e),
                )
            })?
            .ok_or_else(|| reject(StatusCode::UNAUTHORIZED, "Invalid or expired session"))?;

        if role_level(&session.role) < MIN_LEVEL {
            return Err(reject(
                StatusCode::FORBIDDEN,
                format!(
                    "Role '{}' or higher is required (current role: '{}')",
                    role_name(MIN_LEVEL),
                    session.role
                ),
            ));
        }
        Ok(Self(session))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_role_levels_are_ordered() {
        assert!(role_level("admin") > role_level("editor"));
        assert!(role_level("editor") > role_level("viewer"));
        assert_eq!(role_level("viewer"), VIEWER);
        assert_eq!(role_level("superuser"), 0);
        assert_eq!(role_level("Admin"), 0);
        for role in super::super::ROLES {
            assert!(role_level(role) >= VIEWER);
        }
    }

    async fn extract<const MIN_LEVEL: u8>(
        state: &Arc<AppState>,
        token: Option<&str>,
    ) -> Result<RequireRole<MIN_LEVEL>, StatusCode> {
        let mut request = axum::http::Request::builder();
        if let Some(token) = token {
            request = request.header("authorization", format!("Bearer {}", token));
        }
        let (mut parts, _) = request.body(()).unwrap().into_parts();
        RequireRole::<MIN_LEVEL>::from_request_parts(&mut parts, state)
            .await
            .map_err(|(status, _)| status)
    }

    #[tokio::test]
    async fn test_require_role_extractor() {
        let state = crate::test_support::test_state(aegis_runtime::PluginSupervisor::new()).await;
        let session = |role: &'static str| {
            let state = state.clone();
            async move {
                state
                    .sessions
                    .create_session("user1", role, Default::default(), None)
                    .await
                    .unwrap()
                    .id
            }
        };
        let viewer = session("viewer").await;
        let editor = session("editor").await;
        let unknown = session("superuser").await;

        assert_eq!(
            extract::<VIEWER>(&state, None).await.unwrap_err(),
            StatusCode::UNAUTHORIZED
        );
        assert_eq!(
            extract::<VIEWER>(&state, Some("ses_missing"))
                .await
                .unwrap_err(),
            StatusCode::UNAUTHORIZED
        );
        assert_eq!(
            extract::<EDITOR>(&state, Some(&viewer)).await.unwrap_err(),
            StatusCode::FORBIDDEN
        );
        assert_eq!(
            extract::<VIEWER>(&state, Some(&unknown)).await.unwrap_err(),
            StatusCode::FORBIDDEN
        );
        let RequireRole(granted) = extract::<EDITOR>(&state, Some(&editor)).await.unwrap();
        assert_eq!(granted.role, "editor");
        assert!(extract::<VIEWER>(&state, Some(&editor)).await.is_ok());
        assert_eq!(
            extract::<ADMIN>(&state, Some(&editor)).await.unwrap_err(),
            StatusCode::FORBIDDEN
        );
    }
}
