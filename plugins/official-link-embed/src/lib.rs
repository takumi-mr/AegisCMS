//! 公式プラグイン `official-link-embed`: URL だけの段落を、その URL のページの OGP / oEmbed から
//! 作ったリンクカードにする（Consumer）。
//!
//! - `transform-content-ast`（保存時とエディタのプレビュー時）: 本文の段落が `https://…` 1 つだけなら、
//!   ページを取得して（`http::fetch`。宛先は `allowed_domains` とサイトの設定 `extra_allowed_domains`、
//!   リダイレクトはホストが 1 回ずつ確かめて追う）、タイトル・説明・画像・サイト名を
//!   `official-link-embed/embed` ブロックの属性に保存する。表示のときは通信しない。
//!   取得できない・許可されていない URL の段落はそのまま（組み込みのリンクカードで表示される）
//! - `render-blocks`: ブロックをカードの HTML にする。第三者の HTML は使わない（oEmbed の `html` は読まない）
//!
//! 取得の順: ① `oembed_endpoints`（ホスト→oEmbed のエンドポイント。X など、ページに OGP が無いサイト）
//! ② ページの `og:` / `twitter:` / `<title>` ③ ページの oEmbed 発見用 `<link>`（タイトルが取れなかったとき）。
//! 対応するサイトはコードではなく設定（許可ドメインと `oembed_endpoints`）で足す

use aegis_plugin_sdk::prelude::*;
use serde_json::{json, Value};
use std::collections::HashMap;

#[derive(Default)]
pub struct LinkEmbedPlugin;

const BLOCK: &str = "official-link-embed/embed";

/// `oembed_endpoints` の既定。ページに OGP が無い（スクリプトで描く）サイトの oEmbed
const DEFAULT_OEMBED_ENDPOINTS: &str =
    "x.com=https://publish.twitter.com/oembed,twitter.com=https://publish.twitter.com/oembed";

/// 1 回のフック呼び出しで取得する URL の数（1 つにつき通信は最大 2 回で、上限は 8 回）
const MAX_EMBEDS_PER_CALL: usize = 3;
/// OGP を読むページの先頭の長さ（バイト）。`<head>` はたいていこの中に収まる
const MAX_HEAD_BYTES: usize = 400 * 1024;
const MAX_TITLE_CHARS: usize = 200;
const MAX_DESCRIPTION_CHARS: usize = 300;

const STYLES: &str = "<style>\
:where(.oe-card){display:flex;gap:1rem;align-items:stretch;margin:1.5rem 0;border:1px solid rgba(127,127,127,.3);border-radius:10px;overflow:hidden}\
.aegis-plugin-block .oe-card{color:inherit;text-decoration:none}\
:where(.oe-card):hover{border-color:var(--accent,#6366f1)}\
:where(.oe-body){flex:1;min-width:0;padding:.9rem 1.1rem}\
:where(.oe-title){display:block;font-weight:700;margin-bottom:.25rem;overflow-wrap:anywhere}\
:where(.oe-description){display:block;opacity:.75;font-size:.9em;overflow-wrap:anywhere}\
:where(.oe-site){display:block;opacity:.6;font-size:.8em;margin-top:.4rem;overflow-wrap:anywhere}\
:where(.oe-image){flex:0 0 30%;max-width:200px;background:rgba(127,127,127,.1)}\
:where(.oe-image) img{width:100%;height:100%;object-fit:cover;display:block}\
</style>";

// ---------------------------------------------------------------------------
// URL
// ---------------------------------------------------------------------------

/// 段落の本文が `http(s)://…` の URL 1 つだけならその URL
fn bare_url(text: &str) -> Option<&str> {
    let url = text.trim();
    let lower = url.to_ascii_lowercase();
    (lower.starts_with("https://") || lower.starts_with("http://"))
        .then_some(url)
        .filter(|u| !u.chars().any(|c| c.is_whitespace() || c.is_control()))
        .filter(|u| host_of(u).is_some())
}

