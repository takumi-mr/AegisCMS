pub mod config;
pub(crate) mod footnote;
pub mod inline;
pub mod node;
pub mod renderer;
pub mod rewriter;
pub mod transform;

pub use config::{SecurityConfig, StrictCsp};
pub use inline::{plain_text, render_inline};
pub use node::BlockNode;
pub use renderer::{extract_links, HtmlRenderer};
pub use rewriter::{HtmlRewriterConfig, RewriterError, StreamingHtmlRewriter};
pub use transform::{AstPipeline, AstTransformer, HeadingIdTransformer};

/// 「ここから先は会員向け」の区切りのブロック名（Markdown では `<!-- paywall -->`。docs/members.md 15.3）
pub const PAYWALL_BLOCK: &str = "core/paywall";

/// 閲覧を制限した記事・ページで、だれにでも見せてよい本文。**最初の最上位の区切り（`core/paywall`）
/// より前のブロック**。区切りが無ければ、本文は全部隠す（空）。入れ子の中の区切りは数えない
/// （数えると、区切りの前後を取り違えて隠すべき本文を出しかねない）。
/// 制限した本文をページ以外（フィード・検索・一覧・API）に出すときは、必ずこれを通す。
pub fn public_blocks(blocks: &[BlockNode]) -> Vec<BlockNode> {
    match blocks.iter().position(|b| b.name == PAYWALL_BLOCK) {
        Some(at) => blocks[..at].to_vec(),
        None => Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(blocks: &[BlockNode]) -> Vec<&str> {
        blocks
            .iter()
            .map(|b| b.text.as_deref().unwrap_or(&b.name))
            .collect()
    }

    /// 区切りより前だけが、だれにでも見せてよい部分。区切りが無ければ何も見せない
    #[test]
    fn public_blocks_is_what_comes_before_the_first_paywall() {
        let blocks = vec![
            BlockNode::paragraph("teaser 1"),
            BlockNode::heading(2, "teaser 2"),
            BlockNode::new(PAYWALL_BLOCK),
            BlockNode::paragraph("paid 1"),
            BlockNode::new(PAYWALL_BLOCK),
            BlockNode::paragraph("paid 2"),
        ];
        assert_eq!(names(&public_blocks(&blocks)), vec!["teaser 1", "teaser 2"]);
        // 先頭の区切りは、全部を隠す
        assert!(public_blocks(&blocks[2..]).is_empty());
        // 区切りが無い本文は、全部隠す
        assert!(public_blocks(&blocks[..2]).is_empty());
        assert!(public_blocks(&[]).is_empty());
    }

    /// 入れ子の中の区切りは数えない（外の本文は、区切りの前にあっても出さない）
    #[test]
    fn a_nested_paywall_does_not_open_the_text_around_it() {
        let mut callout = BlockNode::new("core/callout");
        callout.children = vec![BlockNode::new(PAYWALL_BLOCK)];
        let blocks = vec![
            BlockNode::paragraph("before"),
            callout,
            BlockNode::paragraph("after"),
        ];
        assert!(public_blocks(&blocks).is_empty());
    }

    /// 区切りは、読める閲覧者には何も出さない
    #[test]
    fn the_paywall_renders_nothing() {
        let html = HtmlRenderer::render_nodes(&[
            BlockNode::paragraph("a"),
            BlockNode::new(PAYWALL_BLOCK),
            BlockNode::paragraph("b"),
        ]);
        assert_eq!(html.matches("<p>").count(), 2);
        assert!(!html.contains("paywall"));
    }

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
