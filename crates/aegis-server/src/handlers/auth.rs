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

use crate::state::AppState;

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

fn extract_session_id(headers: &HeaderMap) -> Option<String> {
    if let Some(auth_val) = headers.get("authorization") {
        if let Ok(auth_str) = auth_val.to_str() {
            if let Some(token) = auth_str.strip_prefix("Bearer ") {
                let trimmed = token.trim();
                if !trimmed.is_empty() {
                    return Some(trimmed.to_string());
                }
            }
        }
    }

    if let Some(header_val) = headers.get("x-session-id") {
        if let Ok(val_str) = header_val.to_str() {
            let trimmed = val_str.trim();
            if !trimmed.is_empty() {
                return Some(trimmed.to_string());
            }
        }
    }

    None
}

pub async fn create_session(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<CreateSessionRequest>,
) -> Result<impl IntoResponse, (StatusCode, Json<serde_json::Value>)> {
    let session = state
        .sessions
        .create_session(
            &payload.user_id,
            &payload.role,
            payload.metadata,
            payload.ttl_secs,
        )
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": format!("Failed to create session: {}", e) })),
            )
        })?;

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
    headers: HeaderMap,
) -> Result<impl IntoResponse, (StatusCode, Json<serde_json::Value>)> {
    let session_id = extract_session_id(&headers).ok_or_else(|| {
        (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "Missing session identifier" })),
        )
    })?;

    state
        .sessions
        .destroy_session(&session_id)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": format!("Failed to delete session: {}", e) })),
            )
        })?;

    Ok((StatusCode::OK, Json(json!({ "status": "logged_out" }))))
}
