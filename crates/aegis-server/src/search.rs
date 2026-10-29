//! サイト内検索（`[search]`、Cargo の feature `search`）。索引と検索は `aegis-search`、ここは DB からの読み込み・
//! 変更の反映・API・検索ページ
//!
//! - 起動時に、公開中のコンテンツ（`[search].content_types`）から索引を作り直す（裏で行い、起動は待たせない）
//! - コンテンツの公開・更新・非公開・削除は、イベントバス（[`TOPIC_SEARCH`]）で全サーバーに知らせ、
//!   受け取ったサーバーが DB から読み直して自分の索引に反映する（送ったサーバー自身にも届く）
//! - 索引への書き込みは 1 つずつ（[`SiteSearch::write`]）。読み直しは書き込みの順番が来てから行うので、
//!   作り直しと個別の反映が重なっても新しい内容が残る

use std::path::Path;
use std::sync::Arc;

use aegis_ast::BlockNode;
use aegis_core::{ContentSummary, PageRenderContext, Pagination};
use aegis_db::{ContentListQuery, ContentRecord};
use aegis_search::{SearchChange, SearchDocument, SearchIndex, SearchQuery, SearchResults};
use aegis_theme::FrontPage;
use axum::extract::{Query, State};
use axum::http::{header, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::handlers::content::{is_currently_published, STATUS_PUBLISHED};
use crate::state::AppState;

/// コンテンツが変わった（payload は `{"content_type": ..., "id": ...}`）。受け取ったら DB から読み直して索引に反映する
pub const TOPIC_SEARCH: &str = "search.content";

/// API で 1 回に返す件数の上限
pub const MAX_API_LIMIT: usize = 50;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct SearchConfig {
    /// サイト内検索（`/search`・`GET /api/v1/search`）を使う
    pub enabled: bool,
    /// 索引を置くディレクトリ。空ならメモリの中だけ（起動のたびに作り直す）
    pub index_dir: String,
    /// 検索の対象にするコンテンツの種類
    pub content_types: Vec<String>,
}

impl Default for SearchConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            index_dir: "data/search".to_string(),
            content_types: vec!["article".to_string(), "page".to_string()],
        }
    }
}

/// 検索の索引と、その書き込みの順番待ち
pub struct SiteSearch {
    index: Arc<SearchIndex>,
    content_types: Vec<String>,
    write: tokio::sync::Mutex<()>,
}

impl SiteSearch {
    /// `[search]` から作る。索引のディレクトリが使えない（別のサーバーが使っているなど）ときはメモリの索引にする
    pub fn from_config(config: &SearchConfig) -> anyhow::Result<Self> {
        let dir = config.index_dir.trim();
        let index = if dir.is_empty() {
            SearchIndex::in_memory()?
        } else {
            match SearchIndex::open_in_dir(Path::new(dir)) {
                Ok(index) => index,
                Err(e) => {
                    tracing::warn!(
                        "Could not open the search index in {} ({}); keeping it in memory instead",
                        dir,
                        e
                    );
                    SearchIndex::in_memory()?
                }
            }
        };
        Ok(Self {
            index: Arc::new(index),
            content_types: config.content_types.clone(),
            write: tokio::sync::Mutex::new(()),
        })
    }

    pub fn indexes(&self, content_type: &str) -> bool {
        self.content_types.iter().any(|t| t == content_type)
    }

    pub fn content_types(&self) -> &[String] {
        &self.content_types
    }

    /// 検索する（索引を読むだけなので順番待ちはしない）
    pub async fn search(
        &self,
        text: String,
        content_types: Vec<String>,
        limit: usize,
        offset: usize,
    ) -> anyhow::Result<SearchResults> {
        let index = self.index.clone();
        let results = tokio::task::spawn_blocking(move || {
            index.search(&SearchQuery {
                text: &text,
                content_types: &content_types,
                limit,
                offset,
            })
        })
        .await??;
        Ok(results)
    }
}

/// 公開ページのパス（`base_path` を除く）
fn public_path(state: &AppState, record: &ContentRecord) -> String {
    match record.content_type.as_str() {
        "article" => format!("/articles/{}", record.slug),
        "page" => match state.config.theme.front_page() {
            FrontPage::Page(front) if front == record.slug => "/".to_string(),
            _ => format!("/{}", record.slug),
        },
        t if t == state.config.commerce.product_type => format!("/products/{}", record.slug),
        _ => format!("/{}", record.slug),
    }
}

fn to_document(state: &AppState, record: &ContentRecord) -> SearchDocument {
    let blocks: Vec<BlockNode> = serde_json::from_str(&record.blocks_json).unwrap_or_default();
    let mut body = String::new();
    // 抜粋（excerpt）を書いていれば、それも検索の対象にする
    if let Some(excerpt) = record.custom_fields.get("excerpt").and_then(|v| v.as_str()) {
        body.push_str(excerpt.trim());
        body.push('\n');
    }
    body.push_str(&aegis_search::blocks_text(&blocks));
    SearchDocument {
        content_type: record.content_type.clone(),
        id: record.id.clone(),
        slug: record.slug.clone(),
        title: record.title.clone(),
        body: body.trim().to_string(),
        url: public_path(state, record),
        published_at: record.published_at.clone(),
    }
}

