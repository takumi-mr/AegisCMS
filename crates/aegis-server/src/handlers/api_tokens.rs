//! API トークンの管理 API（`/api/v1/users/me/tokens`。docs/authentication.md 13）。
//! LLM・スクリプト・CI にパスワードの代わりに渡すトークンを、ログイン中のユーザーが自分用に発行・
//! 一覧・失効する。トークン自身では管理できない（漏れたトークンで別のトークンを増やせないように）

use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::sync::Arc;

use crate::audit::AuditContext;
use crate::auth::authorize::RequireRole;
use crate::auth::ViewerSession;
use crate::state::AppState;
use aegis_auth::api_tokens::{self, NewApiToken};
use aegis_auth::SessionData;
use aegis_core::{AegisError, AuditAction, AuditOutcome};
use aegis_db::ApiTokenRecord;

type ApiError = (StatusCode, Json<serde_json::Value>);

fn api_error(status: StatusCode, message: impl Into<String>) -> ApiError {
    (status, Json(json!({ "error": message.into() })))
}

fn from_aegis(e: AegisError) -> ApiError {
    match e {
        AegisError::Validation(m) => api_error(StatusCode::BAD_REQUEST, m),
        AegisError::NotFound(m) => api_error(StatusCode::NOT_FOUND, m),
        other => api_error(StatusCode::INTERNAL_SERVER_ERROR, other.to_string()),
    }
}

/// API トークンで認証したリクエストを、人間のセッションに限る操作から断る
pub(crate) fn require_login_session(session: &SessionData) -> Result<(), ApiError> {
    if api_tokens::is_api_token_session(session) {
        return Err(api_error(
            StatusCode::FORBIDDEN,
            "This operation requires a login session, not an API token",
        ));
    }
    Ok(())
}

/// API が返すトークン（ハッシュは含めない）
#[derive(Debug, Serialize)]
pub struct ApiTokenView {
    pub id: String,
    pub name: String,
    /// トークンの先頭（一覧で見分けるため）
    pub prefix: String,
    /// 許すロールの上限
    pub role: String,
    pub created_at: String,
    pub expires_at: Option<String>,
    pub last_used_at: Option<String>,
}