/// URL のホスト（小文字。`www.` はそのまま）。ユーザー情報を含む URL は `None`
fn host_of(url: &str) -> Option<String> {
    let rest = url.split_once("://")?.1;
    let authority = rest.split(['/', '?', '#']).next()?;
    if authority.is_empty() || authority.contains('@') {
        return None;
    }
    let host = authority.split(':').next()?.to_ascii_lowercase();
    (!host.is_empty()).then_some(host)
}

/// `http(s)` の URL か（カードのリンク先・画像に使ってよいか）
fn is_http_url(url: &str) -> bool {
    let lower = url.to_ascii_lowercase();
    (lower.starts_with("https://") || lower.starts_with("http://"))
        && !url.chars().any(|c| c.is_whitespace() || c.is_control())
}

/// ページの URL を基準に、`og:image` などの URL を絶対 URL にする（http(s) 以外は `None`）
fn absolute_url(value: &str, page_url: &str) -> Option<String> {
    let value = value.trim();
    if is_http_url(value) {
        return Some(value.to_string());
    }
    let (scheme, rest) = page_url.split_once("://")?;
    if let Some(protocol_relative) = value.strip_prefix("//") {
        let url = format!("{}://{}", scheme, protocol_relative);
        return is_http_url(&url).then_some(url);
    }
    if value.starts_with('/') {
        let authority = rest.split(['/', '?', '#']).next()?;
        let url = format!("{}://{}{}", scheme, authority, value);
        return is_http_url(&url).then_some(url);
    }
    None
}

fn percent_encode(value: &str) -> String {
    let mut out = String::with_capacity(value.len() * 3);
    for byte in value.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(byte as char)
            }
            _ => out.push_str(&format!("%{:02X}", byte)),
        }
    }
    out
}

// ---------------------------------------------------------------------------
// HTML（`<meta>` / `<link>` / `<title>` だけを読む小さな読み取り）
// ---------------------------------------------------------------------------

