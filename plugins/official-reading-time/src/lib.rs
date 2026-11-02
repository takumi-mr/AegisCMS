//! 公式プラグイン `official-reading-time`: 記事・固定ページに推定読了時間と文字数/単語数を表示する（`on-render-page`）。
//!
//! - 本文の HTML からタグを除去したプレーンテキストを解析し、日本語・中国語などの CJK 文字と
//!   欧文（スペース区切り単語）を正確に識別して計算します。
//! - 表示位置（本文上部・下部・両方）、表示モード（時間のみ・文字数のみ・両方）、バッジの見た目
//!   （ピル型・シンプル・カード型）を設定（`[plugins.settings.official-reading-time]`）で変更可能。

use aegis_plugin_sdk::prelude::*;

#[derive(Default)]
pub struct ReadingTimePlugin;

const DEFAULT_ACCENT: &str = "var(--accent, #6366f1)";

fn setting(key: &str) -> Option<String> {
    config::get(key)
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
}

fn setting_bool(key: &str, default: bool) -> bool {
    match setting(key).map(|v| v.to_ascii_lowercase()).as_deref() {
        Some("true" | "1" | "yes" | "on") => true,
        Some("false" | "0" | "no" | "off") => false,
        _ => default,
    }
}

fn setting_int(key: &str, default: u32, min: u32, max: u32) -> u32 {
    setting(key)
        .and_then(|v| v.parse::<u32>().ok())
        .filter(|v| (min..=max).contains(v))
        .unwrap_or(default)
}

fn setting_list(key: &str, default: &str) -> Vec<String> {
    setting(key)
        .unwrap_or_else(|| default.to_string())
        .split(',')
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
        .collect()
}

fn css_color(value: &str) -> Option<String> {
    let v = value.trim();
    let ok = if let Some(hex) = v.strip_prefix('#') {
        matches!(hex.len(), 3 | 4 | 6 | 8) && hex.bytes().all(|b| b.is_ascii_hexdigit())
    } else if let Some(name) = v.strip_prefix("var(--").and_then(|r| r.strip_suffix(')')) {
        !name.is_empty()
            && name
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
    } else if ["rgb(", "rgba(", "hsl(", "hsla("]
        .iter()
        .any(|f| v.starts_with(f))
    {
        v.ends_with(')')
            && v.matches('(').count() == 1
            && v.matches(')').count() == 1
            && v.bytes().all(|b| {
                b.is_ascii_alphanumeric()
                    || matches!(b, b'(' | b')' | b',' | b'.' | b'%' | b' ' | b'/')
            })
    } else {
        !v.is_empty() && v.len() <= 20 && v.bytes().all(|b| b.is_ascii_alphabetic())
    };
    ok.then(|| v.to_string())
}

#[derive(Debug, PartialEq, Eq)]
enum DisplayMode {
    Both,
    TimeOnly,
    CharsOnly,
}

