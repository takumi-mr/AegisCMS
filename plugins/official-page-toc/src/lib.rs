//! 公式プラグイン `official-page-toc`: 記事・固定ページの横に、見出しへのリンク（目次）とスクロールの進捗を出す
//! （`on-render-page`）。
//!
//! - 目次はサーバー側で `content_html` の見出し（`<h2 id="...">` など。`id` は組み込みの処理が付ける）から作るので、
//!   JavaScript が無くてもリンクとして動く。進捗の表示と「いま読んでいる見出し」の強調だけがインラインの
//!   スクリプト（`extra_head`）で動く（Admin Studio のプレビューは `sandbox` でスクリプトが動かない）
//! - 見た目は設定（`[plugins.settings.official-page-toc]`）で変える。値は許可する形だけを CSS に入れる
//!   （色は `#hex`・`rgb()`・`hsl()`・`var(--名前)`・色名、数値は範囲内）。設定が読めなければ既定値
//! - 配置・余白は `extra_css` の固定配置の `<nav>`。色はテーマの `--accent`（無ければ紫）を使う

use aegis_plugin_sdk::prelude::*;

#[derive(Default)]
pub struct PageTocPlugin;

/// 目次に入れる見出しの数の上限
const MAX_HEADINGS: usize = 100;
/// 見出しの文字の長さの上限（超えたら切る）
const MAX_HEADING_CHARS: usize = 120;
/// 既定の強調色（テーマの `--accent`）
const DEFAULT_ACCENT: &str = "var(--accent,#6366f1)";

const SCRIPT: &str = "(function(){var go=function(f){document.readyState==='loading'?document.addEventListener('DOMContentLoaded',f):f()};\
go(function(){var nav=document.querySelector('.apt');if(!nav)return;\
var bar=nav.querySelector('.apt-track'),fill=nav.querySelector('.apt-fill'),body=nav.querySelector('.apt-body'),\
links=[].slice.call(nav.querySelectorAll('.apt-list a')),\
heads=links.map(function(a){try{return document.getElementById(decodeURIComponent(a.getAttribute('href').slice(1)))}catch(e){return null}}),\
off=__OFFSET__+8,busy=false,cur=-1;\
function update(){busy=false;var d=document.documentElement,max=d.scrollHeight-window.innerHeight,y=window.pageYOffset||d.scrollTop,\
p=max>0?Math.max(0,Math.min(1,y/max)):0;\
if(fill)fill.style.transform='scaleY('+p+')';if(bar)bar.setAttribute('aria-valuenow',Math.round(p*100));\
var n=-1;for(var i=0;i<heads.length;i++){if(heads[i]&&heads[i].getBoundingClientRect().top<=off)n=i}\
if(n!==cur){if(cur>=0){links[cur].classList.remove('apt-active');links[cur].removeAttribute('aria-current')}\
cur=n;if(n>=0){var l=links[n];l.classList.add('apt-active');l.setAttribute('aria-current','location');\
if(body&&(l.offsetTop<body.scrollTop||l.offsetTop+l.offsetHeight>body.scrollTop+body.clientHeight))body.scrollTop=l.offsetTop-body.clientHeight/2}}}\
function req(){if(!busy){busy=true;window.requestAnimationFrame(update)}}\
window.addEventListener('scroll',req,{passive:true});window.addEventListener('resize',req);update()})})();";

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

