//! 公開サイトのタグ: タグのページ（`/tags/{slug}`）、タグの一覧（`/tags`）、タグごとの記事のフィード
//! （`/tags/{slug}/feed.xml`）。記事・固定ページ・商品のページと記事一覧には、付いているタグを
//! `fields.tags`（`id`・`slug`・`name`・`description`・`url`）で渡す。タグは種類をまたぐので、
//! タグのページは記事・固定ページ・商品（EC が有効なとき）の公開中のものを新しい順に並べる

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use aegis_core::{ContentSummary, PageRenderContext, Pagination};
use aegis_db::{ContentListQuery, ContentRecord, TagRecord};
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use serde_json::json;

use crate::cdn::CacheTags;
use crate::handlers::content::is_currently_published;
use crate::handlers::public::{
    extract_excerpt, public_content_path, render_not_found, themed_response, ArticleFilter,
    IndexParams,
};
use crate::render;
use crate::state::AppState;

fn base_path(state: &AppState) -> String {
    state
        .config
        .server
        .normalized_base_path()
        .unwrap_or_default()
}

/// タグを付けて公開できる種類（タグのページに並ぶ）
fn listed_types(state: &AppState) -> Vec<String> {
    let mut types = vec!["article".to_string(), "page".to_string()];
    if state.commerce.is_some() {
        types.push(state.config.commerce.product_type.clone());
    }
    types
}

/// 公開ページのテンプレートに渡すタグ（`fields.tags` の 1 件）
fn tag_value(base: &str, tag: &TagRecord) -> serde_json::Value {
    json!({
        "id": tag.id,
        "slug": tag.slug,
        "name": tag.name,
        "description": tag.description,
        "url": format!("{}/tags/{}", base, tag.slug),
    })
}

/// 同じ種類のコンテンツ数件のタグ（1 回の問い合わせで読む）
pub(crate) struct TagLookup {
    base: String,
    by_content: HashMap<String, Vec<TagRecord>>,
}

impl TagLookup {
    /// `ids` のコンテンツのタグを読む。タグを無効にしているときや、読めなかったときは空
    pub(crate) async fn load(state: &AppState, content_type: &str, ids: &[String]) -> Self {
        let mut by_content: HashMap<String, Vec<TagRecord>> = HashMap::new();
        if state.tags.is_some() && !ids.is_empty() {
            match state.db.list_tags_for_contents(content_type, ids).await {
                Ok(rows) => {
                    for (content_id, tag) in rows {
                        by_content.entry(content_id).or_default().push(tag);
                    }
                }
                Err(e) => tracing::warn!(
                    "Could not read the tags of {} contents: {}",
                    content_type,
                    e
                ),
            }
        }
        Self {
            base: base_path(state),
            by_content,
        }
    }

    /// `fields.tags` に付いているタグを入れ、ページの依存タグ（タグの名前・slug を変えたときに
    /// キャッシュを消すため）を足す。スキーマに `tags` のフィールドがあれば、それを優先して何もしない
    pub(crate) fn attach(
        &self,
        id: &str,
        fields: &mut HashMap<String, serde_json::Value>,
        cache_tags: &mut CacheTags,
    ) {
        let Some(tags) = self.by_content.get(id) else {
            return;
        };
        if fields.contains_key("tags") {
            return;
        }
        for tag in tags {
            cache_tags.add_dependency("tag", &tag.id);
        }
        fields.insert(
            "tags".to_string(),
            serde_json::Value::Array(tags.iter().map(|t| tag_value(&self.base, t)).collect()),
        );
    }
}

/// 公開中のコンテンツ（`listed_types`）を種類ごとに読む
async fn published_records(state: &AppState) -> Vec<ContentRecord> {
    let now = chrono::Utc::now();
    let mut out = Vec::new();
    for content_type in listed_types(state) {
        let schema = state.schemas.read().await.get(&content_type).cloned();
        let Some(schema) = schema else { continue };
        let query = ContentListQuery {
            status: Some("published".to_string()),
            ..Default::default()
        };
        match state.db.list_published_contents_page(&schema, &query).await {
            Ok(page) => out.extend(
                page.records
                    .into_iter()
                    .filter(|r| is_currently_published(&r.status, r.unpublish_at.as_deref(), now)),
            ),
            Err(e) => tracing::error!("Could not list the published {}: {}", content_type, e),
        }
    }
    out
}

/// 新しい順（公開日時、同じなら ID）
fn sort_newest_first(records: &mut [ContentRecord]) {
    records.sort_by(|a, b| {
        b.published_at
            .cmp(&a.published_at)
            .then_with(|| b.id.cmp(&a.id))
    });
}

