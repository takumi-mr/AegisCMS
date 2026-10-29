use std::sync::Arc;

use axum::extract::State;
use axum::response::IntoResponse;
use axum::Json;

use crate::auth::ViewerSession;
use crate::state::AppState;
use aegis_schema::OpenApiGenerator;

/// Handler for `GET /api/v1/openapi.json`. The spec lists every schema and endpoint, so it
/// needs the same `viewer` role as `GET /api/v1/schemas`
pub async fn get_openapi_spec(
    State(state): State<Arc<AppState>>,
    _session: ViewerSession,
) -> impl IntoResponse {
    let schemas = state.schemas.read().await;
    let schemas_vec: Vec<_> = schemas.values().cloned().collect();
    let spec = OpenApiGenerator::generate_value(&schemas_vec);
    Json(spec)
}

#[cfg(test)]
mod tests {
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use tower::ServiceExt;

    #[tokio::test]
    async fn test_openapi_spec_requires_a_session() {
        let state = crate::test_support::test_state(aegis_runtime::PluginSupervisor::new()).await;
        let token = state
            .sessions
            .create_session("u1", "viewer", Default::default(), None)
            .await
            .unwrap()
            .id;
        let app = crate::app::router(&state.config.clone(), state);

        let res = app
            .clone()
            .oneshot(
                Request::get("/api/v1/openapi.json")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::UNAUTHORIZED);

        let res = app
            .oneshot(
                Request::get("/api/v1/openapi.json")
                    .header("authorization", format!("Bearer {}", token))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
    }
}
