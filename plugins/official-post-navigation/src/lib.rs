//! 公式プラグイン `official-post-navigation`: 記事詳細ページに前後の記事ナビゲーション（Prev/Next）を表示する
//! （`on-render-page`）。
//!
//! - 前後の記事（`ctx.prev_article`, `ctx.next_article`）へのリンクを美しくレスポンシブなカードやリスト形式で出力します。
//! - 前後の記事の算出条件（同一カテゴリ優先か、全体の日付順か）は `[plugins.settings.official-post-navigation]` の
//!   `mode = "category"` / `"date"` で設定可能。

use aegis_plugin_sdk::prelude::*;

#[derive(Default)]
pub struct PostNavigationPlugin;

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
enum Position {
    Top,
    Bottom,
    Both,
}

impl Position {
    fn from_str(s: &str) -> Self {
        match s {
            "top" => Self::Top,
            "both" => Self::Both,
            _ => Self::Bottom,
        }
    }
}

struct Settings {
    content_types: Vec<String>,
    position: Position,
    layout: String,
    show_labels: bool,
    prev_label: String,
    next_label: String,
    show_thumbnail: bool,
    accent: String,
}

impl Settings {
    fn load() -> Self {
        let is_ja = setting("locale").as_deref() == Some("ja");
        let prev_default = if is_ja {
            "前の記事"
        } else {
            "Previous Article"
        };
        let next_default = if is_ja {
            "次の記事"
        } else {
            "Next Article"
        };

        Self {
            content_types: setting_list("content_types", "article"),
            position: Position::from_str(&setting("position").unwrap_or_default()),
            layout: setting("layout")
                .filter(|s| matches!(s.as_str(), "split" | "stacked" | "cards"))
                .unwrap_or_else(|| "split".to_string()),
            show_labels: setting_bool("show_labels", true),
            prev_label: setting("prev_label").unwrap_or_else(|| prev_default.to_string()),
            next_label: setting("next_label").unwrap_or_else(|| next_default.to_string()),
            show_thumbnail: setting_bool("show_thumbnail", true),
            accent: setting("accent_color")
                .and_then(|v| css_color(&v))
                .unwrap_or_else(|| DEFAULT_ACCENT.to_string()),
        }
    }
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

fn get_thumbnail_url(summary: &ContentSummary) -> Option<String> {
    summary
        .fields
        .get("featured_image")
        .or_else(|| summary.fields.get("image"))
        .and_then(|v| v.as_str())
        .filter(|s| !s.trim().is_empty())
        .map(|s| s.to_string())
}

fn css(settings: &Settings) -> String {
    format!(
        ".apn{{\
            display:grid;grid-template-columns:1fr 1fr;gap:1rem;\
            margin:2.5rem 0;padding-top:1.5rem;\
            border-top:1px solid rgba(127,127,127,.2)\
        }}\
        .apn-stacked{{grid-template-columns:1fr}}\
        .apn-item{{\
            display:flex;align-items:center;gap:.75rem;\
            padding:.875rem 1rem;border-radius:.5rem;\
            background:rgba(127,127,127,.05);border:1px solid rgba(127,127,127,.12);\
            color:inherit;text-decoration:none;\
            transition:all .15s ease-in-out\
        }}\
        .apn-cards .apn-item{{\
            background:rgba(127,127,127,.08);border:1px solid rgba(127,127,127,.2);\
            box-shadow:0 1px 3px rgba(0,0,0,.04)\
        }}\
        .apn-item:hover{{\
            border-color:{accent};background:rgba(127,127,127,.1);\
            transform:translateY(-1px)\
        }}\
        .apn-item.apn-next{{text-align:right;justify-content:flex-end}}\
        .apn-item.apn-empty{{visibility:hidden;pointer-events:none;border:none;background:none}}\
        .apn-arrow{{\
            font-size:1.25rem;line-height:1;color:{accent};flex-shrink:0;\
            transition:transform .15s ease-in-out\
        }}\
        .apn-item.apn-prev:hover .apn-arrow{{transform:translateX(-3px)}}\
        .apn-item.apn-next:hover .apn-arrow{{transform:translateX(3px)}}\
        .apn-thumb{{\
            width:3.25rem;height:3.25rem;border-radius:.375rem;\
            object-fit:cover;flex-shrink:0;background:rgba(127,127,127,.15)\
        }}\
        .apn-body{{display:flex;flex-direction:column;gap:.25rem;min-width:0;flex:1}}\
        .apn-label{{\
            font-size:.75rem;font-weight:600;opacity:.65;\
            text-transform:uppercase;letter-spacing:.025em\
        }}\
        .apn-title{{\
            font-size:.9rem;font-weight:600;line-height:1.3;\
            overflow:hidden;text-overflow:ellipsis;white-space:nowrap\
        }}\
        @media (max-width:640px){{\
            .apn{{grid-template-columns:1fr}}\
            .apn-item.apn-empty{{display:none}}\
        }}\
        @media print{{.apn{{display:none}}}}",
        accent = settings.accent
    )
}

fn render_nav_html(
    settings: &Settings,
    prev: Option<&ContentSummary>,
    next: Option<&ContentSummary>,
) -> String {
    let mut html = format!(
        "<nav class=\"apn apn-{}\" aria-label=\"Post navigation\">",
        escape(&settings.layout)
    );

    // 前の記事 (Prev)
    if let Some(p) = prev {
        let label_part = if settings.show_labels {
            format!(
                "<span class=\"apn-label\">{}</span>",
                escape(&settings.prev_label)
            )
        } else {
            String::new()
        };
        let thumb_part = if settings.show_thumbnail {
            if let Some(src) = get_thumbnail_url(p) {
                format!(
                    "<img src=\"{}\" alt=\"\" class=\"apn-thumb\" loading=\"lazy\" />",
                    escape(&src)
                )
            } else {
                String::new()
            }
        } else {
            String::new()
        };

        html.push_str(&format!(
            "<a href=\"/articles/{}\" class=\"apn-item apn-prev\" rel=\"prev\">\
                <span class=\"apn-arrow\" aria-hidden=\"true\">&larr;</span>\
                {}\
                <div class=\"apn-body\">{}{}<span class=\"apn-title\">{}</span></div>\
            </a>",
            escape(&p.slug),
            thumb_part,
            label_part,
            "",
            escape(&p.title)
        ));
    } else if settings.layout != "stacked" {
        // グリッドレイアウトを保つための空セル
        html.push_str("<div class=\"apn-item apn-empty\" aria-hidden=\"true\"></div>");
    }

    // 次の記事 (Next)
    if let Some(n) = next {
        let label_part = if settings.show_labels {
            format!(
                "<span class=\"apn-label\">{}</span>",
                escape(&settings.next_label)
            )
        } else {
            String::new()
        };
        let thumb_part = if settings.show_thumbnail {
            if let Some(src) = get_thumbnail_url(n) {
                format!(
                    "<img src=\"{}\" alt=\"\" class=\"apn-thumb\" loading=\"lazy\" />",
                    escape(&src)
                )
            } else {
                String::new()
            }
        } else {
            String::new()
        };

        html.push_str(&format!(
            "<a href=\"/articles/{}\" class=\"apn-item apn-next\" rel=\"next\">\
                <div class=\"apn-body\">{}{}<span class=\"apn-title\">{}</span></div>\
                {}\
                <span class=\"apn-arrow\" aria-hidden=\"true\">&rarr;</span>\
            </a>",
            escape(&n.slug),
            label_part,
            "",
            escape(&n.title),
            thumb_part
        ));
    } else if settings.layout != "stacked" {
        html.push_str("<div class=\"apn-item apn-empty\" aria-hidden=\"true\"></div>");
    }

    html.push_str("</nav>");
    html
}

fn push_line(target: &mut String, addition: &str) {
    if !target.is_empty() {
        target.push('\n');
    }
    target.push_str(addition);
}

impl Plugin for PostNavigationPlugin {
    fn on_render_page(&mut self, mut ctx: PageRenderContext) -> Result<PageRenderContext> {
        let settings = Settings::load();
        if !settings.content_types.contains(&ctx.content_type)
            || ctx.fields.get("hide_post_nav").is_some_and(truthy)
            || ctx.fields.get("hide_post_navigation").is_some_and(truthy)
        {
            return Ok(ctx);
        }

        if ctx.prev_article.is_none() && ctx.next_article.is_none() {
            return Ok(ctx);
        }

        let nav = render_nav_html(
            &settings,
            ctx.prev_article.as_ref(),
            ctx.next_article.as_ref(),
        );

        push_line(&mut ctx.extra_css, &css(&settings));

        match settings.position {
            Position::Top => {
                let mut new_html = nav;
                new_html.push('\n');
                new_html.push_str(&ctx.content_html);
                ctx.content_html = new_html;
            }
            Position::Bottom => {
                ctx.content_html.push('\n');
                ctx.content_html.push_str(&nav);
            }
            Position::Both => {
                let mut new_html = nav.clone();
                new_html.push('\n');
                new_html.push_str(&ctx.content_html);
                new_html.push('\n');
                new_html.push_str(&nav);
                ctx.content_html = new_html;
            }
        }

        Ok(ctx)
    }
}

aegis_plugin_sdk::register_plugin!(PostNavigationPlugin);

#[cfg(test)]
mod tests {
    use super::*;
    use aegis_plugin_sdk::host::mock;

