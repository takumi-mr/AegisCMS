//! 公式プラグイン `official-back-to-top`: 記事・固定ページに「トップへ戻る」ボタンを足す（`on-render-page`）。
//!
//! - `style = "inline"`（既定）: 本文の末尾にリンクを置く。JavaScript なしで動く（`href="#top"`）
//! - `style = "fixed"`: 画面に固定表示し、`show_after_px` スクロールしたら現れる。スクリプトが動かない
//!   ときは、常に表示する（隠すのはスクリプトが `<html>` に付ける `obt-js` クラスのとき）
//! - 見た目は設定（`[plugins.settings.official-back-to-top]`）で変える。値は許可する形だけを CSS に入れる
//!   （色は `#hex`・`rgb()`・`hsl()`・`var(--名前)`・色名、数値は範囲内）。設定が読めなければ既定値

use aegis_plugin_sdk::prelude::*;

#[derive(Default)]
pub struct BackToTopPlugin;

/// 既定の背景色（テーマの `--accent`）と文字の色（テーマの `--accent-contrast`）
const DEFAULT_BG: &str = "var(--accent,#6366f1)";
const DEFAULT_FG: &str = "var(--accent-contrast,#fff)";

const ARROW: &str = "<svg viewBox=\"0 0 24 24\" width=\"20\" height=\"20\" fill=\"none\" stroke=\"currentColor\" stroke-width=\"2.5\" stroke-linecap=\"round\" stroke-linejoin=\"round\" aria-hidden=\"true\"><path d=\"M12 19V5M5 12l7-7 7 7\"/></svg>";

const SCRIPT: &str = "(function(){var d=document.documentElement;d.classList.add('obt-js');\
function init(){var b=document.querySelector('a.obt-fixed');if(!b)return;var busy=false;\
function u(){busy=false;b.classList.toggle('obt-show',(window.pageYOffset||d.scrollTop)>__SHOW__)}\
window.addEventListener('scroll',function(){if(!busy){busy=true;window.requestAnimationFrame(u)}},{passive:true});u();\
b.addEventListener('click',function(e){e.preventDefault();\
var r=window.matchMedia&&window.matchMedia('(prefers-reduced-motion: reduce)').matches;\
window.scrollTo({top:0,behavior:(r||!__SMOOTH__)?'auto':'smooth'})})}\
document.readyState==='loading'?document.addEventListener('DOMContentLoaded',init):init()})();";

// --- 設定 ---

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

/// `allowed` のどれかなら、その値。それ以外は `default`
fn setting_choice(key: &str, allowed: &[&'static str], default: &'static str) -> &'static str {
    setting(key)
        .and_then(|v| allowed.iter().copied().find(|a| *a == v))
        .unwrap_or(default)
}

/// CSS に入れてよい色（`#hex`・`rgb()`・`hsl()`・`var(--名前)`・色名）だけ。それ以外は `None`
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

fn setting_color(key: &str, default: &str) -> String {
    setting(key)
        .and_then(|v| css_color(&v))
        .unwrap_or_else(|| default.to_string())
}

struct Settings {
    content_types: Vec<String>,
    fixed: bool,
    position: &'static str,
    shape: &'static str,
    label: String,
    bg: String,
    fg: String,
    size: u32,
    offset: u32,
    show_after: u32,
    smooth_scroll: bool,
}

