//! MCP サーバー（`POST /mcp`）の結合テスト（実際のルーターを通す）

use std::sync::Arc;

use aegis_runtime::PluginSupervisor;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use serde_json::{json, Value};
use tower::ServiceExt;

use crate::config::AegisConfig;
use crate::state::AppState;

async fn state() -> Arc<AppState> {
    Arc::new(
        crate::test_support::test_app_state(
            AegisConfig::default(),
            PluginSupervisor::new(),
            Vec::new(),
        )
        .await,
    )
}

async fn session(state: &AppState, role: &str) -> String {
    state
        .sessions
        .create_session(&format!("{role}-user"), role, Default::default(), None)
        .await
        .unwrap()
        .id
}

async fn rpc(state: &Arc<AppState>, token: Option<&str>, message: Value) -> (StatusCode, Value) {
    let mut request = Request::builder()
        .method("POST")
        .uri("/mcp")
        .header("content-type", "application/json");
    if let Some(token) = token {
        request = request.header("authorization", format!("Bearer {token}"));
    }
    let response = crate::app::router(&state.config, state.clone())
        .oneshot(request.body(Body::from(message.to_string())).unwrap())
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

/// ツールを呼び、`(isError, 本文の JSON)` を返す
async fn call(state: &Arc<AppState>, token: &str, name: &str, args: Value) -> (bool, Value) {
    let (status, reply) = rpc(
        state,
        Some(token),
        json!({ "jsonrpc": "2.0", "id": 1, "method": "tools/call",
                "params": { "name": name, "arguments": args } }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{reply}");
    let result = &reply["result"];
    let text = result["content"][0]["text"].as_str().expect("text content");
    (
        result["isError"].as_bool().unwrap(),
        serde_json::from_str(text).unwrap(),
    )
}

#[tokio::test]
async fn test_requires_a_bearer_token() {
    let state = state().await;
    let (status, _) = rpc(
        &state,
        None,
        json!({ "jsonrpc": "2.0", "id": 1, "method": "ping" }),
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn test_initialize_and_tool_list() {
    let state = state().await;
    let editor = session(&state, "editor").await;

    let (_, reply) = rpc(
        &state,
        Some(&editor),
        json!({ "jsonrpc": "2.0", "id": 1, "method": "initialize",
                "params": { "protocolVersion": "2025-03-26" } }),
    )
    .await;
    assert_eq!(reply["result"]["protocolVersion"], "2025-03-26");
    assert_eq!(reply["result"]["serverInfo"]["name"], "aegiscms");
    // 知らない版には、こちらの最新で答える
    let (_, reply) = rpc(
        &state,
        Some(&editor),
        json!({ "jsonrpc": "2.0", "id": 2, "method": "initialize",
                "params": { "protocolVersion": "1999-01-01" } }),
    )
    .await;
    assert_eq!(reply["result"]["protocolVersion"], "2025-06-18");

    // 通知には本文を返さない
    let (status, _) = rpc(
        &state,
        Some(&editor),
        json!({ "jsonrpc": "2.0", "method": "notifications/initialized" }),
    )
    .await;
    assert_eq!(status, StatusCode::ACCEPTED);

    let (_, reply) = rpc(
        &state,
        Some(&editor),
        json!({ "jsonrpc": "2.0", "id": 3, "method": "tools/list" }),
    )
    .await;
    let names: Vec<&str> = reply["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["name"].as_str().unwrap())
        .collect();
    assert!(names.contains(&"create_draft") && names.contains(&"get_content"));
    // 公開・削除のツールは無い
    assert!(!names
        .iter()
        .any(|n| n.contains("publish") || n.contains("delete")));
}

#[tokio::test]
async fn test_create_draft_cannot_publish_and_can_be_read_back() {
    let state = state().await;
    let editor = session(&state, "editor").await;

    let (is_error, created) = call(
        &state,
        &editor,
        "create_draft",
        json!({ "content_type": "page", "title": "Via MCP", "slug": "via-mcp",
                // 余計な引数は無視され、公開にはならない
                "status": "published", "publish": true,
                "blocks": [{ "name": "core/paragraph", "attributes": {}, "text": "Hi", "children": [] }],
                "fields": { "layout": "blank" } }),
    )
    .await;
    assert!(!is_error, "{created}");
    assert_eq!(created["status"], "draft");

    let (_, found) = call(
        &state,
        &editor,
        "list_content",
        json!({ "content_type": "page", "q": "via-mcp" }),
    )
    .await;
    assert_eq!(found["contents"][0]["slug"], "via-mcp", "{found}");

    let (_, got) = call(
        &state,
        &editor,
        "get_content",
        json!({ "content_type": "page", "slug_or_id": "via-mcp" }),
    )
    .await;
    assert_eq!(got["fields"]["layout"], "blank");
    assert_eq!(got["blocks"][0]["text"], "Hi");

    // スラッグの重複は失敗として返る（ステータスは REST と同じ。409 になるのは別 PR の修正後）
    let (is_error, dup) = call(
        &state,
        &editor,
        "create_draft",
        json!({ "content_type": "page", "title": "Again", "slug": "via-mcp" }),
    )
    .await;
    assert!(is_error, "{dup}");
}

#[tokio::test]
async fn test_update_merges_fields_and_keeps_the_status() {
    let state = state().await;
    let editor = session(&state, "editor").await;
    // 公開済みのコンテンツを REST で用意する
    let response = crate::app::router(&state.config, state.clone())
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/contents/page")
                .header("authorization", format!("Bearer {editor}"))
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({ "title": "Live", "slug": "live", "status": "published", "publish": true,
                            "blocks": [{ "name": "core/paragraph", "attributes": {}, "text": "v1", "children": [] }],
                            "fields": { "layout": "blank", "custom_meta": { "a": 1 } } })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::CREATED);

    let (is_error, updated) = call(
        &state,
        &editor,
        "update_content",
        json!({ "content_type": "page", "slug_or_id": "live", "title": "Live 2",
                "fields": { "custom_meta": { "b": 2 } } }),
    )
    .await;
    assert!(!is_error, "{updated}");
    assert_eq!(updated["status"], "published");
    assert_eq!(updated["title"], "Live 2");
    // 送らなかった本文とフィールドは残り、送ったフィールドは差し替わる
    assert_eq!(updated["blocks"][0]["text"], "v1");
    assert_eq!(updated["fields"]["layout"], "blank");
    assert_eq!(updated["fields"]["custom_meta"], json!({ "b": 2 }));
}

#[tokio::test]
async fn test_roles_still_apply() {
    let state = state().await;
    let viewer = session(&state, "viewer").await;
    let (is_error, denied) = call(
        &state,
        &viewer,
        "create_draft",
        json!({ "content_type": "page", "title": "No", "slug": "no" }),
    )
    .await;
    assert!(is_error);
    assert_eq!(denied["status"], 403, "{denied}");

    let (is_error, missing) = call(
        &state,
        &viewer,
        "get_content",
        json!({ "content_type": "page", "slug_or_id": "nothing" }),
    )
    .await;
    assert!(is_error);
    assert_eq!(missing["status"], 404);
}
