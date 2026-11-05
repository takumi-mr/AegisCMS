//! ヘッドレス配信 API（`[headless]`、feature `headless`）: 認証なしで公開済みのコンテンツだけを JSON で返す。
//! 設計は `docs/headless.md`。
//!
//! 返す内容の判断（公開済みか・どのフィールドを出すか・カーソル）はクレート `aegis-headless` にあり、
//! ここはルート、無効時の 404、IP ごとのレート制限、`Cache-Control`・`ETag`（`304`）、
//! CDN のキャッシュタグ、`?include=html` の描画だけを扱う。

use std::sync::Arc;

use aegis_ast::BlockNode;
use aegis_headless::{HeadlessError, ListParams};
use axum::{
    extract::{Path, Query, State},
    http::{header, HeaderMap, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::audit::ClientIp;
use crate::cdn::{apply_cache_tags, CacheTags};
use crate::state::AppState;

/// ルートのパス（キャッシュの消去でも使う）
pub const LIST_PATH: &str = "/api/v1/public/contents";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct HeadlessConfig {
    /// 配信 API（`GET /api/v1/public/contents/...`）を使う。既定は無効
    pub enabled: bool,
    /// 配信する型。ここに無い型は存在しないものとして扱う
    pub content_types: Vec<String>,
    /// 1 ページの件数の上限
    pub max_page_size: u32,
    /// `Cache-Control` の `max-age`（秒）
    pub cache_max_age: u64,
    /// IP ごとの 1 分あたりの上限。0 なら制限しない
    pub rate_limit_per_minute: u64,
}

impl Default for HeadlessConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            content_types: vec!["article".to_string(), "page".to_string()],
            max_page_size: 100,
            cache_max_age: 60,
            rate_limit_per_minute: 600,
        }
    }
}

