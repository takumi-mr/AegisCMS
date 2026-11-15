//! MCP（Model Context Protocol）サーバー。`POST /mcp` で Streamable HTTP（JSON 応答のみ・状態なし）を話す。
//!
//! 認証と権限は REST API と同じ。`Authorization: Bearer` に API トークン（`aegis_pat_...`）かセッション ID を付ける。
//! ツールは中で REST API のルーターを呼ぶだけで、承認フロー・検証・監査ログ・ロールの確認は REST と同じ経路を通る。
//! 最初のリリースは読み取りと「下書きの作成・更新」だけ。公開（`publish`）や状態の変更はできない。

use axum::{
    body::Body,
    extract::Extension,
    http::{header, HeaderMap, Method, Request, StatusCode},
    response::{IntoResponse, Response},
    Json, Router,
};
use serde_json::{json, Map, Value};
use tower::ServiceExt;

/// 実装している MCP のバージョン（新しい順）。クライアントが求めたものがこの中にあればそれで応じる
const SUPPORTED_VERSIONS: [&str; 3] = ["2025-06-18", "2025-03-26", "2024-11-05"];
/// 内部の API 応答を読む上限
const MAX_API_RESPONSE_BYTES: usize = 16 * 1024 * 1024;
/// `list_content` の件数の既定と上限
const DEFAULT_LIST_LIMIT: u64 = 20;
const MAX_LIST_LIMIT: u64 = 100;

const INSTRUCTIONS: &str = "AegisCMS content tools. Reads see what the token's role may see. \
Writes only ever save drafts or working copies: nothing here publishes, unpublishes or deletes content, \
so visitors never see a change until a person publishes it in Admin Studio. \
Body text is a list of blocks (see the content API guide); use get_content_schema for the custom fields of a type.";

/// `/mcp` の本体。`api` は REST API のルーター（`/mcp` 自身は含まない）
pub async fn handle(
    Extension(api): Extension<Router>,
    headers: HeaderMap,
    Json(message): Json<Value>,
) -> Response {
    let Some(auth) = headers
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .filter(|v| v.len() > 7 && v[..7].eq_ignore_ascii_case("bearer "))
    else {
        return unauthorized();
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
        "tools/list" => rpc_result(id, json!({ "tools": tool_definitions() })),
        "tools/call" => {
            let name = params.get("name").and_then(Value::as_str).unwrap_or("");
            let args = params
                .get("arguments")
                .cloned()
                .unwrap_or_else(|| json!({}));
            if !args.is_object() {
                return rpc_error(id, -32602, "arguments must be an object");
            }
            match call_tool(&api, auth, name, &args).await {
                Outcome::Unauthorized => unauthorized(),
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

fn unauthorized() -> Response {
    (
        StatusCode::UNAUTHORIZED,
        [(header::WWW_AUTHENTICATE, "Bearer")],
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
    let mut builder = Request::builder()
        .method(method)
        .uri(uri)
        .header(header::AUTHORIZATION, auth);
    let body = match body {
        Some(value) => {
            builder = builder.header(header::CONTENT_TYPE, "application/json");
            Body::from(value.to_string())
        }
        None => Body::empty(),
    };
    let request = builder.body(body).expect("internal request");
    let response = match router.clone().oneshot(request).await {
        Ok(response) => response,
        Err(never) => match never {},
    };
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), MAX_API_RESPONSE_BYTES)
        .await
        .unwrap_or_default();
    let value = serde_json::from_slice(&bytes)
        .unwrap_or_else(|_| Value::String(String::from_utf8_lossy(&bytes).into_owned()));
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
        _ => Outcome::UnknownTool,
    }
}

/// PUT は本文・フィールドを丸ごと置き換えるので、今の内容を読んで、送られた項目だけ差し替えて書き戻す。
/// 状態は今のまま（`publish` は付けない）なので、公開中のコンテンツは作業用のコピーとして保存される
async fn update_content(router: &Router, auth: &str, args: &Value) -> Outcome {
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
    body.insert(
        "title".into(),
        json!(str_arg(args, "title")
            .or_else(|| current["title"].as_str())
            .unwrap_or_default()),
    );
    body.insert(
        "slug".into(),
        json!(str_arg(args, "slug")
            .or_else(|| current["slug"].as_str())
            .unwrap_or_default()),
    );
    body.insert("status".into(), current["status"].clone());
    for key in ["author_id", "published_at", "unpublish_at"] {
        if !current[key].is_null() {
            body.insert(key.into(), current[key].clone());
        }
    }
    body.insert(
        "blocks".into(),
        args.get("blocks")
            .cloned()
            .unwrap_or_else(|| current["blocks"].clone()),
    );
    let mut fields = current["fields"].as_object().cloned().unwrap_or_default();
    if let Some(new_fields) = args.get("fields").and_then(Value::as_object) {
        for (key, value) in new_fields {
            fields.insert(key.clone(), value.clone());
        }
    }
    body.insert("fields".into(), Value::Object(fields));

    let (s, v) = api(router, auth, Method::PUT, &path, Some(&Value::Object(body))).await;
    finish(s, v)
}