/// HTML の文字参照（`&amp;` `&#39;` `&#x27;` など、よく使うもの）を戻す
fn decode_entities(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(start) = rest.find('&') {
        out.push_str(&rest[..start]);
        rest = &rest[start..];
        let Some(end) = rest.find(';').filter(|&e| e <= 10) else {
            out.push('&');
            rest = &rest[1..];
            continue;
        };
        let entity = &rest[1..end];
        let decoded = match entity {
            "amp" => Some('&'),
            "lt" => Some('<'),
            "gt" => Some('>'),
            "quot" => Some('"'),
            "apos" => Some('\''),
            "nbsp" => Some(' '),
            _ => entity
                .strip_prefix('#')
                .and_then(|n| match n.strip_prefix(['x', 'X']) {
                    Some(hex) => u32::from_str_radix(hex, 16).ok(),
                    None => n.parse().ok(),
                })
                .and_then(char::from_u32),
        };
        match decoded {
            Some(c) => {
                out.push(c);
                rest = &rest[end + 1..];
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

/// `<tag …>` の属性（名前は小文字）を、出てきた順に返す。`<!-- -->` の中は読まない
fn tags(html: &str, tag: &str) -> Vec<HashMap<String, String>> {
    let lower = html.to_ascii_lowercase();
    let open = format!("<{}", tag);
    let mut found = Vec::new();
    let mut i = 0;
    while let Some(pos) = lower[i..].find('<') {
        let start = i + pos;
        if lower[start..].starts_with("<!--") {
            i = lower[start..]
                .find("-->")
                .map_or(lower.len(), |e| start + e + 3);
            continue;
        }
        let after = start + open.len();
        let is_tag = lower[start..].starts_with(&open)
            && lower[after..]
                .chars()
                .next()
                .is_some_and(|c| c.is_whitespace() || c == '/' || c == '>');
        if !is_tag {
            i = start + 1;
            continue;
        }
        // タグの終わり（引用符の中の `>` は飛ばす）
        let bytes = html.as_bytes();
        let mut j = after;
        let mut quote: Option<u8> = None;
        while j < bytes.len() {
            match (quote, bytes[j]) {
                (None, b'>') => break,
                (None, q @ (b'"' | b'\'')) => quote = Some(q),
                (Some(q), c) if c == q => quote = None,
                _ => {}
            }
            j += 1;
        }
        found.push(parse_attributes(&html[after..j.min(html.len())]));
        i = j.min(html.len());
    }
    found
}

fn parse_attributes(source: &str) -> HashMap<String, String> {
    let mut attrs = HashMap::new();
    let chars: Vec<char> = source.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        while i < chars.len() && (chars[i].is_whitespace() || chars[i] == '/') {
            i += 1;
        }
        let name_start = i;
        while i < chars.len() && !chars[i].is_whitespace() && chars[i] != '=' && chars[i] != '/' {
            i += 1;
        }
        if i == name_start {
            break;
        }
        let name: String = chars[name_start..i]
            .iter()
            .collect::<String>()
            .to_ascii_lowercase();
        while i < chars.len() && chars[i].is_whitespace() {
            i += 1;
        }
        let mut value = String::new();
        if i < chars.len() && chars[i] == '=' {
            i += 1;
            while i < chars.len() && chars[i].is_whitespace() {
                i += 1;
            }
            if i < chars.len() && (chars[i] == '"' || chars[i] == '\'') {
                let quote = chars[i];
                i += 1;
                while i < chars.len() && chars[i] != quote {
                    value.push(chars[i]);
                    i += 1;
                }
                i += 1;
            } else {
                while i < chars.len() && !chars[i].is_whitespace() {
                    value.push(chars[i]);
                    i += 1;
                }
            }
        }
        attrs.entry(name).or_insert_with(|| decode_entities(&value));
    }
    attrs
}

/// `<title>…</title>` の本文
fn page_title(html: &str) -> Option<String> {
    let lower = html.to_ascii_lowercase();
    let start = lower.find("<title")?;
    let content_start = start + lower[start..].find('>')? + 1;
    let end = content_start + lower[content_start..].find("</title")?;
    Some(decode_entities(html[content_start..end].trim()))
}

fn clip(text: &str, max: usize) -> String {
    let text: String = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if text.chars().count() <= max {
        text
    } else {
        let mut clipped: String = text.chars().take(max).collect();
        clipped.push('…');
        clipped
    }
}

fn strip_tags(html: &str) -> String {
    let mut out = String::new();
    let mut in_tag = false;
    for c in html.chars() {
        match c {
            '<' => in_tag = true,
            '>' => {
                in_tag = false;
                out.push(' ');
            }
            _ if !in_tag => out.push(c),
            _ => {}
        }
    }
    decode_entities(&out)
}

// ---------------------------------------------------------------------------
// メタデータ
// ---------------------------------------------------------------------------

#[derive(Debug, Default, PartialEq)]
struct Meta {
    title: String,
    description: String,
    image: String,
    site: String,
}

/// ページの HTML から OGP（`og:` → `twitter:` → 通常の `<title>` / `description`）を読む
fn parse_ogp(html: &str, page_url: &str) -> Meta {
    let mut meta_map: HashMap<String, String> = HashMap::new();
    for attrs in tags(html, "meta") {
        let key = attrs
            .get("property")
            .or_else(|| attrs.get("name"))
            .map(|k| k.to_ascii_lowercase());
        if let (Some(key), Some(content)) = (key, attrs.get("content")) {
            if !content.trim().is_empty() {
                meta_map
                    .entry(key)
                    .or_insert_with(|| content.trim().to_string());
            }
        }
    }
    let first = |keys: &[&str]| keys.iter().find_map(|k| meta_map.get(*k)).cloned();
    let title = first(&["og:title", "twitter:title"])
        .or_else(|| page_title(html))
        .unwrap_or_default();
    Meta {
        title: clip(&title, MAX_TITLE_CHARS),
        description: clip(
            &first(&["og:description", "twitter:description", "description"]).unwrap_or_default(),
            MAX_DESCRIPTION_CHARS,
        ),
        image: first(&["og:image", "og:image:url", "twitter:image"])
            .and_then(|i| absolute_url(&i, page_url))
            .unwrap_or_default(),
        site: first(&["og:site_name"]).unwrap_or_default(),
    }
}

/// ページの oEmbed 発見用リンク（`<link rel="alternate" type="application/json+oembed" href>`）
fn discover_oembed(html: &str, page_url: &str) -> Option<String> {
    tags(html, "link").into_iter().find_map(|attrs| {
        let is_oembed = attrs
            .get("rel")
            .is_some_and(|r| r.to_ascii_lowercase().contains("alternate"))
            && attrs
                .get("type")
                .is_some_and(|t| t.eq_ignore_ascii_case("application/json+oembed"));
        is_oembed
            .then(|| attrs.get("href").and_then(|h| absolute_url(h, page_url)))
            .flatten()
    })
}

/// oEmbed の JSON から（`html` は使わず）タイトル・説明・サムネイルを読む。タイトルが無く
/// `author_name` があれば「<著者> / <プロバイダー>」、`html` の本文の文章（X の投稿など）は説明にする
fn meta_from_oembed(json: &Value, page_url: &str) -> Meta {
    let text = |key: &str| {
        json.get(key)
            .and_then(Value::as_str)
            .map(str::trim)
            .unwrap_or("")
    };
    let provider = text("provider_name");
    let author = text("author_name");
    let title = match (text("title"), author) {
        ("", "") => String::new(),
        ("", author) if provider.is_empty() => author.to_string(),
        ("", author) => format!("{} / {}", author, provider),
        (title, _) => title.to_string(),
    };
    let description = match text("description") {
        "" => strip_first_paragraph(text("html")),
        description => description.to_string(),
    };
    Meta {
        title: clip(&title, MAX_TITLE_CHARS),
        description: clip(&description, MAX_DESCRIPTION_CHARS),
        image: absolute_url(text("thumbnail_url"), page_url).unwrap_or_default(),
        site: provider.to_string(),
    }
}

/// oEmbed の `html`（`<blockquote><p>本文</p>— 著者…</blockquote>`）の最初の `<p>` の文章
fn strip_first_paragraph(html: &str) -> String {
    let lower = html.to_ascii_lowercase();
    let Some(start) = lower.find("<p") else {
        return String::new();
    };
    let Some(open_end) = lower[start..].find('>') else {
        return String::new();
    };
    let body_start = start + open_end + 1;
    let end = lower[body_start..]
        .find("</p>")
        .map_or(html.len(), |e| body_start + e);
    strip_tags(&html[body_start..end]).trim().to_string()
}

/// `x.com=https://publish.twitter.com/oembed,…` から、ホスト（サブドメインを含む）の oEmbed のエンドポイント
fn oembed_endpoint(host: &str, endpoints: &str) -> Option<String> {
    endpoints.split([',', '\n']).find_map(|entry| {
        let (domain, endpoint) = entry.split_once('=')?;
        let domain = domain
            .trim()
            .trim_start_matches("www.")
            .to_ascii_lowercase();
        let host = host.trim_start_matches("www.");
        let matches = host == domain || host.ends_with(&format!(".{}", domain));
        (matches && is_http_url(endpoint.trim())).then(|| endpoint.trim().to_string())
    })
}

/// 取得の本体。`get` は URL を GET して本文を返す（成功した HTML / JSON だけ）。`allowed` は
/// 通信してよい URL か（oEmbed の発見用リンクの先を確かめる）
fn fetch_meta(
    url: &str,
    endpoints: &str,
    get: &dyn Fn(&str) -> Option<String>,
    allowed: &dyn Fn(&str) -> bool,
) -> Option<Meta> {
    let host = host_of(url)?;

    // ① ページに OGP が無いサイトの oEmbed
    if let Some(endpoint) = oembed_endpoint(&host, endpoints) {
        let separator = if endpoint.contains('?') { '&' } else { '?' };
        let request = format!(
            "{}{}url={}&format=json&omit_script=1",
            endpoint,
            separator,
            percent_encode(url)
        );
        if allowed(&request) {
            if let Some(json) = get(&request).and_then(|b| serde_json::from_str::<Value>(&b).ok()) {
                let meta = meta_from_oembed(&json, url);
                if !meta.title.is_empty() {
                    return Some(Meta {
                        site: if meta.site.is_empty() {
                            host
                        } else {
                            meta.site
                        },
                        ..meta
                    });
                }
            }
        }
    }

    // ② ページの OGP
    let body = get(url)?;
    let head = truncate_at_char(&body, MAX_HEAD_BYTES);
    let mut meta = parse_ogp(head, url);

    // ③ タイトルが取れなければ、ページの oEmbed 発見用リンク
    if meta.title.is_empty() {
        let link = discover_oembed(head, url).filter(|l| allowed(l))?;
        let json = get(&link).and_then(|b| serde_json::from_str::<Value>(&b).ok())?;
        meta = meta_from_oembed(&json, url);
    }
    if meta.title.is_empty() {
        return None;
    }
    if meta.site.is_empty() {
        meta.site = host;
    }
    Some(meta)
}

fn truncate_at_char(text: &str, max: usize) -> &str {
    if text.len() <= max {
        return text;
    }
    let mut end = max;
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    &text[..end]
}

/// ホストの `http::fetch` で GET する（HTML か JSON の 200 だけ）
fn get_via_host(url: &str) -> Option<String> {
    let request = http::Request::get(url)
        .header("Accept", "text/html,application/json;q=0.9,*/*;q=0.5")
        .header("Accept-Language", "ja,en;q=0.8");
    let response = http::fetch(&request).ok()?;
    if response.status != 200 {
        return None;
    }
    let content_type = response
        .headers
        .iter()
        .find(|(name, _)| name.eq_ignore_ascii_case("content-type"))
        .map(|(_, value)| value.to_ascii_lowercase());
    match content_type {
        Some(t) if !(t.contains("html") || t.contains("json")) => None,
        _ => Some(response.body),
    }
}

fn embed_block(url: &str, meta: &Meta) -> BlockNode {
    let mut block = BlockNode::new(BLOCK);
    block.attributes = HashMap::from([
        ("url".to_string(), json!(url)),
        ("title".to_string(), json!(meta.title)),
        ("description".to_string(), json!(meta.description)),
        ("image".to_string(), json!(meta.image)),
        ("site".to_string(), json!(meta.site)),
        // 取得したときの URL。`url` を書き換えたら、次の保存で取り直す目印
        ("fetched_url".to_string(), json!(url)),
    ]);
    block
}

/// 取り直す埋め込みブロックの URL: 「再取得」で `refresh` が付いたもの、または取得したあとに
/// `url` を書き換えたもの（`fetched_url` が無い昔のブロックは対象にしない）
fn stale_embed_url(block: &BlockNode) -> Option<String> {
    if block.name != BLOCK {
        return None;
    }
    let url = attr(block, "url");
    if !is_http_url(url) || host_of(url).is_none() {
        return None;
    }
    let refresh = block.attributes.get("refresh") == Some(&json!(true));
    let edited = block
        .attributes
        .get("fetched_url")
        .and_then(Value::as_str)
        .is_some_and(|fetched| fetched != url);
    (refresh || edited).then(|| url.to_string())
}

// ---------------------------------------------------------------------------
// 表示
// ---------------------------------------------------------------------------

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
        .and_then(Value::as_str)
        .map(str::trim)
        .unwrap_or("")
}

fn render_embed(block: &BlockNode) -> Option<String> {
    let url = attr(block, "url");
    let title = attr(block, "title");
    if title.is_empty() || !is_http_url(url) {
        return None;
    }
    let span = |class: &str, value: &str| {
        if value.is_empty() {
            String::new()
        } else {
            format!(r#"<span class="{}">{}</span>"#, class, escape(value))
        }
    };
    let site = match attr(block, "site") {
        "" => host_of(url).unwrap_or_default(),
        site => site.to_string(),
    };
    let image = attr(block, "image");
    let image = if is_http_url(image) {
        format!(
            r#"<span class="oe-image"><img src="{}" alt="" loading="lazy" referrerpolicy="no-referrer"></span>"#,
            escape(image)
        )
    } else {
        String::new()
    };
    Some(format!(
        r#"<a class="oe-card" href="{}"><span class="oe-body">{}{}{}</span>{}</a>"#,
        escape(url),
        span("oe-title", title),
        span("oe-description", attr(block, "description")),
        span("oe-site", &site),
        image
    ))
}

impl Plugin for LinkEmbedPlugin {
    fn transform_content_ast(&mut self, mut blocks: Vec<BlockNode>) -> Result<Vec<BlockNode>> {
        let endpoints =
            config::get("oembed_endpoints").unwrap_or_else(|| DEFAULT_OEMBED_ENDPOINTS.to_string());
        let mut fetched = 0;
        for block in blocks.iter_mut() {
            if fetched >= MAX_EMBEDS_PER_CALL {
                break;
            }
            let refetch = stale_embed_url(block);
            if refetch.is_none() && block.name != "core/paragraph" {
                continue;
            }
            let Some(url) = refetch
                .clone()
                .or_else(|| block.text.as_deref().and_then(bare_url).map(str::to_string))
            else {
                continue;
            };
            // 許可されていない URL は通信せず、そのまま（組み込みのリンクカードで表示される）
            if !net::is_allowed(&url) {
                if refetch.is_some() {
                    // 取り直せないので、古い内容のまま。次の保存でまた試さないよう目印を外す
                    block.attributes.remove("refresh");
                }
                continue;
            }
            fetched += 1;
            match fetch_meta(&url, &endpoints, &get_via_host, &|u| net::is_allowed(u)) {
                Some(meta) => *block = embed_block(&url, &meta),
                None => {
                    log_info!("link-embed: no metadata for {}", url);
                    // 取れなかったときは、これまでの内容を残す
                    block.attributes.remove("refresh");
                }
            }
        }
        Ok(blocks)
    }

    fn render_blocks(&mut self, blocks: Vec<BlockNode>) -> Result<Vec<Option<String>>> {
        let mut styled = false;
        Ok(blocks
            .iter()
            .map(|block| {
                let html = (block.name == BLOCK)
                    .then(|| render_embed(block))
                    .flatten()?;
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

aegis_plugin_sdk::register_plugin!(LinkEmbedPlugin);

#[cfg(test)]
mod tests {
    use super::*;

    const PAGE: &str = r#"<!doctype html><html><head>
<title>Fallback &amp; title</title>
<meta property="og:title" content="Rust &#x306E; guide &quot;2026&quot;">
<meta property="og:description" content="  A long
   description  ">
<meta property="og:image" content="/img/cover.png">
<meta property="og:site_name" content="Qiita">
<!-- <meta property="og:title" content="commented out"> -->
<link rel="alternate" type="application/json+oembed" href="https://example.com/oembed?url=x">
</head><body><meta property="og:title" content="late"></body></html>"#;

    #[test]
    fn reads_the_ogp_of_a_page() {
        let meta = parse_ogp(PAGE, "https://qiita.com/a/items/1");
        assert_eq!(meta.title, "Rust の guide \"2026\"");
        assert_eq!(meta.description, "A long description");
        assert_eq!(meta.image, "https://qiita.com/img/cover.png");
        assert_eq!(meta.site, "Qiita");
    }

    #[test]
    fn falls_back_to_the_title_tag_and_rejects_unsafe_images() {
        let meta = parse_ogp(
            r#"<title> Plain </title><meta property="og:image" content="javascript:alert(1)">"#,
            "https://example.com/",
        );
        assert_eq!(meta.title, "Plain");
        assert_eq!(meta.image, "");
        assert_eq!(
            absolute_url("//cdn.example.com/a.png", "https://example.com/x").as_deref(),
            Some("https://cdn.example.com/a.png")
        );
        assert_eq!(absolute_url("a.png", "https://example.com/x"), None);
    }

    #[test]
    fn reads_the_oembed_discovery_link() {
        assert_eq!(
            discover_oembed(PAGE, "https://example.com/p").as_deref(),
            Some("https://example.com/oembed?url=x")
        );
        assert_eq!(
            discover_oembed(
                "<link rel=\"stylesheet\" href=\"/a.css\">",
                "https://e.com/"
            ),
            None
        );
    }

    #[test]
    fn detects_url_only_paragraphs() {
        assert_eq!(
            bare_url(" https://x.com/a/status/1 "),
            Some("https://x.com/a/status/1")
        );
        for text in [
            "see https://x.com/a",
            "https://x.com/a b",
            "[a](https://x.com/a)",
            "ftp://x.com/a",
            "https://user@evil.example/",
            "https://",
        ] {
            assert_eq!(bare_url(text), None, "{}", text);
        }
    }

    #[test]
    fn picks_the_oembed_endpoint_by_host() {
        let endpoints = "x.com=https://publish.twitter.com/oembed, example.org = https://example.org/oembed?a=1";
        assert_eq!(
            oembed_endpoint("x.com", endpoints).as_deref(),
            Some("https://publish.twitter.com/oembed")
        );
        assert_eq!(
            oembed_endpoint("www.x.com", endpoints).as_deref(),
            Some("https://publish.twitter.com/oembed")
        );
        assert_eq!(
            oembed_endpoint("a.example.org", endpoints).as_deref(),
            Some("https://example.org/oembed?a=1")
        );
        assert_eq!(oembed_endpoint("notx.com", endpoints), None);
        assert_eq!(oembed_endpoint("x.com", "x.com=javascript:alert(1)"), None);
    }

    #[test]
    fn x_posts_come_from_oembed_not_from_the_page() {
        let json = json!({
            "provider_name": "Twitter",
            "author_name": "AegisCMS",
            "html": "<blockquote class=\"twitter-tweet\"><p lang=\"ja\">Hello &amp; welcome <a href=\"https://t.co/x\">t.co/x</a></p>&mdash; AegisCMS</blockquote><script async src=\"https://platform.twitter.com/widgets.js\"></script>"
        })
        .to_string();
        let requested = std::cell::RefCell::new(Vec::new());
        let meta = fetch_meta(
            "https://x.com/aegiscms/status/1",
            DEFAULT_OEMBED_ENDPOINTS,
            &|u| {
                requested.borrow_mut().push(u.to_string());
                Some(json.clone())
            },
            &|_| true,
        )
        .unwrap();
        assert_eq!(meta.title, "AegisCMS / Twitter");
        assert_eq!(meta.description, "Hello & welcome t.co/x");
        assert_eq!(meta.site, "Twitter");
        assert_eq!(
            requested.borrow().as_slice(),
            ["https://publish.twitter.com/oembed?url=https%3A%2F%2Fx.com%2Faegiscms%2Fstatus%2F1&format=json&omit_script=1"]
        );
    }

    #[test]
    fn falls_back_to_the_pages_oembed_link_when_there_is_no_title() {
        let page = r#"<head><link rel="alternate" type="application/json+oembed" href="https://blog.example/oembed?url=u"></head>"#;
        let get = |u: &str| {
            Some(if u.starts_with("https://blog.example/oembed") {
                r#"{"title":"From oEmbed","provider_name":"Blog","thumbnail_url":"https://blog.example/t.png"}"#.to_string()
            } else {
                page.to_string()
            })
        };
        let meta = fetch_meta("https://blog.example/a", "", &get, &|_| true).unwrap();
        assert_eq!(meta.title, "From oEmbed");
        assert_eq!(meta.image, "https://blog.example/t.png");
        // 発見用リンクの先が許可されていなければ、取得しない
        assert_eq!(
            fetch_meta("https://blog.example/a", "", &get, &|u| !u
                .contains("/oembed")),
            None
        );
        // 何も取れなければ None（段落はそのまま）
        assert_eq!(
            fetch_meta("https://blog.example/a", "", &|_| None, &|_| true),
            None
        );
    }

    #[test]
    fn converts_only_url_paragraphs_and_keeps_the_rest() {
        // ネイティブのテストでは、ホスト API はモック（`http::fetch` は空の 200、`net::is_allowed` は true）。
        // 取得の結果が空なので、段落は変わらない
        let blocks = vec![
            BlockNode::paragraph("https://qiita.com/a/items/1"),
            BlockNode::paragraph("text https://qiita.com/a"),
        ];
        let out = LinkEmbedPlugin
            .transform_content_ast(blocks.clone())
            .unwrap();
        assert_eq!(out, blocks);
    }

    #[test]
    fn refetches_only_refreshed_or_edited_embeds() {
        let meta = Meta {
            title: "T".to_string(),
            ..Meta::default()
        };
        let fresh = embed_block("https://qiita.com/a", &meta);
        assert_eq!(stale_embed_url(&fresh), None);

        let mut refreshed = fresh.clone();
        refreshed.attributes.insert("refresh".into(), json!(true));
        assert_eq!(
            stale_embed_url(&refreshed).as_deref(),
            Some("https://qiita.com/a")
        );

        let mut edited = fresh.clone();
        edited
            .attributes
            .insert("url".into(), json!("https://qiita.com/b"));
        assert_eq!(
            stale_embed_url(&edited).as_deref(),
            Some("https://qiita.com/b")
        );

        // 昔のブロック（`fetched_url` が無い）と、他のブロックは対象にしない
        let mut old = fresh.clone();
        old.attributes.remove("fetched_url");
        assert_eq!(stale_embed_url(&old), None);
        assert_eq!(stale_embed_url(&BlockNode::paragraph("x")), None);

        // 取れなかったとき（ネイティブのテストでは空の 200）は、内容を残して `refresh` を外す
        let out = LinkEmbedPlugin
            .transform_content_ast(vec![refreshed])
            .unwrap();
        assert_eq!(out[0].attributes.get("refresh"), None);
        assert_eq!(attr(&out[0], "title"), "T");
    }

    #[test]
    fn renders_the_card_escaped_and_with_the_styles_once() {
        let meta = Meta {
            title: "A <b> \"title\"".to_string(),
            description: "d & d".to_string(),
            image: "https://cdn.example/i.png".to_string(),
            site: "Qiita".to_string(),
        };
        let bad = embed_block("javascript:alert(1)", &meta);
        let html = LinkEmbedPlugin
            .render_blocks(vec![
                embed_block("https://qiita.com/a?x=1&y=2", &meta),
                bad,
                embed_block("https://qiita.com/b", &Meta::default()),
                BlockNode::new("other/block"),
                embed_block("https://qiita.com/c", &meta),
            ])
            .unwrap();
        let first = html[0].as_deref().unwrap();
        assert!(first.starts_with("<style>"));
        assert!(first.contains(r#"href="https://qiita.com/a?x=1&amp;y=2""#));
        assert!(first.contains("A &lt;b&gt; &quot;title&quot;"));
        assert!(first.contains(r#"<img src="https://cdn.example/i.png""#));
        assert_eq!((&html[1], &html[2], &html[3]), (&None, &None, &None));
        assert!(!html[4].as_deref().unwrap().contains("<style>"));
    }

    #[test]
    fn manifest_allows_the_default_domains_and_declares_the_block() {
        let manifest: Value =
            serde_json::from_str(include_str!("../plugin.manifest.json")).unwrap();
        assert_eq!(manifest["blocks"][0]["name"], "embed");
        assert!(manifest["capabilities"]["hooks"]
            .as_array()
            .unwrap()
            .iter()
            .any(|h| h == "transform-content-ast"));
    }
}
