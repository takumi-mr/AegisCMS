//! Content rendering shared by saving, the public site, the content API and the editor preview,
//! so every place produces the same HTML for the same blocks.

use aegis_ast::SecurityConfig;
use aegis_ast::{
    AstPipeline, BlockNode, HeadingIdTransformer, HtmlRenderer, HtmlRewriterConfig,
    StreamingHtmlRewriter,
};
use aegis_core::PageRenderContext;
use aegis_schema::SchemaDefinition;
use std::collections::HashMap;

use crate::state::AppState;
use aegis_runtime::AegisPlugin;
use std::sync::Arc;

/// Blocks after the save-time transforms (what gets stored and later rendered)
pub struct TransformedBlocks {
    pub blocks: Vec<BlockNode>,
    /// Set when the plugins' `transform-content-ast` hook failed; `blocks` then holds the
    /// built-in pipeline's result only (the hook's result is discarded as a whole)
    pub plugin_error: Option<String>,
}

/// Save-time transforms: the built-in AST pipeline (heading ids), then every plugin's
/// `transform-content-ast` hook
pub async fn transform_blocks(state: &AppState, blocks: Vec<BlockNode>) -> TransformedBlocks {
    let blocks = builtin_transforms(blocks);

    let supervisor = state.supervisor.read().await;
    match supervisor.apply_ast_transforms(blocks.clone()).await {
        Ok(transformed) => TransformedBlocks {
            blocks: transformed,
            plugin_error: None,
        },
        Err(e) => TransformedBlocks {
            blocks,
            plugin_error: Some(e.to_string()),
        },
    }
}

/// The built-in AST pipeline alone (heading ids). Idempotent: it only fills in what is missing,
/// so it can run again on blocks a plugin changed in `on-before-save`
pub fn builtin_transforms(blocks: Vec<BlockNode>) -> Vec<BlockNode> {
    AstPipeline::new().add(HeadingIdTransformer).execute(blocks)
}

/// Renders stored (already transformed) blocks to the HTML the public site shows:
/// `HtmlRenderer`, then the streaming rewriter with the `[security]` settings
pub fn render_blocks_html(security: &SecurityConfig, blocks: &[BlockNode]) -> String {
    render_blocks_html_with(security, blocks, &HashMap::new())
}

/// [`render_blocks_html`] with the HTML of some top-level blocks made by the caller
/// ([`HtmlRenderer::render_nodes_with`])
fn render_blocks_html_with(
    security: &SecurityConfig,
    blocks: &[BlockNode],
    prerendered: &HashMap<usize, String>,
) -> String {
    let raw_html = HtmlRenderer::render_nodes_with(blocks, prerendered);
    let rewriter_config = HtmlRewriterConfig::from(security.clone());
    StreamingHtmlRewriter::rewrite(&raw_html, &rewriter_config).unwrap_or(raw_html)
}

/// Body HTML and the problems met while rendering it (shown in the editor preview)
pub struct RenderedContent {
    pub html: String,
    /// `<plugin>: <error>` for each plugin whose `render-blocks` hook failed
    pub warnings: Vec<String>,
}

/// 問い合わせフォームのブロック（`{"form": "<フォームの名前>"}`。トップレベルに置いたものだけ描く）
pub const CONTACT_FORM_BLOCK: &str = "core/contact-form";