impl DisplayMode {
    fn from_str(s: &str) -> Self {
        match s {
            "time_only" => Self::TimeOnly,
            "chars_only" => Self::CharsOnly,
            _ => Self::Both,
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
enum Position {
    Top,
    Bottom,
    Both,
}

impl Position {
    fn from_str(s: &str) -> Self {
        match s {
            "bottom" => Self::Bottom,
            "both" => Self::Both,
            _ => Self::Top,
        }
    }
}

struct Settings {
    content_types: Vec<String>,
    display_mode: DisplayMode,
    position: Position,
    badge_style: String,
    show_icon: bool,
    words_per_minute: u32,
    chars_per_minute: u32,
    accent: String,
    format: Option<String>,
}

impl Settings {
    fn load() -> Self {
        Self {
            content_types: setting_list("content_types", "article"),
            display_mode: DisplayMode::from_str(&setting("display_mode").unwrap_or_default()),
            position: Position::from_str(&setting("position").unwrap_or_default()),
            badge_style: setting("badge_style")
                .filter(|s| matches!(s.as_str(), "badge" | "simple" | "card"))
                .unwrap_or_else(|| "badge".to_string()),
            show_icon: setting_bool("show_icon", true),
            words_per_minute: setting_int("words_per_minute", 200, 50, 1000),
            chars_per_minute: setting_int("chars_per_minute", 500, 100, 2000),
            accent: setting("accent_color")
                .and_then(|v| css_color(&v))
                .unwrap_or_else(|| DEFAULT_ACCENT.to_string()),
            format: setting("format"),
        }
    }
}

/// HTML タグを除去し、平文テキストを抽出する（script, style タグとその中身は除外）
pub fn extract_text(html: &str) -> String {
    let mut clean_html = String::with_capacity(html.len());
    let mut cur = html;
    loop {
        let lower = cur.to_ascii_lowercase();
        let script_pos = lower.find("<script");
        let style_pos = lower.find("<style");
        let target = match (script_pos, style_pos) {
            (Some(sc), Some(st)) => {
                if sc < st {
                    Some((sc, "</script>"))
                } else {
                    Some((st, "</style>"))
                }
            }
            (Some(sc), None) => Some((sc, "</script>")),
            (None, Some(st)) => Some((st, "</style>")),
            (None, None) => None,
        };

        if let Some((start, close_tag)) = target {
            clean_html.push_str(&cur[..start]);
            let after_start = &cur[start..];
            let after_lower = &lower[start..];
            if let Some(close_idx) = after_lower.find(close_tag) {
                cur = &after_start[close_idx + close_tag.len()..];
            } else {
                break;
            }
        } else {
            clean_html.push_str(cur);
            break;
        }
    }

    let mut out = String::with_capacity(clean_html.len());
    let mut in_tag = false;
    for c in clean_html.chars() {
        if c == '<' {
            in_tag = true;
        } else if c == '>' {
            in_tag = false;
        } else if !in_tag {
            out.push(c);
        }
    }
    out
}

/// CJK 文字か判定
fn is_cjk(c: char) -> bool {
    matches!(c,
        '\u{4E00}'..='\u{9FFF}'   // CJK Unified Ideographs
        | '\u{3400}'..='\u{4DBF}' // CJK Extension A
        | '\u{3040}'..='\u{309F}' // Hiragana
        | '\u{30A0}'..='\u{30FF}' // Katakana
        | '\u{31F0}'..='\u{31FF}' // Katakana Phonetic Extensions
        | '\u{AC00}'..='\u{D7AF}' // Hangul Syllables
        | '\u{FF00}'..='\u{FFEF}' // Halfwidth and Fullwidth Forms
        | '\u{3000}'..='\u{303F}' // CJK Symbols and Punctuation
    )
}

#[derive(Debug, PartialEq, Eq, Default)]
pub struct TextStats {
    pub cjk_chars: usize,
    pub words: usize,
    pub total_chars: usize,
}

/// プレーンテキストから CJK 文字数、欧文単語数、総文字数を集計
pub fn analyze_text(text: &str) -> TextStats {
    let mut cjk_chars = 0;
    let mut words = 0;
    let mut total_chars = 0;
    let mut in_word = false;

    for c in text.chars() {
        if c.is_whitespace() {
            if in_word {
                words += 1;
                in_word = false;
            }
            continue;
        }

        total_chars += 1;

        if is_cjk(c) {
            if in_word {
                words += 1;
                in_word = false;
            }
            cjk_chars += 1;
        } else if c.is_alphanumeric() {
            in_word = true;
        } else if in_word {
            // 記号等で単語が途切れる
            words += 1;
            in_word = false;
        }
    }
    if in_word {
        words += 1;
    }

    TextStats {
        cjk_chars,
        words,
        total_chars,
    }
}

/// 読了時間（分）の算出
pub fn calculate_reading_time(stats: &TextStats, wpm: u32, cpm: u32) -> u32 {
    let wpm = wpm.max(1) as f64;
    let cpm = cpm.max(1) as f64;
    let minutes = (stats.cjk_chars as f64 / cpm) + (stats.words as f64 / wpm);
    minutes.ceil().max(1.0) as u32
}

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

fn truthy(value: &serde_json::Value) -> bool {
    match value {
        serde_json::Value::Bool(b) => *b,
        serde_json::Value::String(s) => {
            matches!(
                s.trim().to_ascii_lowercase().as_str(),
                "true" | "1" | "yes" | "on"
            )
        }
        serde_json::Value::Number(n) => n.as_i64().is_some_and(|n| n != 0),
        _ => false,
    }
}

const CLOCK_ICON: &str = "<svg class=\"art-icon\" viewBox=\"0 0 24 24\" width=\"14\" height=\"14\" fill=\"none\" stroke=\"currentColor\" stroke-width=\"2\" stroke-linecap=\"round\" stroke-linejoin=\"round\"><circle cx=\"12\" cy=\"12\" r=\"10\"></circle><polyline points=\"12 6 12 12 16 14\"></polyline></svg>";

fn generate_label(settings: &Settings, stats: &TextStats, time_min: u32, is_ja: bool) -> String {
    if let Some(ref tmpl) = settings.format {
        return tmpl
            .replace("{time}", &time_min.to_string())
            .replace("{chars}", &stats.total_chars.to_string())
            .replace("{words}", &stats.words.to_string());
    }

    if is_ja {
        match settings.display_mode {
            DisplayMode::Both => format!("読了目安: 約{}分 ({}文字)", time_min, stats.total_chars),
            DisplayMode::TimeOnly => format!("読了目安: 約{}分", time_min),
            DisplayMode::CharsOnly => format!("{}文字", stats.total_chars),
        }
    } else {
        match settings.display_mode {
            DisplayMode::Both => {
                if stats.cjk_chars > stats.words {
                    format!("{} min read ({} chars)", time_min, stats.total_chars)
                } else {
                    format!("{} min read ({} words)", time_min, stats.words)
                }
            }
            DisplayMode::TimeOnly => format!("{} min read", time_min),
            DisplayMode::CharsOnly => {
                if stats.cjk_chars > stats.words {
                    format!("{} characters", stats.total_chars)
                } else {
                    format!("{} words", stats.words)
                }
            }
        }
    }
}

fn css(settings: &Settings) -> String {
    format!(
        ".art-badge{{\
            display:inline-flex;align-items:center;gap:.4rem;\
            font-size:.825rem;line-height:1;margin:1rem 0;\
            color:inherit;opacity:.85\
        }}\
        .art-badge.art-style-badge{{\
            padding:.3rem .75rem;border-radius:9999px;\
            background:rgba(127,127,127,.1);border:1px solid rgba(127,127,127,.15)\
        }}\
        .art-badge.art-style-card{{\
            padding:.5rem .9rem;border-radius:.375rem;\
            background:rgba(127,127,127,.06);border:1px solid rgba(127,127,127,.2)\
        }}\
        .art-badge.art-style-simple{{\
            padding:0;background:none;border:none\
        }}\
        .art-badge .art-icon{{color:{accent};flex-shrink:0}}\
        @media print{{.art-badge{{display:none}}}}",
        accent = settings.accent
    )
}

fn badge_html(settings: &Settings, stats: &TextStats, time_min: u32, is_ja: bool) -> String {
    let label = generate_label(settings, stats, time_min, is_ja);
    let icon = if settings.show_icon { CLOCK_ICON } else { "" };
    format!(
        "<div class=\"art-badge art-style-{}\" role=\"note\" aria-label=\"{}\">{}{}</div>",
        escape(&settings.badge_style),
        escape(&label),
        icon,
        escape(&label)
    )
}

fn push_line(target: &mut String, addition: &str) {
    if !target.is_empty() {
        target.push('\n');
    }
    target.push_str(addition);
}

impl Plugin for ReadingTimePlugin {
    fn on_render_page(&mut self, mut ctx: PageRenderContext) -> Result<PageRenderContext> {
        let settings = Settings::load();
        if ctx.content_html.is_empty()
            || !settings.content_types.contains(&ctx.content_type)
            || ctx.fields.get("hide_reading_time").is_some_and(truthy)
        {
            return Ok(ctx);
        }

        let plain = extract_text(&ctx.content_html);
        let stats = analyze_text(&plain);
        if stats.total_chars == 0 {
            return Ok(ctx);
        }

        let time_min =
            calculate_reading_time(&stats, settings.words_per_minute, settings.chars_per_minute);
        let is_ja = setting("locale").as_deref() == Some("ja") || stats.cjk_chars > stats.words;

        let badge = badge_html(&settings, &stats, time_min, is_ja);
        push_line(&mut ctx.extra_css, &css(&settings));

        match settings.position {
            Position::Top => {
                let mut new_html = badge;
                new_html.push('\n');
                new_html.push_str(&ctx.content_html);
                ctx.content_html = new_html;
            }
            Position::Bottom => {
                ctx.content_html.push('\n');
                ctx.content_html.push_str(&badge);
            }
            Position::Both => {
                let mut new_html = badge.clone();
                new_html.push('\n');
                new_html.push_str(&ctx.content_html);
                new_html.push('\n');
                new_html.push_str(&badge);
                ctx.content_html = new_html;
            }
        }

        Ok(ctx)
    }
}

aegis_plugin_sdk::register_plugin!(ReadingTimePlugin);

#[cfg(test)]
mod tests {
    use super::*;
    use aegis_plugin_sdk::host::mock;

    fn page(html: &str) -> PageRenderContext {
        PageRenderContext {
            content_type: "article".to_string(),
            content_html: html.to_string(),
            ..Default::default()
        }
    }

    #[test]
    fn test_extract_text_and_stats_japanese() {
        let html = "<p>こんにちは、世界！</p><style>.test{color:red}</style><script>console.log('hi');</script><p>これはテストです。</p>";
        let text = extract_text(html);
        assert_eq!(text, "こんにちは、世界！これはテストです。");
        let stats = analyze_text(&text);
        assert_eq!(stats.cjk_chars, 18);
        assert_eq!(stats.words, 0);
        let time = calculate_reading_time(&stats, 200, 500);
        assert_eq!(time, 1);
    }

    #[test]
    fn test_extract_text_and_stats_english() {
        let html = "<p>Hello world! This is a fast and secure CMS written in Rust.</p>";
        let text = extract_text(html);
        let stats = analyze_text(&text);
        assert_eq!(stats.cjk_chars, 0);
        assert_eq!(stats.words, 12);
        let time = calculate_reading_time(&stats, 200, 500);
        assert_eq!(time, 1);
    }

    #[test]
    fn test_render_page_injects_badge_and_css() {
        mock::reset();
        mock::set_config("display_mode", "both");
        let ctx = page("<p>これはサンプル本文です。テスト用の記事です。</p>");
        let res = ReadingTimePlugin.on_render_page(ctx).unwrap();
        assert!(res.content_html.contains("art-badge"));
        assert!(res.content_html.contains("読了目安: 約1分"));
        assert!(res.extra_css.contains(".art-badge"));
    }

    #[test]
    fn test_hide_reading_time_field_skips_rendering() {
        mock::reset();
        let mut ctx = page("<p>サンプル本文</p>");
        ctx.fields
            .insert("hide_reading_time".to_string(), serde_json::json!(true));
        let res = ReadingTimePlugin.on_render_page(ctx).unwrap();
        assert!(!res.content_html.contains("art-badge"));
    }

    #[test]
    fn test_custom_format_template() {
        mock::reset();
        mock::set_config("format", "{time} min | {chars} characters");
        let ctx = page("<p>Short article for test</p>");
        let res = ReadingTimePlugin.on_render_page(ctx).unwrap();
        assert!(res.content_html.contains("1 min | 19 characters"));
    }
}
