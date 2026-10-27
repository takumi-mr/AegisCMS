use crate::node::BlockNode;
use lol_html::{element, HtmlRewriter, Settings};

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

/// Unsafe URI schemes blocked on URL-bearing attributes in sanitized HTML mode.
fn is_unsafe_uri(val: &str) -> bool {
    let normalized: String = val
        .chars()
        .filter(|c| !c.is_control() && !c.is_whitespace())
        .collect::<String>()
        .to_ascii_lowercase();
    normalized.starts_with("javascript:")
        || normalized.starts_with("vbscript:")
        || normalized.starts_with("data:text/html")
        || normalized.starts_with("data:application/xhtml+xml")
}

/// Sanitizes a raw HTML fragment using `lol_html`, stripping executable scripts,
/// dangerous embedded frames/objects, inline event handlers (`on*`), and `javascript:` URLs.
pub fn sanitize_html_fragment(raw: &str) -> String {
    let mut output = Vec::with_capacity(raw.len());
    let settings = Settings::new()
        .append_element_content_handler(element!(
            "script, iframe, object, embed, applet, base, meta",
            |el| {
                el.remove();
                Ok(())
            }
        ))
        .append_element_content_handler(element!("*", |el| {
            let to_remove: Vec<String> = el
                .attributes()
                .iter()
                .filter_map(|attr| {
                    let name = attr.name().to_ascii_lowercase();
                    let val = attr.value();
                    if name.starts_with("on")
                        || name == "srcdoc"
                        || (matches!(
                            name.as_str(),
                            "href" | "src" | "action" | "formaction" | "xlink:href"
                        ) && is_unsafe_uri(&val))
                    {
                        Some(attr.name())
                    } else {
                        None
                    }
                })
                .collect();

            for attr_name in to_remove {
                el.remove_attribute(&attr_name);
            }
            Ok(())
        }));

    let mut rewriter = HtmlRewriter::new(settings, |c: &[u8]| output.extend_from_slice(c));
    if rewriter.write(raw.as_bytes()).is_ok() && rewriter.end().is_ok() {
        String::from_utf8(output).unwrap_or_else(|_| escape_html(raw))
    } else {
        escape_html(raw)
    }
}

/// Renders custom HTML/JS inside an opaque-origin `<iframe sandbox="allow-scripts ...">`
/// with automatic height synchronization via `postMessage`.
fn render_sandboxed_html(raw: &str) -> String {
    let srcdoc_document = format!(
        "<!DOCTYPE html><html><head><meta charset=\"UTF-8\">\
        <meta name=\"viewport\" content=\"width=device-width,initial-scale=1\">\
        <style>html,body{{margin:0;padding:0;font-family:system-ui,-apple-system,sans-serif;}}</style>\
        </head><body>{raw}<script>\
        (function(){{\
            function sync(){{\
                var h=Math.max(document.documentElement.scrollHeight,document.body?document.body.scrollHeight:0);\
                parent.postMessage({{type:'aegis-sandbox-resize',height:h}},'*');\
            }}\
            window.addEventListener('load',sync);\
            if(typeof ResizeObserver!=='undefined'){{\
                new ResizeObserver(sync).observe(document.documentElement);\
            }}\
            sync();\
        }})();\
        </script></body></html>"
    );

    let escaped_srcdoc = escape_html(&srcdoc_document);
    // Parent-side listener is idempotent (guarded by window.__aegisSandboxInit)
    format!(
        "<iframe class=\"aegis-html-sandbox\" sandbox=\"allow-scripts allow-forms allow-popups\" referrerpolicy=\"no-referrer\" loading=\"lazy\" style=\"width:100%;border:0;display:block;min-height:48px;\" srcdoc=\"{escaped_srcdoc}\"></iframe>\
        <script>(function(){{\
            if(window.__aegisSandboxInit)return;\
            window.__aegisSandboxInit=true;\
            window.addEventListener('message',function(e){{\
                if(!e.data||e.data.type!=='aegis-sandbox-resize'||typeof e.data.height!=='number')return;\
                var frames=document.querySelectorAll('iframe.aegis-html-sandbox');\
                for(var i=0;i<frames.length;i++){{\
                    if(frames[i].contentWindow===e.source){{\
                        frames[i].style.height=Math.max(16,Math.min(e.data.height,20000))+'px';\
                        break;\
                    }};\
                }};\
            }});\
        }})();</script>\n"
    )
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
                    return String::new();
                }
                let mode = node
                    .attributes
                    .get("mode")
                    .and_then(|v| v.as_str())
                    .unwrap_or("sanitized");
                match mode {
                    "sandbox" => render_sandboxed_html(inner),
                    _ => {
                        let clean = sanitize_html_fragment(inner);
                        if clean.trim().is_empty() {
                            String::new()
                        } else {
                            format!("{}\n", clean)
                        }
                    }
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

    #[test]
    fn test_core_html_sanitized_mode_strips_scripts_and_events() {
        let mut node = BlockNode::new("core/html");
        node.text = Some(
            r#"<div class="banner" onclick="alert(1)"><p>Safe</p><script>window.evil=true</script><a href="javascript:alert(2)">Bad Link</a><a href="/good">Good</a></div>"#.to_string(),
        );
        let rendered = HtmlRenderer::render_nodes(&[node]);
        assert_eq!(
            rendered,
            "<div class=\"banner\"><p>Safe</p><a>Bad Link</a><a href=\"/good\">Good</a></div>\n"
        );
    }

    #[test]
    fn test_core_html_sandbox_mode_renders_isolated_iframe() {
        let mut node = BlockNode::new("core/html").with_attr("mode", "sandbox");
        node.text = Some("<div id=\"app\"></div><script>console.log('ok')</script>".to_string());
        let rendered = HtmlRenderer::render_nodes(&[node]);
        assert!(rendered.contains("<iframe class=\"aegis-html-sandbox\""));
        assert!(rendered.contains("sandbox=\"allow-scripts allow-forms allow-popups\""));
        assert!(!rendered.contains("allow-same-origin"));
        assert!(rendered.contains("&lt;script&gt;console.log(&#x27;ok&#x27;)&lt;/script&gt;"));
    }
}