    fn page(
        prev: Option<ContentSummary>,
        next: Option<ContentSummary>,
        html: &str,
    ) -> PageRenderContext {
        PageRenderContext {
            content_type: "article".to_string(),
            content_html: html.to_string(),
            prev_article: prev,
            next_article: next,
            ..Default::default()
        }
    }

    fn sample_article(id: &str, title: &str, slug: &str) -> ContentSummary {
        ContentSummary {
            id: id.to_string(),
            content_type: "article".to_string(),
            title: title.to_string(),
            slug: slug.to_string(),
            ..Default::default()
        }
    }

    #[test]
    fn test_skips_when_no_adjacent_posts() {
        mock::reset();
        let ctx = page(None, None, "<p>Hello</p>");
        let res = PostNavigationPlugin.on_render_page(ctx).unwrap();
        assert_eq!(res.content_html, "<p>Hello</p>");
        assert!(res.extra_css.is_empty());
    }

    #[test]
    fn test_renders_both_prev_and_next() {
        mock::reset();
        let prev = sample_article("1", "First Post", "first-post");
        let next = sample_article("3", "Third Post", "third-post");
        let ctx = page(Some(prev), Some(next), "<p>Current Post Body</p>");

        let res = PostNavigationPlugin.on_render_page(ctx).unwrap();
        assert!(res.content_html.contains("First Post"));
        assert!(res.content_html.contains("Third Post"));
        assert!(res.content_html.contains("/articles/first-post"));
        assert!(res.content_html.contains("/articles/third-post"));
        assert!(res.content_html.contains("rel=\"prev\""));
        assert!(res.content_html.contains("rel=\"next\""));
        assert!(res.extra_css.contains(".apn"));
    }

