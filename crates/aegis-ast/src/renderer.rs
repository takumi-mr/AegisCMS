use crate::inline::{plain_text, render_inline};
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

/// 注記（`core/callout`）の種類。GitHub のアラートと同じ
pub const CALLOUT_VARIANTS: [&str; 5] = ["note", "tip", "important", "warning", "caution"];

/// 空でない文字列の属性
fn attr_str<'a>(node: &'a BlockNode, key: &str) -> Option<&'a str> {
    node.attributes
        .get(key)
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
}

/// 空行で区切ったテキストを段落（インライン書式つき）にする
fn render_paragraphs(text: &str) -> String {
    let normalized = text.replace("\r\n", "\n");
    let mut html = String::new();
    for para in normalized.split("\n\n") {
        let para = para.trim();
        if !para.is_empty() {
            html.push_str(&format!("<p>{}</p>\n", render_inline(para)));
        }
    }
    html
}

/// 表（`core/table`）。`attributes.rows` は文字列の配列の配列（1 行目が見出し行。`header: false` で見出し行なし）、
/// `attributes.align` は列ごとの `"left"` / `"center"` / `"right"`（`null` は指定なし）
fn render_table(node: &BlockNode) -> String {
    let rows: Vec<Vec<String>> = node
        .attributes
        .get("rows")
        .and_then(|v| v.as_array())
        .map(|rows| {
            rows.iter()
                .map(|row| {
                    row.as_array()
                        .map(|cells| {
                            cells
                                .iter()
                                .map(|c| match c {
                                    serde_json::Value::String(s) => s.clone(),
                                    serde_json::Value::Null => String::new(),
                                    other => other.to_string(),
                                })
                                .collect()
                        })
                        .unwrap_or_default()
                })
                .collect()
        })
        .unwrap_or_default();
    let columns = rows.iter().map(Vec::len).max().unwrap_or(0);
    if columns == 0 {
        return String::new();
    }
    let header = node
        .attributes
        .get("header")
        .and_then(|v| v.as_bool())
        .unwrap_or(true);
    let align: Vec<Option<&str>> = (0..columns)
        .map(|i| {
            node.attributes
                .get("align")
                .and_then(|v| v.get(i))
                .and_then(|v| v.as_str())
                .filter(|a| matches!(*a, "left" | "center" | "right"))
        })
        .collect();

    let row_html = |row: &[String], tag: &str| {
        let mut html = String::from("<tr>");
        for (i, a) in align.iter().enumerate() {
            let cell = row.get(i).map(String::as_str).unwrap_or("");
            let style = a
                .map(|a| format!(" style=\"text-align: {}\"", a))
                .unwrap_or_default();
            html.push_str(&format!("<{tag}{style}>{}</{tag}>", render_inline(cell)));
        }
        html.push_str("</tr>\n");
        html
    };

    let mut html = String::from("<div class=\"aegis-table\"><table>\n");
    let mut body = rows.as_slice();
    if header {
        html.push_str("<thead>\n");
        html.push_str(&row_html(&rows[0], "th"));
        html.push_str("</thead>\n");
        body = &rows[1..];
    }
    if !body.is_empty() {
        html.push_str("<tbody>\n");
        for row in body {
            html.push_str(&row_html(row, "td"));
        }
        html.push_str("</tbody>\n");
    }
    html.push_str("</table></div>\n");
    html
}

