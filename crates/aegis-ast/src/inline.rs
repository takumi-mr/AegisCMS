//! ブロックのテキストの中のインライン書式（Markdown の一部）。
//!
//! 段落・リスト・見出し・表のセル・引用・注記のテキストで使える書式:
//! `**太字**`、`*斜体*`、`~~取り消し線~~`、`` `コード` ``、`[リンク](URL)`、`\*`（記号をそのまま出す）。
//! 画像 `![代替テキスト](URL)` も使える（`[![alt](バッジの画像)](リンク)` でバッジになる）。
//! それ以外（HTML タグ・`_斜体_`・自動リンクなど）は文字のまま出力する。
//! リンクの URL は `http(s):`・`mailto:`・`tel:` とスキームの無い相対 URL だけを受け付け、
//! それ以外（`javascript:` など）のリンクはテキストだけを出す。画像の URL は
//! `http(s):` と相対 URL だけで、それ以外は代替テキストだけを出す。

use crate::footnote;
use crate::renderer::escape_html;

#[derive(Debug, Clone, PartialEq)]
enum Inline {
    Text(String),
    Code(String),
    Strong(Vec<Inline>),
    Em(Vec<Inline>),
    Del(Vec<Inline>),
    /// `href` が `None` なら安全でない URL（テキストだけを出す）
    Link(Vec<Inline>, Option<String>),
    /// `src` が `None` なら安全でない URL（代替テキストだけを出す）
    Image {
        alt: String,
        src: Option<String>,
    },
    /// 脚注の参照 `[^label]`（ラベルは `footnote::is_valid_label` を満たす）
    FootnoteRef(String),
}

/// インライン書式を HTML にする（テキストはエスケープする）
pub fn render_inline(text: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::with_capacity(text.len() + 16);
    write_html(&parse(&chars), &mut out);
    out
}

/// インライン書式の記号を取り除いたテキスト（見出しの id・目次・抜粋に使う）
pub fn plain_text(text: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::with_capacity(text.len());
    write_plain(&parse(&chars), &mut out);
    out
}

fn write_html(nodes: &[Inline], out: &mut String) {
    for node in nodes {
        match node {
            Inline::Text(t) => out.push_str(&escape_html(t)),
            Inline::Code(c) => {
                out.push_str("<code>");
                out.push_str(&escape_html(c));
                out.push_str("</code>");
            }
            Inline::Strong(inner) => wrap("strong", inner, out),
            Inline::Em(inner) => wrap("em", inner, out),
            Inline::Del(inner) => wrap("del", inner, out),
            Inline::Link(inner, Some(href)) => {
                out.push_str("<a href=\"");
                out.push_str(&escape_html(href));
                out.push_str("\">");
                write_html(inner, out);
                out.push_str("</a>");
            }
            Inline::Link(inner, None) => write_html(inner, out),
            Inline::Image {
                alt,
                src: Some(src),
            } => {
                out.push_str("<img src=\"");
                out.push_str(&escape_html(src));
                out.push_str("\" alt=\"");
                out.push_str(&escape_html(alt));
                out.push_str("\" loading=\"lazy\" decoding=\"async\">");
            }
            Inline::Image { alt, src: None } => out.push_str(&escape_html(alt)),
            // 定義が無い・文脈が無いときは文字のまま
            Inline::FootnoteRef(label) => match footnote::reference(label) {
                Some(html) => out.push_str(&html),
                None => {
                    out.push_str("[^");
                    out.push_str(label);
                    out.push(']');
                }
            },
        }
    }
}

fn wrap(tag: &str, inner: &[Inline], out: &mut String) {
    out.push('<');
    out.push_str(tag);
    out.push('>');
    write_html(inner, out);
    out.push_str("</");
    out.push_str(tag);
    out.push('>');
}

fn write_plain(nodes: &[Inline], out: &mut String) {
    for node in nodes {
        match node {
            Inline::Text(t) | Inline::Code(t) => out.push_str(t),
            Inline::Strong(inner) | Inline::Em(inner) | Inline::Del(inner) => {
                write_plain(inner, out)
            }
            Inline::Link(inner, _) => write_plain(inner, out),
            Inline::Image { alt, .. } => out.push_str(alt),
            // 見出しの id・目次・抜粋・検索では脚注の参照を取り除く
            Inline::FootnoteRef(_) => {}
        }
    }
}

