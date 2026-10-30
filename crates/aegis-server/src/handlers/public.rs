use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::{Html, IntoResponse, Response},
};
use std::sync::Arc;

use crate::handlers::comments;
use crate::handlers::tags_public::TagLookup;
use crate::render;
use crate::state::AppState;
use aegis_ast::BlockNode;
use aegis_core::{ContentSummary, PageRenderContext, Pagination};
use aegis_db::{resolve_relations_for_records, ContentListQuery, ContentPage};
use aegis_schema::SchemaDefinition;
use aegis_theme::FrontPage;
use serde::Deserialize;
use std::collections::{HashMap, HashSet};

use crate::handlers::content::is_currently_published;

/// ブロックノードから先頭段落を抽出して抜粋を生成（インライン書式の記号は除く）
pub(crate) fn extract_excerpt(blocks: &[BlockNode], max_chars: usize) -> String {
    for block in blocks {
        if block.name == "core/paragraph" {
            if let Some(ref text) = block.text {
                let plain = aegis_ast::plain_text(text);
                let trimmed = plain.trim();
                if !trimmed.is_empty() {
                    if trimmed.chars().count() > max_chars {
                        let mut s: String = trimmed.chars().take(max_chars).collect();
                        s.push_str("...");
                        return s;
                    }
                    return trimmed.to_string();
                }
            }
        }
    }
    String::new()
}

/// 404 Not Found ページをテーマテンプレートでレンダリング
pub async fn render_not_found(state: &AppState, message: &str) -> Response {
    let ctx = PageRenderContext {
        template: "404.html".to_string(),
        content_type: "error".to_string(),
        title: "Page Not Found".to_string(),
        slug: "404".to_string(),
        content_html: format!("<p>{}</p>", message),
        ..render::site_page_context(state)
    };

    let supervisor = state.supervisor.read().await;
    let final_ctx = supervisor.apply_render_page(ctx).await.unwrap_or_default();

    let rendered = state
        .theme
        .render(&final_ctx.template, &final_ctx)
        .unwrap_or_else(|_| "<h1>404 Not Found</h1>".to_string());

    (StatusCode::NOT_FOUND, Html(rendered)).into_response()
}

#[derive(Debug, Default, Deserialize)]
pub struct IndexParams {
    /// 1 始まりのページ番号（`[theme].posts_per_page` 件ずつ）
    pub page: Option<u64>,
}

/// 記事一覧を絞り込む条件
#[derive(Debug, Clone)]
pub(crate) enum ArticleFilter {
    /// この関係フィールドがこの値の記事だけ（カテゴリの記事一覧）
    Field(String, String),
    /// この ID の記事だけ（タグの記事一覧）
    Ids(HashSet<String>),
}

impl ArticleFilter {
    fn matches(&self, record: &aegis_db::ContentRecord) -> bool {
        match self {
            Self::Field(field, value) => {
                record.custom_fields.get(field).and_then(|v| v.as_str()) == Some(value.as_str())
            }
            Self::Ids(ids) => ids.contains(&record.id),
        }
    }
}

/// 公開ページのパス（`base_path` を除く）。コンテンツの種類ごとの公開ページの URL の決まりを 1 か所に置く
pub(crate) fn public_content_path(state: &AppState, content_type: &str, slug: &str) -> String {
    match content_type {
        "article" => format!("/articles/{}", slug),
        "page" => match state.config.theme.front_page() {
            FrontPage::Page(front) if front == slug => "/".to_string(),
            _ => format!("/{}", slug),
        },
        t if t == state.config.commerce.product_type => format!("/products/{}", slug),
        _ => format!("/{}", slug),
    }
}

/// 記事一覧のページ（トップ・`/articles`・カテゴリ）ごとに違う部分
struct ArticleListView {
    template: &'static str,
    content_type: &'static str,
    title: String,
    slug: String,
    fields: HashMap<String, serde_json::Value>,
    /// ページ送りの URL のパス（`base_path` を除く。例: `/articles`）
    path: String,
    /// 表示するページ（1 始まり）
    page: u64,
    /// 並べる記事の絞り込み（カテゴリの記事一覧）
    filter: Option<ArticleFilter>,
    /// `<head>` に足す HTML（カテゴリのフィードへのリンクなど）
    extra_head: Option<String>,
    tags: crate::cdn::CacheTags,
}

impl ArticleListView {
    /// 記事一覧（`index.html`）
    fn index(path: &str, page: Option<u64>) -> Self {
        let mut tags = crate::cdn::CacheTags::new();
        tags.add_url("/");
        tags.add_url("/articles");
        Self {
            template: "index.html",
            content_type: "index",
            title: "Latest Stories".to_string(),
            slug: String::new(),
            fields: HashMap::new(),
            path: path.to_string(),
            page: page.unwrap_or(1).max(1),
            filter: None,
            extra_head: None,
            tags,
        }
    }
}

/// トップページ (`/`)。`[theme].front_page` で記事一覧（`?page=N` でページ送り）・
/// テーマの `front.html`・固定ページを切り替える
pub async fn render_index(
    State(state): State<Arc<AppState>>,
    Query(params): Query<IndexParams>,
) -> Response {
    match state.config.theme.front_page() {
        FrontPage::Posts => {
            render_article_list(&state, ArticleListView::index("/", params.page)).await
        }
        FrontPage::Template => {
            // 最新の記事を渡す。続きのページ送りは /articles に向ける
            let view = ArticleListView {
                template: "front.html",
                content_type: "front",
                title: String::new(),
                ..ArticleListView::index("/articles", None)
            };
            render_article_list(&state, view).await
        }
        FrontPage::Page(slug) => match render_fixed_page(&state, &slug, "/").await {
            Some(resp) => resp,
            None => {
                tracing::warn!(
                    "[theme].front_page: page '{}' is not published; showing the article list",
                    slug
                );
                render_article_list(&state, ArticleListView::index("/", params.page)).await
            }
        },
    }
}

/// 記事一覧 (`/articles`、`?page=N` でページ送り)
pub async fn render_articles(
    State(state): State<Arc<AppState>>,
    Query(params): Query<IndexParams>,
) -> Response {
    render_article_list(&state, ArticleListView::index("/articles", params.page)).await
}

