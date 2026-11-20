//! MCP（Model Context Protocol）サーバー。`POST /mcp` で Streamable HTTP（JSON 応答のみ・状態なし）を話す。
//!
//! 認証と権限は REST API と同じ。`Authorization: Bearer` に API トークン（`aegis_pat_...`）かセッション ID を付ける。
//! ツールは中で REST API のルーターを呼ぶだけで、承認フロー・検証・監査ログ・ロールの確認は REST と同じ経路を通る。
//! ツールは 3 段に分かれる: 読み取り（viewer）、下書きの作成・更新・レビュー依頼・タグ・メディア（contributor）、
//! 公開・非公開・削除（editor）。`tools/list` は呼び出した人のロールで使えるものだけ返し、呼ばれたときの権限の確認は
//! REST API が行う。

use std::sync::Arc;

use axum::{
    body::Body,
    extract::{Extension, State},
    http::{header, HeaderMap, Method, Request, StatusCode},
    response::{IntoResponse, Response},
    Json, Router,
};
use serde_json::{json, Map, Value};
use tower::ServiceExt;

use crate::state::AppState;

/// 実装している MCP のバージョン（新しい順）。クライアントが求めたものがこの中にあればそれで応じる
const SUPPORTED_VERSIONS: [&str; 3] = ["2025-06-18", "2025-03-26", "2024-11-05"];
/// 内部の API 応答を読む上限
const MAX_API_RESPONSE_BYTES: usize = 16 * 1024 * 1024;
/// `list_content` の件数の既定と上限
const DEFAULT_LIST_LIMIT: u64 = 20;
const MAX_LIST_LIMIT: u64 = 100;

const INSTRUCTIONS: &str = "AegisCMS content tools. What you can see and do follows the role of the token. \
create_draft and update_content only save drafts or working copies: visitors see nothing new until the content is published. \
publish_content, unpublish_content and delete_content change what visitors see, and delete_content cannot be undone, \
so use them only when the user clearly asked for it. \
The comment, review, order, sales and member tools reach customers' personal information and public replies: \
use them only for what the user asked. \
Body text is a list of blocks (see the content API guide); use get_content_schema for the custom fields of a type.";

/// 使うのに要る一番低いロール。ここに無いツールは読み取り（viewer）
fn tool_min_role(name: &str) -> &'static str {
    match name {
        "create_draft" | "update_content" | "request_review" | "set_tags" | "upload_media" => {
            "contributor"
        }
        "publish_content" | "unpublish_content" | "delete_content" | "list_comments"
        | "moderate_comment" | "reply_to_comment" | "list_orders" | "get_order"
        | "sales_report" | "list_reviews" | "moderate_review" | "reply_to_review"
        | "list_members" | "get_member" => "editor",
        _ => "viewer",
    }
}

/// `level` のロールで使えるツールだけ（`None` なら全部）
fn visible_tools(level: Option<u8>) -> Value {
    let all = tool_definitions();
    let Some(level) = level else {
        return all;
    };
    Value::Array(
        all.as_array()
            .into_iter()
            .flatten()
            .filter(|tool| {
                let name = tool["name"].as_str().unwrap_or_default();
                aegis_auth::role_level(tool_min_role(name)) <= level
            })
            .cloned()
            .collect(),
    )
}