/// タグが付いた公開中のコンテンツ（新しい順）
async fn tagged_records(state: &AppState, tag: &TagRecord) -> Vec<ContentRecord> {
    let links = match state.db.list_tagged_contents(&tag.id, None).await {
        Ok(links) => links,
        Err(e) => {
            tracing::error!("Could not read the contents tagged {}: {}", tag.slug, e);
            return Vec::new();
        }
    };
    let tagged: HashSet<(String, String)> = links
        .into_iter()
        .map(|l| (l.content_type, l.content_id))
        .collect();
    let mut records: Vec<ContentRecord> = published_records(state)
        .await
        .into_iter()
        .filter(|r| tagged.contains(&(r.content_type.clone(), r.id.clone())))
        .collect();
    sort_newest_first(&mut records);
    records
}

/// 公開中のコンテンツが 1 件でもあるタグと、その数（名前順）。サイトマップと一覧に使う
pub(crate) async fn tags_in_use(state: &AppState) -> Vec<(TagRecord, usize)> {
    if state.tags.is_none() {
        return Vec::new();
    }
    let published: HashSet<(String, String)> = published_records(state)
        .await
        .into_iter()
        .map(|r| (r.content_type, r.id))
        .collect();
    let links = match state.db.list_all_content_tags().await {
        Ok(links) => links,
        Err(e) => {
            tracing::error!("Could not read the content tags: {}", e);
            return Vec::new();
        }
    };
    let mut counts: HashMap<String, usize> = HashMap::new();
    for link in links {
        if published.contains(&(link.content_type, link.content_id)) {
            *counts.entry(link.tag_id).or_default() += 1;
        }
    }
    let page = match state.db.list_tags(&Default::default()).await {
        Ok(page) => page,
        Err(e) => {
            tracing::error!("Could not read the tags: {}", e);
            return Vec::new();
        }
    };
    page.tags
        .into_iter()
        .filter_map(|t| {
            let count = counts.get(&t.tag.id).copied().unwrap_or(0);
            (count > 0).then_some((t.tag, count))
        })
        .collect()
}

/// `/tags/{slug}` のタグ。無い・タグを無効にしているときは `None`
async fn find_tag(state: &AppState, slug: &str) -> Option<TagRecord> {
    state.tags.as_ref()?;
    state.db.get_tag_by_slug(slug).await.ok().flatten()
}

fn summary_of(state: &AppState, base: &str, record: ContentRecord) -> ContentSummary {
    let blocks: Vec<aegis_ast::BlockNode> =
        serde_json::from_str(&record.blocks_json).unwrap_or_default();
    let url = format!(
        "{}{}",
        base,
        public_content_path(state, &record.content_type, &record.slug)
    );
    let mut fields = record.custom_fields;
    fields.insert("url".to_string(), json!(url));
    ContentSummary {
        id: record.id,
        content_type: record.content_type,
        title: record.title,
        slug: record.slug,
        excerpt: extract_excerpt(&blocks, 140),
        author_id: record.author_id,
        published_at: record.published_at,
        created_at: record.created_at,
        fields,
        relations: Default::default(),
    }
}

/// `GET /tags/{slug}` — タグが付いた公開中のコンテンツ（記事・固定ページ・商品）を新しい順に並べる
pub async fn render_tag(
    State(state): State<Arc<AppState>>,
    Path(slug): Path<String>,
    Query(params): Query<IndexParams>,
) -> Response {
    let Some(tag) = find_tag(&state, &slug).await else {
        return render_not_found(&state, "This tag does not exist.").await;
    };
    let base = base_path(&state);
    let per_page = state.config.theme.posts_per_page.max(1);
    let page = params.page.unwrap_or(1).max(1);

    let records = tagged_records(&state, &tag).await;
    let total_items = records.len() as u64;
    let total_pages = total_items.div_ceil(per_page).max(1);
    if page > total_pages {
        return render_not_found(&state, "This page does not exist.").await;
    }

    let mut cache_tags = CacheTags::new();
    cache_tags.add_url(&format!("/tags/{}", tag.slug));
    cache_tags.add_dependency("site", "global");
    cache_tags.add_dependency("tag", &tag.id);
    cache_tags.add_dependency("tag", &tag.slug);
    for content_type in listed_types(&state) {
        cache_tags.add_dependency("collection", &content_type);
    }
    let offset = usize::try_from((page - 1).saturating_mul(per_page)).unwrap_or(usize::MAX);
    let take = usize::try_from(per_page).unwrap_or(usize::MAX);
    let items: Vec<ContentSummary> = records
        .into_iter()
        .skip(offset)
        .take(take)
        .map(|record| {
            cache_tags.add_dependency(&record.content_type, &record.id);
            cache_tags.add_dependency(&record.content_type, &record.slug);
            summary_of(&state, &base, record)
        })
        .collect();

    let page_url = |n: u64| {
        if n <= 1 {
            format!("{}/tags/{}", base, tag.slug)
        } else {
            format!("{}/tags/{}?page={}", base, tag.slug, n)
        }
    };
    let pagination = Pagination {
        page,
        per_page,
        total_items,
        total_pages,
        prev_url: (page > 1).then(|| page_url(page - 1)),
        next_url: (page < total_pages).then(|| page_url(page + 1)),
    };

    let site = render::site_page_context(&state);
    let mut extra_head = site.extra_head.clone();
    extra_head.push_str(&crate::handlers::feed::feed_link_tag(
        &format!("{}: {}", state.site().title, tag.name),
        &format!("{}/tags/{}/feed.xml", base, tag.slug),
    ));
    let mut fields = HashMap::new();
    if let Some(description) = &tag.description {
        fields.insert("description".to_string(), json!(description));
    }
    let mut ctx = PageRenderContext {
        template: "tag.html".to_string(),
        content_type: "tag".to_string(),
        title: tag.name.clone(),
        slug: tag.slug.clone(),
        fields,
        items,
        pagination: Some(pagination),
        extra_head,
        ..site
    };
    {
        let supervisor = state.supervisor.read().await;
        if let Ok(hooked) = supervisor.apply_render_page(ctx.clone()).await {
            ctx = hooked;
        }
    }
    let resp = themed_response(
        state
            .theme
            .render(&ctx.template, &ctx)
            .map_err(|e| e.to_string()),
    );
    crate::cdn::apply_cache_tags(&state, resp, &cache_tags)
}

