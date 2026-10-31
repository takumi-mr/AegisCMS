//! リンク切れ検査の管理 API。
//!
//! - `GET /api/v1/broken-links` — 検査状態とレポート一覧を取得
//! - `POST /api/v1/broken-links/scan` — リンク切れ検査を開始

use std::sync::Arc;

use axum::extract::State;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde_json::json;

use crate::auth::authorize::{AdminSession, RequireRole};
use crate::broken_links::BrokenLinksStatusResponse;
use crate::state::AppState;

/// `GET /api/v1/broken-links` — 検査状態とレポートを取得（管理者のみ）
pub async fn api_get_broken_links(
    State(state): State<Arc<AppState>>,
    RequireRole(_session): AdminSession,
) -> Response {
    let report = state.broken_links.get_report(&state).await;
    let is_scanning = state.broken_links.is_scanning();
    (
        StatusCode::OK,
        Json(BrokenLinksStatusResponse {
            is_scanning,
            report,
        }),
    )
        .into_response()
}

/// `POST /api/v1/broken-links/scan` — リンク切れ検査を開始（管理者のみ）
pub async fn api_trigger_scan(
    State(state): State<Arc<AppState>>,
    RequireRole(_session): AdminSession,
) -> Response {
    if state.broken_links.start_scan_task(state.clone()) {
        (
            StatusCode::ACCEPTED,
            Json(json!({ "message": "Scan started" })),
        )
            .into_response()
    } else {
        (
            StatusCode::CONFLICT,
            Json(json!({ "error": "A scan is already in progress" })),
        )
            .into_response()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use aegis_runtime::PluginSupervisor;
    use axum::body::Body;
    use axum::http::Request;
    use tower::ServiceExt;

    #[tokio::test]
    async fn test_broken_links_api_auth_and_response() {
        let config = crate::config::AegisConfig::default();
        let state = Arc::new(
            crate::test_support::test_app_state(
                config.clone(),
                PluginSupervisor::new(),
                Vec::new(),
            )
            .await,
        );
        let app = crate::app::router(&config, state.clone());

        // 1. Without auth: GET /api/v1/broken-links -> 401
        let req = Request::builder()
            .uri("/api/v1/broken-links")
            .method("GET")
            .body(Body::empty())
            .unwrap();
        let res = app.clone().oneshot(req).await.unwrap();
        assert_eq!(res.status(), StatusCode::UNAUTHORIZED);

        // 2. With editor auth: GET /api/v1/broken-links -> 403
        let editor_session = state
            .sessions
            .create_session("editor1", "editor", Default::default(), None)
            .await
            .unwrap();
        let req = Request::builder()
            .uri("/api/v1/broken-links")
            .method("GET")
            .header("Authorization", format!("Bearer {}", editor_session.id))
            .body(Body::empty())
            .unwrap();
        let res = app.clone().oneshot(req).await.unwrap();
        assert_eq!(res.status(), StatusCode::FORBIDDEN);

        // 3. With admin auth: GET /api/v1/broken-links -> 200
        let admin_session = state
            .sessions
            .create_session("admin1", "admin", Default::default(), None)
            .await
            .unwrap();
        let req = Request::builder()
            .uri("/api/v1/broken-links")
            .method("GET")
            .header("Authorization", format!("Bearer {}", admin_session.id))
            .body(Body::empty())
            .unwrap();
        let res = app.clone().oneshot(req).await.unwrap();
        assert_eq!(res.status(), StatusCode::OK);

        // 4. Trigger scan: POST /api/v1/broken-links/scan -> 202
        let req = Request::builder()
            .uri("/api/v1/broken-links/scan")
            .method("POST")
            .header("Authorization", format!("Bearer {}", admin_session.id))
            .body(Body::empty())
            .unwrap();
        let res = app.clone().oneshot(req).await.unwrap();
        assert_eq!(res.status(), StatusCode::ACCEPTED);
    }
}