impl HeadlessConfig {
    fn delivery(&self) -> aegis_headless::HeadlessConfig {
        aegis_headless::HeadlessConfig {
            content_types: self.content_types.clone(),
            max_page_size: self.max_page_size,
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct ListQueryParams {
    limit: Option<String>,
    cursor: Option<String>,
    category: Option<String>,
    tag: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct GetQueryParams {
    /// `html` なら、描画済みの HTML（`html`）も返す
    include: Option<String>,
}

fn error(status: StatusCode, message: &str) -> Response {
    (status, Json(json!({ "error": message }))).into_response()
}

fn failure(err: HeadlessError) -> Response {
    match err {
        HeadlessError::NotFound => error(StatusCode::NOT_FOUND, "Not found"),
        HeadlessError::BadRequest(message) => error(StatusCode::BAD_REQUEST, &message),
        HeadlessError::Source(detail) => {
            tracing::error!("Headless delivery failed: {}", detail);
            error(StatusCode::INTERNAL_SERVER_ERROR, "Internal error")
        }
    }
}

/// 無効なら 404、IP ごとの上限を超えたら 429。通すなら `None`
async fn gate(state: &AppState, ip: &ClientIp) -> Option<Response> {
    let settings = &state.config.headless;
    if !settings.enabled {
        return Some(error(StatusCode::NOT_FOUND, "Headless API is not enabled"));
    }
    let limit = settings.rate_limit_per_minute;
    // 接続元が分からないときは数えられない
    let ip = ip.0.as_deref().filter(|_| limit > 0)?;
    match state
        .cache
        .increment(&format!("headless:rate:{}", ip), Some(60))
        .await
    {
        Ok(count) if count > limit => {
            let mut response = error(StatusCode::TOO_MANY_REQUESTS, "Too many requests");
            response
                .headers_mut()
                .insert(header::RETRY_AFTER, HeaderValue::from_static("60"));
            Some(response)
        }
        Ok(_) => None,
        Err(e) => {
            // カウンターの失敗で配信は止めない
            tracing::warn!("Headless rate limit counter failed: {}", e);
            None
        }
    }
}

/// 本文に `ETag` と `Cache-Control` を付ける。`If-None-Match` が一致すれば `304`
fn cached_json(
    state: &AppState,
    request_headers: &HeaderMap,
    etag: &str,
    tags: &CacheTags,
    body: Value,
) -> Response {
    let cache_control = format!("public, max-age={}", state.config.headless.cache_max_age);
    let matches = request_headers
        .get(header::IF_NONE_MATCH)
        .and_then(|value| value.to_str().ok())
        .map(|sent| {
            sent.split(',').any(|candidate| {
                candidate.trim().trim_start_matches("W/") == etag.trim_start_matches("W/")
            })
        })
        .unwrap_or(false);

    let mut response = if matches {
        StatusCode::NOT_MODIFIED.into_response()
    } else {
        Json(body).into_response()
    };
    if let Ok(value) = HeaderValue::from_str(etag) {
        response.headers_mut().insert(header::ETAG, value);
    }
    if let Ok(value) = HeaderValue::from_str(&cache_control) {
        response.headers_mut().insert(header::CACHE_CONTROL, value);
    }
    // 200 のときだけ、CDN のタグを付ける（無効のときは no-store にする）
    apply_cache_tags(state, response, tags)
}

fn add_item_tags(tags: &mut CacheTags, item: &Value) {
    let content_type = item["content_type"].as_str().unwrap_or_default();
    if let Some(id) = item["id"].as_str() {
        tags.add_dependency(content_type, id);
    }
    if let Some(slug) = item["slug"].as_str() {
        tags.add_dependency(content_type, slug);
    }
    if let Some(relations) = item["relations"].as_object() {
        for relation in relations.values() {
            let target_type = relation["content_type"].as_str().unwrap_or_default();
            if let Some(id) = relation["id"].as_str() {
                tags.add_dependency(target_type, id);
            }
            if let Some(slug) = relation["slug"].as_str() {
                tags.add_dependency(target_type, slug);
            }
        }
    }
}

/// `GET /api/v1/public/contents/{type}?limit=&cursor=&category=&tag=` — 公開済みの一覧（ログイン不要）
pub async fn list_contents(
    State(state): State<Arc<AppState>>,
    client_ip: ClientIp,
    Path(content_type): Path<String>,
    Query(query): Query<ListQueryParams>,
    headers: HeaderMap,
) -> Response {
    if let Some(blocked) = gate(&state, &client_ip).await {
        return blocked;
    }
    let schemas = state.schemas.read().await;
    let delivered = aegis_headless::list(
        &*state.db,
        &state.config.headless.delivery(),
        &schemas,
        &content_type,
        &ListParams {
            limit: query.limit,
            cursor: query.cursor,
            category: query.category,
            tag: query.tag,
        },
        chrono::Utc::now(),
    )
    .await;
    drop(schemas);
    let delivered = match delivered {
        Ok(delivered) => delivered,
        Err(err) => return failure(err),
    };

    let mut tags = CacheTags::new();
    tags.add_dependency("collection", &content_type);
    tags.add_url(&format!("{}/{}", LIST_PATH, content_type));
    for item in &delivered.body.items {
        add_item_tags(&mut tags, item);
    }
    let etag = delivered.etag;
    let body = serde_json::to_value(delivered.body).unwrap_or_default();
    cached_json(&state, &headers, &etag, &tags, body)
}

/// `GET /api/v1/public/contents/{type}/{slug}?include=html` — 公開済みの 1 件（ログイン不要）
pub async fn get_content(
    State(state): State<Arc<AppState>>,
    client_ip: ClientIp,
    Path((content_type, slug)): Path<(String, String)>,
    Query(query): Query<GetQueryParams>,
    headers: HeaderMap,
) -> Response {
    if let Some(blocked) = gate(&state, &client_ip).await {
        return blocked;
    }
    let schemas = state.schemas.read().await;
    let delivered = aegis_headless::get(
        &*state.db,
        &state.config.headless.delivery(),
        &schemas,
        &content_type,
        &slug,
        chrono::Utc::now(),
    )
    .await;
    drop(schemas);
    let delivered = match delivered {
        Ok(delivered) => delivered,
        Err(err) => return failure(err),
    };

    let mut body = delivered.body;
    let mut etag = delivered.etag;
    if query.include.as_deref() == Some("html") {
        // 描画はプラグイン（Wasm）を呼ぶので、頼まれたときだけ行う。ETag も別にする
        let blocks: Vec<BlockNode> =
            serde_json::from_value(body["blocks"].clone()).unwrap_or_default();
        body["html"] = json!(crate::render::render_content_html(&state, &blocks).await);
        etag = etag.replacen('"', "\"html-", 1);
    }

    let mut tags = CacheTags::new();
    add_item_tags(&mut tags, &body);
    tags.add_url(&format!("{}/{}/{}", LIST_PATH, content_type, slug));
    cached_json(&state, &headers, &etag, &tags, body)
}

/// `GET /api/v1/public/openapi.json` — 配信する型と公開してよいフィールドだけの OpenAPI（ログイン不要）
pub async fn openapi_spec(
    State(state): State<Arc<AppState>>,
    client_ip: ClientIp,
    headers: HeaderMap,
) -> Response {
    if let Some(blocked) = gate(&state, &client_ip).await {
        return blocked;
    }
    let schemas = state.schemas.read().await;
    let spec = aegis_headless::public_openapi(&schemas, &state.config.headless.delivery());
    drop(schemas);
    let etag = aegis_headless::etag([spec.to_string()]);
    cached_json(&state, &headers, &etag, &CacheTags::new(), spec)
}

#[cfg(test)]
mod tests;