/// 公開中のコンテンツをすべて読み直して索引を作り直す
pub async fn rebuild(state: &AppState) {
    let Some(search) = state.search.clone() else {
        return;
    };
    let _turn = search.write.lock().await;
    let now = chrono::Utc::now();
    let mut docs = Vec::new();
    for content_type in &search.content_types {
        let schema = state.schemas.read().await.get(content_type).cloned();
        let Some(schema) = schema else {
            continue;
        };
        let query = ContentListQuery {
            status: Some(STATUS_PUBLISHED.to_string()),
            ..ContentListQuery::default()
        };
        match state.db.list_published_contents_page(&schema, &query).await {
            Ok(page) => docs.extend(
                page.records
                    .iter()
                    .filter(|r| is_currently_published(&r.status, r.unpublish_at.as_deref(), now))
                    .map(|r| to_document(state, r)),
            ),
            Err(e) => {
                tracing::error!(
                    "Could not read the published {} contents for the search index (keeping the current index): {}",
                    content_type,
                    e
                );
                return;
            }
        }
    }
    let count = docs.len();
    let index = search.index.clone();
    match tokio::task::spawn_blocking(move || index.rebuild(&docs)).await {
        Ok(Ok(())) => tracing::info!("Search index rebuilt ({} documents)", count),
        Ok(Err(e)) => tracing::error!("Could not rebuild the search index: {}", e),
        Err(e) => tracing::error!("Could not rebuild the search index: {}", e),
    }
}

/// 1 件を DB から読み直して索引に反映する（公開中なら追加・置き換え、そうでなければ除く）
pub async fn refresh(state: &AppState, content_type: &str, id: &str) {
    let Some(search) = state.search.clone() else {
        return;
    };
    if !search.indexes(content_type) {
        return;
    }
    let _turn = search.write.lock().await;
    let schema = state.schemas.read().await.get(content_type).cloned();
    let record = match &schema {
        Some(schema) => match state.db.get_published_content(schema, id).await {
            Ok(record) => record,
            Err(e) => {
                tracing::warn!(
                    "Could not read {} {} for the search index: {}",
                    content_type,
                    id,
                    e
                );
                return;
            }
        },
        None => None,
    };
    let now = chrono::Utc::now();
    let change = match record
        .filter(|r| r.id == id)
        .filter(|r| is_currently_published(&r.status, r.unpublish_at.as_deref(), now))
    {
        Some(record) => SearchChange::Upsert(to_document(state, &record)),
        None => SearchChange::Remove {
            content_type: content_type.to_string(),
            id: id.to_string(),
        },
    };
    let index = search.index.clone();
    match tokio::task::spawn_blocking(move || index.apply(&[change])).await {
        Ok(Ok(())) => {}
        Ok(Err(e)) => tracing::error!("Could not update the search index: {}", e),
        Err(e) => tracing::error!("Could not update the search index: {}", e),
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct ChangedContent {
    content_type: String,
    id: String,
}

/// コンテンツの公開・更新・非公開・削除を全サーバーの索引に知らせる（検索の対象でない種類なら何もしない）
pub async fn announce_change(state: &AppState, content_type: &str, id: &str) {
    if !state
        .search
        .as_ref()
        .is_some_and(|s| s.indexes(content_type))
    {
        return;
    }
    let payload = json!({ "content_type": content_type, "id": id }).to_string();
    crate::cluster::announce(state, TOPIC_SEARCH, &payload).await;
}

/// イベントバスで受け取った知らせを反映する
pub async fn handle_event(state: &AppState, payload: &str) {
    match serde_json::from_str::<ChangedContent>(payload) {
        Ok(changed) => refresh(state, &changed.content_type, &changed.id).await,
        Err(e) => tracing::warn!("Ignoring a malformed search event: {}", e),
    }
}

/// 起動時の作り直しを裏で始める
pub fn spawn_initial_rebuild(state: Arc<AppState>) {
    if state.search.is_some() {
        tokio::spawn(async move { rebuild(&state).await });
    }
}

#[derive(Debug, Default, Deserialize)]
pub struct SearchParams {
    #[serde(default)]
    pub q: String,
    /// 絞り込むコンテンツの種類（カンマ区切り）
    #[serde(default, rename = "type")]
    pub content_type: Option<String>,
    pub limit: Option<usize>,
    pub offset: Option<usize>,
    /// 検索ページのページ番号（1 始まり）
    pub page: Option<u64>,
}

fn requested_types(search: &SiteSearch, param: Option<&str>) -> Vec<String> {
    param
        .map(|p| {
            p.split(',')
                .map(str::trim)
                .filter(|t| search.indexes(t))
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

fn base_path(state: &AppState) -> String {
    state
        .config
        .server
        .normalized_base_path()
        .unwrap_or_default()
}

/// `GET /api/v1/search?q=...&type=article,page&limit=10&offset=0` — 公開中のコンテンツを検索する（ログイン不要）
pub async fn search_api(
    State(state): State<Arc<AppState>>,
    Query(params): Query<SearchParams>,
) -> Response {
    let Some(search) = state.search.clone() else {
        return (
            StatusCode::NOT_FOUND,
            Json(json!({ "error": "Site search is not enabled" })),
        )
            .into_response();
    };
    let q = params.q.trim();
    if q.chars().count() > aegis_search::MAX_QUERY_CHARS {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "The search query is too long (200 characters at most)" })),
        )
            .into_response();
    }
    let types = requested_types(&search, params.content_type.as_deref());
    if params.content_type.is_some() && types.is_empty() {
        return Json(json!({ "query": q, "total": 0, "items": [] })).into_response();
    }
    let limit = params.limit.unwrap_or(10).clamp(1, MAX_API_LIMIT);
    let offset = params.offset.unwrap_or(0);
    match search.search(q.to_string(), types, limit, offset).await {
        Ok(mut results) => {
            let base = base_path(&state);
            for hit in &mut results.hits {
                hit.url = format!("{}{}", base, hit.url);
            }
            (
                [(header::CACHE_CONTROL, "public, max-age=60")],
                Json(json!({ "query": q, "total": results.total, "items": results.hits })),
            )
                .into_response()
        }
        Err(e) => {
            tracing::error!("Search failed: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": "Search failed" })),
            )
                .into_response()
        }
    }
}

