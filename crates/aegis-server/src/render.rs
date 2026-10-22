//! Content rendering shared by saving, the public site, the content API and the editor preview,
//! so every place produces the same HTML for the same blocks.

use aegis_ast::{
    AstPipeline, BlockNode, HeadingIdTransformer, HtmlRenderer, HtmlRewriterConfig,
    StreamingHtmlRewriter,
};
use aegis_core::SecurityConfig;

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
    let pipeline = AstPipeline::new().add(HeadingIdTransformer);
    let blocks = pipeline.execute(blocks);

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

/// Renders stored (already transformed) blocks to the HTML the public site shows:
/// `HtmlRenderer`, then the streaming rewriter with the `[security]` settings
pub fn render_blocks_html(security: &SecurityConfig, blocks: &[BlockNode]) -> String {
    let raw_html = HtmlRenderer::render_nodes(blocks);
    let rewriter_config = HtmlRewriterConfig::from(security.clone());
    StreamingHtmlRewriter::rewrite(&raw_html, &rewriter_config).unwrap_or(raw_html)
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
