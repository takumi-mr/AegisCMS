use crate::node::BlockNode;

pub fn escape_html(input: &str) -> String {
    let mut output = String::with_capacity(input.len());
    for c in input.chars() {
        match c {
            '&' => output.push_str("&amp;"),
            '<' => output.push_str("&lt;"),
            '>' => output.push_str("&gt;"),
            '"' => output.push_str("&quot;"),
            '\'' => output.push_str("&#x27;"),
            _ => output.push(c),
        }
    }
    output
}

pub struct HtmlRenderer;

impl HtmlRenderer {
    pub fn render_nodes(nodes: &[BlockNode]) -> String {
        let mut html = String::new();
        for node in nodes {
            html.push_str(&Self::render_node(node));
        }
        html
    }

    pub fn render_node(node: &BlockNode) -> String {
        match node.name.as_str() {
            "core/paragraph" => {
                let inner = node.text.as_deref().map(escape_html).unwrap_or_default();
                format!("<p>{}</p>\n", inner)
            }
            "core/heading" => {
                let level = node
                    .attributes
                    .get("level")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(2)
                    .clamp(1, 6);
                let id_attr = node
                    .attributes
                    .get("id")
                    .and_then(|v| v.as_str())
                    .map(|id| format!(" id=\"{}\"", escape_html(id)))
                    .unwrap_or_default();
                let inner = node.text.as_deref().map(escape_html).unwrap_or_default();
                format!("<h{level}{id_attr}>{inner}</h{level}>\n")
            }
            "core/code" => {
                let lang = node
                    .attributes
                    .get("language")
                    .and_then(|v| v.as_str())
                    .unwrap_or("text");
                let inner = node.text.as_deref().map(escape_html).unwrap_or_default();
                format!(
                    "<pre><code class=\"language-{}\">{}</code></pre>\n",
                    escape_html(lang),
                    inner
                )
            }
            "core/image" => {
                let src = node
                    .attributes
                    .get("src")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                // 画像未選択のブロックは出力しない（`<img src="">` は壊れた画像表示と現在ページへの再リクエストになる）
                if src.trim().is_empty() {
                    return String::new();
                }
                let alt = node
                    .attributes
                    .get("alt")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                format!(
                    "<figure><img src=\"{}\" alt=\"{}\" loading=\"lazy\" /></figure>\n",
                    escape_html(src),
                    escape_html(alt)
                )
            }
            "core/list" => {
                let ordered = node
                    .attributes
                    .get("ordered")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);
                let tag = if ordered { "ol" } else { "ul" };
                let mut children_html = String::new();
                for child in &node.children {
                    children_html.push_str(&Self::render_node(child));
                }
                format!("<{tag}>\n{children_html}</{tag}>\n")
            }
            "core/list-item" => {
                let inner = node.text.as_deref().map(escape_html).unwrap_or_default();
                format!("<li>{}</li>\n", inner)
            }
            "core/html" => {
                let inner = node.text.as_deref().unwrap_or_default();
                if inner.trim().is_empty() {
                    String::new()
                } else {
                    format!("{}\n", inner)
                }
            }
            _ => {
                // カスタム要素またはShadow DOM / Web Component対応
                // 名前が "custom/" やプラグイン名プレフィックスの場合
                let tag = node.name.replace('/', "-");
                let mut children_html = String::new();
                for child in &node.children {
                    children_html.push_str(&Self::render_node(child));
                }
                let text = node.text.as_deref().map(escape_html).unwrap_or_default();
                format!("<{tag}>{text}{children_html}</{tag}>\n")
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn image(src: &str) -> BlockNode {
        let mut node = BlockNode::new("core/image");
        node.attributes = serde_json::json!({ "src": src, "alt": "A \"quoted\" alt" });
        node
    }

    #[test]
    fn test_image_block_renders_a_figure() {
        assert_eq!(
            HtmlRenderer::render_nodes(&[image("/uploads/a.png")]),
            "<figure><img src=\"/uploads/a.png\" alt=\"A &quot;quoted&quot; alt\" loading=\"lazy\" /></figure>\n"
        );
    }

    #[test]
    fn test_image_block_without_src_renders_nothing() {
        assert_eq!(HtmlRenderer::render_nodes(&[image("")]), "");
        assert_eq!(HtmlRenderer::render_nodes(&[image("  ")]), "");
    }

    #[test]
    fn test_core_html_block_renders_raw_html() {
        let mut node = BlockNode::new("core/html");
        node.text = Some("<div class=\"custom-banner\"><p>Hello Custom HTML</p></div>".to_string());
        assert_eq!(
            HtmlRenderer::render_nodes(&[node]),
            "<div class=\"custom-banner\"><p>Hello Custom HTML</p></div>\n"
        );

        let mut empty_node = BlockNode::new("core/html");
        empty_node.text = Some("   ".to_string());
        assert_eq!(HtmlRenderer::render_nodes(&[empty_node]), "");
    }
}