/// `GET /search?q=...&page=2` — 検索ページ（テーマの `search.html`）
pub async fn render_search_page(
    State(state): State<Arc<AppState>>,
    Query(params): Query<SearchParams>,
) -> Response {
    let Some(search) = state.search.clone() else {
        return crate::handlers::public::render_not_found(&state, "Site search is not enabled.")
            .await;
    };
    let q: String = params
        .q
        .trim()
        .chars()
        .take(aegis_search::MAX_QUERY_CHARS)
        .collect();
    let per_page = state.config.theme.posts_per_page.max(1);
    let page = params.page.unwrap_or(1).max(1);
    let types = requested_types(&search, params.content_type.as_deref());
    let base = base_path(&state);

    let mut items = Vec::new();
    let mut total_items = 0u64;
    if !q.is_empty() {
        let offset = (page - 1).saturating_mul(per_page);
        match search
            .search(q.clone(), types, per_page as usize, offset as usize)
            .await
        {
            Ok(results) => {
                total_items = results.total as u64;
                for hit in results.hits {
                    let mut fields = std::collections::HashMap::new();
                    fields.insert("url".to_string(), json!(format!("{}{}", base, hit.url)));
                    fields.insert("snippet_html".to_string(), json!(hit.snippet_html));
                    items.push(ContentSummary {
                        id: hit.id,
                        content_type: hit.content_type,
                        title: hit.title,
                        slug: hit.slug,
                        excerpt: String::new(),
                        author_id: None,
                        published_at: hit.published_at,
                        created_at: String::new(),
                        fields,
                        relations: Default::default(),
                    });
                }
            }
            Err(e) => tracing::error!("Search failed: {}", e),
        }
    }

    let total_pages = total_items.div_ceil(per_page).max(1);
    let page_url = |n: u64| {
        let mut url = format!(
            "{}/search?q={}",
            base,
            url::form_urlencoded::byte_serialize(q.as_bytes()).collect::<String>()
        );
        if let Some(t) = params.content_type.as_deref().filter(|t| !t.is_empty()) {
            url.push_str("&type=");
            url.extend(url::form_urlencoded::byte_serialize(t.as_bytes()));
        }
        if n > 1 {
            url.push_str(&format!("&page={}", n));
        }
        url
    };
    let pagination = Pagination {
        page,
        per_page,
        total_items,
        total_pages,
        prev_url: (page > 1).then(|| page_url(page - 1)),
        next_url: (page < total_pages).then(|| page_url(page + 1)),
    };

    let site = crate::render::site_page_context(&state);
    let mut fields = std::collections::HashMap::new();
    fields.insert("query".to_string(), json!(q));
    let mut ctx = PageRenderContext {
        template: "search.html".to_string(),
        content_type: "search".to_string(),
        title: if q.is_empty() {
            "Search".to_string()
        } else {
            format!("Search: {}", q)
        },
        slug: "search".to_string(),
        fields,
        items,
        pagination: Some(pagination),
        ..site
    };
    {
        let supervisor = state.supervisor.read().await;
        if let Ok(hooked) = supervisor.apply_render_page(ctx.clone()).await {
            ctx = hooked;
        }
    }
    let mut resp = crate::handlers::public::themed_response(
        state
            .theme
            .render(&ctx.template, &ctx)
            .map_err(|e| e.to_string()),
    );
    // 検索結果のページは検索エンジンに載せない
    resp.headers_mut().insert(
        "x-robots-tag",
        axum::http::HeaderValue::from_static("noindex"),
    );
    resp
}

#[cfg(test)]
mod tests;
