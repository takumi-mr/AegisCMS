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
    let raw_html = HtmlRenderer::render_nodes(blocks);
    let rewriter_config = HtmlRewriterConfig::from(security.clone());
    StreamingHtmlRewriter::rewrite(&raw_html, &rewriter_config).unwrap_or(raw_html)
}

/// Content-specific inputs of a themed page (from a stored record, or from the editor for previews)
pub struct PageInput {
    /// Template to render, e.g. `article.html` (plugins may switch it in `on-render-page`)
    pub template: String,
    pub content_type: String,
    pub title: String,
    pub slug: String,
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

/// Extra `<head>` tags containing favicon and SEO / OGP tags for any page
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
    let mut extra_head = site.head_html();
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
    let path = if is_article {
        format!("/articles/{}", page.slug)
    } else {
        format!("/{}", page.slug)
    };
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
}