/// Renders stored blocks to body HTML, including the plugins' custom blocks: a block named
/// `<plugin>/<block>` that an enabled plugin declares is rendered by that plugin's
/// `render-blocks` hook (one call per plugin with all of its blocks, in document order). The
/// plugin's HTML is sanitized like a `core/html` block (no scripts, event attributes or
/// `javascript:` URLs; `<iframe>`s only from the plugin's `embed.frame_domains`) inside
/// `<div class="aegis-plugin-block" data-block="...">`. A block the
/// plugin returns nothing for, or whose plugin fails, shows nothing. Only top-level blocks are
/// handed to plugins. Top-level [`CONTACT_FORM_BLOCK`]s become the configured contact form
pub async fn render_content(state: &AppState, blocks: &[BlockNode]) -> RenderedContent {
    let mut warnings = Vec::new();
    let supervisor = state.supervisor.read().await;
    // プラグインごとの（プラグイン、そのプラグインのブロックの位置）
    let mut groups: Vec<(&Arc<dyn AegisPlugin>, Vec<usize>)> = Vec::new();
    for (i, block) in blocks.iter().enumerate() {
        if let Some(plugin) = supervisor.block_renderer(&block.name) {
            match groups
                .iter_mut()
                .find(|(p, _)| p.manifest().name == plugin.manifest().name)
            {
                Some((_, indexes)) => indexes.push(i),
                None => groups.push((plugin, vec![i])),
            }
        }
    }
    // 問い合わせフォームのブロック（設定のフォームから作る。HTML はエスケープ済み）
    let mut prerendered = HashMap::new();
    for (i, block) in blocks.iter().enumerate() {
        if block.name == CONTACT_FORM_BLOCK {
            prerendered.insert(
                i,
                crate::handlers::contact::block_html(state, &block.attributes),
            );
        }
    }
    if groups.is_empty() && prerendered.is_empty() {
        return RenderedContent {
            html: render_blocks_html(&state.config.security, blocks),
            warnings,
        };
    }

    for (plugin, indexes) in groups {
        let input: Vec<BlockNode> = indexes.iter().map(|&i| blocks[i].clone()).collect();
        let rendered = match plugin.render_blocks(input).await {
            Ok(rendered) => rendered,
            Err(e) => {
                tracing::warn!(
                    "Plugin '{}' failed to render its blocks: {}",
                    plugin.manifest().name,
                    e
                );
                warnings.push(format!("{}: {}", plugin.manifest().name, e));
                Vec::new()
            }
        };
        let frame_domains = plugin.manifest().frame_domains();
        for (k, &i) in indexes.iter().enumerate() {
            let html = rendered.get(k).cloned().flatten();
            prerendered.insert(i, plugin_block_html(&blocks[i].name, html, frame_domains));
        }
    }
    RenderedContent {
        html: render_blocks_html_with(&state.config.security, blocks, &prerendered),
        warnings,
    }
}

/// [`render_content`] without the warnings
pub async fn render_content_html(state: &AppState, blocks: &[BlockNode]) -> String {
    render_content(state, blocks).await.html
}

/// A plugin's HTML for one block, sanitized (keeping `<iframe>`s from `frame_domains`) and
/// wrapped in `div.aegis-plugin-block` (nothing for no HTML)
fn plugin_block_html(name: &str, html: Option<String>, frame_domains: &[String]) -> String {
    let Some(html) = html.filter(|h| !h.trim().is_empty()) else {
        return String::new();
    };
    let wrapped = format!(
        "<div class=\"aegis-plugin-block\" data-block=\"{}\">{}</div>",
        aegis_ast::renderer::escape_html(name),
        html
    );
    let clean = aegis_ast::renderer::sanitize_html_fragment_with_frames(&wrapped, frame_domains);
    format!("{}\n", clean)
}

/// Content-specific inputs of a themed page (from a stored record, or from the editor for previews)
pub struct PageInput {
    /// Template to render, e.g. `article.html` (plugins may switch it in `on-render-page`)
    pub template: String,
    pub content_type: String,
    pub title: String,
    pub slug: String,
    /// URL path the page is served at, for the canonical / OGP URLs (`None`: `/articles/{slug}`
    /// for articles, `/{slug}` otherwise)
    pub path: Option<String>,
    /// Body HTML from [`render_blocks_html`]
    pub content_html: String,
    pub author: Option<String>,
    pub published_at: Option<String>,
    pub updated_at: Option<String>,
    pub fields: HashMap<String, serde_json::Value>,
    /// Comment section (`[comments]`), for content types that take comments
    pub comments: Option<aegis_core::CommentSection>,
}

/// Resolves the schema's relation fields to `{ id, title, slug }` objects for the templates
async fn resolve_page_relations(
    state: &AppState,
    schema: &SchemaDefinition,
    fields: &HashMap<String, serde_json::Value>,
) -> HashMap<String, serde_json::Value> {
    let schemas = state.schemas.read().await;
    aegis_db::resolve_relations_for_records(&*state.db, &schemas, schema, &[fields])
        .await
        .pop()
        .unwrap_or_default()
        .into_iter()
        .map(|(name, rel)| (name, relation_summary(&rel)))
        .collect()
}