/// 目次（`core/toc`）。文書の見出し（H2 〜 `attributes.maxLevel`、既定 H3）を入れ子のリストにする。
/// リンク先は見出しの `id`（保存時に `HeadingIdTransformer` が付ける）
fn render_toc(node: &BlockNode, document: &[BlockNode]) -> String {
    let max_level = node
        .attributes
        .get("maxLevel")
        .and_then(|v| v.as_u64())
        .unwrap_or(3)
        .clamp(2, 6);
    let entries: Vec<(u64, Option<&str>, String)> = document
        .iter()
        .filter(|n| n.name == "core/heading")
        .filter_map(|n| {
            let level = n
                .attributes
                .get("level")
                .and_then(|v| v.as_u64())
                .unwrap_or(2);
            let text = plain_text(n.text.as_deref().unwrap_or_default());
            let text = text.trim();
            ((2..=max_level).contains(&level) && !text.is_empty()).then(|| {
                let id = n.attributes.get("id").and_then(|v| v.as_str());
                (level, id, text.to_string())
            })
        })
        .collect();
    if entries.is_empty() {
        return String::new();
    }

    let base = entries.iter().map(|e| e.0).min().unwrap_or(2);
    let mut list = String::from("<ol>");
    let mut depth = 0u64;
    let mut open_item = false;
    for (level, id, text) in &entries {
        // 1 段ずつしか深くしない（H2 の次の H4 は H3 の位置に）
        let mut target = (level - base).min(depth + 1);
        if !open_item {
            target = target.min(depth);
        }
        if target > depth {
            list.push_str("<ol>");
            depth = target;
        } else {
            if open_item {
                list.push_str("</li>");
            }
            while depth > target {
                list.push_str("</ol></li>");
                depth -= 1;
            }
        }
        match id {
            Some(id) => list.push_str(&format!(
                "<li><a href=\"#{}\">{}</a>",
                escape_html(id),
                escape_html(text)
            )),
            None => list.push_str(&format!("<li>{}", escape_html(text))),
        }
        open_item = true;
    }
    if open_item {
        list.push_str("</li>");
    }
    while depth > 0 {
        list.push_str("</ol></li>");
        depth -= 1;
    }
    list.push_str("</ol>");

    let title = attr_str(node, "title");
    let label = escape_html(title.unwrap_or("Contents"));
    let heading = title
        .map(|t| format!("<p class=\"aegis-toc-title\">{}</p>", escape_html(t)))
        .unwrap_or_default();
    format!("<nav class=\"aegis-toc\" aria-label=\"{label}\">{heading}{list}</nav>\n")
}

pub struct HtmlRenderer;

impl HtmlRenderer {
    pub fn render_nodes(nodes: &[BlockNode]) -> String {
        let mut html = String::new();
        for node in nodes {
            // 目次は文書全体の見出しから作るので、ここで描画する
            if node.name == "core/toc" {
                html.push_str(&render_toc(node, nodes));
            } else {
                html.push_str(&Self::render_node(node));
            }
        }
        html
    }

