use axum::extract::FromRequestParts;
use axum::http::request::Parts;
use axum::http::StatusCode;
use axum::Json;
use serde_json::json;
use std::sync::Arc;

use super::extract_session_id;
use crate::audit::AuditContext;
use crate::state::AppState;
pub use aegis_auth::{role_level, ADMIN, EDITOR, VIEWER};
use aegis_auth::{role_name, session_fingerprint, SessionData};
use aegis_core::{AuditAction, AuditOutcome};

/// 有効なセッションと、`MIN_LEVEL` 以上のロールを要求する extractor。
/// セッションが無い・無効なら 401、ロールが足りなければ 403 で拒否し、拒否を監査ログ
/// （`access.denied` / `denied`）に記録する
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

/// Why a request was turned away (`details.reason` of the `access.denied` event)
enum DenyReason {
    /// No `Authorization: Bearer` / `X-Session-Id` header
    MissingSession,
    /// The token does not match a live session (expired, logged out or forged)
    InvalidSession,
    /// A valid session whose role is below the endpoint's requirement
    InsufficientRole,
}

impl DenyReason {
    fn as_str(&self) -> &'static str {
        match self {
            Self::MissingSession => "missing_session",
            Self::InvalidSession => "invalid_session",
            Self::InsufficientRole => "insufficient_role",
        }
    }
}

/// Records an `access.denied` event. The resource is the endpoint (`METHOD /path`, without the
/// query string); the actor is the session's user for 403s. Tokens are never stored, only the
/// fingerprint of an invalid one.
async fn record_denied(
    parts: &mut Parts,
    state: &Arc<AppState>,
    reason: DenyReason,
    required_role: &str,
    session_id: Option<&str>,
) {
    let endpoint = format!("{} {}", parts.method, parts.uri.path());
    let Ok(audit) = AuditContext::from_request_parts(parts, state).await;
    let mut details = json!({
        "reason": reason.as_str(),
        "required_role": required_role,
    });
    if let (DenyReason::InvalidSession, Some(id)) = (&reason, session_id) {
        details["session"] = json!(session_fingerprint(id));
    }
    state
        .audit
        .record(
            audit
                .event(AuditAction::AccessDenied, AuditOutcome::Denied)
                .with_resource("endpoint", Some(endpoint))
                .with_details(details),
        )
        .await;
}

impl<const MIN_LEVEL: u8> FromRequestParts<Arc<AppState>> for RequireRole<MIN_LEVEL> {
    type Rejection = Rejection;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &Arc<AppState>,
    ) -> Result<Self, Self::Rejection> {
        let required_role = role_name(MIN_LEVEL);
        let Some(session_id) = extract_session_id(&parts.headers) else {
            record_denied(
                parts,
                state,
                DenyReason::MissingSession,
                required_role,
                None,
            )
            .await;
            return Err(reject(
                StatusCode::UNAUTHORIZED,
                "Missing Authorization Bearer token or X-Session-Id header",
            ));
        };
        let session = state.sessions.get_session(&session_id).await.map_err(|e| {
            reject(
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Session query error: {}", e),
            )
        })?;
        let Some(session) = session else {
            record_denied(
                parts,
                state,
                DenyReason::InvalidSession,
                required_role,
                Some(&session_id),
            )
            .await;
            return Err(reject(
                StatusCode::UNAUTHORIZED,
                "Invalid or expired session",
            ));
        };

        if role_level(&session.role) < MIN_LEVEL {
            record_denied(
                parts,
                state,
                DenyReason::InsufficientRole,
                required_role,
                None,
            )
            .await;
            return Err(reject(
                StatusCode::FORBIDDEN,
                format!(
                    "Role '{}' or higher is required (current role: '{}')",
                    required_role, session.role
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
        for role in aegis_auth::ROLES {
            assert!(role_level(role) >= VIEWER);
        }
    }

    async fn extract<const MIN_LEVEL: u8>(
        state: &Arc<AppState>,
        token: Option<&str>,
    ) -> Result<RequireRole<MIN_LEVEL>, StatusCode> {
        let mut request = axum::http::Request::builder()
            .method("POST")
            .uri("/api/v1/contents/article?draft=1");
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

        // Every rejection (and nothing else) is audited as access.denied, newest first
        let events = state
            .db
            .query_audit_events(&aegis_core::AuditQuery {
                limit: 50,
                ..Default::default()
            })
            .await
            .unwrap();
        assert_eq!(events.len(), 5);
        assert!(events
            .iter()
            .all(|e| e.action == AuditAction::AccessDenied && e.outcome == AuditOutcome::Denied));
        assert!(events
            .iter()
            .all(|e| e.resource_type.as_deref() == Some("endpoint")
                && e.resource_id.as_deref() == Some("POST /api/v1/contents/article")));

        let summary: Vec<(String, String, Option<String>)> = events
            .iter()
            .rev()
            .map(|e| {
                (
                    e.details["reason"].as_str().unwrap().to_string(),
                    e.details["required_role"].as_str().unwrap().to_string(),
                    e.actor.as_ref().map(|a| a.role.clone()),
                )
            })
            .collect();
        let row = |reason: &str, required: &str, actor: Option<&str>| {
            (
                reason.to_string(),
                required.to_string(),
                actor.map(str::to_string),
            )
        };
        assert_eq!(
            summary,
            vec![
                row("missing_session", "viewer", None),
                row("invalid_session", "viewer", None),
                row("insufficient_role", "editor", Some("viewer")),
                row("insufficient_role", "viewer", Some("superuser")),
                row("insufficient_role", "admin", Some("editor")),
            ]
        );
        // An invalid token is recorded only as its fingerprint
        let invalid = &events[events.len() - 2];
        assert_eq!(
            invalid.details["session"],
            json!(session_fingerprint("ses_missing"))
        );
        assert!(!serde_json::to_string(&events)
            .unwrap()
            .contains("ses_missing"));
    }
}