/// カテゴリの記事一覧 (`/category/{slug}`、`?page=N` でページ送り)。記事のスキーマで `category` を
/// 指す関係フィールドの値で絞り込み、テンプレート `category.html` で描画する
pub async fn render_category(
    State(state): State<Arc<AppState>>,
    Path(slug): Path<String>,
    Query(params): Query<IndexParams>,
) -> Response {
    let Some((category, field)) = find_category(&state, &slug).await else {
        return render_not_found(&state, "This category does not exist.").await;
    };
    let base_path = state
        .config
        .server
        .normalized_base_path()
        .unwrap_or_default();

    let mut tags = crate::cdn::CacheTags::new();
    tags.add_url(&format!("/category/{}", category.slug));
    tags.add_dependency("category", &category.id);
    tags.add_dependency("category", &category.slug);

    let mut extra_head = render::site_page_context(&state).extra_head;
    extra_head.push_str(&crate::handlers::feed::feed_link_tag(
        &format!("{}: {}", state.site().title, category.title),
        &format!("{}/category/{}/feed.xml", base_path, category.slug),
    ));

    let view = ArticleListView {
        template: "category.html",
        content_type: "category",
        title: category.title,
        slug: category.slug.clone(),
        fields: category.custom_fields,
        path: format!("/category/{}", category.slug),
        page: params.page.unwrap_or(1).max(1),
        filter: Some(ArticleFilter::Field(field.0, field.1)),
        extra_head: Some(extra_head),
        tags,
    };
    render_article_list(&state, view).await
}

/// 公開中のカテゴリと、記事をそのカテゴリで絞り込む条件（記事の関係フィールド名と値）
pub(crate) async fn find_category(
    state: &AppState,
    slug: &str,
) -> Option<(aegis_db::ContentRecord, (String, String))> {
    let (category_schema, article_schema) = {
        let schemas = state.schemas.read().await;
        (
            schemas.get("category").cloned()?,
            schemas.get("article").cloned()?,
        )
    };
    // 記事のスキーマの `target: category.<フィールド>` の関係フィールド（`category_id` など）
    let (field, target_field) = article_schema.fields.iter().find_map(|f| {
        if f.field_type != aegis_schema::FieldType::Relation {
            return None;
        }
        let (target_type, target_field) = f
            .target
            .as_deref()?
            .split_once('.')
            .unwrap_or((f.target.as_deref()?, "id"));
        (target_type == "category").then(|| (f.name.clone(), target_field.to_string()))
    })?;

    let record = state
        .db
        .get_published_content(&category_schema, slug)
        .await
        .ok()
        .flatten()
        .filter(|r| {
            is_currently_published(&r.status, r.unpublish_at.as_deref(), chrono::Utc::now())
        })?;
    let value = match target_field.as_str() {
        "id" => record.id.clone(),
        "slug" => record.slug.clone(),
        other => record.custom_fields.get(other)?.as_str()?.to_string(),
    };
    Some((record, (field, value)))
}

/// 公開中の記事を新しい順に 1 ページ分（`filter` があれば絞り込んで）取得する。
/// 戻り値は（そのページの記事、条件に合う記事の総数）
pub(crate) async fn list_published_articles(
    state: &AppState,
    schema: &SchemaDefinition,
    filter: Option<&ArticleFilter>,
    limit: u64,
    offset: u64,
) -> aegis_core::Result<ContentPage> {
    let status = Some("published".to_string());
    let Some(filter) = filter else {
        let query = ContentListQuery {
            status,
            limit: Some(limit),
            offset,
            ..Default::default()
        };
        return state.db.list_published_contents_page(schema, &query).await;
    };
    // 公開版の一覧は DB からまとめて読んでから並べ替えるので、絞り込みもその後で行う
    let query = ContentListQuery {
        status,
        ..ContentListQuery::default()
    };
    let all = state
        .db
        .list_published_contents_page(schema, &query)
        .await?;
    let matching: Vec<_> = all
        .records
        .into_iter()
        .filter(|r| filter.matches(r))
        .collect();
    Ok(ContentPage {
        total: matching.len() as u64,
        records: matching
            .into_iter()
            .skip(usize::try_from(offset).unwrap_or(usize::MAX))
            .take(usize::try_from(limit).unwrap_or(usize::MAX))
            .collect(),
    })
}