/// `/mcp` の本体。`api` は REST API のルーター（`/mcp` 自身は含まない）
pub async fn handle(
    State(state): State<Arc<AppState>>,
    Extension(router): Extension<Router>,
    headers: HeaderMap,
    Json(message): Json<Value>,
) -> Response {
    let Some(auth) = headers
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .filter(|v| v.len() > 7 && v[..7].eq_ignore_ascii_case("bearer "))
    else {
        return unauthorized(&state, &headers);
    };

    if message.is_array() {
        return rpc_error(Value::Null, -32600, "Batch requests are not supported");
    }
    let id = message.get("id").cloned();
    let method = message.get("method").and_then(Value::as_str).unwrap_or("");
    // 通知（id なし）と、クライアントからの応答には本文を返さない
    let Some(id) = id else {
        return StatusCode::ACCEPTED.into_response();
    };
    if method.is_empty() {
        return StatusCode::ACCEPTED.into_response();
    }
    let params = message.get("params").cloned().unwrap_or(Value::Null);

    match method {
        "initialize" => {
            let requested = params
                .get("protocolVersion")
                .and_then(Value::as_str)
                .unwrap_or("");
            let version = SUPPORTED_VERSIONS
                .iter()
                .find(|v| **v == requested)
                .unwrap_or(&SUPPORTED_VERSIONS[0]);
            rpc_result(
                id,
                json!({
                    "protocolVersion": version,
                    "capabilities": { "tools": { "listChanged": false } },
                    "serverInfo": { "name": "aegiscms", "version": env!("CARGO_PKG_VERSION") },
                    "instructions": INSTRUCTIONS,
                }),
            )
        }
        "ping" => rpc_result(id, json!({})),
        "tools/list" => {
            let (status, me) = api(&router, auth, Method::GET, "/api/v1/auth/session", None).await;
            if status == StatusCode::UNAUTHORIZED {
                return unauthorized(&state, &headers);
            }
            let level = status
                .is_success()
                .then(|| me["role"].as_str().map(aegis_auth::role_level))
                .flatten();
            rpc_result(id, json!({ "tools": visible_tools(level) }))
        }
        "tools/call" => {
            let name = params.get("name").and_then(Value::as_str).unwrap_or("");
            let args = params
                .get("arguments")
                .cloned()
                .unwrap_or_else(|| json!({}));
            if !args.is_object() {
                return rpc_error(id, -32602, "arguments must be an object");
            }
            match call_tool(&router, auth, name, &args).await {
                Outcome::Unauthorized => unauthorized(&state, &headers),
                Outcome::UnknownTool => rpc_error(id, -32602, &format!("Unknown tool: {name}")),
                Outcome::Done(text) => rpc_result(
                    id,
                    json!({ "content": [{ "type": "text", "text": text }], "isError": false }),
                ),
                Outcome::Failed(text) => rpc_result(
                    id,
                    json!({ "content": [{ "type": "text", "text": text }], "isError": true }),
                ),
            }
        }
        _ => rpc_error(id, -32601, &format!("Method not found: {method}")),
    }
}

/// `GET /mcp`（サーバーからの通知用ストリーム）は提供しない
pub async fn not_supported() -> Response {
    let mut response = StatusCode::METHOD_NOT_ALLOWED.into_response();
    response
        .headers_mut()
        .insert(header::ALLOW, "POST".parse().unwrap());
    response
}

/// 401。OAuth で始められるクライアントには、`WWW-Authenticate` で認可サーバーの案内（RFC 9728）を返す
fn unauthorized(state: &AppState, headers: &HeaderMap) -> Response {
    (
        StatusCode::UNAUTHORIZED,
        [(
            header::WWW_AUTHENTICATE,
            crate::oauth::www_authenticate(state, headers),
        )],
        Json(json!({ "error": "Send an API token as 'Authorization: Bearer <token>'" })),
    )
        .into_response()
}

fn rpc_result(id: Value, result: Value) -> Response {
    Json(json!({ "jsonrpc": "2.0", "id": id, "result": result })).into_response()
}

fn rpc_error(id: Value, code: i32, message: &str) -> Response {
    Json(json!({ "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": message } }))
        .into_response()
}

enum Outcome {
    Done(String),
    Failed(String),
    Unauthorized,
    UnknownTool,
}

