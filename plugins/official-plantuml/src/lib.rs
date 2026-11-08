//! 公式プラグイン `official-plantuml`: ` ```plantuml ` のコードブロックを PlantUML の図にする（`on-render-page`）。
//!
//! - 本文の `language-plantuml`（`language-puml` も可）のコードブロックを、PlantUML サーバーが描く画像
//!   （`<img src="<サーバー>/svg/<符号化した図>">`）に置き換える。符号化（deflate + PlantUML 独自の base64）は
//!   このプラグインがサーバー側で行うので、**ページにスクリプトを入れない**
//! - 図の無いページには、何も足さない（CSS も入れない）
//! - `@startuml` / `@enduml` が無いブロックは、包んでから送る
//! - URL が長くなりすぎる図（符号化して [`MAX_ENCODED_LEN`] 文字を超える）は、元のコードのまま残す
//! - 画像は既定で公開の PlantUML サーバー（`www.plantuml.com`）から読む。マニフェストの
//!   `capabilities.csp.img_domains` にそのホストを宣言してあり、有効にしている間、サーバーが CSP の
//!   `img-src` に足す。**図のソースが PlantUML サーバーに送られる**ので、公開したくない図は自分の
//!   サーバー（`server_url`）を使う
//! - 設定は `[plugins.settings.official-plantuml]`（Admin Studio の Settings › Plugins）で変える

use aegis_plugin_sdk::prelude::*;

#[derive(Default)]
pub struct PlantUmlPlugin;

/// 公開の PlantUML サーバー
const DEFAULT_SERVER_URL: &str = "https://www.plantuml.com/plantuml";

const FORMATS: &[&str] = &["svg", "png"];

/// 符号化した図の最大の長さ。これを超えると URL が長すぎて、サーバー（多くは 8KB まで）が受け付けない
const MAX_ENCODED_LEN: usize = 6000;

/// コードブロックの言語（`language-<名前>`）
const LANGUAGES: &[&str] = &["plantuml", "puml"];

/// 図は白地で描かれるので、暗いページでも読めるよう枠の中を白にする
const CSS: &str = ".opu-diagram{display:flex;justify-content:center;align-items:center;margin:1.75rem 0;padding:1.5rem;\
border:1px solid rgba(127,127,127,.3);border-radius:12px;background:#fff;overflow-x:auto}\
.opu-diagram img{max-width:100%;height:auto}";

// --- 設定 ---

fn setting(key: &str) -> Option<String> {
    config::get(key)
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
}

/// 許す値の 1 つ（それ以外・未設定は `default`）
fn setting_choice(key: &str, allowed: &[&str], default: &str) -> String {
    setting(key)
        .map(|v| v.to_ascii_lowercase())
        .filter(|v| allowed.contains(&v.as_str()))
        .unwrap_or_else(|| default.to_string())
}

/// PlantUML サーバーの URL（末尾の `/` は除く）。`http(s)://` か、`/` で始まる（`//` ではない）相対 URL で、
/// 空白・制御文字・`?`・`#` を含まないもの
fn setting_server_url() -> String {
    setting("server_url")
        .map(|url| url.trim_end_matches('/').to_string())
        .filter(|url| {
            let lower = url.to_ascii_lowercase();
            let allowed = (lower.starts_with("https://") && lower.len() > "https://".len())
                || (lower.starts_with("http://") && lower.len() > "http://".len())
                || (url.starts_with('/') && !url.starts_with("//"));
            allowed
                && !url
                    .chars()
                    .any(|c| c.is_control() || c.is_whitespace() || c == '?' || c == '#')
        })
        .unwrap_or_else(|| DEFAULT_SERVER_URL.to_string())
}

// --- PlantUML の符号化 ---

/// PlantUML の URL 用の base64（`0-9A-Za-z-_`）
const ALPHABET: &[u8; 64] = b"0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz-_";

/// 図のソースを、PlantUML サーバーの URL に入れる形にする（raw deflate のあと PlantUML の base64）
pub fn encode(source: &str) -> String {
    let compressed = miniz_oxide::deflate::compress_to_vec(source.as_bytes(), 9);
    let mut out = String::with_capacity(compressed.len().div_ceil(3) * 4);
    for chunk in compressed.chunks(3) {
        let b1 = chunk[0];
        let b2 = chunk.get(1).copied().unwrap_or(0);
        let b3 = chunk.get(2).copied().unwrap_or(0);
        for index in [
            b1 >> 2,
            ((b1 & 0x3) << 4) | (b2 >> 4),
            ((b2 & 0xF) << 2) | (b3 >> 6),
            b3 & 0x3F,
        ] {
            out.push(ALPHABET[index as usize] as char);
        }
    }
    out
}

