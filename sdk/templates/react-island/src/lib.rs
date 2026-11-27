//! React アイランド方式のカスタムブロックプラグインテンプレート。
//!
//! - `render-blocks`: Admin Studio の記事エディタで差し込まれたブロックを
//!   `<div data-aegis-island="react-counter" data-props="...">` として出力する
//! - `on-render-page`: ページ本文にそのアイランドが含まれているときだけ、
//!   プラグイン同梱の `/plugins/<name>/assets/island.js` と `island.css` を注入する
//! - 外部 API と `fetch` 通信したい場合は、`plugin.manifest.json` の
//!   `capabilities.csp.connect_domains` に通信先ホストを宣言する

use aegis_plugin_sdk::prelude::*;
use aegis_plugin_sdk::serde_json::json;

const PLUGIN_NAME: &str = "aegis-plugin-react-island-template";
const ISLAND_NAME: &str = "react-counter";

fn escape_attr(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#x27;"),
            _ => out.push(c),
        }
    }
    out
}

fn attr_str<'a>(block: &'a BlockNode, name: &str, default: &'a str) -> &'a str {
    block
        .attribute(name)
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or(default)
}

fn attr_i64(block: &BlockNode, name: &str, default: i64) -> i64 {
    block
        .attribute(name)
        .and_then(|v| {
            v.as_i64()
                .or_else(|| v.as_str().and_then(|s| s.trim().parse::<i64>().ok()))
        })
        .unwrap_or(default)
}

fn render_counter(block: &BlockNode) -> String {
    let title = attr_str(block, "title", "Interactive Counter");
    let initial = attr_i64(block, "initial", 0);
    let step = attr_i64(block, "step", 1).max(1);
    let props = json!({
        "title": title,
        "initial": initial,
        "step": step,
    })
    .to_string();

    format!(
        "<div class=\"ri-island\" data-aegis-island=\"{ISLAND_NAME}\" data-props=\"{}\"><noscript>{}: {}</noscript></div>",
        escape_attr(&props),
        escape_attr(title),
        initial,
    )
}

#[derive(Default)]
pub struct ReactIslandPlugin;

#[aegis_plugin]
impl Plugin for ReactIslandPlugin {
    fn render_blocks(&mut self, blocks: Vec<BlockNode>) -> Result<Vec<Option<String>>> {
        Ok(blocks
            .iter()
            .map(|block| match block.name.rsplit('/').next() {
                Some("counter") => Some(render_counter(block)),
                _ => None,
            })
            .collect())
    }

    fn on_render_page(&mut self, mut ctx: PageRenderContext) -> Result<PageRenderContext> {
        let marker = format!("data-aegis-island=\"{ISLAND_NAME}\"");
        if ctx.content_html.contains(&marker) {
            ctx.scripts.push(
                InjectedScript::external(format!("/plugins/{PLUGIN_NAME}/assets/island.js"))
                    .deferred(),
            );
            let css_link = format!(
                "<link rel=\"stylesheet\" href=\"/plugins/{PLUGIN_NAME}/assets/island.css\">\n"
            );
            ctx.extra_head.push_str(&css_link);
        }
        Ok(ctx)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_mount_div_with_escaped_props() {
        let mut block = BlockNode::new("aegis-plugin-react-island-template/counter");
        block.set_attribute("title", json!("Clicks <test>"));
        block.set_attribute("initial", json!(5));
        block.set_attribute("step", json!(2));

        let mut plugin = ReactIslandPlugin;
        let out = plugin.render_blocks(vec![block]).unwrap();
        let html = out[0].as_deref().unwrap();
        assert!(html.contains("data-aegis-island=\"react-counter\""));
        assert!(html.contains("&quot;initial&quot;:5"));
        assert!(html.contains("&quot;step&quot;:2"));
        assert!(html.contains("&lt;test&gt;"));
    }

    #[test]
    fn injects_assets_only_when_island_is_present() {
        let mut plugin = ReactIslandPlugin;

        let empty = plugin.on_render_page(PageRenderContext::default()).unwrap();
        assert!(empty.scripts.is_empty());
        assert!(empty.extra_head.is_empty());

        let with_island = plugin
            .on_render_page(PageRenderContext {
                content_html: "<div data-aegis-island=\"react-counter\"></div>".to_string(),
                ..Default::default()
            })
            .unwrap();
        assert_eq!(with_island.scripts.len(), 1);
        assert!(with_island
            .extra_head
            .contains("/plugins/aegis-plugin-react-island-template/assets/island.css"));
    }
}