fn tool_definitions() -> Value {
    let content_type = json!({ "type": "string", "description": "Content type, e.g. \"article\" or \"page\" (see list_content_types)" });
    let blocks = json!({ "type": "array", "items": { "type": "object" },
        "description": "Body blocks, e.g. [{\"name\":\"core/paragraph\",\"attributes\":{},\"text\":\"Hello\",\"children\":[]}]. See the content API guide for the block types." });
    let fields = json!({ "type": "object", "description": "Custom fields of the type (see get_content_schema)" });
    json!([
        {
            "name": "list_content_types",
            "title": "List content types",
            "description": "List the content types of the site (articles, pages, ...).",
            "inputSchema": { "type": "object", "properties": {} },
            "annotations": { "readOnlyHint": true }
        },
        {
            "name": "get_content_schema",
            "title": "Get a content type's schema",
            "description": "Get the fields of one content type.",
            "inputSchema": { "type": "object", "properties": { "content_type": content_type }, "required": ["content_type"] },
            "annotations": { "readOnlyHint": true }
        },
        {
            "name": "list_content",
            "title": "List or search content",
            "description": "List items of a content type, newest first. Returns summaries without the body; use get_content for the body.",
            "inputSchema": { "type": "object", "properties": {
                "content_type": content_type,
                "q": { "type": "string", "description": "Case-insensitive search in title, slug and excerpt" },
                "status": { "type": "string", "enum": ["draft", "pending", "scheduled", "published", "archived"] },
                "tag": { "type": "string", "description": "Tag slug or id" },
                "category": { "type": "string", "description": "Category id, or \"none\"" },
                "limit": { "type": "integer", "minimum": 1, "maximum": MAX_LIST_LIMIT, "description": "Default 20" },
                "offset": { "type": "integer", "minimum": 0 }
            }, "required": ["content_type"] },
            "annotations": { "readOnlyHint": true }
        },
        {
            "name": "get_content",
            "title": "Get one item",
            "description": "Get one item with its body blocks and custom fields, by slug or id.",
            "inputSchema": { "type": "object", "properties": {
                "content_type": content_type,
                "slug_or_id": { "type": "string" }
            }, "required": ["content_type", "slug_or_id"] },
            "annotations": { "readOnlyHint": true }
        },
        {
            "name": "list_tags",
            "title": "List tags",
            "description": "List the tags of the site.",
            "inputSchema": { "type": "object", "properties": {} },
            "annotations": { "readOnlyHint": true }
        },
        {
            "name": "create_draft",
            "title": "Create a draft",
            "description": "Create a new item as a draft. It is not visible to visitors until a person publishes it. Fails with 409 if the slug is already used.",
            "inputSchema": { "type": "object", "properties": {
                "content_type": content_type,
                "title": { "type": "string" },
                "slug": { "type": "string", "description": "URL part: letters, digits and hyphens. Must be unique within the type" },
                "blocks": blocks,
                "fields": fields
            }, "required": ["content_type", "title", "slug"] },
            "annotations": { "readOnlyHint": false, "destructiveHint": false, "idempotentHint": false }
        },
        {
            "name": "update_content",
            "title": "Update an item (draft or working copy)",
            "description": "Change an existing item. Send only what changes: title, slug, blocks (replaces the whole body) and fields (merged into the current custom fields). \
    The status is kept. For published content the change is saved as a working copy and visitors keep seeing the published version until a person publishes it.",
            "inputSchema": { "type": "object", "properties": {
                "content_type": content_type,
                "slug_or_id": { "type": "string" },
                "title": { "type": "string" },
                "slug": { "type": "string" },
                "blocks": blocks,
                "fields": fields
            }, "required": ["content_type", "slug_or_id"] },
            "annotations": { "readOnlyHint": false, "destructiveHint": false, "idempotentHint": true }
        },
        {
            "name": "request_review",
            "title": "Send an item to review",
            "description": "Set the status to \"pending\" so that an editor or reviewer can approve it. Nothing is published.",
            "inputSchema": { "type": "object", "properties": {
                "content_type": content_type,
                "slug_or_id": { "type": "string" },
                "review_note": { "type": "string", "description": "A note for the reviewer" }
            }, "required": ["content_type", "slug_or_id"] },
            "annotations": { "readOnlyHint": false, "destructiveHint": false, "idempotentHint": true }
        },
        {
            "name": "set_tags",
            "title": "Set the tags of an item",
            "description": "Replace the tags of an item. Tags must already exist (see list_tags). An empty list removes all tags.",
            "inputSchema": { "type": "object", "properties": {
                "content_type": content_type,
                "slug_or_id": { "type": "string" },
                "tags": { "type": "array", "items": { "type": "string" }, "description": "Tag slugs or ids" }
            }, "required": ["content_type", "slug_or_id", "tags"] },
            "annotations": { "readOnlyHint": false, "destructiveHint": false, "idempotentHint": true }
        },
        {
            "name": "upload_media",
            "title": "Upload an image or file",
            "description": "Upload a file (up to 8 MB) to the media library. Returns the url to use in an image block (core/image attributes.src). The server adds a prefix to the file name, so use the returned url.",
            "inputSchema": { "type": "object", "properties": {
                "filename": { "type": "string", "description": "e.g. cover.png" },
                "data_base64": { "type": "string", "description": "The file content, base64 encoded" },
                "content_type": { "type": "string", "description": "e.g. image/png. Guessed from the file name if omitted" }
            }, "required": ["filename", "data_base64"] },
            "annotations": { "readOnlyHint": false, "destructiveHint": false, "idempotentHint": false }
        },
        {
            "name": "publish_content",
            "title": "Publish an item",
            "description": "Make an item visible to visitors. The latest saved version (including a working copy) becomes the published version. With publish_at, it is published automatically at that time instead. Do this only when the user asked to publish.",
            "inputSchema": { "type": "object", "properties": {
                "content_type": content_type,
                "slug_or_id": { "type": "string" },
                "publish_at": { "type": "string", "description": "RFC 3339 date and time to publish at, e.g. 2030-01-31T09:00:00+09:00. Omit to publish now" },
                "review_note": { "type": "string" }
            }, "required": ["content_type", "slug_or_id"] },
            "annotations": { "readOnlyHint": false, "destructiveHint": true, "idempotentHint": true }
        },
        {
            "name": "unpublish_content",
            "title": "Take an item off the site",
            "description": "Stop showing an item to visitors by setting its status to \"draft\" (default) or \"archived\". The content is kept.",
            "inputSchema": { "type": "object", "properties": {
                "content_type": content_type,
                "slug_or_id": { "type": "string" },
                "to": { "type": "string", "enum": ["draft", "archived"] }
            }, "required": ["content_type", "slug_or_id"] },
            "annotations": { "readOnlyHint": false, "destructiveHint": true, "idempotentHint": true }
        },
        {
            "name": "delete_content",
            "title": "Delete an item",
            "description": "Delete an item permanently. Do this only when the user clearly asked to delete.",
            "inputSchema": { "type": "object", "properties": {
                "content_type": content_type,
                "slug_or_id": { "type": "string" }
            }, "required": ["content_type", "slug_or_id"] },
            "annotations": { "readOnlyHint": false, "destructiveHint": true, "idempotentHint": true }
        },
        {
            "name": "list_comments",
            "title": "List comments",
            "description": "List comments for moderation, newest first, with the counts per status. Needs the comment feature to be on.",
            "inputSchema": { "type": "object", "properties": {
                "status": { "type": "string", "enum": ["pending", "approved", "spam", "trash"] },
                "content_type": { "type": "string" },
                "content_id": { "type": "string" },
                "limit": { "type": "integer", "minimum": 1, "maximum": MAX_LIST_LIMIT },
                "offset": { "type": "integer", "minimum": 0 }
            } },
            "annotations": { "readOnlyHint": true }
        },
        {
            "name": "moderate_comment",
            "title": "Approve, hold, or reject a comment",
            "description": "Set the status of a comment: approved makes it public; pending, spam and trash hide it. Do this only as the user asked.",
            "inputSchema": { "type": "object", "properties": {
                "id": { "type": "string" },
                "status": { "type": "string", "enum": ["pending", "approved", "spam", "trash"] }
            }, "required": ["id", "status"] },
            "annotations": { "readOnlyHint": false, "destructiveHint": true, "idempotentHint": true }
        },
        {
            "name": "reply_to_comment",
            "title": "Reply to a comment",
            "description": "Post a public reply to a comment, as the owner of the token. The reply is published at once, and the comment it answers is approved with it. Write the text exactly as the user asked.",
            "inputSchema": { "type": "object", "properties": {
                "id": { "type": "string", "description": "The id of the comment to answer" },
                "body": { "type": "string", "description": "Plain text" }
            }, "required": ["id", "body"] },
            "annotations": { "readOnlyHint": false, "destructiveHint": true, "idempotentHint": false }
        },
        {
            "name": "list_orders",
            "title": "List orders",
            "description": "List shop orders, newest first. This includes customers' personal information. Needs the shop to be on.",
            "inputSchema": { "type": "object", "properties": {
                "status": { "type": "string", "description": "e.g. pending, paid, shipped, completed, cancelled" },
                "limit": { "type": "integer", "minimum": 1, "maximum": MAX_LIST_LIMIT },
                "offset": { "type": "integer", "minimum": 0 }
            } },
            "annotations": { "readOnlyHint": true }
        },
        {
            "name": "get_order",
            "title": "Get an order",
            "description": "Get one order with its items and customer details.",
            "inputSchema": { "type": "object", "properties": { "id": { "type": "string" } }, "required": ["id"] },
            "annotations": { "readOnlyHint": true }
        },
        {
            "name": "sales_report",
            "title": "Daily sales report",
            "description": "Sales per day between two UTC dates (inclusive).",
            "inputSchema": { "type": "object", "properties": {
                "start": { "type": "string", "description": "YYYY-MM-DD" },
                "end": { "type": "string", "description": "YYYY-MM-DD" }
            }, "required": ["start", "end"] },
            "annotations": { "readOnlyHint": true }
        },
        {
            "name": "list_reviews",
            "title": "List product reviews",
            "description": "List product reviews for moderation. Needs the shop reviews to be on.",
            "inputSchema": { "type": "object", "properties": {
                "product_id": { "type": "string" },
                "status": { "type": "string", "enum": ["pending", "approved", "spam", "trash"] },
                "limit": { "type": "integer", "minimum": 1, "maximum": MAX_LIST_LIMIT },
                "offset": { "type": "integer", "minimum": 0 }
            } },
            "annotations": { "readOnlyHint": true }
        },
        {
            "name": "moderate_review",
            "title": "Approve, hold, or reject a review",
            "description": "Set the status of a review: approved makes it public. When rejecting, give the reason in rejection_reason. Do this only as the user asked.",
            "inputSchema": { "type": "object", "properties": {
                "id": { "type": "string" },
                "status": { "type": "string", "enum": ["pending", "approved", "spam", "trash"] },
                "rejection_reason": { "type": "string" }
            }, "required": ["id", "status"] },
            "annotations": { "readOnlyHint": false, "destructiveHint": true, "idempotentHint": true }
        },
        {
            "name": "reply_to_review",
            "title": "Reply to a review",
            "description": "Set the shop's public reply to a review. Write the text exactly as the user asked.",
            "inputSchema": { "type": "object", "properties": {
                "id": { "type": "string" },
                "reply": { "type": "string", "description": "Plain text" }
            }, "required": ["id", "reply"] },
            "annotations": { "readOnlyHint": false, "destructiveHint": true, "idempotentHint": true }
        },
        {
            "name": "list_members",
            "title": "List members",
            "description": "List the site's members (visitor accounts). This includes personal information. Needs the member feature to be on.",
            "inputSchema": { "type": "object", "properties": {
                "limit": { "type": "integer", "minimum": 1, "maximum": MAX_LIST_LIMIT },
                "offset": { "type": "integer", "minimum": 0 }
            } },
            "annotations": { "readOnlyHint": true }
        },
        {
            "name": "get_member",
            "title": "Get a member",
            "description": "Get one member with their level and status.",
            "inputSchema": { "type": "object", "properties": { "id": { "type": "string" } }, "required": ["id"] },
            "annotations": { "readOnlyHint": true }
        }
    ])
}

