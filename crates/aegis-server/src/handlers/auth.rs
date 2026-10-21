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
use crate::auth::{extract_session_id, session_fingerprint};
use crate::state::AppState;
use aegis_core::{AuditAction, AuditActor, AuditOutcome};

#[derive(Debug, Deserialize)]
pub struct CreateSessionRequest {
    pub user_id: String,
    #[serde(default = "default_role")]
    pub role: String,
    #[serde(default)]
    pub metadata: HashMap<String, serde_json::Value>,
    #[serde(default)]
    pub ttl_secs: Option<u64>,
}

fn default_role() -> String {
    "editor".to_string()
}

pub async fn create_session(
    State(state): State<Arc<AppState>>,
    audit: AuditContext,
    Json(payload): Json<CreateSessionRequest>,
) -> Result<impl IntoResponse, (StatusCode, Json<serde_json::Value>)> {
    let actor = Some(AuditActor {
        user_id: payload.user_id.clone(),
        role: payload.role.clone(),
    });
    let result = state
        .sessions
        .create_session(
            &payload.user_id,
            &payload.role,
            payload.metadata,
            payload.ttl_secs,
        )
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
                        .with_details(json!({ "expires_at": session.expires_at })),
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