/// `@startuml` などで始まっていなければ、`@startuml` / `@enduml` で包む
fn with_start_end(source: &str) -> String {
    if source.trim_start().starts_with("@start") {
        source.to_string()
    } else {
        format!("@startuml\n{source}\n@enduml")
    }
}

// --- HTML ---

fn escape_attr(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('"', "&quot;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

/// コードブロックの中身（HTML）を文字に戻す。タグ（ほかのプラグインが足した `<span>` など）を除いて、
/// 文字参照を戻す
fn html_to_text(html: &str) -> String {
    let mut without_tags = String::with_capacity(html.len());
    let mut in_tag = false;
    for c in html.chars() {
        match c {
            '<' => in_tag = true,
            '>' if in_tag => in_tag = false,
            _ if !in_tag => without_tags.push(c),
            _ => {}
        }
    }
    unescape_html(&without_tags)
}

fn unescape_html(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(start) = rest.find('&') {
        out.push_str(&rest[..start]);
        rest = &rest[start..];
        let decoded = rest[1..]
            .find(';')
            .filter(|&end| end <= 10)
            .and_then(|end| decode_entity(&rest[1..1 + end]).map(|c| (c, end + 2)));
        match decoded {
            Some((c, len)) => {
                out.push(c);
                rest = &rest[len..];
            }
            None => {
                out.push('&');
                rest = &rest[1..];
            }
        }
    }
    out.push_str(rest);
    out
}

fn decode_entity(name: &str) -> Option<char> {
    match name {
        "lt" => Some('<'),
        "gt" => Some('>'),
        "amp" => Some('&'),
        "quot" => Some('"'),
        "apos" => Some('\''),
        "nbsp" => Some('\u{a0}'),
        _ => {
            let number = name.strip_prefix('#')?;
            let code = match number.strip_prefix(['x', 'X']) {
                Some(hex) => u32::from_str_radix(hex, 16).ok()?,
                None => number.parse().ok()?,
            };
            char::from_u32(code).filter(|c| *c != '\0')
        }
    }
}

/// 属性の値（`name="…"` か `name='…'`）
fn attribute<'a>(tag: &'a str, name: &str) -> Option<&'a str> {
    let lower = tag.to_ascii_lowercase();
    let mut from = 0;
    while let Some(pos) = lower[from..].find(name) {
        let at = from + pos;
        from = at + name.len();
        let before_ok = lower[..at].ends_with(|c: char| c.is_ascii_whitespace());
        let after = lower[from..].trim_start();
        if !before_ok || !after.starts_with('=') {
            continue;
        }
        let value_start = tag.len() - after.len() + 1;
        let value = tag[value_start..].trim_start();
        let quote = value.chars().next()?;
        if quote == '"' || quote == '\'' {
            let inner = &value[1..];
            return inner.find(quote).map(|end| &inner[..end]);
        }
        let end = value
            .find(|c: char| c.is_ascii_whitespace() || c == '>')
            .unwrap_or(value.len());
        return Some(&value[..end]);
    }
    None
}

/// `class` に `language-plantuml`（または `language-puml`）を持つか
fn is_plantuml_tag(tag: &str) -> bool {
    attribute(tag, "class").is_some_and(|class| {
        class.split_ascii_whitespace().any(|token| {
            token
                .strip_prefix("language-")
                .is_some_and(|lang| LANGUAGES.iter().any(|l| lang.eq_ignore_ascii_case(l)))
        })
    })
}

/// 図の `title`（1 行のもの）。画像の代替テキストに使う
fn diagram_title(source: &str) -> Option<String> {
    source.lines().find_map(|line| {
        let line = line.trim();
        let (keyword, rest) = line.split_at_checked(5)?;
        if !keyword.eq_ignore_ascii_case("title") || !rest.starts_with([' ', '\t']) {
            return None;
        }
        let title = rest.trim();
        let title = title
            .strip_prefix('"')
            .and_then(|t| t.strip_suffix('"'))
            .unwrap_or(title)
            .trim();
        (!title.is_empty()).then(|| title.to_string())
    })
}

/// 本文に PlantUML のコードブロックがありそうか（速い前判定）
fn may_have_diagram(content_html: &str) -> bool {
    let lower = content_html.to_ascii_lowercase();
    LANGUAGES
        .iter()
        .any(|lang| lower.contains(&format!("language-{lang}")))
}

struct Options {
    server_url: String,
    format: String,
    default_alt: &'static str,
}