fn str_arg<'a>(args: &'a Value, key: &str) -> Option<&'a str> {
    args.get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
}

fn bad(message: impl Into<String>) -> Outcome {
    Outcome::Failed(json!({ "error": message.into() }).to_string())
}

/// URL の 1 要素（パスの一部またはクエリの値）にする
fn enc(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for b in value.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char)
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

async fn api(
    router: &Router,
    auth: &str,
    method: Method,
    uri: &str,
    body: Option<&Value>,
) -> (StatusCode, Value) {
    match body {
        Some(value) => {
            api_with(
                router,
                auth,
                method,
                uri,
                Some("application/json"),
                Body::from(value.to_string()),
            )
            .await
        }
        None => api_with(router, auth, method, uri, None, Body::empty()).await,
    }
}

async fn api_with(
    router: &Router,
    auth: &str,
    method: Method,
    uri: &str,
    content_type: Option<&str>,
    body: Body,
) -> (StatusCode, Value) {
    let mut builder = Request::builder()
        .method(method)
        .uri(uri)
        .header(header::AUTHORIZATION, auth);
    if let Some(content_type) = content_type {
        builder = builder.header(header::CONTENT_TYPE, content_type);
    }
    let request = builder.body(body).expect("internal request");
    let response = match router.clone().oneshot(request).await {
        Ok(response) => response,
        Err(never) => match never {},
    };
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), MAX_API_RESPONSE_BYTES)
        .await
        .unwrap_or_default();
    let value = if bytes.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&bytes)
            .unwrap_or_else(|_| Value::String(String::from_utf8_lossy(&bytes).into_owned()))
    };
    (status, value)
}