impl Settings {
    fn load() -> Self {
        let ja = setting("locale").as_deref() == Some("ja");
        Self {
            content_types: setting("content_types")
                .unwrap_or_else(|| "article,page".to_string())
                .split(',')
                .map(|v| v.trim().to_string())
                .filter(|v| !v.is_empty())
                .collect(),
            fixed: setting_choice("style", &["inline", "fixed"], "inline") == "fixed",
            position: setting_choice("position", &["right", "left", "center"], "right"),
            shape: setting_choice("shape", &["circle", "square", "pill"], "circle"),
            label: setting("label").unwrap_or_else(|| {
                if ja {
                    "トップへ戻る"
                } else {
                    "Back to top"
                }
                .to_string()
            }),
            bg: setting_color("bg_color", DEFAULT_BG),
            fg: setting_color("fg_color", DEFAULT_FG),
            size: setting_int("size_px", 44, 32, 72),
            offset: setting_int("offset_px", 24, 8, 96),
            show_after: setting_int("show_after_px", 400, 0, 5000),
            smooth_scroll: setting_bool("smooth_scroll", true),
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

// --- 出力 ---

fn css(s: &Settings) -> String {
    // リンクの色・下線は、テーマの本文のリンクの指定（`.prose a` など）に負けないよう `a.obt.obt`（クラス 2 つ）
    let mut css = format!(
        "a.obt.obt{{display:inline-flex;align-items:center;justify-content:center;gap:.4rem;box-sizing:border-box;\
         min-width:{size}px;height:{size}px;background:{bg};color:{fg};text-decoration:none;border:0;\
         font-size:.85rem;font-weight:600;line-height:1;cursor:pointer;box-shadow:0 2px 8px rgba(0,0,0,.2)}}\
         a.obt.obt:hover{{filter:brightness(1.1)}}\
         a.obt.obt:focus-visible{{outline:2px solid {bg};outline-offset:2px}}\
         .obt svg{{flex:none}}\
         .obt-circle{{border-radius:50%}}.obt-square{{border-radius:8px}}.obt-pill{{border-radius:999px;padding:0 1.1rem}}\
         .obt-row{{margin:2rem 0 1rem}}.obt-row.obt-right{{text-align:right}}.obt-row.obt-center{{text-align:center}}\
         a.obt-fixed.obt-fixed{{position:fixed;bottom:{offset}px;z-index:30}}\
         a.obt-fixed.obt-right{{right:{offset}px}}a.obt-fixed.obt-left{{left:{offset}px}}\
         a.obt-fixed.obt-center{{left:50%;transform:translateX(-50%)}}\
         .obt-js a.obt-fixed{{opacity:0;visibility:hidden;transition:opacity .2s,visibility .2s}}\
         .obt-js a.obt-fixed.obt-show{{opacity:1;visibility:visible}}\
         @media print{{.obt-row,a.obt-fixed{{display:none}}}}",
        size = s.size,
        bg = s.bg,
        fg = s.fg,
        offset = s.offset,
    );
    if s.smooth_scroll {
        css.push_str("@media (prefers-reduced-motion:no-preference){html{scroll-behavior:smooth}}");
    }
    css
}

fn button_html(s: &Settings) -> String {
    let label = escape(&s.label);
    let text = if s.shape == "pill" {
        format!("<span>{}</span>", label)
    } else {
        String::new()
    };
    let link = format!(
        "<a class=\"obt {} obt-{} obt-{}\" href=\"#top\" aria-label=\"{}\">{}{}</a>",
        if s.fixed { "obt-fixed" } else { "obt-inline" },
        s.position,
        s.shape,
        label,
        ARROW,
        text
    );
    if s.fixed {
        link
    } else {
        format!("<div class=\"obt-row obt-{}\">{}</div>", s.position, link)
    }
}

fn push_line(target: &mut String, addition: &str) {
    if !target.is_empty() {
        target.push('\n');
    }
    target.push_str(addition);
}

impl Plugin for BackToTopPlugin {
    fn on_render_page(&mut self, mut ctx: PageRenderContext) -> Result<PageRenderContext> {
        let settings = Settings::load();
        if ctx.content_html.is_empty()
            || !settings.content_types.contains(&ctx.content_type)
            || ctx.fields.get("hide_back_to_top").is_some_and(truthy)
        {
            return Ok(ctx);
        }
        push_line(&mut ctx.extra_css, &css(&settings));
        if settings.fixed {
            push_line(
                &mut ctx.extra_head,
                &format!(
                    "<script>{}</script>",
                    SCRIPT
                        .replace("__SHOW__", &settings.show_after.to_string())
                        .replace("__SMOOTH__", &settings.smooth_scroll.to_string())
                ),
            );
        }
        ctx.content_html.push('\n');
        ctx.content_html.push_str(&button_html(&settings));
        Ok(ctx)
    }
}

aegis_plugin_sdk::register_plugin!(BackToTopPlugin);

#[cfg(test)]
mod tests {
    use super::*;
    use aegis_plugin_sdk::host::mock;

    fn page() -> PageRenderContext {
        PageRenderContext {
            content_type: "article".to_string(),
            content_html: "<p>Hello</p>".to_string(),
            ..Default::default()
        }
    }

    fn render(ctx: PageRenderContext) -> PageRenderContext {
        BackToTopPlugin.on_render_page(ctx).unwrap()
    }

    #[test]
    fn adds_an_inline_link_at_the_end_without_script() {
        mock::reset();
        let out = render(page());
        assert!(out
            .content_html
            .starts_with("<p>Hello</p>\n<div class=\"obt-row obt-right\">"));
        assert!(out.content_html.contains(
            "<a class=\"obt obt-inline obt-right obt-circle\" href=\"#top\" aria-label=\"Back to top\">"
        ));
        assert!(!out.content_html.contains("<span>"));
        assert!(out.extra_head.is_empty());
        assert!(out
            .extra_css
            .contains("min-width:44px;height:44px;background:var(--accent,#6366f1)"));
        assert!(out.extra_css.contains("scroll-behavior:smooth"));
    }

    #[test]
    fn fixed_style_floats_and_appears_after_scrolling() {
        mock::reset();
        mock::set_config("style", "fixed");
        mock::set_config("position", "left");
        mock::set_config("shape", "pill");
        mock::set_config("show_after_px", "800");
        mock::set_config("offset_px", "32");
        mock::set_config("locale", "ja");
        let out = render(page());
        assert!(out.content_html.contains(
            "<a class=\"obt obt-fixed obt-left obt-pill\" href=\"#top\" aria-label=\"トップへ戻る\">"
        ));
        assert!(out.content_html.contains("<span>トップへ戻る</span>"));
        assert!(!out.content_html.contains("obt-row"));
        assert!(out.extra_css.contains("a.obt-fixed.obt-left{left:32px}"));
        assert!(out.extra_head.contains(">800)"));
        assert!(out.extra_head.contains("'auto':'smooth'"));
        assert!(!out.extra_head.contains("__"));
        mock::reset();
    }

    #[test]
    fn centered_fixed_and_inline_positions() {
        mock::reset();
        mock::set_config("style", "fixed");
        mock::set_config("position", "center");
        mock::set_config("shape", "square");
        let out = render(page());
        assert!(out
            .content_html
            .contains("class=\"obt obt-fixed obt-center obt-square\""));
        mock::reset();
        mock::set_config("position", "center");
        let out = render(page());
        assert!(out
            .content_html
            .contains("<div class=\"obt-row obt-center\">"));
        assert!(out
            .content_html
            .contains("class=\"obt obt-inline obt-center obt-circle\""));
        mock::reset();
    }

    #[test]
    fn leaves_other_pages_alone() {
        mock::reset();
        let mut index = page();
        index.content_type = "index".to_string();
        assert_eq!(render(index.clone()), index);
        let mut empty = page();
        empty.content_html.clear();
        assert_eq!(render(empty.clone()), empty);
        let mut hidden = page();
        hidden
            .fields
            .insert("hide_back_to_top".to_string(), serde_json::json!("true"));
        assert_eq!(render(hidden.clone()), hidden);
        mock::set_config("content_types", "article, product");
        let mut product = page();
        product.content_type = "product".to_string();
        assert!(render(product).content_html.contains("obt-row"));
        mock::reset();
    }

    #[test]
    fn colors_sizes_and_choices_are_checked() {
        mock::reset();
        mock::set_config("bg_color", "#111827");
        mock::set_config("fg_color", "white;} body{display:none");
        mock::set_config("size_px", "500");
        mock::set_config("shape", "triangle");
        mock::set_config("position", "top");
        let out = render(page());
        assert!(out
            .extra_css
            .contains("background:#111827;color:var(--accent-contrast,#fff)"));
        assert!(!out.extra_css.contains("body{display:none"));
        assert!(out.extra_css.contains("min-width:44px"));
        assert!(out.content_html.contains("obt-right obt-circle"));
        mock::reset();
    }

    #[test]
    fn empty_or_unsafe_colors_fall_back_to_the_defaults() {
        assert_eq!(css_color(""), None);
        assert_eq!(css_color("red;"), None);
        assert_eq!(css_color("#111").as_deref(), Some("#111"));
    }

    #[test]
    fn label_is_escaped() {
        mock::reset();
        mock::set_config("label", "\"><script>alert(1)</script>");
        let out = render(page());
        assert!(!out.content_html.contains("<script>"));
        assert!(out
            .content_html
            .contains("aria-label=\"&quot;&gt;&lt;script&gt;"));
        mock::reset();
    }
}