/// `a,b,c` の各項目（空は捨てる）
fn setting_list(key: &str, default: &str) -> Vec<String> {
    setting(key)
        .unwrap_or_else(|| default.to_string())
        .split(',')
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
        .collect()
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

struct Settings {
    content_types: Vec<String>,
    levels: Vec<u8>,
    min_headings: usize,
    show_toc: bool,
    show_progress: bool,
    left: bool,
    accent: String,
    top_offset: u32,
    width: u32,
    min_viewport: u32,
    smooth_scroll: bool,
    title: String,
    progress_label: String,
}

impl Settings {
    fn load() -> Self {
        let ja = setting("locale").as_deref() == Some("ja");
        let mut levels: Vec<u8> = setting_list("heading_levels", "2,3")
            .iter()
            .filter_map(|v| v.parse::<u8>().ok())
            .filter(|v| (1..=6).contains(v))
            .collect();
        if levels.is_empty() {
            levels = vec![2, 3];
        }
        Self {
            content_types: setting_list("content_types", "article,page"),
            levels,
            min_headings: setting_int("min_headings", 2, 1, 50) as usize,
            show_toc: setting_bool("show_toc", true),
            show_progress: setting_bool("show_progress", true),
            left: setting("position").as_deref() == Some("left"),
            accent: setting("accent_color")
                .and_then(|v| css_color(&v))
                .unwrap_or_else(|| DEFAULT_ACCENT.to_string()),
            top_offset: setting_int("top_offset_px", 96, 0, 400),
            width: setting_int("width_px", 220, 120, 480),
            min_viewport: setting_int("min_viewport_px", 1200, 320, 4000),
            smooth_scroll: setting_bool("smooth_scroll", true),
            title: setting("title")
                .unwrap_or_else(|| if ja { "目次" } else { "On this page" }.to_string()),
            progress_label: setting("progress_label").unwrap_or_else(|| {
                if ja {
                    "読了位置"
                } else {
                    "Reading progress"
                }
                .to_string()
            }),
        }
    }
}

// --- 見出し ---

#[derive(Debug, PartialEq)]
struct Heading {
    level: u8,
    id: String,
    /// タグを除いた見出しの文字（`content_html` の中のエスケープ済みのまま）
    text: String,
}

/// タグ（`<h2 id="x">`）の属性の値。` name="..."` か ` name='...'`
fn attr_value<'a>(tag: &'a str, name: &str) -> Option<&'a str> {
    let needle = format!(" {}=", name);
    let rest = &tag[tag.find(&needle)? + needle.len()..];
    let quote = rest.chars().next().filter(|c| *c == '"' || *c == '\'')?;
    let rest = &rest[1..];
    Some(&rest[..rest.find(quote)?])
}

/// タグを取り除き、空白をひとつにまとめた文字
fn plain_text(html: &str) -> String {
    let mut out = String::with_capacity(html.len());
    let mut in_tag = false;
    for c in html.chars() {
        match c {
            '<' => in_tag = true,
            '>' if in_tag => in_tag = false,
            _ if in_tag => {}
            _ => out.push(c),
        }
    }
    let text = out.split_whitespace().collect::<Vec<_>>().join(" ");
    if text.chars().count() > MAX_HEADING_CHARS {
        let cut: String = text.chars().take(MAX_HEADING_CHARS).collect();
        format!("{}…", cut.trim_end())
    } else {
        text
    }
}

