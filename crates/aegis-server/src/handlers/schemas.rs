use crate::state::AppState;
use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use serde_json::json;
use std::sync::Arc;

pub async fn list_schemas(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let schemas = state.schemas.read().await;
    let list: Vec<_> = schemas.values().cloned().collect();
    Json(json!({
        "schemas": list,
        "total": list.len()
    }))
}

pub async fn get_schema(
    State(state): State<Arc<AppState>>,
    Path(content_type): Path<String>,
) -> Result<impl IntoResponse, (StatusCode, Json<serde_json::Value>)> {
    let schemas = state.schemas.read().await;
    match schemas.get(&content_type) {
        Some(schema) => Ok(Json(schema.clone())),
        None => Err((
            StatusCode::NOT_FOUND,
            Json(json!({
                "error": format!("Schema '{}' not found", content_type)
            })),
        )),
    }
}