/// 1 つの図の HTML。図が大きすぎるときは `None`（元のコードのまま残す）
fn diagram_html(source: &str, options: &Options) -> Option<String> {
    let source = source.trim_matches(['\n', '\r']);
    if source.trim().is_empty() {
        return None;
    }
    let encoded = encode(&with_start_end(source));
    if encoded.len() > MAX_ENCODED_LEN {
        return None;
    }
    let alt = diagram_title(source).unwrap_or_else(|| options.default_alt.to_string());
    Some(format!(
        "<figure class=\"opu-diagram\"><img src=\"{}/{}/{}\" alt=\"{}\" loading=\"lazy\" decoding=\"async\"></figure>",
        escape_attr(&options.server_url),
        options.format,
        encoded,
        escape_attr(&alt)
    ))
}

/// 本文の PlantUML のコードブロック（`<pre><code class="language-plantuml">…</code></pre>`）を、図に置き換える。
/// 置き換えた数を返す
fn replace_diagrams(content_html: &str, options: &Options) -> (String, usize) {
    let lower = content_html.to_ascii_lowercase();
    let mut out = String::with_capacity(content_html.len());
    let mut copied = 0;
    let mut search = 0;
    let mut count = 0;
    while let Some(pos) = lower[search..].find("<pre") {
        let pre_start = search + pos;
        search = pre_start + 4;
        // `<pre>` か `<pre ...>` だけ（`<preview>` などは対象外）
        if !lower[search..].starts_with(['>', ' ', '\t', '\n', '\r']) {
            continue;
        }
        let Some(pre_end) = lower[pre_start..]
            .find("</pre>")
            .map(|p| pre_start + p + "</pre>".len())
        else {
            break;
        };
        let block = &content_html[pre_start..pre_end];
        let block_lower = &lower[pre_start..pre_end];
        let Some(pre_tag_end) = block.find('>') else {
            continue;
        };
        let pre_tag = &block[..pre_tag_end];
        let inner = &block[pre_tag_end + 1..block.len() - "</pre>".len()];
        let inner_lower = &block_lower[pre_tag_end + 1..block.len() - "</pre>".len()];

        // `<pre><code class="language-plantuml">` か `<pre class="language-plantuml">`
        let inner_trimmed_offset = inner.len() - inner.trim_start().len();
        let source_html = if inner_lower[inner_trimmed_offset..].starts_with("<code") {
            let code = &inner[inner_trimmed_offset..];
            let code_lower = &inner_lower[inner_trimmed_offset..];
            match (code.find('>'), code_lower.rfind("</code>")) {
                (Some(tag_end), Some(close)) if tag_end < close => {
                    is_plantuml_tag(&code[..tag_end]).then(|| &code[tag_end + 1..close])
                }
                _ => None,
            }
        } else if is_plantuml_tag(pre_tag) {
            Some(inner)
        } else {
            None
        };

        if let Some(html) = diagram_html_from(source_html, options) {
            out.push_str(&content_html[copied..pre_start]);
            out.push_str(&html);
            copied = pre_end;
            count += 1;
        }
        search = pre_end;
    }
    out.push_str(&content_html[copied..]);
    (out, count)
}

fn diagram_html_from(source_html: Option<&str>, options: &Options) -> Option<String> {
    diagram_html(&html_to_text(source_html?), options)
}

impl Plugin for PlantUmlPlugin {
    fn on_render_page(&mut self, mut ctx: PageRenderContext) -> Result<PageRenderContext> {
        if !may_have_diagram(&ctx.content_html) {
            return Ok(ctx);
        }
        let ja = ctx
            .theme_vars
            .get("locale")
            .is_some_and(|locale| locale.starts_with("ja"));
        let options = Options {
            server_url: setting_server_url(),
            format: setting_choice("format", FORMATS, "svg"),
            default_alt: if ja {
                "PlantUML の図"
            } else {
                "PlantUML diagram"
            },
        };
        let (html, count) = replace_diagrams(&ctx.content_html, &options);
        if count == 0 {
            return Ok(ctx);
        }
        ctx.content_html = html;
        if !ctx.extra_css.is_empty() {
            ctx.extra_css.push('\n');
        }
        ctx.extra_css.push_str(CSS);
        Ok(ctx)
    }
}

aegis_plugin_sdk::register_plugin!(PlantUmlPlugin);

#[cfg(test)]
mod tests {
    use super::*;
    use aegis_plugin_sdk::host::mock;