fn finish(status: StatusCode, value: Value) -> Outcome {
    if status == StatusCode::UNAUTHORIZED {
        Outcome::Unauthorized
    } else if status.is_success() {
        Outcome::Done(value.to_string())
    } else {
        Outcome::Failed(json!({ "status": status.as_u16(), "response": value }).to_string())
    }
}

async fn call_tool(router: &Router, auth: &str, name: &str, args: &Value) -> Outcome {
    let get = |uri: String| async move { api(router, auth, Method::GET, &uri, None).await };
    match name {
        "list_content_types" => {
            let (s, v) = get("/api/v1/schemas".into()).await;
            finish(s, v)
        }
        "get_content_schema" => {
            let Some(ct) = str_arg(args, "content_type") else {
                return bad("content_type is required");
            };
            let (s, v) = get(format!("/api/v1/schemas/{}", enc(ct))).await;
            finish(s, v)
        }
        "list_tags" => {
            let (s, v) = get("/api/v1/tags".into()).await;
            finish(s, v)
        }
        "list_content" => {
            let Some(ct) = str_arg(args, "content_type") else {
                return bad("content_type is required");
            };
            let limit = args
                .get("limit")
                .and_then(Value::as_u64)
                .unwrap_or(DEFAULT_LIST_LIMIT)
                .clamp(1, MAX_LIST_LIMIT);
            let offset = args.get("offset").and_then(Value::as_u64).unwrap_or(0);
            let mut query = format!("view=summary&limit={limit}&offset={offset}");
            for key in ["q", "status", "tag", "category"] {
                if let Some(value) = str_arg(args, key) {
                    query.push_str(&format!("&{key}={}", enc(value)));
                }
            }
            let (s, v) = get(format!("/api/v1/contents/{}?{query}", enc(ct))).await;
            finish(s, v)
        }
        "get_content" => {
            let (Some(ct), Some(item)) =
                (str_arg(args, "content_type"), str_arg(args, "slug_or_id"))
            else {
                return bad("content_type and slug_or_id are required");
            };
            let (s, v) = get(format!("/api/v1/contents/{}/{}", enc(ct), enc(item))).await;
            finish(s, v)
        }
        "create_draft" => {
            let (Some(ct), Some(title), Some(slug)) = (
                str_arg(args, "content_type"),
                str_arg(args, "title"),
                str_arg(args, "slug"),
            ) else {
                return bad("content_type, title and slug are required");
            };
            let mut body = Map::new();
            body.insert("title".into(), json!(title));
            body.insert("slug".into(), json!(slug));
            // 下書きだけ。公開・予約・レビュー待ちには、このツールからはできない
            body.insert("status".into(), json!("draft"));
            if let Some(blocks) = args.get("blocks") {
                body.insert("blocks".into(), blocks.clone());
            }
            if let Some(fields) = args.get("fields") {
                body.insert("fields".into(), fields.clone());
            }
            let (s, v) = api(
                router,
                auth,
                Method::POST,
                &format!("/api/v1/contents/{}", enc(ct)),
                Some(&Value::Object(body)),
            )
            .await;
            finish(s, v)
        }
        "update_content" => update_content(router, auth, args).await,
        "request_review" => change_status(router, auth, args, Change::Review).await,
        "publish_content" => change_status(router, auth, args, Change::Publish).await,
        "unpublish_content" => change_status(router, auth, args, Change::Unpublish).await,
        "delete_content" => delete_content(router, auth, args).await,
        "set_tags" => set_tags(router, auth, args).await,
        "upload_media" => upload_media(router, auth, args).await,
        "list_comments" => {
            let query = query_of(args, &["status", "content_type", "content_id"]);
            site_get(router, auth, &format!("/api/v1/comments?{query}")).await
        }
        "moderate_comment" => {
            let (Some(id), Some(status)) = (str_arg(args, "id"), str_arg(args, "status")) else {
                return bad("id and status are required");
            };
            site_write(
                router,
                auth,
                Method::PATCH,
                &format!("/api/v1/comments/{}", enc(id)),
                json!({ "status": status }),
            )
            .await
        }
        "reply_to_comment" => {
            let (Some(id), Some(body)) = (str_arg(args, "id"), str_arg(args, "body")) else {
                return bad("id and body are required");
            };
            site_write(
                router,
                auth,
                Method::POST,
                &format!("/api/v1/comments/{}/replies", enc(id)),
                json!({ "body": body }),
            )
            .await
        }
        "list_orders" => {
            let query = query_of(args, &["status"]);
            site_get(router, auth, &format!("/api/v1/orders?{query}")).await
        }
        "get_order" => {
            let Some(id) = str_arg(args, "id") else {
                return bad("id is required");
            };
            site_get(router, auth, &format!("/api/v1/orders/{}", enc(id))).await
        }
        "sales_report" => {
            let (Some(start), Some(end)) = (str_arg(args, "start"), str_arg(args, "end")) else {
                return bad("start and end are required (YYYY-MM-DD)");
            };
            site_get(
                router,
                auth,
                &format!(
                    "/api/v1/orders/report?start={}&end={}",
                    enc(start),
                    enc(end)
                ),
            )
            .await
        }
        "list_reviews" => {
            let query = query_of(args, &["product_id", "status"]);
            site_get(router, auth, &format!("/api/v1/admin/reviews?{query}")).await
        }
        "moderate_review" => {
            let (Some(id), Some(status)) = (str_arg(args, "id"), str_arg(args, "status")) else {
                return bad("id and status are required");
            };
            let mut body = json!({ "status": status });
            if let Some(reason) = str_arg(args, "rejection_reason") {
                body["rejection_reason"] = json!(reason);
            }
            site_write(
                router,
                auth,
                Method::PATCH,
                &format!("/api/v1/admin/reviews/{}", enc(id)),
                body,
            )
            .await
        }
        "reply_to_review" => {
            let (Some(id), Some(reply)) = (str_arg(args, "id"), str_arg(args, "reply")) else {
                return bad("id and reply are required");
            };
            site_write(
                router,
                auth,
                Method::POST,
                &format!("/api/v1/admin/reviews/{}/reply", enc(id)),
                json!({ "reply": reply }),
            )
            .await
        }
        "list_members" => {
            let query = query_of(args, &[]);
            site_get(router, auth, &format!("/api/v1/admin/members?{query}")).await
        }
        "get_member" => {
            let Some(id) = str_arg(args, "id") else {
                return bad("id is required");
            };
            site_get(router, auth, &format!("/api/v1/admin/members/{}", enc(id))).await
        }
        _ => Outcome::UnknownTool,
    }
}

