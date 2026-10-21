use std::sync::Arc;

use axum::extract::State;
use axum::response::IntoResponse;
use axum::Json;

use crate::state::AppState;
use aegis_schema::OpenApiGenerator;

/// Handler for `GET /api/v1/openapi.json`
pub async fn get_openapi_spec(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let schemas = state.schemas.read().await;
    let schemas_vec: Vec<_> = schemas.values().cloned().collect();
    let spec = OpenApiGenerator::generate_value(&schemas_vec);
    Json(spec)
}