/// The `{ id, title, slug }` object a template sees for a resolved relation
pub fn relation_summary(rel: &aegis_db::ResolvedRelation) -> serde_json::Value {
    serde_json::json!({ "id": rel.id, "title": rel.title, "slug": rel.slug })
}

/// Extra `<head>` tags for any page: the base styles of the content blocks, the favicon, the link
/// to the site's Atom feed and the SEO / OGP tags
pub fn build_extra_head(
    state: &AppState,
    title: Option<&str>,
    description: Option<&str>,
    image: Option<&str>,
    path: Option<&str>,
    is_article: bool,
) -> String {
    let site = state.site();
    let seo = state.seo();
    let head_params = crate::seo::PageHeadParams {
        site_title: &site.title,
        theme_description: &state.config.theme.site_description,
        title,
        description,
        image,
        path,
        is_article,
    };
    let mut extra_head = String::from(aegis_theme::BLOCK_STYLES);
    #[cfg(feature = "analytics")]
    {
        let beacon = crate::analytics::head_html(state);
        if !beacon.is_empty() {
            extra_head.push_str("\n  ");
            extra_head.push_str(&beacon);
        }
    }
    let site_head = site.head_html();
    if !site_head.is_empty() {
        extra_head.push_str("\n  ");
        extra_head.push_str(&site_head);
    }
    let base_path = state
        .config
        .server
        .normalized_base_path()
        .unwrap_or_default();
    let feed_link =
        crate::handlers::feed::feed_link_tag(&site.title, &format!("{}/feed.xml", base_path));
    extra_head.push_str("\n  ");
    extra_head.push_str(&feed_link);
    let seo_tags = seo.generate_head_tags(&head_params);
    if !seo_tags.is_empty() {
        if !extra_head.is_empty() {
            extra_head.push('\n');
            extra_head.push_str("  ");
        }
        extra_head.push_str(&seo_tags);
    }
    extra_head
}

/// A page context holding only the site-wide values: the site title and favicon (Admin Studio's
/// settings over `[theme]`), the description and the custom CSS. The favicon `<link>` goes into
/// `extra_head`, which every theme outputs, so `on-render-page` plugins can still change it
pub fn site_page_context(state: &AppState) -> PageRenderContext {
    let site = state.site();
    let seo = state.seo();
    let effective_desc = seo
        .meta_description
        .as_deref()
        .filter(|d| !d.trim().is_empty())
        .unwrap_or(&state.config.theme.site_description);
    PageRenderContext {
        extra_head: build_extra_head(state, None, None, None, Some("/"), false),
        site_title: site.title,
        site_description: effective_desc.to_string(),
        extra_css: state.config.theme.custom_css.clone().unwrap_or_default(),
        ..Default::default()
    }
}

