use axum::{
    extract::State,
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
    Json,
};
use serde::Deserialize;
use serde_json::json;
use std::collections::HashMap;
use std::sync::Arc;

use crate::audit::AuditContext;
use crate::auth::local::PASSWORD_CREDENTIALS;
use crate::auth::{extract_session_id, session_fingerprint};
use crate::state::AppState;
use aegis_core::{AegisError, AuditAction, AuditActor, AuditOutcome, AuthCredentials};

#[derive(Debug, Deserialize)]
pub struct CreateSessionRequest {
    pub username: String,
    pub password: String,
    /// 有効期限（秒）。サーバーの既定値（`[cache].default_ttl_secs`）より長くはできない
    #[serde(default)]
    pub ttl_secs: Option<u64>,
}

/// `POST /api/v1/auth/session` — ユーザー名 / パスワードで認証し、セッションを発行する。
/// ロールはリクエストではなくユーザーアカウントから決まる
pub async fn create_session(
    State(state): State<Arc<AppState>>,
    audit: AuditContext,
    Json(payload): Json<CreateSessionRequest>,
) -> Result<impl IntoResponse, (StatusCode, Json<serde_json::Value>)> {
    let username = payload.username.trim().to_string();
    let credentials = AuthCredentials {
        kind: PASSWORD_CREDENTIALS.to_string(),
        identifier: Some(username.clone()),
        secret: Some(payload.password),
        ..Default::default()
    };

    let identity = match state.auth_provider.authenticate(&credentials).await {
        Ok(identity) => identity,
        Err(e) => {
            // 失敗理由（ユーザー不在・パスワード不一致・無効化）は監査ログにだけ残し、応答では区別しない
            let (status, message, reason) = match &e {
                AegisError::Authentication(reason) => (
                    StatusCode::UNAUTHORIZED,
                    "Invalid username or password".to_string(),
                    reason.clone(),
                ),
                other => (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    format!("Failed to authenticate: {}", other),
                    other.to_string(),
                ),
            };
            state
                .audit
                .record(
                    audit
                        .event(AuditAction::SessionCreate, AuditOutcome::Failure)
                        .with_resource("session", None)
                        .with_details(json!({
                            "username": username,
                            "provider": state.auth_provider.provider_id(),
                            "error": reason,
                        })),
                )
                .await;
            return Err((status, Json(json!({ "error": message }))));
        }
    };

    let actor = Some(AuditActor {
        user_id: identity.external_id.clone(),
        role: identity.role.clone(),
    });
    let mut metadata = HashMap::new();
    metadata.insert(
        "provider".to_string(),
        json!(state.auth_provider.provider_id()),
    );
    if let Some(display_name) = &identity.display_name {
        metadata.insert("display_name".to_string(), json!(display_name));
    }
    if !identity.email.is_empty() {
        metadata.insert("email".to_string(), json!(identity.email));
    }
    let max_ttl = state.config.cache.default_ttl_secs;
    let ttl_secs = payload.ttl_secs.map(|ttl| ttl.clamp(1, max_ttl));

    let result = state
        .sessions
        .create_session(&identity.external_id, &identity.role, metadata, ttl_secs)
        .await;

    let session = match result {
        Ok(session) => {
            state
                .audit
                .record(
                    audit
                        .event(AuditAction::SessionCreate, AuditOutcome::Success)
                        .with_actor(actor)
                        .with_resource("session", Some(session_fingerprint(&session.id)))
                        .with_details(json!({
                            "expires_at": session.expires_at,
                            "provider": state.auth_provider.provider_id(),
                        })),
                )
                .await;
            session
        }
        Err(e) => {
            state
                .audit
                .record(
                    audit
                        .event(AuditAction::SessionCreate, AuditOutcome::Failure)
                        .with_actor(actor)
                        .with_resource("session", None)
                        .with_details(json!({ "error": e.to_string() })),
                )
                .await;
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": format!("Failed to create session: {}", e) })),
            ));
        }
    };

    Ok((StatusCode::CREATED, Json(session)))
}

pub async fn get_current_session(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Result<impl IntoResponse, (StatusCode, Json<serde_json::Value>)> {
    let session_id = extract_session_id(&headers).ok_or_else(|| {
        (
            StatusCode::UNAUTHORIZED,
            Json(json!({ "error": "Missing Authorization Bearer token or X-Session-Id header" })),
        )
    })?;

    let session = state
        .sessions
        .get_session(&session_id)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": format!("Session query error: {}", e) })),
            )
        })?
        .ok_or_else(|| {
            (
                StatusCode::UNAUTHORIZED,
                Json(json!({ "error": "Invalid or expired session" })),
            )
        })?;

    Ok((StatusCode::OK, Json(session)))
}

pub async fn delete_session(
    State(state): State<Arc<AppState>>,
    audit: AuditContext,
    headers: HeaderMap,
) -> Result<impl IntoResponse, (StatusCode, Json<serde_json::Value>)> {
    let session_id = extract_session_id(&headers).ok_or_else(|| {
        (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "Missing session identifier" })),
        )
    })?;

    let result = state.sessions.destroy_session(&session_id).await;
    let outcome = if result.is_ok() {
        AuditOutcome::Success
    } else {
        AuditOutcome::Failure
    };
    let details = match &result {
        Ok(()) => json!({}),
        Err(e) => json!({ "error": e.to_string() }),
    };
    state
        .audit
        .record(
            audit
                .event(AuditAction::SessionDelete, outcome)
                .with_resource("session", Some(session_fingerprint(&session_id)))
                .with_details(details),
        )
        .await;

    result.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": format!("Failed to delete session: {}", e) })),
        )
    })?;

    Ok((StatusCode::OK, Json(json!({ "status": "logged_out" }))))
}