/// 一覧の問い合わせ文字列: 渡された文字列の絞り込み（`keys`）と、件数（既定 20・最大 100）・位置
fn query_of(args: &Value, keys: &[&str]) -> String {
    let limit = args
        .get("limit")
        .and_then(Value::as_u64)
        .unwrap_or(DEFAULT_LIST_LIMIT)
        .clamp(1, MAX_LIST_LIMIT);
    let offset = args.get("offset").and_then(Value::as_u64).unwrap_or(0);
    let mut query = format!("limit={limit}&offset={offset}");
    for key in keys {
        if let Some(value) = str_arg(args, key) {
            query.push_str(&format!("&{key}={}", enc(value)));
        }
    }
    query
}

async fn site_get(router: &Router, auth: &str, path: &str) -> Outcome {
    let (s, v) = api(router, auth, Method::GET, path, None).await;
    finish(s, v)
}

/// コメント・レビューの書き込み。本文の無い成功（204）には、した内容を返す
async fn site_write(
    router: &Router,
    auth: &str,
    method: Method,
    path: &str,
    body: Value,
) -> Outcome {
    let (s, v) = api(router, auth, method, path, Some(&body)).await;
    if s.is_success() && v.is_null() {
        Outcome::Done(json!({ "ok": true, "request": body }).to_string())
    } else {
        finish(s, v)
    }
}