    fn page(content_html: &str) -> PageRenderContext {
        PageRenderContext {
            template: "article.html".to_string(),
            content_type: "article".to_string(),
            content_html: content_html.to_string(),
            ..Default::default()
        }
    }

    /// PlantUML の base64 を戻して、deflate を解く（テスト用）
    fn decode(encoded: &str) -> String {
        let mut bytes = Vec::new();
        let values: Vec<u8> = encoded
            .bytes()
            .map(|b| ALPHABET.iter().position(|a| *a == b).unwrap() as u8)
            .collect();
        for chunk in values.chunks(4) {
            let [c1, c2, c3, c4] = [chunk[0], chunk[1], chunk[2], chunk[3]];
            bytes.push((c1 << 2) | (c2 >> 4));
            bytes.push(((c2 & 0xF) << 4) | (c3 >> 2));
            bytes.push(((c3 & 0x3) << 6) | c4);
        }
        // 最後の組の埋め草（0）は、deflate の終わりのあとなので無視される
        let raw = miniz_oxide::inflate::decompress_to_vec(&bytes).unwrap();
        String::from_utf8(raw).unwrap()
    }

    fn encoded_in(html: &str) -> String {
        let start = html.find("/svg/").expect("svg url") + "/svg/".len();
        let end = start + html[start..].find('"').unwrap();
        html[start..end].to_string()
    }

    const DIAGRAM: &str =
        "<pre><code class=\"language-plantuml\">Alice -&gt; Bob : hello &amp; bye\n</code></pre>";