/// 画像の URL として受け付けるか（http(s) とスキームの無い相対 URL。`mailto:` などは除く）
fn safe_src(url: &str) -> Option<String> {
    let href = safe_href(url)?;
    let before_path = href.split(['/', '?', '#']).next().unwrap_or("");
    (!before_path.contains(':') || href.to_ascii_lowercase().starts_with("http")).then_some(href)
}

/// リンクの URL として受け付けるか（スキームの無い相対 URL と http(s) / mailto / tel）
fn safe_href(url: &str) -> Option<String> {
    let url = url.trim();
    if url.is_empty() || url.chars().any(|c| c.is_control() || c.is_whitespace()) {
        return None;
    }
    // スキームの有無: 最初の `/` `?` `#` より前に `:` があればスキームとみなす
    let before_path = url.split(['/', '?', '#']).next().unwrap_or("");
    match before_path.split_once(':') {
        None => Some(url.to_string()),
        Some((scheme, _)) => {
            let scheme = scheme.to_ascii_lowercase();
            matches!(scheme.as_str(), "http" | "https" | "mailto" | "tel").then(|| url.to_string())
        }
    }
}

fn push_text(nodes: &mut Vec<Inline>, s: &str) {
    if let Some(Inline::Text(last)) = nodes.last_mut() {
        last.push_str(s);
    } else {
        nodes.push(Inline::Text(s.to_string()));
    }
}

fn starts_with(chars: &[char], i: usize, pat: &str) -> bool {
    pat.chars()
        .enumerate()
        .all(|(k, p)| chars.get(i + k) == Some(&p))
}

/// `i` から始まるコードスパン（`` ` `` の連続 n 個で開き、同じ数で閉じる）の終わり（閉じの直後）と中身
fn code_span(chars: &[char], i: usize) -> Option<(usize, String)> {
    let n = chars[i..].iter().take_while(|&&c| c == '`').count();
    let mut j = i + n;
    while j < chars.len() {
        if chars[j] == '`' {
            let run = chars[j..].iter().take_while(|&&c| c == '`').count();
            if run == n {
                let inner: String = chars[i + n..j].iter().collect();
                // 両端に空白が 1 つずつあれば取る（`` ` `a` ` `` のため）
                let inner = if inner.len() >= 2 && inner.starts_with(' ') && inner.ends_with(' ') {
                    inner[1..inner.len() - 1].to_string()
                } else {
                    inner
                };
                return Some((j + n, inner));
            }
            j += run;
        } else {
            j += 1;
        }
    }
    None
}

/// `start` 以降で、閉じの区切り `delim` の位置を探す（コードスパンの中は飛ばす）。
/// 閉じの直前は空白以外、`single_star` のときは `**` の一部でないこと
fn find_closing(chars: &[char], start: usize, delim: &str, single_star: bool) -> Option<usize> {
    let mut j = start;
    while j < chars.len() {
        match chars[j] {
            '\\' => j += 2,
            '`' => match code_span(chars, j) {
                Some((end, _)) => j = end,
                None => j += 1,
            },
            _ if starts_with(chars, j, delim) => {
                let prev_ok = j > start && !chars[j - 1].is_whitespace();
                if single_star && chars.get(j + 1) == Some(&'*') {
                    // `**` は中の太字として読み飛ばす
                    j += 2;
                    continue;
                }
                if prev_ok {
                    return Some(j);
                }
                j += delim.chars().count();
            }
            _ => j += 1,
        }
    }
    None
}

/// `[` に対応する `]` の位置（入れ子とコードスパンを考慮）
fn find_bracket(chars: &[char], open: usize) -> Option<usize> {
    let mut depth = 0;
    let mut j = open;
    while j < chars.len() {
        match chars[j] {
            '\\' => j += 1,
            '`' => {
                if let Some((end, _)) = code_span(chars, j) {
                    j = end;
                    continue;
                }
            }
            '[' => depth += 1,
            ']' => {
                depth -= 1;
                if depth == 0 {
                    return Some(j);
                }
            }
            _ => {}
        }
        j += 1;
    }
    None
}

/// `(` に対応する `)` の位置（URL の中の括弧の対を考慮）
fn find_paren(chars: &[char], open: usize) -> Option<usize> {
    let mut depth = 0;
    for (j, &c) in chars.iter().enumerate().skip(open) {
        match c {
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth == 0 {
                    return Some(j);
                }
            }
            '\n' => return None,
            _ => {}
        }
    }
    None
}