/// PUT は本文・フィールドを丸ごと置き換えるので、今の内容を読んで、`change` で差し替えた分だけ書き戻す。
/// 状態は `change` が変えなければ今のまま。`publish` は付けないので、公開中のコンテンツは作業用のコピーとして保存される
async fn rewrite(
    router: &Router,
    auth: &str,
    args: &Value,
    change: impl FnOnce(&mut Map<String, Value>) -> Result<(), String>,
) -> Outcome {
    let (Some(ct), Some(item)) = (str_arg(args, "content_type"), str_arg(args, "slug_or_id"))
    else {
        return bad("content_type and slug_or_id are required");
    };
    let path = format!("/api/v1/contents/{}/{}", enc(ct), enc(item));
    let (status, current) = api(router, auth, Method::GET, &path, None).await;
    if !status.is_success() {
        return finish(status, current);
    }
    let mut body = Map::new();
    for key in ["title", "slug", "status", "blocks"] {
        body.insert(key.into(), current[key].clone());
    }
    for key in ["author_id", "published_at", "unpublish_at"] {
        if !current[key].is_null() {
            body.insert(key.into(), current[key].clone());
        }
    }
    body.insert(
        "fields".into(),
        Value::Object(current["fields"].as_object().cloned().unwrap_or_default()),
    );
    if let Err(message) = change(&mut body) {
        return bad(message);
    }
    let (s, v) = api(router, auth, Method::PUT, &path, Some(&Value::Object(body))).await;
    finish(s, v)
}

async fn update_content(router: &Router, auth: &str, args: &Value) -> Outcome {
    rewrite(router, auth, args, |body| {
        for key in ["title", "slug"] {
            if let Some(value) = str_arg(args, key) {
                body.insert(key.into(), json!(value));
            }
        }
        if let Some(blocks) = args.get("blocks") {
            body.insert("blocks".into(), blocks.clone());
        }
        if let Some(new_fields) = args.get("fields").and_then(Value::as_object) {
            if let Some(Value::Object(fields)) = body.get_mut("fields") {
                for (key, value) in new_fields {
                    fields.insert(key.clone(), value.clone());
                }
            }
        }
        Ok(())
    })
    .await
}

enum Change {
    Review,
    Publish,
    Unpublish,
}

/// 状態を変える 3 つ: レビューに出す（pending）、公開か予約公開にする、公開をやめる（draft か archived）
async fn change_status(router: &Router, auth: &str, args: &Value, change: Change) -> Outcome {
    rewrite(router, auth, args, |body| {
        match change {
            Change::Review => {
                body.insert("status".into(), json!("pending"));
            }
            Change::Publish => {
                if let Some(at) = str_arg(args, "publish_at") {
                    // 予約公開: 日時になると自動で公開される
                    if aegis_core::time::parse_rfc3339(at).is_err() {
                        return Err(
                            "publish_at must be an RFC 3339 date and time, e.g. 2030-01-31T09:00:00+09:00"
                                .into(),
                        );
                    }
                    body.insert("status".into(), json!("scheduled"));
                    body.insert("published_at".into(), json!(at));
                } else {
                    body.insert("status".into(), json!("published"));
                    // 作業用のコピーがあれば、それを公開版にする
                    body.insert("publish".into(), json!(true));
                }
            }
            Change::Unpublish => {
                let to = str_arg(args, "to").unwrap_or("draft");
                if to != "draft" && to != "archived" {
                    return Err("to must be \"draft\" or \"archived\"".into());
                }
                body.insert("status".into(), json!(to));
            }
        }
        if let Some(note) = str_arg(args, "review_note") {
            body.insert("review_note".into(), json!(note));
        }
        Ok(())
    })
    .await
}