    #[test]
    fn encodes_like_plantuml() {
        // URL に入る文字だけを使い、戻すと元のソースになる
        let encoded = encode("@startuml\nAlice -> Bob : hello\n@enduml");
        assert!(encoded
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_'));
        assert_eq!(encoded.len() % 4, 0);
        assert_eq!(decode(&encoded), "@startuml\nAlice -> Bob : hello\n@enduml");
    }

    #[test]
    fn reads_the_reference_encoding() {
        // PlantUML の説明（https://plantuml.com/text-encoding）の例。同じ規則（ビットの並び・文字の表）で
        // 戻せることで、符号化が PlantUML サーバーと合っていることを確かめる
        assert_eq!(
            decode("SyfFKj2rKt3CoKnELR1Io4ZDoSa70000"),
            "Bob -> Alice : hello"
        );
    }

    #[test]
    fn leaves_pages_without_a_diagram_alone() {
        mock::reset();
        for html in [
            "",
            "<p>No diagram</p>",
            "<pre><code class=\"language-rust\">fn main() {}</code></pre>",
            // 言語の名前を本文で書いただけ
            "<p>Use <code>language-plantuml</code> for diagrams</p>",
            // 空のブロック
            "<pre><code class=\"language-plantuml\">\n</code></pre>",
        ] {
            let out = PlantUmlPlugin.on_render_page(page(html)).unwrap();
            assert_eq!(out.content_html, html);
            assert!(
                out.extra_css.is_empty() && out.extra_head.is_empty(),
                "{html:?}"
            );
        }
    }

    #[test]
    fn replaces_the_code_block_with_an_image() {
        mock::reset();
        let html = format!("<p>Before</p>{DIAGRAM}<p>After</p>");
        let out = PlantUmlPlugin.on_render_page(page(&html)).unwrap();
        assert!(out.content_html.starts_with(
            "<p>Before</p><figure class=\"opu-diagram\"><img src=\"https://www.plantuml.com/plantuml/svg/"
        ));
        assert!(out.content_html.ends_with(
            "\" alt=\"PlantUML diagram\" loading=\"lazy\" decoding=\"async\"></figure><p>After</p>"
        ));
        assert!(!out.content_html.contains("<pre"));
        // 文字参照を戻し、@startuml / @enduml で包んでから送る
        assert_eq!(
            decode(&encoded_in(&out.content_html)),
            "@startuml\nAlice -> Bob : hello & bye\n@enduml"
        );
        assert!(out.extra_css.contains(".opu-diagram"));
        // スクリプトは入れない
        assert!(out.extra_head.is_empty());
    }

    #[test]
    fn handles_several_blocks_and_their_variants() {
        mock::reset();
        let html = "<pre><code class=\"language-plantuml\">@startuml\ntitle Login &quot;flow&quot;\nA -&gt; B\n@enduml</code></pre>\
<pre><code class=\"language-rust\">fn a() {}</code></pre>\
<pre class=\"line-numbers\"><code class=\"hljs language-PUML\" data-x='1'><span class=\"line\">A -&gt; B</span></code></pre>";
        let out = PlantUmlPlugin.on_render_page(page(html)).unwrap();
        assert_eq!(out.content_html.matches("opu-diagram").count(), 2);
        assert!(out
            .content_html
            .contains("<pre><code class=\"language-rust\">fn a() {}</code></pre>"));
        // `@startuml` があればそのまま。`title` は代替テキストになる
        assert!(out.content_html.contains("alt=\"Login &quot;flow&quot;\""));
        let first = encoded_in(&out.content_html);
        assert_eq!(
            decode(&first),
            "@startuml\ntitle Login \"flow\"\nA -> B\n@enduml"
        );
        // ほかのプラグインが足したタグは除く
        let second_start = out.content_html.rfind("/svg/").unwrap();
        let second = encoded_in(&out.content_html[second_start..]);
        assert_eq!(decode(&second), "@startuml\nA -> B\n@enduml");
        // CSS は 1 回だけ
        assert_eq!(out.extra_css.matches(".opu-diagram{").count(), 1);
    }

    #[test]
    fn uses_the_page_locale_for_the_alt_text() {
        mock::reset();
        let mut ctx = page(DIAGRAM);
        ctx.theme_vars
            .insert("locale".to_string(), "ja-JP".to_string());
        let out = PlantUmlPlugin.on_render_page(ctx).unwrap();
        assert!(out.content_html.contains("alt=\"PlantUML の図\""));
    }

    #[test]
    fn leaves_too_large_diagrams_as_code() {
        mock::reset();
        // 圧縮が効かない（乱数のような）長いソース
        let mut seed: u32 = 1;
        let noise: String = (0..12000)
            .map(|_| {
                seed = seed.wrapping_mul(1103515245).wrapping_add(12345);
                ALPHABET[(seed >> 16) as usize % 64] as char
            })
            .collect();
        let html =
            format!("<pre><code class=\"language-plantuml\">A -&gt; B : {noise}</code></pre>");
        let out = PlantUmlPlugin.on_render_page(page(&html)).unwrap();
        assert_eq!(out.content_html, html);
        assert!(out.extra_css.is_empty());
    }

    #[test]
    fn settings_change_the_server_and_the_format() {
        mock::reset();
        mock::set_config("server_url", "https://plantuml.example.com/plantuml/");
        mock::set_config("format", "PNG");
        let out = PlantUmlPlugin.on_render_page(page(DIAGRAM)).unwrap();
        assert!(out
            .content_html
            .contains("<img src=\"https://plantuml.example.com/plantuml/png/"));
        mock::set_config("server_url", "/plantuml");
        mock::set_config("format", "svg");
        let out = PlantUmlPlugin.on_render_page(page(DIAGRAM)).unwrap();
        assert!(out.content_html.contains("<img src=\"/plantuml/svg/"));
        mock::reset();
    }

    #[test]
    fn unsafe_or_unknown_settings_fall_back_to_the_defaults() {
        mock::reset();
        mock::set_config("format", "\"><script>alert(1)</script>");
        for bad in [
            "javascript:alert(1)",
            "//evil.example/plantuml",
            "https://a.example/p\" onerror=\"x()",
            "https://a.example/ p",
            "https://a.example/p?x=",
            "ftp://a.example/p",
            "https://",
        ] {
            mock::set_config("server_url", bad);
            let out = PlantUmlPlugin.on_render_page(page(DIAGRAM)).unwrap();
            assert!(
                out.content_html
                    .contains("<img src=\"https://www.plantuml.com/plantuml/svg/"),
                "{bad}"
            );
            assert!(!out.content_html.contains("alert(1)"));
        }
        // 代替テキストはエスケープする
        let out = PlantUmlPlugin
            .on_render_page(page(
                "<pre><code class=\"language-plantuml\">title &lt;b&gt;&quot; x\nA -&gt; B</code></pre>",
            ))
            .unwrap();
        assert!(out.content_html.contains("alt=\"&lt;b&gt;&quot; x\""));
        mock::reset();
    }

    #[test]
    fn appends_to_what_other_plugins_added() {
        mock::reset();
        let mut ctx = page(DIAGRAM);
        ctx.extra_css = "a{}".to_string();
        let out = PlantUmlPlugin.on_render_page(ctx).unwrap();
        assert!(out.extra_css.starts_with("a{}\n.opu-diagram"));
    }

    #[test]
    fn unescapes_html() {
        assert_eq!(
            html_to_text("a &lt;b&gt; &amp;amp; &#39;c&#x27; &unknown; & d"),
            "a <b> &amp; 'c' &unknown; & d"
        );
    }
}