/// `i` から始まる脚注の参照 `[^label]` のラベルと、閉じの直後の位置
fn footnote_ref(chars: &[char], i: usize) -> Option<(String, usize)> {
    if chars.get(i + 1) != Some(&'^') {
        return None;
    }
    let close = chars[i + 2..].iter().position(|&c| c == ']')? + i + 2;
    let label: String = chars[i + 2..close].iter().collect();
    footnote::is_valid_label(&label).then_some((label, close + 1))
}

fn parse(chars: &[char]) -> Vec<Inline> {
    let mut nodes = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        match c {
            '\\' if chars.get(i + 1).is_some_and(|n| n.is_ascii_punctuation()) => {
                push_text(&mut nodes, &chars[i + 1].to_string());
                i += 2;
                continue;
            }
            '`' => {
                if let Some((end, inner)) = code_span(chars, i) {
                    nodes.push(Inline::Code(inner));
                    i = end;
                    continue;
                }
                // 閉じの無いバッククォートの連続はそのまま
                let n = chars[i..].iter().take_while(|&&c| c == '`').count();
                push_text(&mut nodes, &"`".repeat(n));
                i += n;
                continue;
            }
            '*' | '~' => {
                let (delim, single) = if starts_with(chars, i, "**") {
                    ("**", false)
                } else if starts_with(chars, i, "~~") {
                    ("~~", false)
                } else if c == '*' {
                    ("*", true)
                } else {
                    ("", false)
                };
                let len = delim.chars().count();
                // 開きの直後は空白以外
                if len > 0 && chars.get(i + len).is_some_and(|n| !n.is_whitespace()) {
                    if let Some(close) = find_closing(chars, i + len, delim, single) {
                        let inner = parse(&chars[i + len..close]);
                        nodes.push(match delim {
                            "**" => Inline::Strong(inner),
                            "~~" => Inline::Del(inner),
                            _ => Inline::Em(inner),
                        });
                        i = close + len;
                        continue;
                    }
                }
            }
            '!' if chars.get(i + 1) == Some(&'[') => {
                if let Some(close) = find_bracket(chars, i + 1) {
                    if chars.get(close + 1) == Some(&'(') {
                        if let Some(paren) = find_paren(chars, close + 1) {
                            let mut alt = String::new();
                            write_plain(&parse(&chars[i + 2..close]), &mut alt);
                            let url: String = chars[close + 2..paren].iter().collect();
                            nodes.push(Inline::Image {
                                alt,
                                src: safe_src(&url),
                            });
                            i = paren + 1;
                            continue;
                        }
                    }
                }
            }
            '[' => {
                if let Some((label, end)) = footnote_ref(chars, i) {
                    nodes.push(Inline::FootnoteRef(label));
                    i = end;
                    continue;
                }
                if let Some(close) = find_bracket(chars, i) {
                    if chars.get(close + 1) == Some(&'(') {
                        if let Some(paren) = find_paren(chars, close + 1) {
                            let label = parse(&chars[i + 1..close]);
                            let url: String = chars[close + 2..paren].iter().collect();
                            nodes.push(Inline::Link(label, safe_href(&url)));
                            i = paren + 1;
                            continue;
                        }
                    }
                }
            }
            _ => {}
        }
        push_text(&mut nodes, &c.to_string());
        i += 1;
    }
    nodes
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_formatting() {
        assert_eq!(
            render_inline("a **bold** and *em* and ~~gone~~ and `x < y`"),
            "a <strong>bold</strong> and <em>em</em> and <del>gone</del> and <code>x &lt; y</code>"
        );
        assert_eq!(
            render_inline("**bold with *em* inside**"),
            "<strong>bold with <em>em</em> inside</strong>"
        );
        assert_eq!(
            render_inline("*em with **bold***"),
            "<em>em with <strong>bold</strong></em>"
        );
    }

    #[test]
    fn test_text_is_escaped_and_html_stays_text() {
        assert_eq!(
            render_inline("<script>alert(1)</script> & \"q\""),
            "&lt;script&gt;alert(1)&lt;/script&gt; &amp; &quot;q&quot;"
        );
    }

    #[test]
    fn test_unmatched_and_spaced_markers_stay_literal() {
        assert_eq!(render_inline("2 * 3 * 4"), "2 * 3 * 4");
        assert_eq!(render_inline("**not closed"), "**not closed");
        assert_eq!(render_inline("a ` b"), "a ` b");
        assert_eq!(render_inline("snake_case_name"), "snake_case_name");
        assert_eq!(render_inline(r"\*literal\*"), "*literal*");
        assert_eq!(render_inline("[not a link] (x)"), "[not a link] (x)");
    }

    #[test]
    fn test_code_span_keeps_markers() {
        assert_eq!(render_inline("`**x**`"), "<code>**x**</code>");
        assert_eq!(render_inline("``a ` b``"), "<code>a ` b</code>");
        assert_eq!(
            render_inline("**a `*` b**"),
            "<strong>a <code>*</code> b</strong>"
        );
    }

    #[test]
    fn test_footnote_references_are_literal_without_a_document_and_stripped_from_plain_text() {
        assert_eq!(render_inline("a[^1] b"), "a[^1] b");
        assert_eq!(plain_text("a[^1] b[^note]"), "a b");
        // ラベルに使えない文字・空のラベルは参照にならない
        assert_eq!(plain_text("a[^x y] [^]"), "a[^x y] [^]");
        // エスケープすれば文字のまま
        assert_eq!(plain_text(r"a\[^1]"), "a[^1]");
    }

    #[test]
    fn test_links() {
        assert_eq!(
            render_inline("see [the **docs**](https://example.com/a?b=1&c=2)"),
            "see <a href=\"https://example.com/a?b=1&amp;c=2\">the <strong>docs</strong></a>"
        );
        assert_eq!(render_inline("[top](/)"), "<a href=\"/\">top</a>");
        assert_eq!(
            render_inline("[sec](#install)"),
            "<a href=\"#install\">sec</a>"
        );
        assert_eq!(
            render_inline("[wiki](https://en.wikipedia.org/wiki/Rust_(language))"),
            "<a href=\"https://en.wikipedia.org/wiki/Rust_(language)\">wiki</a>"
        );
        assert_eq!(
            render_inline("[mail](mailto:a@example.com)"),
            "<a href=\"mailto:a@example.com\">mail</a>"
        );
    }

    #[test]
    fn test_unsafe_links_keep_only_the_text() {
        for url in [
            "javascript:alert(1)",
            "JavaScript:alert(1)",
            "data:text/html,x",
            "vbscript:x",
            "java\tscript:x",
        ] {
            assert_eq!(
                render_inline(&format!("[click]({})", url)),
                "click",
                "{}",
                url
            );
        }
        assert_eq!(render_inline("[x](\" onmouseover=\"a)"), "x");
    }

    #[test]
    fn test_images_and_badges() {
        assert_eq!(
            render_inline("![logo](/uploads/a.png)"),
            "<img src=\"/uploads/a.png\" alt=\"logo\" loading=\"lazy\" decoding=\"async\">"
        );
        assert_eq!(
            render_inline(
                "[![CI](https://img.shields.io/github/actions/workflow/status/o/r/ci.yml?a=1&b=2)](https://github.com/o/r/actions)"
            ),
            "<a href=\"https://github.com/o/r/actions\"><img src=\"https://img.shields.io/github/actions/workflow/status/o/r/ci.yml?a=1&amp;b=2\" alt=\"CI\" loading=\"lazy\" decoding=\"async\"></a>"
        );
        // 続けて並べたバッジ
        let row =
            render_inline("[![a](https://x.test/a.svg)](/a) [![b](https://x.test/b.svg)](/b)");
        assert_eq!(row.matches("<a href=").count(), 2);
        assert_eq!(row.matches("<img ").count(), 2);
        // 代替テキストの書式は外し、エスケープする
        assert_eq!(
            render_inline("![**a** \"b\"](/x.png)"),
            "<img src=\"/x.png\" alt=\"a &quot;b&quot;\" loading=\"lazy\" decoding=\"async\">"
        );
        // `!` だけ・閉じの無いものはそのまま
        assert_eq!(render_inline("wow! [x]"), "wow! [x]");
        assert_eq!(render_inline("![alt]"), "![alt]");
    }

    #[test]
    fn test_unsafe_images_keep_only_the_alt_text() {
        for url in [
            "javascript:alert(1)",
            "data:image/svg+xml,x",
            "mailto:a@example.com",
            "x\" onerror=\"a",
        ] {
            assert_eq!(render_inline(&format!("![pic]({})", url)), "pic", "{}", url);
        }
    }

    #[test]
    fn test_plain_text_of_images() {
        assert_eq!(
            plain_text("[![build](https://x.test/b.svg)](/ci) ok"),
            "build ok"
        );
    }

    #[test]
    fn test_plain_text() {
        assert_eq!(
            plain_text("Install **AegisCMS** with `cargo` ([guide](/docs))"),
            "Install AegisCMS with cargo (guide)"
        );
    }
}
