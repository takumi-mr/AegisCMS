pub mod config;
pub(crate) mod footnote;
pub mod inline;
pub mod node;
pub mod renderer;
pub mod rewriter;
pub mod transform;

pub use config::SecurityConfig;
pub use inline::{plain_text, render_inline};
pub use node::BlockNode;
pub use renderer::{extract_links, HtmlRenderer};
pub use rewriter::{HtmlRewriterConfig, RewriterError, StreamingHtmlRewriter};
pub use transform::{AstPipeline, AstTransformer, HeadingIdTransformer};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ast_rendering_and_transform() {
        let blocks = vec![
            BlockNode::heading(2, "Architecture Overview"),
            BlockNode::paragraph("AegisCMS uses WebAssembly sandboxing for zero-trust extensions."),
            BlockNode::code("rust", "fn main() { println!(\"Hello Aegis\"); }"),
        ];

        let pipeline = AstPipeline::new().add(HeadingIdTransformer);
        let transformed = pipeline.execute(blocks);

        assert_eq!(
            transformed[0].attributes.get("id").and_then(|v| v.as_str()),
            Some("architecture-overview")
        );

        let html = HtmlRenderer::render_nodes(&transformed);
        assert!(html.contains("<h2 id=\"architecture-overview\">Architecture Overview</h2>"));
        assert!(
            html.contains("<p>AegisCMS uses WebAssembly sandboxing for zero-trust extensions.</p>")
        );
        assert!(html.contains("<pre><code class=\"language-rust\">fn main() { println!(&quot;Hello Aegis&quot;); }</code></pre>"));
    }
}

/// 閲覧を制限した記事・ページで、だれにでも見せてよい本文。今は無し（本文は全部隠す）。
/// 「ここから先」の区切り（docs/members.md 15.3）を入れたら、区切りより前のブロックを返す。
/// 制限した本文をページ以外（フィード・検索・一覧・API）に出すときは、必ずこれを通す。
pub fn public_blocks(_blocks: &[BlockNode]) -> Vec<BlockNode> {
    Vec::new()
}
