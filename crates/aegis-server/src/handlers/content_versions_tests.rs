//! 版の内容を返す API（`GET .../versions/{revision}`）の結合テスト（実際のルーターを通す）

use std::sync::Arc;

use aegis_runtime::PluginSupervisor;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use serde_json::{json, Value};
use tower::ServiceExt;

use crate::config::AegisConfig;
use crate::state::AppState;

async fn call(
    state: &Arc<AppState>,
    method: &str,
    path: &str,
    body: Option<Value>,
    token: &str,
) -> (StatusCode, Value) {
    let request = Request::builder()
        .method(method)
        .uri(path)
        .header("authorization", format!("Bearer {}", token));
    let request = match body {
        Some(body) => request
            .header("content-type", "application/json")
            .body(Body::from(body.to_string())),
        None => request.body(Body::empty()),
    }
    .unwrap();
    let response = crate::app::router(&state.config, state.clone())
        .oneshot(request)
        .await
        .unwrap();
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    )
}

fn paragraph(text: &str) -> Value {
    json!([{ "name": "core/paragraph", "text": text }])
}

#[tokio::test]
async fn test_a_version_returns_its_own_content_not_the_current_one() {
    let state = Arc::new(
        crate::test_support::test_app_state(
            AegisConfig::default(),
            PluginSupervisor::new(),
            Vec::new(),
        )
        .await,
    );
    let token = state
        .sessions
        .create_session("editor1", "editor", Default::default(), None)
        .await
        .unwrap()
        .id;

    let (status, body) = call(
        &state,
        "POST",
        "/api/v1/contents/article",
        Some(json!({ "title": "First", "slug": "first", "blocks": paragraph("old body") })),
        &token,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{}", body);
    let (status, body) = call(
        &state,
        "PUT",
        "/api/v1/contents/article/first",
        Some(json!({ "title": "Second", "slug": "first", "blocks": paragraph("new body") })),
        &token,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{}", body);

    let (_, versions) = call(
        &state,
        "GET",
        "/api/v1/contents/article/first/versions",
        None,
        &token,
    )
    .await;
    let oldest = versions
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v["revision"].as_i64().unwrap())
        .min()
        .unwrap();

    let (status, old) = call(
        &state,
        "GET",
        &format!("/api/v1/contents/article/first/versions/{}", oldest),
        None,
        &token,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{}", old);
    assert_eq!(old["title"], "First");
    assert_eq!(old["blocks"][0]["text"], "old body");
    // 状態や公開日時など、復元で戻らないものは返さない
    assert!(old.get("status").is_none());

    let (status, _) = call(
        &state,
        "GET",
        "/api/v1/contents/article/first/versions/999",
        None,
        &token,
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (status, _) = call(
        &state,
        "GET",
        &format!("/api/v1/contents/article/first/versions/{}", oldest),
        None,
        "not-a-session",
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}