async fn delete_content(router: &Router, auth: &str, args: &Value) -> Outcome {
    let (Some(ct), Some(item)) = (str_arg(args, "content_type"), str_arg(args, "slug_or_id"))
    else {
        return bad("content_type and slug_or_id are required");
    };
    let (s, v) = api(
        router,
        auth,
        Method::DELETE,
        &format!("/api/v1/contents/{}/{}", enc(ct), enc(item)),
        None,
    )
    .await;
    if s.is_success() {
        Outcome::Done(
            json!({ "deleted": true, "content_type": ct, "slug_or_id": item }).to_string(),
        )
    } else {
        finish(s, v)
    }
}

/// タグを置き換える。`tags` は slug か ID
async fn set_tags(router: &Router, auth: &str, args: &Value) -> Outcome {
    let (Some(ct), Some(item)) = (str_arg(args, "content_type"), str_arg(args, "slug_or_id"))
    else {
        return bad("content_type and slug_or_id are required");
    };
    let Some(wanted) = args.get("tags").and_then(Value::as_array) else {
        return bad(
            "tags is required: a list of tag slugs or ids (an empty list removes all tags)",
        );
    };
    let mut ids: Vec<String> = Vec::new();
    for value in wanted {
        let Some(value) = value.as_str().map(str::trim).filter(|v| !v.is_empty()) else {
            return bad("tags must be non-empty strings");
        };
        let (s, found) = api(
            router,
            auth,
            Method::GET,
            &format!("/api/v1/tags?limit=50&q={}", enc(value)),
            None,
        )
        .await;
        if !s.is_success() {
            return finish(s, found);
        }
        let hit = found["tags"].as_array().into_iter().flatten().find(|t| {
            t["id"].as_str() == Some(value)
                || t["slug"]
                    .as_str()
                    .is_some_and(|slug| slug.eq_ignore_ascii_case(value))
        });
        match hit.and_then(|t| t["id"].as_str()) {
            Some(id) => {
                if !ids.iter().any(|known| known == id) {
                    ids.push(id.to_string());
                }
            }
            None => {
                return bad(format!(
                    "Unknown tag '{value}'. See list_tags for the existing tags"
                ))
            }
        }
    }
    let (s, v) = api(
        router,
        auth,
        Method::PUT,
        &format!("/api/v1/contents/{}/{}/tags", enc(ct), enc(item)),
        Some(&json!({ "tag_ids": ids })),
    )
    .await;
    finish(s, v)
}

/// MCP 経由でアップロードできる大きさ（デコード後）。それ以上は REST API を使う
const MAX_MEDIA_BYTES: usize = 8 * 1024 * 1024;

fn guess_content_type(filename: &str) -> &'static str {
    match filename
        .rsplit('.')
        .next()
        .unwrap_or_default()
        .to_ascii_lowercase()
        .as_str()
    {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "avif" => "image/avif",
        "pdf" => "application/pdf",
        "txt" => "text/plain",
        _ => "application/octet-stream",
    }
}

async fn upload_media(router: &Router, auth: &str, args: &Value) -> Outcome {
    use base64::Engine;
    let too_large = || {
        bad(format!(
            "The file is too large for MCP (limit: {} MB). Upload it with the REST API instead",
            MAX_MEDIA_BYTES / (1024 * 1024)
        ))
    };
    let (Some(filename), Some(data)) = (str_arg(args, "filename"), str_arg(args, "data_base64"))
    else {
        return bad("filename and data_base64 are required");
    };
    if data.len() > MAX_MEDIA_BYTES / 3 * 4 + 8 {
        return too_large();
    }
    let Ok(bytes) = base64::engine::general_purpose::STANDARD.decode(data) else {
        return bad("data_base64 is not valid base64");
    };
    if bytes.len() > MAX_MEDIA_BYTES {
        return too_large();
    }
    // ヘッダーに入るので、引用符・改行・パスの区切りを除く
    let filename: String = filename
        .chars()
        .filter(|c| !c.is_control() && !matches!(c, '"' | '/' | '\\'))
        .collect();
    if filename.is_empty() {
        return bad("filename is empty");
    }
    let content_type = str_arg(args, "content_type")
        .filter(|t| !t.chars().any(|c| c.is_control() || c == '"'))
        .unwrap_or_else(|| guess_content_type(&filename));
    let boundary = format!("aegis{}", uuid::Uuid::new_v4().simple());
    let mut body = format!(
        "--{boundary}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"{filename}\"\r\nContent-Type: {content_type}\r\n\r\n"
    )
    .into_bytes();
    body.extend_from_slice(&bytes);
    body.extend_from_slice(format!("\r\n--{boundary}--\r\n").as_bytes());
    let (s, v) = api_with(
        router,
        auth,
        Method::POST,
        "/api/v1/storage/upload",
        Some(&format!("multipart/form-data; boundary={boundary}")),
        Body::from(body),
    )
    .await;
    finish(s, v)
}