/// 公開中の記事の一覧ページを描画する
async fn render_article_list(state: &AppState, view: ArticleListView) -> Response {
    let per_page = state.config.theme.posts_per_page.max(1);
    let page = view.page;
    let mut items = Vec::new();
    let mut total_items = 0;
    let mut tags = view.tags;
    tags.add_dependency("site", "global");
    tags.add_dependency("collection", "article");

    let article_schema = {
        let schemas = state.schemas.read().await;
        schemas.get("article").cloned()
    };
    if let Some(schema) = article_schema {
        let offset = (page - 1).saturating_mul(per_page);
        match list_published_articles(state, &schema, view.filter.as_ref(), per_page, offset).await
        {
            Ok(content_page) => {
                total_items = content_page.total;
                let relations = {
                    let schemas = state.schemas.read().await;
                    let fields: Vec<_> = content_page
                        .records
                        .iter()
                        .map(|r| &r.custom_fields)
                        .collect();
                    resolve_relations_for_records(&*state.db, &schemas, &schema, &fields).await
                };
                let ids: Vec<String> = content_page.records.iter().map(|r| r.id.clone()).collect();
                let tag_lookup = TagLookup::load(state, "article", &ids).await;
                for (r, relations) in content_page.records.into_iter().zip(relations) {
                    tags.add_dependency("article", &r.id);
                    tags.add_dependency("article", &r.slug);
                    crate::cdn::add_relation_tags(&mut tags, &relations);
                    let blocks: Vec<BlockNode> =
                        serde_json::from_str(&r.blocks_json).unwrap_or_default();
                    let mut fields = r.custom_fields;
                    tag_lookup.attach(&r.id, &mut fields, &mut tags);
                    items.push(ContentSummary {
                        id: r.id,
                        content_type: r.content_type,
                        title: r.title,
                        slug: r.slug,
                        excerpt: extract_excerpt(&blocks, 140),
                        author_id: r.author_id,
                        published_at: r.published_at,
                        created_at: r.created_at,
                        fields,
                        relations: relations
                            .iter()
                            .map(|(name, rel)| (name.clone(), render::relation_summary(rel)))
                            .collect(),
                    });
                }
            }
            Err(e) => tracing::error!("Failed to list articles for {}: {}", view.path, e),
        }
    }

    let total_pages = total_items.div_ceil(per_page).max(1);
    if page > total_pages {
        return render_not_found(state, "This page does not exist.").await;
    }
    let base_path = state
        .config
        .server
        .normalized_base_path()
        .unwrap_or_default();
    let page_url = |n: u64| {
        if n <= 1 {
            format!("{}{}", base_path, view.path)
        } else {
            format!("{}{}?page={}", base_path, view.path, n)
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

    let site = render::site_page_context(state);
    let mut ctx = PageRenderContext {
        template: view.template.to_string(),
        content_type: view.content_type.to_string(),
        title: view.title,
        slug: view.slug,
        fields: view.fields,
        items,
        pagination: Some(pagination),
        extra_head: view.extra_head.unwrap_or(site.extra_head.clone()),
        ..site
    };

    // プラグインによるテーマ・コンテキスト書き換えフックの適用
    {
        let supervisor = state.supervisor.read().await;
        if let Ok(hooked_ctx) = supervisor.apply_render_page(ctx.clone()).await {
            ctx = hooked_ctx;
        }
    }

    let resp = themed_response(
        state
            .theme
            .render(&ctx.template, &ctx)
            .map_err(|e| e.to_string()),
    );
    crate::cdn::apply_cache_tags(state, resp, &tags)
}

/// 記事詳細ページ (/articles/{slug})
pub async fn render_article(
    State(state): State<Arc<AppState>>,
    Path(slug): Path<String>,
) -> impl IntoResponse {
    let schema = {
        let schemas = state.schemas.read().await;
        schemas.get("article").cloned()
    };

    let schema = match schema {
        Some(s) => s,
        None => return render_not_found(&state, "Article schema not found").await,
    };

    let maybe_record = match state.db.get_published_content(&schema, &slug).await {
        Ok(rec) => rec,
        Err(e) => {
            tracing::error!("Database error fetching article '{}': {}", slug, e);
            return render_not_found(&state, "Error querying database").await;
        }
    };

    let now = chrono::Utc::now();
    let record = match maybe_record {
        Some(r) if is_currently_published(&r.status, r.unpublish_at.as_deref(), now) => r,
        _ => return render_not_found(&state, "The requested article is not published.").await,
    };

    // AST から HTML 変換 & ストリーミングリライター適用
    let blocks: Vec<BlockNode> = serde_json::from_str(&record.blocks_json).unwrap_or_default();
    let content_html = render::render_content_html(&state, &blocks).await;

    let mut tags = crate::cdn::CacheTags::new();
    tags.add_url(&format!("/articles/{}", record.slug));
    tags.add_dependency("site", "global");
    tags.add_dependency("article", &record.id);
    tags.add_dependency("article", &record.slug);
    tags.add_dependency("comments", &record.id);
    crate::cdn::add_field_relation_tags(&mut tags, &schema, &record.custom_fields);

    let comments = comments::page_section(&state, "article", &record.id).await;
    let mut fields = record.custom_fields;
    TagLookup::load(&state, "article", std::slice::from_ref(&record.id))
        .await
        .attach(&record.id, &mut fields, &mut tags);
    let page = render::PageInput {
        template: "article.html".to_string(),
        content_type: "article".to_string(),
        title: record.title,
        slug: record.slug,
        path: None,
        content_html,
        author: record.author_id,
        published_at: record.published_at,
        updated_at: Some(record.updated_at),
        fields,
        comments,
    };
    let resp = themed_response(render::render_page(&state, &schema, page).await);
    crate::cdn::apply_cache_tags(&state, resp, &tags)
}

/// Turns a rendered page (or a template error) into the HTTP response
pub(crate) fn themed_response(result: Result<String, String>) -> Response {
    match result {
        Ok(html) => (StatusCode::OK, Html(html)).into_response(),
        Err(e) => {
            tracing::error!("Template rendering error: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Html(format!("<h1>Internal Server Error</h1><p>{}</p>", e)),
            )
                .into_response()
        }
    }
}

/// 公開中の固定ページ（`page`）を描画する。無い・非公開なら `None`。
/// `path` はそのページを出している URL のパス（`/{slug}`、トップページなら `/`）
async fn render_fixed_page(state: &AppState, slug: &str, path: &str) -> Option<Response> {
    let schema = {
        let schemas = state.schemas.read().await;
        schemas.get("page").cloned()
    }?;
    let record = state
        .db
        .get_published_content(&schema, slug)
        .await
        .ok()
        .flatten()
        .filter(|r| {
            is_currently_published(&r.status, r.unpublish_at.as_deref(), chrono::Utc::now())
        })?;

    let blocks: Vec<BlockNode> = serde_json::from_str(&record.blocks_json).unwrap_or_default();
    let content_html = render::render_content_html(state, &blocks).await;

    let comments = comments::page_section(state, "page", &record.id).await;
    let template = match record.custom_fields.get("layout").and_then(|v| v.as_str()) {
        Some("blank") => "blank.html".to_string(),
        Some(custom) if !custom.trim().is_empty() && custom != "default" => {
            let c = custom.trim();
            if c.ends_with(".html") {
                c.to_string()
            } else {
                format!("{}.html", c)
            }
        }
        _ => "page.html".to_string(),
    };
    let mut tags = crate::cdn::CacheTags::new();
    tags.add_url(&format!("/{}", record.slug));
    tags.add_url(path);
    tags.add_dependency("site", "global");
    tags.add_dependency("page", &record.id);
    tags.add_dependency("page", &record.slug);
    tags.add_dependency("comments", &record.id);
    crate::cdn::add_field_relation_tags(&mut tags, &schema, &record.custom_fields);

    let mut fields = record.custom_fields;
    TagLookup::load(state, "page", std::slice::from_ref(&record.id))
        .await
        .attach(&record.id, &mut fields, &mut tags);
    let page = render::PageInput {
        template,
        content_type: "page".to_string(),
        title: record.title,
        slug: record.slug,
        path: Some(path.to_string()),
        content_html,
        author: record.author_id,
        published_at: record.published_at,
        updated_at: Some(record.updated_at),
        fields,
        comments,
    };
    let resp = themed_response(render::render_page(state, &schema, page).await);
    Some(crate::cdn::apply_cache_tags(state, resp, &tags))
}

/// 固定ページまたはスラッグの汎用ルーティング (/{slug})
pub async fn render_page_or_slug(
    State(state): State<Arc<AppState>>,
    Path(slug): Path<String>,
) -> impl IntoResponse {
    // 1. 固定ページ。トップページに出しているページは、canonical などの URL を `/` にする
    let path = match state.config.theme.front_page() {
        FrontPage::Page(front) if front == slug => "/".to_string(),
        _ => format!("/{}", slug),
    };
    if let Some(resp) = render_fixed_page(&state, &slug, &path).await {
        return resp;
    }

    // 2. article スキーマへのフォールバック
    let article_schema = {
        let schemas = state.schemas.read().await;
        schemas.get("article").cloned()
    };

    let now = chrono::Utc::now();
    if let Some(schema) = article_schema {
        if let Ok(Some(record)) = state.db.get_published_content(&schema, &slug).await {
            if is_currently_published(&record.status, record.unpublish_at.as_deref(), now) {
                let blocks: Vec<BlockNode> =
                    serde_json::from_str(&record.blocks_json).unwrap_or_default();
                let content_html = render::render_content_html(&state, &blocks).await;

                let mut tags = crate::cdn::CacheTags::new();
                tags.add_url(&format!("/{}", record.slug));
                tags.add_url(&format!("/articles/{}", record.slug));
                tags.add_dependency("site", "global");
                tags.add_dependency("article", &record.id);
                tags.add_dependency("article", &record.slug);
                tags.add_dependency("comments", &record.id);
                crate::cdn::add_field_relation_tags(&mut tags, &schema, &record.custom_fields);

                let comments = comments::page_section(&state, "article", &record.id).await;
                let page = render::PageInput {
                    template: "article.html".to_string(),
                    content_type: "article".to_string(),
                    title: record.title,
                    slug: record.slug,
                    path: None,
                    content_html,
                    author: record.author_id,
                    published_at: record.published_at,
                    updated_at: Some(record.updated_at),
                    fields: record.custom_fields,
                    comments,
                };
                let resp = themed_response(render::render_page(&state, &schema, page).await);
                return crate::cdn::apply_cache_tags(&state, resp, &tags);
            }
        }
    }

    render_not_found(&state, &format!("Content '{}' not found", slug)).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use aegis_auth::SessionService;
    use aegis_cache::MemoryCacheBackend;
    use aegis_db::{ContentRecord, DatabaseAdapter, SqliteAdapter};
    use aegis_storage::LocalStorageBackend;
    use std::collections::HashMap;

    use crate::config::AegisConfig;
    use aegis_core::{
        PluginCapabilities, PluginManifest, PluginResourceLimits, Result as AegisResult,
    };
    use aegis_runtime::{AegisPlugin, PluginSupervisor};
    use aegis_schema::{FieldDefinition, FieldType, SchemaDefinition};
    use aegis_theme::ThemeEngine;
    use axum::response::IntoResponse;
    use sqlx::sqlite::SqlitePoolOptions;
    use std::path::PathBuf;

    struct TestThemePlugin {
        manifest: PluginManifest,
    }

    #[async_trait::async_trait]
    impl AegisPlugin for TestThemePlugin {
        fn manifest(&self) -> &PluginManifest {
            &self.manifest
        }

        async fn on_render_page(
            &self,
            mut ctx: PageRenderContext,
        ) -> AegisResult<PageRenderContext> {
            ctx.site_title = format!("{} [Cyberpunk Neon]", ctx.site_title);
            ctx.extra_css
                .push_str("body { background: #000; --accent: #00ffcc; }");
            Ok(ctx)
        }
    }

    async fn create_test_state(supervisor: PluginSupervisor) -> Arc<AppState> {
        let mut config = AegisConfig::default();
        config.theme.site_title = "AegisCMS Test Site".to_string();
        config.theme.site_description = "Secure sandboxed content platform".to_string();

        let pool = SqlitePoolOptions::new()
            .connect("sqlite::memory:")
            .await
            .expect("Failed to connect to in-memory sqlite");
        let adapter = SqliteAdapter::new(pool);
        adapter.init().await.expect("init should succeed");
        let db = Arc::new(adapter);

        let article_schema = SchemaDefinition {
            content_type: "article".to_string(),
            label: "Articles".to_string(),
            description: None,
            fields: vec![FieldDefinition {
                name: "title".to_string(),
                field_type: FieldType::String,
                required: true,
                nullable: false,
                unique: false,
                index: true,
                max_length: Some(255),
                target: None,
                indexed_paths: vec![],
            }],
        };

        let page_schema = SchemaDefinition {
            content_type: "page".to_string(),
            label: "Pages".to_string(),
            description: None,
            fields: vec![FieldDefinition {
                name: "layout".to_string(),
                field_type: FieldType::String,
                required: false,
                nullable: true,
                unique: false,
                index: false,
                max_length: Some(100),
                target: None,
                indexed_paths: vec![],
            }],
        };

        let mut schemas = HashMap::new();
        schemas.insert("article".to_string(), article_schema.clone());
        schemas.insert("page".to_string(), page_schema);

        db.run_migrations(&schemas)
            .await
            .expect("Migrations failed");

        let storage = Arc::new(LocalStorageBackend::new(
            PathBuf::from("target/test_uploads"),
            "/uploads".to_string(),
        ));
        let cache = Arc::new(MemoryCacheBackend::new());
        let sessions = Arc::new(SessionService::new(cache.clone(), 3600));
        let audit = Arc::new(aegis_audit::AuditService::from_config(
            &config.audit,
            db.clone(),
        ));
        let theme_engine = Arc::new(ThemeEngine::new(&PathBuf::from("themes"), "default"));

        let auth_provider = Arc::new(aegis_auth::LocalAuthProvider::new(db.clone()));
        let login_limiter = Arc::new(aegis_auth::LoginRateLimiter::new(
            cache.clone(),
            config.auth.login_rate_limit.clone(),
        ));

        Arc::new(AppState::new(
            config,
            db,
            storage,
            cache,
            sessions,
            auth_provider,
            login_limiter,
            schemas,
            supervisor,
            audit,
            theme_engine,
        ))
    }

    #[tokio::test]
    async fn test_render_index_paginates_published_posts() {
        let mut config = AegisConfig::default();
        config.theme.posts_per_page = 2;
        config.server.base_path = "/cms".to_string();
        let state =
            crate::test_support::test_state_with_config(config, PluginSupervisor::new()).await;
        let schema = state.schemas.read().await["article"].clone();
        for n in 1..=4 {
            let status = if n == 2 { "draft" } else { "published" };
            state
                .db
                .insert_content(
                    &schema,
                    &ContentRecord {
                        id: format!("id-{}", n),
                        content_type: "article".to_string(),
                        title: format!("Post {}", n),
                        slug: format!("post-{}", n),
                        status: status.to_string(),
                        blocks_json: "[]".to_string(),
                        author_id: None,
                        published_at: None,
                        unpublish_at: None,
                        created_at: format!("2026-10-22T00:00:0{}Z", n),
                        updated_at: format!("2026-10-22T00:00:0{}Z", n),
                        custom_fields: HashMap::new(),
                    },
                )
                .await
                .unwrap();
        }
        let page = |n: Option<u64>| {
            let state = state.clone();
            async move {
                let resp = render_index(State(state), Query(IndexParams { page: n })).await;
                let status = resp.status();
                let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
                    .await
                    .unwrap();
                // テンプレートは URL の `/` を `&#x2f;` にエスケープする（ブラウザでは同じ URL）
                let html = String::from_utf8(body.to_vec())
                    .unwrap()
                    .replace("&#x2f;", "/");
                (status, html)
            }
        };

        // 公開済み 3 件（post-4, post-3, post-1）を 2 件ずつ。下書きは含めない
        let (status, first) = page(None).await;
        assert_eq!(status, StatusCode::OK);
        assert!(first.contains("/cms/articles/post-4") && first.contains("/cms/articles/post-3"));
        assert!(!first.contains("post-1") && !first.contains("post-2"));
        assert!(
            first.contains(r#"href="/cms/?page=2" rel="next""#),
            "{}",
            first
        );
        assert!(!first.contains(r#"rel="prev""#));
        assert!(first.contains("3 posts"));

        let (_, second) = page(Some(2)).await;
        assert!(second.contains("/cms/articles/post-1") && !second.contains("post-4"));
        assert!(second.contains(r#"href="/cms/" rel="prev""#));
        assert!(!second.contains(r#"rel="next""#));

        let (status, _) = page(Some(3)).await;
        assert_eq!(status, StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn test_render_index_empty_and_with_published_post() {
        let state = create_test_state(PluginSupervisor::new()).await;

        // 1. 記事がない状態のトップページ
        let resp = render_index(State(state.clone()), Query(IndexParams::default()))
            .await
            .into_response();
        assert_eq!(resp.status(), StatusCode::OK);
        let body_bytes = axum::body::to_bytes(resp.into_body(), 100_000)
            .await
            .unwrap();
        let html = String::from_utf8(body_bytes.to_vec()).unwrap();
        assert!(html.contains("AegisCMS Test Site"));
        assert!(html.contains("No Published Content Yet"));

        // 2. 公開記事と下書き記事を挿入
        let schema = {
            let s = state.schemas.read().await;
            s.get("article").unwrap().clone()
        };

        let pub_blocks = vec![
            BlockNode::heading(1, "Public Post Title"),
            BlockNode::paragraph("This is the lead summary of the public article."),
        ];
        let pub_record = ContentRecord {
            id: "art-1".to_string(),
            content_type: "article".to_string(),
            title: "Public Post Title".to_string(),
            slug: "public-post".to_string(),
            status: "published".to_string(),
            blocks_json: serde_json::to_string(&pub_blocks).unwrap(),
            author_id: Some("takumi".to_string()),
            published_at: Some("2026-10-22T00:00:00Z".to_string()),
            unpublish_at: None,
            created_at: "2026-10-22T00:00:00Z".to_string(),
            updated_at: "2026-10-22T00:00:00Z".to_string(),
            custom_fields: HashMap::new(),
        };
        state.db.insert_content(&schema, &pub_record).await.unwrap();

        let draft_record = ContentRecord {
            id: "art-2".to_string(),
            content_type: "article".to_string(),
            title: "Draft Secret Post".to_string(),
            slug: "draft-secret".to_string(),
            status: "draft".to_string(),
            blocks_json: "[]".to_string(),
            author_id: None,
            published_at: None,
            unpublish_at: None,
            created_at: "2026-10-22T00:00:00Z".to_string(),
            updated_at: "2026-10-22T00:00:00Z".to_string(),
            custom_fields: HashMap::new(),
        };
        state
            .db
            .insert_content(&schema, &draft_record)
            .await
            .unwrap();

        // 3. 再度トップページを取得
        let resp = render_index(State(state.clone()), Query(IndexParams::default()))
            .await
            .into_response();
        assert_eq!(resp.status(), StatusCode::OK);
        let body_bytes = axum::body::to_bytes(resp.into_body(), 100_000)
            .await
            .unwrap();
        let html = String::from_utf8(body_bytes.to_vec()).unwrap();

        // 公開記事は含まれ、下書き記事は含まれないこと
        assert!(html.contains("Public Post Title"));
        assert!(html.contains("/articles/public-post"));
        assert!(html.contains("This is the lead summary"));
        assert!(!html.contains("Draft Secret Post"));
    }

    #[tokio::test]
    async fn test_render_article_published_vs_draft() {
        let state = create_test_state(PluginSupervisor::new()).await;
        let schema = {
            let s = state.schemas.read().await;
            s.get("article").unwrap().clone()
        };

        let blocks = vec![
            BlockNode::heading(1, "Deep Dive Architecture"),
            BlockNode::paragraph("Detailed explanation of sandboxed Wasm plugins."),
        ];
        let pub_record = ContentRecord {
            id: "art-pub".to_string(),
            content_type: "article".to_string(),
            title: "Deep Dive Architecture".to_string(),
            slug: "deep-dive".to_string(),
            status: "published".to_string(),
            blocks_json: serde_json::to_string(&blocks).unwrap(),
            author_id: Some("takumi".to_string()),
            published_at: Some("2026-10-22T00:00:00Z".to_string()),
            unpublish_at: None,
            created_at: "2026-10-22T00:00:00Z".to_string(),
            updated_at: "2026-10-22T00:00:00Z".to_string(),
            custom_fields: HashMap::new(),
        };
        state.db.insert_content(&schema, &pub_record).await.unwrap();

        let draft_record = ContentRecord {
            id: "art-draft".to_string(),
            content_type: "article".to_string(),
            title: "Unpublished Idea".to_string(),
            slug: "unpublished-idea".to_string(),
            status: "draft".to_string(),
            blocks_json: "[]".to_string(),
            author_id: None,
            published_at: None,
            unpublish_at: None,
            created_at: "2026-10-22T00:00:00Z".to_string(),
            updated_at: "2026-10-22T00:00:00Z".to_string(),
            custom_fields: HashMap::new(),
        };
        state
            .db
            .insert_content(&schema, &draft_record)
            .await
            .unwrap();

        // 1. 公開記事の取得 -> 200 OK
        let resp = render_article(State(state.clone()), Path("deep-dive".to_string()))
            .await
            .into_response();
        assert_eq!(resp.status(), StatusCode::OK);
        let bytes = axum::body::to_bytes(resp.into_body(), 100_000)
            .await
            .unwrap();
        let html = String::from_utf8(bytes.to_vec()).unwrap();
        assert!(html.contains("Deep Dive Architecture"));
        assert!(html.contains("Detailed explanation of sandboxed Wasm plugins."));

        // 2. 下書き記事の取得 -> 404 NOT_FOUND
        let resp = render_article(State(state.clone()), Path("unpublished-idea".to_string()))
            .await
            .into_response();
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
        let bytes = axum::body::to_bytes(resp.into_body(), 100_000)
            .await
            .unwrap();
        let html = String::from_utf8(bytes.to_vec()).unwrap();
        assert!(html.contains("Page Not Found"));

        // 3. 存在しないスラッグ -> 404 NOT_FOUND
        let resp = render_article(State(state.clone()), Path("non-existent".to_string()))
            .await
            .into_response();
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn test_plugin_theme_rewrite_and_template_override() {
        let mut supervisor = PluginSupervisor::new();
        let plugin = Arc::new(TestThemePlugin {
            manifest: PluginManifest {
                name: "theme-neon".to_string(),
                version: "1.0.0".to_string(),
                entrypoint: "plugin.wasm".to_string(),
                capabilities: PluginCapabilities {
                    embed: None,
                    network: None,
                    storage: None,
                    hooks: vec!["on-render-page".to_string()],
                },
                resource_limits: PluginResourceLimits::default(),
                ..Default::default()
            },
        });
        supervisor.register(plugin);

        let state = create_test_state(supervisor).await;

        // プラグインによるテンプレートの動的オーバーライド
        state.theme.register_template(
            "article.html",
            r#"<div class="neon-wrapper"><h1>{{ title }}</h1><div class="content">{{ content_html | safe }}</div><style>{{ extra_css | safe }}</style></div>"#,
        );

        let schema = {
            let s = state.schemas.read().await;
            s.get("article").unwrap().clone()
        };

        let blocks = vec![BlockNode::paragraph("Cyberpunk aesthetic content.")];
        let record = ContentRecord {
            id: "art-cyber".to_string(),
            content_type: "article".to_string(),
            title: "Neon Horizon".to_string(),
            slug: "neon-horizon".to_string(),
            status: "published".to_string(),
            blocks_json: serde_json::to_string(&blocks).unwrap(),
            author_id: Some("cyber-author".to_string()),
            published_at: Some("2026-10-22T00:00:00Z".to_string()),
            unpublish_at: None,
            created_at: "2026-10-22T00:00:00Z".to_string(),
            updated_at: "2026-10-22T00:00:00Z".to_string(),
            custom_fields: HashMap::new(),
        };
        state.db.insert_content(&schema, &record).await.unwrap();

        // 1. トップページでプラグインの on-render-page によるタイトル書き換えとCSS注入を検証
        let resp = render_index(State(state.clone()), Query(IndexParams::default()))
            .await
            .into_response();
        assert_eq!(resp.status(), StatusCode::OK);
        let bytes = axum::body::to_bytes(resp.into_body(), 100_000)
            .await
            .unwrap();
        let html = String::from_utf8(bytes.to_vec()).unwrap();
        assert!(html.contains("AegisCMS Test Site [Cyberpunk Neon]"));
        assert!(html.contains("--accent: #00ffcc"));

        // 2. 記事ページで動的オーバーライドされたテンプレートが適用されていることを検証
        let resp = render_article(State(state.clone()), Path("neon-horizon".to_string()))
            .await
            .into_response();
        assert_eq!(resp.status(), StatusCode::OK);
        let bytes = axum::body::to_bytes(resp.into_body(), 100_000)
            .await
            .unwrap();
        let html = String::from_utf8(bytes.to_vec()).unwrap();
        assert!(html.contains(r#"<div class="neon-wrapper">"#));
        assert!(html.contains("Cyberpunk aesthetic content."));
        assert!(html.contains("--accent: #00ffcc"));
    }

    #[tokio::test]
    async fn test_render_page_or_slug_with_layouts() {
        let supervisor = PluginSupervisor::new();
        let state = create_test_state(supervisor).await;

        let page_schema = {
            let s = state.schemas.read().await;
            s.get("page").unwrap().clone()
        };

        // 1. テーマ標準レイアウトの固定ページ (layout: "default")
        let standard_page = ContentRecord {
            id: "page-standard".to_string(),
            content_type: "page".to_string(),
            title: "About Us".to_string(),
            slug: "about".to_string(),
            status: "published".to_string(),
            blocks_json: serde_json::to_string(&[BlockNode::paragraph("About company text.")])
                .unwrap(),
            author_id: None,
            published_at: Some("2026-10-22T00:00:00Z".to_string()),
            unpublish_at: None,
            created_at: "2026-10-22T00:00:00Z".to_string(),
            updated_at: "2026-10-22T00:00:00Z".to_string(),
            custom_fields: HashMap::from([("layout".to_string(), serde_json::json!("default"))]),
        };
        state
            .db
            .insert_content(&page_schema, &standard_page)
            .await
            .unwrap();

        // 2. ブランク/フル幅LPレイアウトの固定ページ (layout: "blank")
        let blank_page = ContentRecord {
            id: "page-lp".to_string(),
            content_type: "page".to_string(),
            title: "Special LP".to_string(),
            slug: "lp".to_string(),
            status: "published".to_string(),
            blocks_json: serde_json::to_string(&[BlockNode::new("core/html")]).unwrap(),
            author_id: None,
            published_at: Some("2026-10-22T00:00:00Z".to_string()),
            unpublish_at: None,
            created_at: "2026-10-22T00:00:00Z".to_string(),
            updated_at: "2026-10-22T00:00:00Z".to_string(),
            custom_fields: HashMap::from([("layout".to_string(), serde_json::json!("blank"))]),
        };
        state
            .db
            .insert_content(&page_schema, &blank_page)
            .await
            .unwrap();

        // Standard page は site-header や page-detail を含む
        let resp = render_page_or_slug(State(state.clone()), Path("about".to_string()))
            .await
            .into_response();
        assert_eq!(resp.status(), StatusCode::OK);
        let bytes = axum::body::to_bytes(resp.into_body(), 100_000)
            .await
            .unwrap();
        let html = String::from_utf8(bytes.to_vec()).unwrap();
        assert!(html.contains("About Us"));
        assert!(html.contains("About company text."));
        assert!(html.contains("page-detail"));
        assert!(html.contains("site-header"));

        // Blank LP は site-header や site-footer を含まず、フル幅で出力される
        let resp = render_page_or_slug(State(state.clone()), Path("lp".to_string()))
            .await
            .into_response();
        assert_eq!(resp.status(), StatusCode::OK);
        let bytes = axum::body::to_bytes(resp.into_body(), 100_000)
            .await
            .unwrap();
        let html = String::from_utf8(bytes.to_vec()).unwrap();
        assert!(html.contains("Special LP"));
        assert!(!html.contains("site-header"));
        assert!(!html.contains("site-footer"));
    }

    /// 公開中のコンテンツを 1 件入れる（`n` が大きいほど新しい）
    async fn insert_published(
        state: &AppState,
        content_type: &str,
        slug: &str,
        n: u32,
        blocks: &[BlockNode],
        fields: HashMap<String, serde_json::Value>,
    ) -> String {
        let schema = state.schemas.read().await[content_type].clone();
        let id = format!("{}-{}", content_type, slug);
        state
            .db
            .insert_content(
                &schema,
                &ContentRecord {
                    id: id.clone(),
                    content_type: content_type.to_string(),
                    title: format!("Title of {}", slug),
                    slug: slug.to_string(),
                    status: "published".to_string(),
                    blocks_json: serde_json::to_string(blocks).unwrap(),
                    author_id: None,
                    published_at: Some(format!("2026-10-22T00:00:{:02}Z", n)),
                    unpublish_at: None,
                    created_at: format!("2026-10-22T00:00:{:02}Z", n),
                    updated_at: format!("2026-10-22T00:00:{:02}Z", n),
                    custom_fields: fields,
                },
            )
            .await
            .unwrap();
        id
    }

    async fn body_of(resp: Response) -> (StatusCode, String) {
        let status = resp.status();
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let html = String::from_utf8(body.to_vec())
            .unwrap()
            .replace("&#x2f;", "/");
        (status, html)
    }

    /// `[theme].front_page` と、記事・カテゴリのスキーマ（記事は `category_id` でカテゴリを指す）を持つ状態
    async fn front_page_state(front_page: &str) -> Arc<AppState> {
        let mut config = AegisConfig::default();
        config.theme.front_page = front_page.to_string();
        config.theme.posts_per_page = 2;
        let article: SchemaDefinition = serde_json::from_value(serde_json::json!({
            "content_type": "article",
            "label": "Articles",
            "fields": [
                { "name": "excerpt", "type": "string", "required": false },
                { "name": "category_id", "type": "relation", "target": "category.id", "required": false, "nullable": true }
            ]
        }))
        .unwrap();
        let category: SchemaDefinition = serde_json::from_value(serde_json::json!({
            "content_type": "category",
            "label": "Categories",
            "fields": [{ "name": "description", "type": "text", "nullable": true }]
        }))
        .unwrap();
        Arc::new(
            crate::test_support::test_app_state(
                config,
                PluginSupervisor::new(),
                vec![article, category],
            )
            .await,
        )
    }

    #[tokio::test]
    async fn test_front_page_template_separates_the_article_list() {
        let state = front_page_state("template").await;
        for n in 1..=3 {
            insert_published(
                &state,
                "article",
                &format!("post-{}", n),
                n,
                &[],
                HashMap::new(),
            )
            .await;
        }
        state.theme.register_template(
            "front.html",
            "FRONT[{% for item in items %}{{ item.slug }} {% endfor %}] next={{ pagination.next_url }}",
        );

        // トップはテーマの front.html。最新の記事と、続きのページ（/articles?page=2）を渡す
        let (status, front) =
            body_of(render_index(State(state.clone()), Query(IndexParams::default())).await).await;
        assert_eq!(status, StatusCode::OK);
        assert!(front.contains("FRONT[post-3 post-2 ]"), "{}", front);
        assert!(front.contains("next=/articles?page=2"), "{}", front);

        // /articles は記事一覧（index.html）で、ページ送りも /articles のまま
        let (_, list) = body_of(
            render_articles(State(state.clone()), Query(IndexParams { page: Some(2) })).await,
        )
        .await;
        assert!(!list.contains("FRONT["));
        assert!(list.contains("/articles/post-1") && !list.contains("/articles/post-3"));
        assert!(list.contains(r#"href="/articles" rel="prev""#), "{}", list);

        // テーマに front.html が無ければ記事一覧と同じ表示
        state.theme.unregister_template("front.html");
        let (status, fallback) =
            body_of(render_index(State(state.clone()), Query(IndexParams::default())).await).await;
        assert_eq!(status, StatusCode::OK);
        assert!(fallback.contains("/articles/post-3"), "{}", fallback);
    }

    #[tokio::test]
    async fn test_front_page_fixed_page() {
        let state = front_page_state("page:home").await;
        insert_published(&state, "article", "post-1", 1, &[], HashMap::new()).await;

        // ページが無いうちは記事一覧
        let (status, html) =
            body_of(render_index(State(state.clone()), Query(IndexParams::default())).await).await;
        assert_eq!(status, StatusCode::OK);
        assert!(html.contains("/articles/post-1"), "{}", html);

        insert_published(
            &state,
            "page",
            "home",
            2,
            &[BlockNode::paragraph("Welcome to the home page.")],
            HashMap::new(),
        )
        .await;
        let (status, html) =
            body_of(render_index(State(state.clone()), Query(IndexParams::default())).await).await;
        assert_eq!(status, StatusCode::OK);
        assert!(html.contains("Welcome to the home page."), "{}", html);
        assert!(!html.contains("/articles/post-1"));

        // 記事一覧は /articles に残る
        let (_, list) =
            body_of(render_articles(State(state.clone()), Query(IndexParams::default())).await)
                .await;
        assert!(list.contains("/articles/post-1"));
    }

    #[tokio::test]
    async fn test_category_archive_lists_only_its_articles() {
        let state = front_page_state("posts").await;
        let news = insert_published(
            &state,
            "category",
            "news",
            1,
            &[],
            HashMap::from([(
                "description".to_string(),
                serde_json::json!("Announcements"),
            )]),
        )
        .await;
        let in_news =
            |id: &str| HashMap::from([("category_id".to_string(), serde_json::json!(id))]);
        for n in 1..=3 {
            insert_published(
                &state,
                "article",
                &format!("news-{}", n),
                n,
                &[],
                in_news(&news),
            )
            .await;
        }
        insert_published(&state, "article", "other", 9, &[], HashMap::new()).await;

        let category = |slug: &str, page: Option<u64>| {
            let state = state.clone();
            let slug = slug.to_string();
            async move {
                body_of(
                    render_category(State(state), Path(slug), Query(IndexParams { page })).await,
                )
                .await
            }
        };

        let (status, first) = category("news", None).await;
        assert_eq!(status, StatusCode::OK);
        assert!(first.contains("Title of news"), "{}", first);
        assert!(first.contains("Announcements"));
        assert!(first.contains("/articles/news-3") && first.contains("/articles/news-2"));
        assert!(!first.contains("/articles/other") && !first.contains("/articles/news-1"));
        assert!(
            first.contains(r#"href="/category/news?page=2" rel="next""#),
            "{}",
            first
        );
        assert!(first.contains(r#"type="application/atom+xml""#));
        assert!(first.contains("/category/news/feed.xml"));

        let (_, second) = category("news", Some(2)).await;
        assert!(second.contains("/articles/news-1") && !second.contains("/articles/news-3"));

        let (status, _) = category("news", Some(3)).await;
        assert_eq!(status, StatusCode::NOT_FOUND);
        let (status, _) = category("no-such-category", None).await;
        assert_eq!(status, StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn test_atom_feeds() {
        let state = front_page_state("posts").await;
        let news = insert_published(&state, "category", "news", 1, &[], HashMap::new()).await;
        insert_published(
            &state,
            "article",
            "launch",
            2,
            &[BlockNode::paragraph("We <launched> today.")],
            HashMap::from([("category_id".to_string(), serde_json::json!(news))]),
        )
        .await;
        insert_published(&state, "article", "other", 3, &[], HashMap::new()).await;

        let mut headers = axum::http::HeaderMap::new();
        headers.insert(axum::http::header::HOST, "example.com".parse().unwrap());
        headers.insert("x-forwarded-proto", "https".parse().unwrap());

        let resp = crate::handlers::feed::get_feed(State(state.clone()), headers.clone()).await;
        assert_eq!(
            resp.headers()[axum::http::header::CONTENT_TYPE],
            "application/atom+xml; charset=utf-8"
        );
        let (status, xml) = body_of(resp).await;
        assert_eq!(status, StatusCode::OK);
        assert!(
            xml.contains(
                r#"<feed xmlns="http://www.w3.org/2005/Atom" xml:base="https://example.com/">"#
            ),
            "{}",
            xml
        );
        assert!(xml.contains(
            r#"<link rel="self" type="application/atom+xml" href="https://example.com/feed.xml"/>"#
        ));
        assert!(xml.contains(r#"href="https://example.com/articles/launch""#));
        assert!(xml.contains("<id>https://example.com/articles/article-launch</id>"));
        assert!(xml.contains("<updated>2026-10-22T00:00:03Z</updated>"));
        // 製品名を出さない（CMS の種類を特定させない）
        assert!(!xml.contains("<generator"), "{}", xml);
        // 本文の HTML は 1 回エスケープして入れる（テキストの < は HTML で &lt; なので &amp;lt;）
        assert!(
            xml.contains("We &amp;lt;launched&amp;gt; today."),
            "{}",
            xml
        );
        // 新しい順
        assert!(xml.find("articles/other").unwrap() < xml.find("articles/launch").unwrap());

        let resp = crate::handlers::feed::get_category_feed(
            State(state.clone()),
            Path("news".to_string()),
            headers.clone(),
        )
        .await;
        let (status, xml) = body_of(resp).await;
        assert_eq!(status, StatusCode::OK);
        assert!(xml.contains("articles/launch") && !xml.contains("articles/other"));
        assert!(xml.contains(
            r#"<link rel="alternate" type="text/html" href="https://example.com/category/news"/>"#
        ));

        let resp = crate::handlers::feed::get_category_feed(
            State(state.clone()),
            Path("missing".to_string()),
            headers,
        )
        .await;
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);

        // 各ページの <head> にフィードの自動検出用のリンクが入る
        let (_, html) =
            body_of(render_index(State(state.clone()), Query(IndexParams::default())).await).await;
        assert!(
            html.contains(r#"type="application/atom+xml""#) && html.contains(r#"href="/feed.xml""#),
            "{}",
            html
        );
    }
}