/// Renders a full themed page: resolves relations, applies the plugins' `on-render-page`
/// hook (a failing hook keeps the original context) and renders the theme template
pub async fn render_page(
    state: &AppState,
    schema: &SchemaDefinition,
    page: PageInput,
) -> Result<String, String> {
    let relations = resolve_page_relations(state, schema, &page.fields).await;
    let is_article = page.content_type == "article";
    let path = page.path.clone().unwrap_or_else(|| {
        if is_article {
            format!("/articles/{}", page.slug)
        } else {
            format!("/{}", page.slug)
        }
    });
    let excerpt = page.fields.get("excerpt").and_then(|v| v.as_str());
    let image = page
        .fields
        .get("featured_image")
        .or_else(|| page.fields.get("image"))
        .and_then(|v| v.as_str());

    let extra_head = build_extra_head(
        state,
        Some(&page.title),
        excerpt,
        image,
        Some(&path),
        is_article,
    );

    let mut ctx = PageRenderContext {
        template: page.template,
        content_type: page.content_type,
        title: page.title,
        slug: page.slug,
        content_html: page.content_html,
        author: page.author,
        published_at: page.published_at,
        updated_at: page.updated_at,
        fields: page.fields,
        relations,
        extra_head,
        comments: page.comments,
        ..site_page_context(state)
    };

    {
        let supervisor = state.supervisor.read().await;
        if let Ok(hooked_ctx) = supervisor.apply_render_page(ctx.clone()).await {
            ctx = hooked_ctx;
        }
    }

    state
        .theme
        .render(&ctx.template, &ctx)
        .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn block(name: &str, text: &str, attributes: serde_json::Value) -> BlockNode {
        BlockNode {
            name: name.to_string(),
            attributes,
            children: vec![],
            text: Some(text.to_string()),
        }
    }

    #[test]
    fn test_render_blocks_html_applies_the_rewriter() {
        let blocks = vec![
            block("core/paragraph", "text", serde_json::json!({})),
            block(
                "core/image",
                "",
                serde_json::json!({ "src": "/uploads/a.png", "alt": "A" }),
            ),
        ];
        let html = render_blocks_html(&SecurityConfig::default(), &blocks);
        assert!(html.contains("loading=\"lazy\""), "{}", html);
        assert!(html.contains("decoding=\"async\""), "{}", html);
    }
    /// Declares `cards/card` and `cards/broken`; renders each block as its `title` attribute
    /// (with a script and an event attribute to strip), fails when asked for `cards/broken`
    struct CardPlugin {
        manifest: aegis_core::PluginManifest,
        calls: std::sync::Mutex<Vec<usize>>,
    }

    #[async_trait::async_trait]
    impl AegisPlugin for CardPlugin {
        fn manifest(&self) -> &aegis_core::PluginManifest {
            &self.manifest
        }

        async fn render_blocks(
            &self,
            blocks: Vec<BlockNode>,
        ) -> aegis_core::Result<Vec<Option<String>>> {
            self.calls.lock().unwrap().push(blocks.len());
            if blocks.iter().any(|b| b.name == "cards/broken") {
                return Err(aegis_core::AegisError::PluginRuntime("boom".to_string()));
            }
            Ok(blocks
                .iter()
                .map(|b| {
                    b.attributes["title"].as_str().map(|t| {
                        format!(
                            r#"<p class="card" onclick="x()">{}</p><script>evil()</script>"#,
                            t
                        )
                    })
                })
                .collect())
        }
    }

    fn card_plugin() -> Arc<CardPlugin> {
        let manifest: aegis_core::PluginManifest = serde_json::from_value(serde_json::json!({
            "name": "cards",
            "version": "1.0.0",
            "entrypoint": "plugin.wasm",
            "capabilities": { "hooks": ["render-blocks"] },
            "blocks": [
                { "name": "card", "label": "Card", "fields": [{ "name": "title", "type": "string", "label": "Title" }] },
                { "name": "broken", "label": "Broken" }
            ]
        }))
        .unwrap();
        Arc::new(CardPlugin {
            manifest,
            calls: std::sync::Mutex::new(Vec::new()),
        })
    }

    #[tokio::test]
    async fn test_render_content_renders_plugin_blocks_in_one_call_and_sanitizes_them() {
        let plugin = card_plugin();
        let mut supervisor = aegis_runtime::PluginSupervisor::new();
        supervisor.register(plugin.clone());
        let state = crate::test_support::test_state(supervisor).await;

        let blocks = vec![
            block("cards/card", "", serde_json::json!({ "title": "First" })),
            block("core/paragraph", "between", serde_json::json!({})),
            block("cards/card", "", serde_json::json!({ "title": "Second" })),
            // 属性が無く、プラグインが何も返さないブロック
            block("cards/card", "", serde_json::json!({})),
            // 宣言されていないブロックは今までどおりカスタム要素
            block("cards/unknown", "", serde_json::json!({})),
        ];
        let rendered = render_content(&state, &blocks).await;
        assert!(rendered.warnings.is_empty());
        assert_eq!(*plugin.calls.lock().unwrap(), vec![3]);
        let html = rendered.html;
        assert!(
            html.contains(r#"<div class="aegis-plugin-block" data-block="cards/card"><p class="card">First</p></div>"#),
            "{}",
            html
        );
        assert!(html.find("First").unwrap() < html.find("between").unwrap());
        assert!(html.find("between").unwrap() < html.find("Second").unwrap());
        assert!(
            !html.contains("script") && !html.contains("onclick"),
            "{}",
            html
        );
        assert_eq!(html.matches("aegis-plugin-block").count(), 2);
        assert!(html.contains("<cards-unknown>"));
    }

    #[tokio::test]
    async fn test_a_failing_plugin_shows_nothing_and_reports_a_warning() {
        let mut supervisor = aegis_runtime::PluginSupervisor::new();
        supervisor.register(card_plugin());
        let state = crate::test_support::test_state(supervisor).await;

        let blocks = vec![
            block("cards/card", "", serde_json::json!({ "title": "Lost" })),
            block("cards/broken", "", serde_json::json!({})),
            block("core/paragraph", "kept", serde_json::json!({})),
        ];
        let rendered = render_content(&state, &blocks).await;
        assert_eq!(
            rendered.warnings,
            vec!["cards: Plugin runtime error: boom".to_string()]
        );
        assert!(!rendered.html.contains("Lost"), "{}", rendered.html);
        assert!(rendered.html.contains("<p>kept</p>"));
    }

    #[tokio::test]
    async fn test_blocks_of_a_plugin_without_the_hook_are_not_rendered_by_it() {
        let mut manifest = card_plugin().manifest.clone();
        manifest.capabilities.hooks.clear();
        let plugin = Arc::new(CardPlugin {
            manifest,
            calls: std::sync::Mutex::new(Vec::new()),
        });
        let mut supervisor = aegis_runtime::PluginSupervisor::new();
        supervisor.register(plugin.clone());
        let state = crate::test_support::test_state(supervisor).await;

        let html = render_content_html(
            &state,
            &[block("cards/card", "", serde_json::json!({ "title": "x" }))],
        )
        .await;
        assert!(plugin.calls.lock().unwrap().is_empty());
        assert!(html.contains("<cards-card>"), "{}", html);
    }

    /// `video/player` を描画する。許可したホストと許可していないホストの iframe を返す
    struct VideoPlugin(aegis_core::PluginManifest);

    #[async_trait::async_trait]
    impl AegisPlugin for VideoPlugin {
        fn manifest(&self) -> &aegis_core::PluginManifest {
            &self.0
        }

        async fn render_blocks(
            &self,
            blocks: Vec<BlockNode>,
        ) -> aegis_core::Result<Vec<Option<String>>> {
            Ok(blocks
                .iter()
                .map(|_| {
                    Some(
                        r#"<iframe src="https://player.example.com/v/1" title="ok" srcdoc="x"></iframe><iframe src="https://evil.example/x"></iframe>"#
                            .to_string(),
                    )
                })
                .collect())
        }
    }

    fn video_supervisor() -> aegis_runtime::PluginSupervisor {
        let manifest: aegis_core::PluginManifest = serde_json::from_value(serde_json::json!({
            "name": "video",
            "version": "1.0.0",
            "entrypoint": "plugin.wasm",
            "capabilities": {
                "hooks": ["render-blocks"],
                "embed": { "frame_domains": ["player.example.com"] }
            },
            "blocks": [{ "name": "player", "label": "Player" }]
        }))
        .unwrap();
        let mut supervisor = aegis_runtime::PluginSupervisor::new();
        supervisor.register(Arc::new(VideoPlugin(manifest)));
        supervisor
    }

    #[tokio::test]
    async fn test_plugin_blocks_keep_only_frames_from_the_declared_hosts() {
        let mut supervisor = video_supervisor();
        // 埋め込みを宣言していない別のプラグインは、同じ iframe を返しても残らない
        supervisor.register(card_plugin());
        let state = crate::test_support::test_state(supervisor).await;
        let html = render_content_html(
            &state,
            &[
                block("video/player", "", serde_json::json!({})),
                block("cards/card", "", serde_json::json!({ "title": "<iframe src=\"https://player.example.com/v/2\"></iframe>" })),
            ],
        )
        .await;
        assert!(
            html.contains(r#"<iframe src="https://player.example.com/v/1" title="ok"></iframe>"#),
            "{}",
            html
        );
        assert!(!html.contains("evil.example") && !html.contains("srcdoc"));
        assert!(!html.contains("/v/2"), "{}", html);
    }

    #[tokio::test]
    async fn test_responses_allow_the_declared_frame_hosts_in_the_csp() {
        use tower::ServiceExt;
        let state = crate::test_support::test_state(video_supervisor()).await;
        let config = state.config.clone();
        let response = crate::app::router(&config, state)
            .oneshot(
                axum::http::Request::get("/api/v1/health")
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let csp = response.headers()["content-security-policy"]
            .to_str()
            .unwrap();
        assert!(
            csp.contains("frame-src 'self' https://player.example.com"),
            "{}",
            csp
        );
    }
}