    pub fn render_node(node: &BlockNode) -> String {
        match node.name.as_str() {
            "core/paragraph" => {
                let inner = node.text.as_deref().map(render_inline).unwrap_or_default();
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
                let inner = node.text.as_deref().map(render_inline).unwrap_or_default();
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
                let inner = node.text.as_deref().map(render_inline).unwrap_or_default();
                format!("<li>{}</li>\n", inner)
            }
            "core/table" => render_table(node),
            // 文書の中にない目次（render_node を直接呼んだとき）は見出しが無いので出さない
            "core/toc" => render_toc(node, &[]),
            "core/quote" => {
                let body = render_paragraphs(node.text.as_deref().unwrap_or_default());
                if body.is_empty() {
                    return String::new();
                }
                match attr_str(node, "cite") {
                    Some(cite) => format!(
                        "<figure class=\"aegis-quote\"><blockquote>\n{body}</blockquote><figcaption>{}</figcaption></figure>\n",
                        render_inline(cite)
                    ),
                    None => format!("<blockquote>\n{body}</blockquote>\n"),
                }
            }
            "core/separator" => "<hr>\n".to_string(),
            "core/callout" => {
                let variant = attr_str(node, "variant")
                    .filter(|v| CALLOUT_VARIANTS.contains(v))
                    .unwrap_or("note");
                let title = attr_str(node, "title")
                    .map(|t| {
                        format!(
                            "<p class=\"aegis-callout-title\">{}</p>\n",
                            render_inline(t)
                        )
                    })
                    .unwrap_or_default();
                let body = render_paragraphs(node.text.as_deref().unwrap_or_default());
                if title.is_empty() && body.is_empty() {
                    return String::new();
                }
                format!(
                    "<aside class=\"aegis-callout aegis-callout-{variant}\" role=\"note\">\n{title}{body}</aside>\n"
                )
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
    fn heading(level: u8, text: &str, id: Option<&str>) -> BlockNode {
        let node = BlockNode::heading(level, text);
        match id {
            Some(id) => node.with_attr("id", id),
            None => node,
        }
    }

    #[test]
    fn test_inline_formatting_in_text_blocks() {
        let list = BlockNode::new("core/list").with_child({
            let mut item = BlockNode::new("core/list-item");
            item.text = Some("see [docs](/docs)".to_string());
            item
        });
        let html = HtmlRenderer::render_nodes(&[
            BlockNode::paragraph("a **b** <i>"),
            heading(2, "Use `cargo`", Some("use-cargo")),
            list,
        ]);
        assert!(
            html.contains("<p>a <strong>b</strong> &lt;i&gt;</p>"),
            "{}",
            html
        );
        assert!(html.contains("<h2 id=\"use-cargo\">Use <code>cargo</code></h2>"));
        assert!(html.contains("<li>see <a href=\"/docs\">docs</a></li>"));
    }

    #[test]
    fn test_table_block() {
        let table = BlockNode::new("core/table")
            .with_attr(
                "rows",
                serde_json::json!([["Name", "Price"], ["**Beans**", "1,200"], ["Short"]]),
            )
            .with_attr("align", serde_json::json!([null, "right"]));
        assert_eq!(
            HtmlRenderer::render_nodes(&[table.clone()]),
            "<div class=\"aegis-table\"><table>\n<thead>\n<tr><th>Name</th><th style=\"text-align: right\">Price</th></tr>\n</thead>\n<tbody>\n<tr><td><strong>Beans</strong></td><td style=\"text-align: right\">1,200</td></tr>\n<tr><td>Short</td><td style=\"text-align: right\"></td></tr>\n</tbody>\n</table></div>\n"
        );

        let no_header = table
            .with_attr("header", false)
            .with_attr("align", serde_json::json!(["\" onclick=\"x"]));
        let html = HtmlRenderer::render_nodes(&[no_header]);
        assert!(
            !html.contains("<thead>") && html.contains("<td>Name</td>"),
            "{}",
            html
        );
        assert!(!html.contains("onclick"));

        assert_eq!(
            HtmlRenderer::render_nodes(&[BlockNode::new("core/table")]),
            ""
        );
    }

    #[test]
    fn test_toc_block_nests_the_document_headings() {
        let blocks = vec![
            BlockNode::new("core/toc").with_attr("title", "目次"),
            heading(1, "Page title", Some("page-title")),
            heading(2, "Install", Some("install")),
            heading(3, "With **Docker**", Some("with-docker")),
            heading(4, "Too deep", Some("too-deep")),
            heading(2, "Use <it>", Some("use")),
            heading(3, "No id", None),
        ];
        let html = HtmlRenderer::render_nodes(&blocks);
        assert!(
            html.starts_with("<nav class=\"aegis-toc\" aria-label=\"目次\"><p class=\"aegis-toc-title\">目次</p><ol><li><a href=\"#install\">Install</a><ol><li><a href=\"#with-docker\">With Docker</a></li></ol></li><li><a href=\"#use\">Use &lt;it&gt;</a><ol><li>No id</li></ol></li></ol></nav>\n"),
            "{}",
            html
        );

        // maxLevel、見出しが深いところから始まる文書、見出しが無い文書
        let deep = vec![
            BlockNode::new("core/toc").with_attr("maxLevel", 4),
            heading(3, "C", Some("c")),
            heading(2, "B", Some("b")),
            heading(4, "D", Some("d")),
        ];
        let html = HtmlRenderer::render_nodes(&deep);
        assert!(
            html.contains("aria-label=\"Contents\"><ol><li><a href=\"#c\">C</a></li><li><a href=\"#b\">B</a><ol><li><a href=\"#d\">D</a></li></ol></li></ol>"),
            "{}",
            html
        );
        assert_eq!(
            HtmlRenderer::render_nodes(&[BlockNode::new("core/toc")]),
            ""
        );
    }

    #[test]
    fn test_quote_separator_and_callout_blocks() {
        let mut quote = BlockNode::new("core/quote");
        quote.text = Some("First *line*\n\nSecond".to_string());
        assert_eq!(
            HtmlRenderer::render_nodes(&[quote.clone()]),
            "<blockquote>\n<p>First <em>line</em></p>\n<p>Second</p>\n</blockquote>\n"
        );
        let cited = HtmlRenderer::render_nodes(&[quote.with_attr("cite", "Ada <L>")]);
        assert!(cited.starts_with("<figure class=\"aegis-quote\"><blockquote>"));
        assert!(cited.contains("<figcaption>Ada &lt;L&gt;</figcaption>"));

        assert_eq!(
            HtmlRenderer::render_nodes(&[BlockNode::new("core/separator")]),
            "<hr>\n"
        );

        let mut callout = BlockNode::new("core/callout")
            .with_attr("variant", "warning")
            .with_attr("title", "Back up first");
        callout.text = Some("Run `task backup`.".to_string());
        assert_eq!(
            HtmlRenderer::render_nodes(&[callout.clone()]),
            "<aside class=\"aegis-callout aegis-callout-warning\" role=\"note\">\n<p class=\"aegis-callout-title\">Back up first</p>\n<p>Run <code>task backup</code>.</p>\n</aside>\n"
        );
        // 知らない種類は note に
        let html = HtmlRenderer::render_nodes(&[callout.with_attr("variant", "x\" onclick=\"y")]);
        assert!(html.contains("aegis-callout-note") && !html.contains("onclick"));
        assert_eq!(
            HtmlRenderer::render_nodes(&[BlockNode::new("core/callout")]),
            ""
        );
    }
}
