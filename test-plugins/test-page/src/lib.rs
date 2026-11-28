//! AegisCMS 本体のテストのためのプラグイン（同梱しない・ストアに載せない）。
//!
//! - `on-render-page`: 記事の本文の後ろに `<p class="tp-mark">` を足し、インラインのスクリプトで
//!   `tp-ran` のクラスを付ける（スクリプトが CSP のハッシュで動いたかを E2E で確かめる）
//! - `render-blocks`: `test-page/box` を、ブロック自身の `<style>` つきで描く
//! - 設定（マニフェストの `settings`）は、各型の検証を確かめるために宣言だけしてある

use aegis_plugin_sdk::prelude::*;

/// 本文の後ろに足す印
const MARK: &str = "<p class=\"tp-mark\">test-page</p>";

/// 印に `tp-ran` を付ける（`<head>` に入るので、本文を読み終えてから動かす）
const SCRIPT: &str = "document.addEventListener('DOMContentLoaded',function(){\
document.querySelectorAll('.tp-mark').forEach(function(e){e.classList.add('tp-ran')})})";

/// `test-page/box` の CSS。`display` が効いたかを E2E で確かめる
const STYLES: &str =
    "<style>:where(.tp-box){display:inline-block;padding:.25em .5em;border:1px solid}</style>";

fn escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

#[derive(Default)]
pub struct TestPagePlugin;

impl Plugin for TestPagePlugin {
    fn on_render_page(&mut self, mut ctx: PageRenderContext) -> Result<PageRenderContext> {
        let show = config::get("show_marker").is_none_or(|v| v.trim() != "false");
        if ctx.content_type != "article" || !show {
            return Ok(ctx);
        }
        ctx.append_content = MARK.to_string();
        ctx.scripts.push(InjectedScript::inline(SCRIPT));
        Ok(ctx)
    }

    fn render_blocks(&mut self, blocks: Vec<BlockNode>) -> Result<Vec<Option<String>>> {
        let mut styled = false;
        Ok(blocks
            .iter()
            .map(|block| {
                let text = block.attribute("text").and_then(|v| v.as_str())?.trim();
                if !block.name.ends_with("/box") || text.is_empty() {
                    return None;
                }
                let html = format!("<span class=\"tp-box\">{}</span>", escape(text));
                if styled {
                    Some(html)
                } else {
                    styled = true;
                    Some(format!("{}{}", STYLES, html))
                }
            })
            .collect())
    }
}

aegis_plugin_sdk::register_plugin!(TestPagePlugin);

#[cfg(test)]
mod tests {
    use super::*;
    use aegis_plugin_sdk::host::mock;
    use aegis_plugin_sdk::serde_json::json;

    fn page(content_type: &str) -> PageRenderContext {
        PageRenderContext {
            content_type: content_type.to_string(),
            ..Default::default()
        }
    }

    #[test]
    fn marks_articles_with_a_script() {
        mock::reset();
        let ctx = TestPagePlugin.on_render_page(page("article")).unwrap();
        assert_eq!(ctx.append_content, MARK);
        assert_eq!(ctx.scripts.len(), 1);

        let other = TestPagePlugin.on_render_page(page("page")).unwrap();
        assert!(other.append_content.is_empty() && other.scripts.is_empty());

        mock::set_config("show_marker", "false");
        let off = TestPagePlugin.on_render_page(page("article")).unwrap();
        assert!(off.scripts.is_empty());
    }

    #[test]
    fn renders_boxes_with_the_style_once() {
        let mut block = BlockNode::new("test-page/box");
        block.set_attribute("text", json!("<b>Hi</b>"));
        let mut empty = BlockNode::new("test-page/box");
        empty.set_attribute("text", json!(" "));
        let html = TestPagePlugin
            .render_blocks(vec![block.clone(), empty, block])
            .unwrap();
        assert!(html[0].as_deref().unwrap().starts_with("<style>"));
        assert!(html[0]
            .as_deref()
            .unwrap()
            .ends_with("&lt;b&gt;Hi&lt;/b&gt;</span>"));
        assert_eq!(html[1], None);
        assert_eq!(
            html[2].as_deref(),
            Some("<span class=\"tp-box\">&lt;b&gt;Hi&lt;/b&gt;</span>")
        );
    }
}