    #[test]
    fn test_renders_prev_only_with_empty_placeholder() {
        mock::reset();
        let prev = sample_article("1", "Older Post", "older-post");
        let ctx = page(Some(prev), None, "<p>Current Post Body</p>");

        let res = PostNavigationPlugin.on_render_page(ctx).unwrap();
        assert!(res.content_html.contains("Older Post"));
        assert!(res.content_html.contains("apn-empty"));
    }

    #[test]
    fn test_hide_post_nav_field_skips() {
        mock::reset();
        let prev = sample_article("1", "Older Post", "older-post");
        let mut ctx = page(Some(prev), None, "<p>Current Post Body</p>");
        ctx.fields
            .insert("hide_post_nav".to_string(), serde_json::json!(true));

        let res = PostNavigationPlugin.on_render_page(ctx).unwrap();
        assert!(!res.content_html.contains("Older Post"));
    }

    #[test]
    fn test_custom_labels_and_layout() {
        mock::reset();
        mock::set_config("prev_label", "過去記事");
        mock::set_config("next_label", "最新記事");
        mock::set_config("layout", "cards");

        let prev = sample_article("1", "過去", "prev");
        let next = sample_article("2", "次", "next");
        let ctx = page(Some(prev), Some(next), "<p>Body</p>");

        let res = PostNavigationPlugin.on_render_page(ctx).unwrap();
        assert!(res.content_html.contains("過去記事"));
        assert!(res.content_html.contains("最新記事"));
        assert!(res.content_html.contains("apn-cards"));
    }
}