/// `GET /tags` — 公開中のコンテンツに付いているタグの一覧（名前順）
pub async fn render_tags_index(State(state): State<Arc<AppState>>) -> Response {
    if state.tags.is_none() {
        return render_not_found(&state, "Tags are not enabled.").await;
    }
    let base = base_path(&state);
    let mut cache_tags = CacheTags::new();
    cache_tags.add_url("/tags");
    cache_tags.add_dependency("site", "global");
    cache_tags.add_dependency("collection", "tags");
    for content_type in listed_types(&state) {
        cache_tags.add_dependency("collection", &content_type);
    }
    let items: Vec<ContentSummary> = tags_in_use(&state)
        .await
        .into_iter()
        .map(|(tag, count)| {
            cache_tags.add_dependency("tag", &tag.id);
            let mut fields = HashMap::new();
            fields.insert(
                "url".to_string(),
                json!(format!("{}/tags/{}", base, tag.slug)),
            );
            fields.insert("count".to_string(), json!(count));
            if let Some(description) = &tag.description {
                fields.insert("description".to_string(), json!(description));
            }
            ContentSummary {
                id: tag.id,
                content_type: "tag".to_string(),
                title: tag.name,
                slug: tag.slug,
                fields,
                ..Default::default()
            }
        })
        .collect();

    let site = render::site_page_context(&state);
    let mut ctx = PageRenderContext {
        template: "tags.html".to_string(),
        content_type: "tags".to_string(),
        title: "Tags".to_string(),
        slug: "tags".to_string(),
        items,
        ..site
    };
    {
        let supervisor = state.supervisor.read().await;
        if let Ok(hooked) = supervisor.apply_render_page(ctx.clone()).await {
            ctx = hooked;
        }
    }
    let resp = themed_response(
        state
            .theme
            .render(&ctx.template, &ctx)
            .map_err(|e| e.to_string()),
    );
    crate::cdn::apply_cache_tags(&state, resp, &cache_tags)
}

/// `GET /tags/{slug}/feed.xml` — タグが付いた記事の Atom フィード
pub async fn get_tag_feed(
    State(state): State<Arc<AppState>>,
    Path(slug): Path<String>,
    headers: HeaderMap,
) -> Response {
    let Some(tag) = find_tag(&state, &slug).await else {
        return (StatusCode::NOT_FOUND, "Tag not found").into_response();
    };
    let ids: HashSet<String> = tagged_records(&state, &tag)
        .await
        .into_iter()
        .filter(|r| r.content_type == "article")
        .map(|r| r.id)
        .collect();
    let mut cache_tags = CacheTags::new();
    cache_tags.add_url(&format!("/tags/{}/feed.xml", tag.slug));
    cache_tags.add_dependency("tag", &tag.id);
    cache_tags.add_dependency("tag", &tag.slug);
    let title = format!("{}: {}", state.site().title, tag.name);
    crate::handlers::feed::build_feed(
        &state,
        &headers,
        &title,
        &format!("/tags/{}/feed.xml", tag.slug),
        &format!("/tags/{}", tag.slug),
        Some(&ArticleFilter::Ids(ids)),
        cache_tags,
    )
    .await
}
