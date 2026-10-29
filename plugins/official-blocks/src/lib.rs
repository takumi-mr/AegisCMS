//! 公式プラグイン `official-blocks`: 本文に置けるボタンとリンクカードのブロック（`render-blocks`）。
//! カスタムブロックを作るときのお手本でもある。
//!
//! - `official-blocks/button`: `label`・`url`・`style`（`primary` / `secondary`）・`align`（`left` / `center`）
//! - `official-blocks/link-card`: `url`・`title`・`description`・`image`
//!
//! 見た目は最初のブロックの前に 1 回だけ入れる `<style>`。配置・余白は `:where()` で詳細度 0
//! （テーマの CSS が優先）。リンクの色と下線だけは、テーマの本文のリンクの指定（`.prose a` など）に
//! 負けないようクラス 2 つの詳細度にする（負けると塗りつぶしのボタンの文字が背景と同じ色になる）。
//! 色はテーマの `--accent`（無ければ紫）と、塗りつぶしのボタンの文字の `--accent-contrast`（無ければ白）、
//! 文字色の濃淡だけを使う

use aegis_plugin_sdk::prelude::*;

#[derive(Default)]
pub struct BlocksPlugin;

const STYLES: &str = "<style>\
:where(.ob-button-row){margin:1.5rem 0}\
:where(.ob-button-row.ob-center){text-align:center}\
:where(.ob-button){display:inline-block;padding:.65rem 1.3rem;border-radius:8px;font-weight:600;border:1.5px solid var(--accent,#6366f1)}\
.ob-button-row .ob-button{text-decoration:none}\
.ob-button-row .ob-button-primary{background:var(--accent,#6366f1);color:var(--accent-contrast,#fff)}\
.ob-button-row .ob-button-secondary{color:var(--accent,#6366f1);background:transparent}\
:where(.ob-card){display:flex;gap:1rem;align-items:stretch;margin:1.5rem 0;border:1px solid rgba(127,127,127,.3);border-radius:10px;overflow:hidden}\
.aegis-plugin-block .ob-card{color:inherit;text-decoration:none}\
:where(.ob-card):hover{border-color:var(--accent,#6366f1)}\
:where(.ob-card-body){flex:1;min-width:0;padding:.9rem 1.1rem}\
:where(.ob-card-title){display:block;font-weight:700;margin-bottom:.25rem}\
:where(.ob-card-description){display:block;opacity:.75;font-size:.9em}\
:where(.ob-card-site){display:block;opacity:.6;font-size:.8em;margin-top:.4rem;overflow-wrap:anywhere}\
:where(.ob-card-image){flex:0 0 30%;max-width:200px;background:rgba(127,127,127,.1)}\
:where(.ob-card-image) img{width:100%;height:100%;object-fit:cover;display:block}\
</style>";

fn escape(text: &str) -> String {
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

fn attr<'a>(block: &'a BlockNode, name: &str) -> &'a str {
    block
        .attributes
        .get(name)
        .and_then(|v| v.as_str())
        .map(str::trim)
        .unwrap_or("")
}

/// リンクとして出してよい URL（http(s)・mailto とスキームの無い相対 URL）
fn safe_url(url: &str) -> Option<&str> {
    let before_path = url.split(['/', '?', '#']).next().unwrap_or("");
    let allowed = match before_path.split_once(':') {
        None => true,
        Some((scheme, _)) => matches!(
            scheme.to_ascii_lowercase().as_str(),
            "http" | "https" | "mailto"
        ),
    };
    (!url.is_empty() && allowed && !url.chars().any(char::is_whitespace)).then_some(url)
}

/// 外部のサイトか（`http(s)://` で始まる）
fn is_external(url: &str) -> bool {
    let lower = url.to_ascii_lowercase();
    lower.starts_with("http://") || lower.starts_with("https://")
}

/// リンクカードに出すサイト名（URL のホスト）
fn host_of(url: &str) -> Option<&str> {
    let rest = url.split_once("://")?.1;
    let host = rest.split(['/', '?', '#']).next()?;
    (!host.is_empty()).then_some(host)
}

fn render_button(block: &BlockNode) -> Option<String> {
    let label = attr(block, "label");
    let url = safe_url(attr(block, "url"))?;
    if label.is_empty() {
        return None;
    }
    let style = match attr(block, "style") {
        "secondary" => "secondary",
        _ => "primary",
    };
    let align = if attr(block, "align") == "center" {
        " ob-center"
    } else {
        ""
    };
    Some(format!(
        r#"<p class="ob-button-row{align}"><a class="ob-button ob-button-{style}" href="{}">{}</a></p>"#,
        escape(url),
        escape(label)
    ))
}