/// `content_html` の見出しのうち、`levels` の階層で `id` を持つもの（文書の順）。
/// `id` に引用符・`<` `>` `&`・空白を含むものは、属性に入れられないので飛ばす
fn extract_headings(html: &str, levels: &[u8]) -> Vec<Heading> {
    let bytes = html.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while out.len() < MAX_HEADINGS {
        let Some(rel) = html[i..].find("<h") else {
            break;
        };
        let start = i + rel;
        i = start + 2;
        let (Some(&digit), Some(&after)) = (bytes.get(start + 2), bytes.get(start + 3)) else {
            break;
        };
        if !(b'1'..=b'6').contains(&digit) || !matches!(after, b'>' | b' ' | b'\n' | b'\t' | b'\r')
        {
            continue;
        }
        let level = digit - b'0';
        let Some(tag_len) = html[start..].find('>') else {
            break;
        };
        let tag_end = start + tag_len;
        let close = format!("</h{}", level);
        let Some(inner_len) = html[tag_end + 1..].find(&close) else {
            continue;
        };
        let inner = &html[tag_end + 1..tag_end + 1 + inner_len];
        i = tag_end + 1 + inner_len;
        if !levels.contains(&level) {
            continue;
        }
        let Some(id) = attr_value(&html[start..tag_end], "id") else {
            continue;
        };
        if id.is_empty()
            || id
                .chars()
                .any(|c| c.is_whitespace() || matches!(c, '"' | '\'' | '<' | '>' | '&' | '\\'))
        {
            continue;
        }
        let text = plain_text(inner);
        if text.is_empty() {
            continue;
        }
        out.push(Heading {
            level,
            id: id.to_string(),
            text,
        });
    }
    out
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

fn css(s: &Settings, track_only: bool) -> String {
    let (side, direction) = if s.left {
        ("left", "row-reverse")
    } else {
        ("right", "row")
    };
    let mut css = format!(
        ".apt{{position:fixed;top:{top}px;{side}:1rem;width:{width}px;max-height:calc(100vh - {top}px - 1rem);\
         display:flex;flex-direction:{direction};gap:.75rem;font-size:.8rem;line-height:1.4;z-index:20}}\
         .apt-track{{flex:0 0 3px;align-self:stretch;border-radius:2px;background:rgba(127,127,127,.25);overflow:hidden}}\
         .apt-fill{{width:100%;height:100%;background:{accent};transform:scaleY(0);transform-origin:top}}\
         .apt-body{{flex:1;min-width:0;overflow-y:auto}}\
         .apt .apt-title{{margin:0 0 .5rem;font-weight:700;opacity:.8}}\
         .apt .apt-list{{list-style:none;margin:0;padding:0}}\
         .apt .apt-list li{{margin:0;padding:0}}\
         .apt .apt-list a{{display:block;padding:.2rem 0 .2rem .6rem;border-left:2px solid transparent;color:inherit;opacity:.65;text-decoration:none;overflow-wrap:anywhere}}\
         .apt .apt-list a:hover{{opacity:1}}\
         .apt .apt-list a.apt-active{{opacity:1;font-weight:600;border-left-color:{accent}}}\
         .apt .apt-d1 a{{padding-left:1.35rem}}.apt .apt-d2 a{{padding-left:2.1rem}}.apt .apt-d3 a{{padding-left:2.85rem}}.apt .apt-d4 a{{padding-left:3.6rem}}.apt .apt-d5 a{{padding-left:4.35rem}}\
         :where(h1,h2,h3,h4,h5,h6)[id]{{scroll-margin-top:{top}px}}\
         @media (max-width:{below}px){{.apt{{display:none}}}}\
         @media print{{.apt{{display:none}}}}",
        top = s.top_offset,
        side = side,
        width = s.width,
        direction = direction,
        accent = s.accent,
        below = s.min_viewport - 1,
    );
    if track_only {
        css.push_str(".apt{width:auto}.apt .apt-track{height:min(60vh,24rem)}");
    }
    if s.smooth_scroll {
        css.push_str("@media (prefers-reduced-motion:no-preference){html{scroll-behavior:smooth}}");
    }
    css
}

fn nav_html(s: &Settings, headings: &[Heading]) -> String {
    let side = if s.left { "apt-left" } else { "apt-right" };
    let mut html = format!(
        "<nav class=\"apt {}{}\" aria-label=\"{}\">",
        side,
        if headings.is_empty() { " apt-only" } else { "" },
        escape(&s.title)
    );
    if s.show_progress {
        html.push_str(&format!(
            "<div class=\"apt-track\" role=\"progressbar\" aria-label=\"{}\" aria-valuemin=\"0\" aria-valuemax=\"100\" aria-valuenow=\"0\"><div class=\"apt-fill\"></div></div>",
            escape(&s.progress_label)
        ));
    }
    if !headings.is_empty() {
        let top = headings.iter().map(|h| h.level).min().unwrap_or(2);
        html.push_str(&format!(
            "<div class=\"apt-body\"><p class=\"apt-title\">{}</p><ol class=\"apt-list\">",
            escape(&s.title)
        ));
        for h in headings {
            html.push_str(&format!(
                "<li class=\"apt-d{}\"><a href=\"#{}\">{}</a></li>",
                h.level - top,
                h.id,
                h.text
            ));
        }
        html.push_str("</ol></div>");
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

impl Plugin for PageTocPlugin {
    fn on_render_page(&mut self, mut ctx: PageRenderContext) -> Result<PageRenderContext> {
        let settings = Settings::load();
        if ctx.content_html.is_empty()
            || !settings.content_types.contains(&ctx.content_type)
            || ctx.fields.get("hide_toc").is_some_and(truthy)
        {
            return Ok(ctx);
        }
        let mut headings = if settings.show_toc {
            extract_headings(&ctx.content_html, &settings.levels)
        } else {
            Vec::new()
        };
        if headings.len() < settings.min_headings {
            headings.clear();
        }
        if headings.is_empty() && !settings.show_progress {
            return Ok(ctx);
        }

        push_line(&mut ctx.extra_css, &css(&settings, headings.is_empty()));
        push_line(
            &mut ctx.extra_head,
            &format!(
                "<script>{}</script>",
                SCRIPT.replace("__OFFSET__", &settings.top_offset.to_string())
            ),
        );
        ctx.content_html.push('\n');
        ctx.content_html.push_str(&nav_html(&settings, &headings));
        Ok(ctx)
    }
}

aegis_plugin_sdk::register_plugin!(PageTocPlugin);

#[cfg(test)]
mod tests {
    use super::*;
    use aegis_plugin_sdk::host::mock;

    const ARTICLE: &str = "<p>intro</p><h2 id=\"first\">First &amp; <em>best</em></h2><p>x</p>\
        <h3 id=\"sub\">Sub</h3><h2 id=\"second\">Second</h2><h4 id=\"deep\">Deep</h4><h2>No id</h2>";

    fn page(html: &str) -> PageRenderContext {
        PageRenderContext {
            content_type: "article".to_string(),
            content_html: html.to_string(),
            ..Default::default()
        }
    }

    fn render(ctx: PageRenderContext) -> PageRenderContext {
        PageTocPlugin.on_render_page(ctx).unwrap()
    }

    #[test]
    fn extracts_headings_with_ids_in_the_chosen_levels() {
        let found = extract_headings(ARTICLE, &[2, 3]);
        let summary: Vec<(u8, &str, &str)> = found
            .iter()
            .map(|h| (h.level, h.id.as_str(), h.text.as_str()))
            .collect();
        assert_eq!(
            summary,
            [
                (2, "first", "First &amp; best"),
                (3, "sub", "Sub"),
                (2, "second", "Second"),
            ]
        );
        assert_eq!(extract_headings(ARTICLE, &[4]).len(), 1);
        // <header> や <hr> は見出しではない
        assert!(extract_headings(
            "<header id=\"x\">a</header><hr id=\"y\">",
            &[1, 2, 3, 4, 5, 6]
        )
        .is_empty());
    }

    #[test]
    fn skips_ids_that_cannot_go_into_an_attribute() {
        let html = "<h2 id=\"a b\">One</h2><h2 id=\"x&quot;y\">Two</h2><h2 id=\"ok\">Three</h2>";
        let found = extract_headings(html, &[2]);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].id, "ok");
    }

    #[test]
    fn truncates_long_headings_and_keeps_japanese_ids() {
        let long = "あ".repeat(300);
        let html = format!("<h2 id=\"見出し\">{}</h2>", long);
        let found = extract_headings(&html, &[2]);
        assert_eq!(found[0].id, "見出し");
        assert!(found[0].text.ends_with('…'));
        assert_eq!(found[0].text.chars().count(), MAX_HEADING_CHARS + 1);
    }

    #[test]
    fn adds_the_nav_styles_and_script() {
        mock::reset();
        let out = render(page(ARTICLE));
        assert!(out.content_html.starts_with("<p>intro</p>"));
        assert!(out
            .content_html
            .contains("<nav class=\"apt apt-right\" aria-label=\"On this page\">"));
        assert!(out
            .content_html
            .contains("<li class=\"apt-d0\"><a href=\"#first\">First &amp; best</a></li>"));
        assert!(out
            .content_html
            .contains("<li class=\"apt-d1\"><a href=\"#sub\">Sub</a></li>"));
        assert!(out.content_html.contains("role=\"progressbar\""));
        assert!(out.extra_css.contains("top:96px;right:1rem;width:220px"));
        assert!(out.extra_css.contains("var(--accent,#6366f1)"));
        assert!(out.extra_css.contains("@media (max-width:1199px)"));
        assert!(out.extra_head.starts_with("<script>"));
        assert!(out.extra_head.contains("off=96+8"));
        assert!(!out.extra_head.contains("__OFFSET__"));
    }

    #[test]
    fn appends_to_existing_head_and_css() {
        mock::reset();
        let mut ctx = page(ARTICLE);
        ctx.extra_css = "a{}".to_string();
        ctx.extra_head = "<meta name=\"x\">".to_string();
        let out = render(ctx);
        assert!(out.extra_css.starts_with("a{}\n.apt{"));
        assert!(out.extra_head.starts_with("<meta name=\"x\">\n<script>"));
    }

    #[test]
    fn leaves_other_pages_alone() {
        mock::reset();
        let mut index = page(ARTICLE);
        index.content_type = "index".to_string();
        assert_eq!(render(index.clone()), index);
        let empty = page("");
        assert_eq!(render(empty.clone()), empty);
        let mut hidden = page(ARTICLE);
        hidden
            .fields
            .insert("hide_toc".to_string(), serde_json::json!(true));
        assert_eq!(render(hidden.clone()), hidden);
        // 見出しが少ないとき（既定は 2 つ以上）は目次を出さず、進捗だけ出す
        let few = render(page("<h2 id=\"only\">Only</h2><p>x</p>"));
        assert!(!few.content_html.contains("apt-list"));
        assert!(few.content_html.contains("apt-only"));
        assert!(few.extra_css.contains(".apt{width:auto}"));
    }

    #[test]
    fn settings_change_the_look() {
        mock::reset();
        mock::set_config("position", "left");
        mock::set_config("accent_color", "#ff0066");
        mock::set_config("top_offset_px", "64");
        mock::set_config("width_px", "300");
        mock::set_config("min_viewport_px", "900");
        mock::set_config("heading_levels", "2");
        mock::set_config("show_progress", "false");
        mock::set_config("locale", "ja");
        mock::set_config("smooth_scroll", "false");
        let out = render(page(ARTICLE));
        assert!(out
            .content_html
            .contains("class=\"apt apt-left\" aria-label=\"目次\""));
        assert!(!out.content_html.contains("apt-track"));
        assert!(!out.content_html.contains("#sub"));
        assert!(out.extra_css.contains("top:64px;left:1rem;width:300px"));
        assert!(out.extra_css.contains("flex-direction:row-reverse"));
        assert!(out.extra_css.contains("background:#ff0066"));
        assert!(out.extra_css.contains("@media (max-width:899px)"));
        assert!(!out.extra_css.contains("scroll-behavior"));
        assert!(out.extra_head.contains("off=64+8"));
        mock::reset();
    }

    #[test]
    fn only_safe_values_reach_the_css() {
        mock::reset();
        mock::set_config("accent_color", "red;} body{display:none");
        mock::set_config("top_offset_px", "99999");
        mock::set_config("width_px", "abc");
        let out = render(page(ARTICLE));
        assert!(!out.extra_css.contains("body{display:none"));
        assert!(out.extra_css.contains("var(--accent,#6366f1)"));
        assert!(out.extra_css.contains("top:96px;right:1rem;width:220px"));
        mock::reset();

        for ok in [
            "#fff",
            "#ff0066cc",
            "red",
            "var(--brand)",
            "rgb(1, 2, 3)",
            "rgba(0,0,0,.5)",
            "hsl(200 50% 40%)",
        ] {
            assert_eq!(css_color(ok).as_deref(), Some(ok), "{}", ok);
        }
        for bad in [
            "#ff",
            "#gggggg",
            "red;",
            "url(x)",
            "var(--a;b)",
            "rgb(1,2,3);color:red",
            "expression(alert(1))",
            "rgb(1,2,3)(",
            "",
        ] {
            assert_eq!(css_color(bad), None, "{}", bad);
        }
    }

    #[test]
    fn custom_title_is_escaped() {
        mock::reset();
        mock::set_config("title", "<b>Contents</b>");
        let out = render(page(ARTICLE));
        assert!(out.content_html.contains("&lt;b&gt;Contents&lt;/b&gt;"));
        assert!(!out.content_html.contains("<b>Contents"));
        mock::reset();
    }
}
