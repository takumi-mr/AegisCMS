use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::{Html, IntoResponse, Response},
};
use std::collections::HashMap;
use std::sync::Arc;

use crate::render;
use crate::state::AppState;
use aegis_ast::BlockNode;
use aegis_core::{ContentSummary, PageRenderContext};

/// ブロックノードから先頭段落を抽出して抜粋を生成
fn extract_excerpt(blocks: &[BlockNode], max_chars: usize) -> String {
    for block in blocks {
        if block.name == "core/paragraph" {
            if let Some(ref text) = block.text {
                let trimmed = text.trim();
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
        site_title: state.config.theme.site_title.clone(),
        site_description: state.config.theme.site_description.clone(),
        template: "404.html".to_string(),
        content_type: "error".to_string(),
        title: "Page Not Found".to_string(),
        slug: "404".to_string(),
        content_html: format!("<p>{}</p>", message),
        extra_css: state.config.theme.custom_css.clone().unwrap_or_default(),
        ..Default::default()
    };

    let supervisor = state.supervisor.read().await;
    let final_ctx = supervisor.apply_render_page(ctx).await.unwrap_or_default();

    let rendered = state
        .theme
        .render(&final_ctx.template, &final_ctx)
        .unwrap_or_else(|_| "<h1>404 Not Found</h1>".to_string());

    (StatusCode::NOT_FOUND, Html(rendered)).into_response()
}

/// トップページ（最新の公開記事一覧）
pub async fn render_index(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let article_schema = {
        let schemas = state.schemas.read().await;
        schemas.get("article").cloned()
    };

    let mut items = Vec::new();

    if let Some(schema) = article_schema {
        if let Ok(records) = state.db.list_contents(&schema).await {
            let schemas_guard = state.schemas.read().await;

            for r in records {
                if r.status != "published" {
                    continue;
                }

                let blocks: Vec<BlockNode> =
                    serde_json::from_str(&r.blocks_json).unwrap_or_default();
                let excerpt = extract_excerpt(&blocks, 140);

                let mut rel_values = HashMap::new();
                for (field_def, target) in schema.relation_fields() {
                    if let Some(val_json) = r.custom_fields.get(&field_def.name) {
                        if let Some(ref_id) = val_json.as_str() {
                            if let Some(target_schema) = schemas_guard.get(&target.content_type) {
                                if let Ok(Some(rel)) = state
                                    .db
                                    .resolve_relation(target_schema, &target.field, ref_id)
                                    .await
                                {
                                    rel_values.insert(
                                        field_def.name.clone(),
                                        serde_json::json!({
                                            "id": rel.id,
                                            "title": rel.title,
                                            "slug": rel.slug,
                                        }),
                                    );
                                }
                            }
                        }
                    }
                }

                items.push(ContentSummary {
                    id: r.id,
                    content_type: r.content_type,
                    title: r.title,
                    slug: r.slug,
                    excerpt,
                    author_id: r.author_id,
                    published_at: r.published_at,
                    created_at: r.created_at,
                    fields: r.custom_fields,
                    relations: rel_values,
                });
            }
        }
    }

    let mut ctx = PageRenderContext {
        site_title: state.config.theme.site_title.clone(),
        site_description: state.config.theme.site_description.clone(),
        template: "index.html".to_string(),
        content_type: "index".to_string(),
        title: "Latest Stories".to_string(),
        items,
        extra_css: state.config.theme.custom_css.clone().unwrap_or_default(),
        ..Default::default()
    };

    // プラグインによるテーマ・コンテキスト書き換えフックの適用
    {
        let supervisor = state.supervisor.read().await;
        if let Ok(hooked_ctx) = supervisor.apply_render_page(ctx.clone()).await {
            ctx = hooked_ctx;
        }
    }

    match state.theme.render(&ctx.template, &ctx) {
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

    let maybe_record = match state.db.get_content(&schema, &slug).await {
        Ok(rec) => rec,
        Err(e) => {
            tracing::error!("Database error fetching article '{}': {}", slug, e);
            return render_not_found(&state, "Error querying database").await;
        }
    };

    let record = match maybe_record {
        Some(r) if r.status == "published" => r,
        _ => return render_not_found(&state, "The requested article is not published.").await,
    };

    // AST から HTML 変換 & ストリーミングリライター適用
    let blocks: Vec<BlockNode> = serde_json::from_str(&record.blocks_json).unwrap_or_default();
    let content_html = render::render_blocks_html(&state.config.security, &blocks);

    let page = render::PageInput {
        template: "article.html".to_string(),
        content_type: "article".to_string(),
        title: record.title,
        slug: record.slug,
        content_html,
        author: record.author_id,
        published_at: record.published_at,
        updated_at: Some(record.updated_at),
        fields: record.custom_fields,
    };
    themed_response(render::render_page(&state, &schema, page).await)
}

/// Turns a rendered page (or a template error) into the HTTP response
fn themed_response(result: Result<String, String>) -> Response {
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

/// 固定ページまたはスラッグの汎用ルーティング (/{slug})
pub async fn render_page_or_slug(
    State(state): State<Arc<AppState>>,
    Path(slug): Path<String>,
) -> impl IntoResponse {
    // 1. pages スキーマをチェック
    let page_schema = {
        let schemas = state.schemas.read().await;
        schemas.get("page").cloned()
    };

    if let Some(schema) = page_schema {
        if let Ok(Some(record)) = state.db.get_content(&schema, &slug).await {
            if record.status == "published" {
                let blocks: Vec<BlockNode> =
                    serde_json::from_str(&record.blocks_json).unwrap_or_default();
                let content_html = render::render_blocks_html(&state.config.security, &blocks);

                let page = render::PageInput {
                    template: "page.html".to_string(),
                    content_type: "page".to_string(),
                    title: record.title,
                    slug: record.slug,
                    content_html,
                    author: record.author_id,
                    published_at: record.published_at,
                    updated_at: Some(record.updated_at),
                    fields: record.custom_fields,
                };
                return themed_response(render::render_page(&state, &schema, page).await);
            }
        }
    }

    // 2. article スキーマへのフォールバック
    let article_schema = {
        let schemas = state.schemas.read().await;
        schemas.get("article").cloned()
    };

    if let Some(schema) = article_schema {
        if let Ok(Some(record)) = state.db.get_content(&schema, &slug).await {
            if record.status == "published" {
                let blocks: Vec<BlockNode> =
                    serde_json::from_str(&record.blocks_json).unwrap_or_default();
                let content_html = render::render_blocks_html(&state.config.security, &blocks);

                let page = render::PageInput {
                    template: "article.html".to_string(),
                    content_type: "article".to_string(),
                    title: record.title,
                    slug: record.slug,
                    content_html,
                    author: record.author_id,
                    published_at: record.published_at,
                    updated_at: Some(record.updated_at),
                    fields: record.custom_fields,
                };
                return themed_response(render::render_page(&state, &schema, page).await);
            }
        }
    }

    render_not_found(&state, &format!("Content '{}' not found", slug)).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth::SessionService;
    use crate::cache::MemoryCacheBackend;
    use crate::db::{ContentRecord, DatabaseAdapter, SqliteAdapter};
    use crate::storage::LocalStorageBackend;

    use aegis_core::{
        AegisConfig, PluginCapabilities, PluginManifest, PluginResourceLimits,
        Result as AegisResult,
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

        let mut schemas = HashMap::new();
        schemas.insert("article".to_string(), article_schema.clone());

        db.run_migrations(&schemas)
            .await
            .expect("Migrations failed");

        let storage = Arc::new(LocalStorageBackend::new(
            PathBuf::from("target/test_uploads"),
            "/uploads".to_string(),
        ));
        let cache = Arc::new(MemoryCacheBackend::new());
        let sessions = Arc::new(SessionService::new(cache.clone(), 3600));
        let audit = Arc::new(crate::audit::AuditService::from_config(
            &config.audit,
            db.clone(),
        ));
        let theme_engine = Arc::new(ThemeEngine::new(&PathBuf::from("themes"), "default"));

        let auth_provider = Arc::new(crate::auth::LocalAuthProvider::new(db.clone()));

        Arc::new(AppState::new(
            config,
            db,
            storage,
            cache,
            sessions,
            auth_provider,
            schemas,
            supervisor,
            audit,
            theme_engine,
        ))
    }

    #[tokio::test]
    async fn test_render_index_empty_and_with_published_post() {
        let state = create_test_state(PluginSupervisor::new()).await;

        // 1. 記事がない状態のトップページ
        let resp = render_index(State(state.clone())).await.into_response();
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
        let resp = render_index(State(state.clone())).await.into_response();
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
                    network: None,
                    storage: None,
                    hooks: vec!["on-render-page".to_string()],
                },
                resource_limits: PluginResourceLimits::default(),
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
            created_at: "2026-10-22T00:00:00Z".to_string(),
            updated_at: "2026-10-22T00:00:00Z".to_string(),
            custom_fields: HashMap::new(),
        };
        state.db.insert_content(&schema, &record).await.unwrap();

        // 1. トップページでプラグインの on-render-page によるタイトル書き換えとCSS注入を検証
        let resp = render_index(State(state.clone())).await.into_response();
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
}