impl From<ApiTokenRecord> for ApiTokenView {
    fn from(t: ApiTokenRecord) -> Self {
        Self {
            id: t.id,
            name: t.name,
            prefix: t.token_prefix,
            role: t.role,
            created_at: t.created_at,
            expires_at: t.expires_at,
            last_used_at: t.last_used_at,
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct CreateApiTokenRequest {
    pub name: String,
    /// 許すロールの上限（`viewer` / `contributor` / `reviewer` / `editor`。既定 `viewer`）
    #[serde(default = "default_role")]
    pub role: String,
    /// 有効日数（既定 90）。`null` は無期限
    #[serde(default = "default_days")]
    pub expires_in_days: Option<u32>,
}

fn default_role() -> String {
    "viewer".to_string()
}

fn default_days() -> Option<u32> {
    Some(90)
}

#[derive(Debug, Serialize)]
pub struct CreatedApiToken {
    #[serde(flatten)]
    pub token_info: ApiTokenView,
    /// トークン本体。この応答でしか返さない
    pub token: String,
}

/// `GET /api/v1/users/me/tokens` — 自分のトークン（新しい順）
pub async fn list_api_tokens(
    State(state): State<Arc<AppState>>,
    RequireRole(session): ViewerSession,
) -> Result<Json<serde_json::Value>, ApiError> {
    require_login_session(&session)?;
    let tokens = state
        .db
        .list_api_tokens(&session.user_id)
        .await
        .map_err(from_aegis)?;
    let views: Vec<ApiTokenView> = tokens.into_iter().map(Into::into).collect();
    Ok(Json(json!({ "tokens": views })))
}

/// `POST /api/v1/users/me/tokens` — 発行する（`201`。本文の `token` は 1 度しか返さない）
pub async fn create_api_token(
    State(state): State<Arc<AppState>>,
    RequireRole(session): ViewerSession,
    audit: AuditContext,
    Json(payload): Json<CreateApiTokenRequest>,
) -> Result<(StatusCode, Json<CreatedApiToken>), ApiError> {
    require_login_session(&session)?;
    let result = api_tokens::issue_api_token(
        &*state.db,
        NewApiToken {
            username: session.user_id.clone(),
            name: payload.name,
            role: payload.role,
            expires_in_days: payload.expires_in_days,
        },
    )
    .await;
    match result {
        Ok(issued) => {
            state
                .audit
                .record(
                    audit
                        .event(AuditAction::UserUpdate, AuditOutcome::Success)
                        .with_resource("user", Some(session.user_id.clone()))
                        .with_details(json!({
                            "action": "api_token.create",
                            "token_id": issued.record.id,
                            "name": issued.record.name,
                            "role": issued.record.role,
                            "expires_at": issued.record.expires_at,
                        })),
                )
                .await;
            Ok((
                StatusCode::CREATED,
                Json(CreatedApiToken {
                    token: issued.token,
                    token_info: issued.record.into(),
                }),
            ))
        }
        Err(e) => {
            state
                .audit
                .record(
                    audit
                        .event(AuditAction::UserUpdate, AuditOutcome::Failure)
                        .with_resource("user", Some(session.user_id.clone()))
                        .with_details(json!({
                            "action": "api_token.create",
                            "error": e.to_string(),
                        })),
                )
                .await;
            Err(from_aegis(e))
        }
    }
}

/// `DELETE /api/v1/users/me/tokens/{id}` — 失効する（`204`）
pub async fn revoke_api_token(
    State(state): State<Arc<AppState>>,
    RequireRole(session): ViewerSession,
    audit: AuditContext,
    Path(id): Path<String>,
) -> Result<StatusCode, ApiError> {
    require_login_session(&session)?;
    let deleted = state
        .db
        .delete_api_token(&session.user_id, &id)
        .await
        .map_err(from_aegis)?;
    if !deleted {
        return Err(api_error(StatusCode::NOT_FOUND, "API token not found"));
    }
    state
        .audit
        .record(
            audit
                .event(AuditAction::UserUpdate, AuditOutcome::Success)
                .with_resource("user", Some(session.user_id.clone()))
                .with_details(json!({ "action": "api_token.revoke", "token_id": id })),
        )
        .await;
    Ok(StatusCode::NO_CONTENT)
}

#[cfg(test)]
mod tests {
    use super::*;
    use aegis_auth::users::{create_user, NewUser};
    use tower::ServiceExt;

    async fn state() -> Arc<AppState> {
        let state = crate::test_support::test_state(aegis_runtime::PluginSupervisor::new()).await;
        for (username, role) in [("root", "admin"), ("alice", "editor")] {
            create_user(
                state.db.as_ref(),
                NewUser {
                    username: username.to_string(),
                    password: "initial-pass".to_string(),
                    role: role.to_string(),
                    email: None,
                    display_name: None,
                },
            )
            .await
            .unwrap();
        }
        state
    }

    async fn call(
        state: &Arc<AppState>,
        method: &str,
        path: &str,
        token: &str,
        body: Option<serde_json::Value>,
    ) -> (StatusCode, serde_json::Value) {
        let mut request = axum::http::Request::builder()
            .method(method)
            .uri(path)
            .header("authorization", format!("Bearer {}", token));
        let body = match body {
            Some(b) => {
                request = request.header("content-type", "application/json");
                axum::body::Body::from(b.to_string())
            }
            None => axum::body::Body::empty(),
        };
        let response = crate::app::router(&state.config, state.clone())
            .oneshot(request.body(body).unwrap())
            .await
            .unwrap();
        let status = response.status();
        let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        (status, serde_json::from_slice(&bytes).unwrap_or_default())
    }

    #[tokio::test]
    async fn test_issue_use_and_revoke_through_the_router() {
        let state = state().await;
        let login = state
            .sessions
            .create_session("alice", "editor", Default::default(), None)
            .await
            .unwrap();

        // The login session issues a viewer-capped token
        let (status, created) = call(
            &state,
            "POST",
            "/api/v1/users/me/tokens",
            &login.id,
            Some(json!({ "name": "claude-code", "role": "viewer" })),
        )
        .await;
        assert_eq!(status, StatusCode::CREATED, "{created}");
        let token = created["token"].as_str().unwrap().to_string();
        let id = created["id"].as_str().unwrap().to_string();
        assert!(token.starts_with("aegis_pat_"));
        assert_eq!(created["role"], "viewer");
        assert!(created["expires_at"].is_string());

        // The list never contains the token or its hash
        let (status, listed) =
            call(&state, "GET", "/api/v1/users/me/tokens", &login.id, None).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(listed["tokens"].as_array().unwrap().len(), 1);
        assert!(!listed.to_string().contains(&token));
        assert!(!listed.to_string().contains("token_hash"));

        // The token authenticates as alice, capped at viewer (editor-only endpoints are refused)
        let (status, me) = call(&state, "GET", "/api/v1/auth/session", &token, None).await;
        assert_eq!(status, StatusCode::OK, "{me}");
        assert_eq!(me["user_id"], "alice");
        assert_eq!(me["role"], "viewer");
        assert_eq!(me["metadata"]["provider"], "api_token");
        assert!(!me.to_string().contains(&token));
        let (status, _) = call(&state, "GET", "/api/v1/schemas", &token, None).await;
        assert_eq!(status, StatusCode::OK);
        let (status, _) = call(&state, "GET", "/api/v1/orders", &token, None).await;
        assert_eq!(status, StatusCode::FORBIDDEN);

        // A token cannot manage tokens, change a password or set up 2FA
        let (status, _) = call(&state, "GET", "/api/v1/users/me/tokens", &token, None).await;
        assert_eq!(status, StatusCode::FORBIDDEN);
        let (status, _) = call(
            &state,
            "POST",
            "/api/v1/users/me/tokens",
            &token,
            Some(json!({ "name": "more" })),
        )
        .await;
        assert_eq!(status, StatusCode::FORBIDDEN);
        let (status, _) = call(
            &state,
            "PATCH",
            "/api/v1/users/alice",
            &token,
            Some(json!({ "password": "new-password-1", "current_password": "initial-pass" })),
        )
        .await;
        assert_eq!(status, StatusCode::FORBIDDEN);
        let (status, _) = call(
            &state,
            "POST",
            "/api/v1/users/me/2fa/setup",
            &token,
            Some(json!({})),
        )
        .await;
        assert_eq!(status, StatusCode::FORBIDDEN);

        // A role above the owner's, and an admin cap, are refused
        let (status, _) = call(
            &state,
            "POST",
            "/api/v1/users/me/tokens",
            &login.id,
            Some(json!({ "name": "x", "role": "admin" })),
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST);

        // Revoking (only the owner can) makes the token stop working at once
        let root = state
            .sessions
            .create_session("root", "admin", Default::default(), None)
            .await
            .unwrap();
        let (status, _) = call(
            &state,
            "DELETE",
            &format!("/api/v1/users/me/tokens/{id}"),
            &root.id,
            None,
        )
        .await;
        assert_eq!(status, StatusCode::NOT_FOUND);
        let (status, _) = call(
            &state,
            "DELETE",
            &format!("/api/v1/users/me/tokens/{id}"),
            &login.id,
            None,
        )
        .await;
        assert_eq!(status, StatusCode::NO_CONTENT);
        let (status, _) = call(&state, "GET", "/api/v1/auth/session", &token, None).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED);

        // Audit log records the lifecycle without the token itself
        let events = state
            .db
            .query_audit_events(&aegis_core::AuditQuery {
                limit: 50,
                ..Default::default()
            })
            .await
            .unwrap();
        let dump = serde_json::to_string(&events).unwrap();
        assert!(dump.contains("api_token.create"));
        assert!(dump.contains("api_token.revoke"));
        assert!(!dump.contains(&token));
    }

    #[tokio::test]
    async fn test_disabling_the_owner_stops_the_token() {
        let state = state().await;
        let issued = api_tokens::issue_api_token(
            state.db.as_ref(),
            NewApiToken {
                username: "alice".into(),
                name: "ci".into(),
                role: "editor".into(),
                expires_in_days: None,
            },
        )
        .await
        .unwrap();
        let (status, _) = call(&state, "GET", "/api/v1/auth/session", &issued.token, None).await;
        assert_eq!(status, StatusCode::OK);
        aegis_auth::users::set_disabled(state.db.as_ref(), "alice", true)
            .await
            .unwrap();
        let (status, _) = call(&state, "GET", "/api/v1/auth/session", &issued.token, None).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED);
    }
}