fn render_link_card(block: &BlockNode) -> Option<String> {
    let url = safe_url(attr(block, "url"))?;
    let title = attr(block, "title");
    if title.is_empty() {
        return None;
    }
    let description = attr(block, "description");
    let description = if description.is_empty() {
        String::new()
    } else {
        format!(
            r#"<span class="ob-card-description">{}</span>"#,
            escape(description)
        )
    };
    let site = if is_external(url) {
        host_of(url)
            .map(|h| format!(r#"<span class="ob-card-site">{}</span>"#, escape(h)))
            .unwrap_or_default()
    } else {
        String::new()
    };
    let image = safe_url(attr(block, "image"))
        .map(|src| {
            format!(
                r#"<span class="ob-card-image"><img src="{}" alt=""></span>"#,
                escape(src)
            )
        })
        .unwrap_or_default();
    Some(format!(
        r#"<a class="ob-card" href="{}"><span class="ob-card-body"><span class="ob-card-title">{}</span>{description}{site}</span>{image}</a>"#,
        escape(url),
        escape(title)
    ))
}

impl Plugin for BlocksPlugin {
    fn render_blocks(&mut self, blocks: Vec<BlockNode>) -> Result<Vec<Option<String>>> {
        let mut styled = false;
        Ok(blocks
            .iter()
            .map(|block| {
                let html = match block.name.as_str() {
                    "official-blocks/button" => render_button(block),
                    "official-blocks/link-card" => render_link_card(block),
                    _ => None,
                }?;
                // スタイルは最初に表示するブロックにだけ付ける
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

aegis_plugin_sdk::register_plugin!(BlocksPlugin);

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn block(name: &str, attrs: &[(&str, &str)]) -> BlockNode {
        BlockNode {
            name: format!("official-blocks/{}", name),
            attributes: attrs
                .iter()
                .map(|(k, v)| (k.to_string(), serde_json::json!(v)))
                .collect::<HashMap<_, _>>(),
            text: None,
            children: Vec::new(),
        }
    }

    #[test]
    fn renders_buttons_and_cards_with_the_styles_once() {
        let html = BlocksPlugin
            .render_blocks(vec![
                block(
                    "button",
                    &[
                        ("label", "Start <now>"),
                        ("url", "/start"),
                        ("align", "center"),
                    ],
                ),
                block(
                    "link-card",
                    &[
                        ("url", "https://example.com/docs?a=1"),
                        ("title", "Docs"),
                        ("description", "Read \"this\""),
                        ("image", "/uploads/a.png"),
                    ],
                ),
            ])
            .unwrap();
        assert_eq!(html.len(), 2);
        let button = html[0].as_deref().unwrap();
        assert!(button.starts_with("<style>"));
        assert!(button.ends_with(r#"<p class="ob-button-row ob-center"><a class="ob-button ob-button-primary" href="/start">Start &lt;now&gt;</a></p>"#));
        let card = html[1].as_deref().unwrap();
        assert!(!card.contains("<style>"));
        assert!(card.contains(r#"href="https://example.com/docs?a=1""#));
        assert!(card.contains("Read &quot;this&quot;"));
        assert!(card.contains(r#"<span class="ob-card-site">example.com</span>"#));
        assert!(card.contains(r#"<img src="/uploads/a.png" alt="">"#));
    }

    #[test]
    fn skips_incomplete_or_unsafe_blocks() {
        let html = BlocksPlugin
            .render_blocks(vec![
                block("button", &[("label", "x"), ("url", "javascript:alert(1)")]),
                block("button", &[("label", ""), ("url", "/a")]),
                block("link-card", &[("url", "https://a.example")]),
                block("unknown", &[]),
                block(
                    "button",
                    &[
                        ("label", "ok"),
                        ("url", "mailto:a@example.com"),
                        ("style", "secondary"),
                    ],
                ),
            ])
            .unwrap();
        assert_eq!(&html[..4], &[None, None, None, None]);
        // スタイルは実際に表示する最初のブロックに付く
        let last = html[4].as_deref().unwrap();
        assert!(last.starts_with("<style>") && last.contains("ob-button-secondary"));
    }
}
