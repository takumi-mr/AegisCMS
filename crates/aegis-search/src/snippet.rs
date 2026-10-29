//! 検索結果の抜粋: 本文から検索語の前後を切り出し、検索語を `<mark>` で囲んだ HTML にする

use aegis_ast::renderer::escape_html;

use crate::tokenizer::{fold_char, runs};

/// 抜粋の長さ（文字数）
pub const SNIPPET_CHARS: usize = 120;
/// 最初に一致した所より前に入れる文字数
const LEADING_CHARS: usize = 30;

/// 本文で探す語: 検索語そのものと、それを文字の種類（英数字・CJK）ごとに分けたもの
fn needles(query: &str) -> Vec<Vec<char>> {
    let mut out: Vec<Vec<char>> = Vec::new();
    for word in query.split_whitespace() {
        let whole: Vec<char> = word.chars().map(fold_char).collect();
        let mut parts: Vec<Vec<char>> = vec![whole];
        for (_, chars) in runs(word) {
            parts.push(chars.iter().map(|&(_, c)| fold_char(c)).collect());
        }
        for part in parts {
            if !part.is_empty() && !out.contains(&part) {
                out.push(part);
            }
        }
    }
    out
}

/// 一致した範囲（文字の位置、`start..end`）を重なりをまとめて返す
fn matches(haystack: &[char], needles: &[Vec<char>]) -> Vec<(usize, usize)> {
    let mut ranges: Vec<(usize, usize)> = Vec::new();
    for needle in needles {
        if needle.len() > haystack.len() {
            continue;
        }
        for start in 0..=haystack.len() - needle.len() {
            if haystack[start..start + needle.len()] == needle[..] {
                ranges.push((start, start + needle.len()));
            }
        }
    }
    ranges.sort_unstable();
    let mut merged: Vec<(usize, usize)> = Vec::new();
    for (start, end) in ranges {
        match merged.last_mut() {
            Some(last) if start <= last.1 => last.1 = last.1.max(end),
            _ => merged.push((start, end)),
        }
    }
    merged
}

/// `text` から `query` の語の前後 [`SNIPPET_CHARS`] 文字を切り出した HTML（エスケープ済み、語は `<mark>`）。
/// 本文に語が無ければ先頭から切り出す
pub fn snippet_html(text: &str, query: &str) -> String {
    let original: Vec<char> = text
        .chars()
        .map(|c| if c.is_whitespace() { ' ' } else { c })
        .collect();
    // 空白の連続は 1 つにまとめる
    let mut chars: Vec<char> = Vec::with_capacity(original.len());
    for c in original {
        if c == ' ' && chars.last().is_none_or(|&p| p == ' ') {
            continue;
        }
        chars.push(c);
    }
    while chars.last() == Some(&' ') {
        chars.pop();
    }
    let folded: Vec<char> = chars.iter().map(|&c| fold_char(c)).collect();
    let found = matches(&folded, &needles(query));

    let start = found
        .first()
        .map(|&(s, _)| s.saturating_sub(LEADING_CHARS))
        .unwrap_or(0);
    let end = (start + SNIPPET_CHARS).min(chars.len());
    // 長い本文で後ろの方だけが一致したときも、抜粋の長さを保つ
    let start = end.saturating_sub(SNIPPET_CHARS).min(start);

    let mut html = String::new();
    if start > 0 {
        html.push('…');
    }
    let mut pos = start;
    for &(s, e) in &found {
        if e <= start || s >= end {
            continue;
        }
        let (s, e) = (s.max(start), e.min(end));
        html.push_str(&escape_html(&chars[pos..s].iter().collect::<String>()));
        html.push_str("<mark>");
        html.push_str(&escape_html(&chars[s..e].iter().collect::<String>()));
        html.push_str("</mark>");
        pos = e;
    }
    html.push_str(&escape_html(&chars[pos..end].iter().collect::<String>()));
    if end < chars.len() {
        html.push('…');
    }
    html
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_marks_matches_case_insensitively() {
        assert_eq!(
            snippet_html("Install AegisCMS with Docker.", "aegiscms docker"),
            "Install <mark>AegisCMS</mark> with <mark>Docker</mark>."
        );
    }

    #[test]
    fn test_marks_japanese_and_escapes_html() {
        assert_eq!(
            snippet_html("<b>東京都</b>の天気", "東京"),
            "&lt;b&gt;<mark>東京</mark>都&lt;/b&gt;の天気"
        );
    }

    #[test]
    fn test_mixed_word_matches_parts_when_spaced_differently() {
        assert_eq!(
            snippet_html("2026 年のリリース", "2026年"),
            "<mark>2026</mark> <mark>年</mark>のリリース"
        );
    }

    #[test]
    fn test_window_around_a_late_match() {
        let text = format!("{}検索{}", "あ".repeat(200), "い".repeat(200));
        let html = snippet_html(&text, "検索");
        assert!(html.starts_with('…'));
        assert!(html.ends_with('…'));
        assert!(html.contains("<mark>検索</mark>"));
        let visible = html.replace("<mark>", "").replace("</mark>", "");
        assert_eq!(visible.chars().count(), SNIPPET_CHARS + 2);
    }

    #[test]
    fn test_no_match_starts_at_the_beginning() {
        assert_eq!(snippet_html("a\n\n  b", "zzz"), "a b");
    }
}
