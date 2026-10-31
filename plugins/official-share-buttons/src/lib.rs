//! 公式プラグイン `official-share-buttons`: 記事・固定ページに SNS 共有ボタンを足す（`on-render-page`）。
//!
//! - X (Twitter)、LINE、URL コピー、Facebook、はてなブックマーク、Bluesky、Threads、Pocket、LinkedIn に標準対応
//! - 設定（`[plugins.settings.official-share-buttons]` または Admin Studio）で表示するプラットフォームを切り替え・並び替え可能
//! - `custom_services` 設定により、任意の SNS・Web サービスの共有リンクを追加可能
//! - `style = "buttons"`（アイコン＋文字）または `"icons"`（アイコンのみ）
//! - `position = "bottom"`（本文末尾、既定）、`"top"`（本文先頭）、`"both"`（両方）
//! - JavaScript なしでも安全に動作（スクリプト有効時は現在の URL・タイトルと連携し、URL コピーも動作）

use aegis_plugin_sdk::prelude::*;

#[derive(Default)]
pub struct ShareButtonsPlugin;

const SCRIPT: &str = "(function(){\
function init(){\
var u=encodeURIComponent(window.location.href),t=encodeURIComponent(document.title);\
var btns=document.querySelectorAll('.osb-btn[data-tmpl]');\
for(var i=0;i<btns.length;i++){\
var b=btns[i];b.href=b.getAttribute('data-tmpl').replace(/__URL__/g,u).replace(/__TITLE__/g,t);\
}\
var copyBtns=document.querySelectorAll('.osb-btn-copy');\
for(var j=0;j<copyBtns.length;j++){\
(function(btn){\
btn.addEventListener('click',function(e){\
e.preventDefault();var txt=window.location.href;\
function done(){\
var tip=btn.getAttribute('data-copied')||'Copied!';\
var lbl=btn.querySelector('.osb-lbl');\
if(lbl){\
var old=lbl.textContent;lbl.textContent=tip;\
setTimeout(function(){lbl.textContent=old;},2000);\
}else{\
var oldT=btn.getAttribute('title');btn.setAttribute('title',tip);\
setTimeout(function(){btn.setAttribute('title',oldT||'');},2000);\
}\
}\
if(navigator.clipboard&&navigator.clipboard.writeText){\
navigator.clipboard.writeText(txt).then(done).catch(function(){});\
}else{\
var inp=document.createElement('input');inp.value=txt;\
document.body.appendChild(inp);inp.select();\
try{document.execCommand('copy');done();}catch(_){}\
document.body.removeChild(inp);\
}\
});\
})(copyBtns[j]);\
}\
}\
document.readyState==='loading'?document.addEventListener('DOMContentLoaded',init):init();\
})();";

// --- 設定ヘルパー ---

fn setting(key: &str) -> Option<String> {
    config::get(key)
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
}

fn setting_choice(key: &str, allowed: &[&'static str], default: &'static str) -> &'static str {
    setting(key)
        .and_then(|v| allowed.iter().copied().find(|a| *a == v))
        .unwrap_or(default)
}

fn css_color(value: &str) -> Option<String> {
    let v = value.trim();
    if let Some(hex) = v.strip_prefix('#') {
        if matches!(hex.len(), 3 | 4 | 6 | 8) && hex.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Some(format!("#{}", hex));
        }
    } else if let Some(name) = v.strip_prefix("var(--").and_then(|r| r.strip_suffix(')')) {
        if !name.is_empty()
            && name
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
        {
            return Some(format!("var(--{})", name));
        }
    }
    None
}

fn escape_html(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            other => out.push(other),
        }
    }
    out
}

// --- アイコン SVG ---

const SVG_X: &str = "<svg viewBox=\"0 0 24 24\" width=\"16\" height=\"16\" fill=\"currentColor\" aria-hidden=\"true\"><path d=\"M18.244 2.25h3.308l-7.227 8.26 8.502 11.24H16.17l-5.214-6.817L4.99 21.75H1.68l7.73-8.835L1.254 2.25H8.08l4.713 6.231zm-1.161 17.52h1.833L7.084 4.126H5.117z\"/></svg>";
const SVG_LINE: &str = "<svg viewBox=\"0 0 24 24\" width=\"16\" height=\"16\" fill=\"currentColor\" aria-hidden=\"true\"><path d=\"M12 2C6.48 2 2 5.82 2 10.53c0 4.22 3.6 7.76 8.47 8.41.33.07.78.22.89.5.1.26.07.66.03.92l-.15.93c-.05.28-.22 1.1.97.6 1.18-.5 6.38-3.76 8.71-6.44C22.47 13.56 22 12.08 22 10.53 22 5.82 17.52 2 12 2zm-5.18 10.6H5.35c-.2 0-.36-.16-.36-.36V8.67c0-.2.16-.36.36-.36h1.47c.2 0 .36.16.36.36v3.57c0 .2-.16.36-.36.36zm2.74 0H8.09c-.2 0-.36-.16-.36-.36V8.67c0-.2.16-.36.36-.36h.72c.2 0 .36.16.36.36v2.85h1.75c.2 0 .36.16.36.36v.36c0 .2-.16.36-.36.36zm2.84 0h-.72c-.2 0-.36-.16-.36-.36V8.67c0-.2.16-.36.36-.36h.72c.2 0 .36.16.36.36v3.57c0 .2-.16.36-.36.36zm4.33 0h-2.18c-.2 0-.36-.16-.36-.36V8.67c0-.2.16-.36.36-.36h2.18c.2 0 .36.16.36.36v.36c0 .2-.16.36-.36.36h-1.46v.7h1.46c.2 0 .36.16.36.36v.36c0 .2-.16.36-.36.36h-1.46v.7h1.46c.2 0 .36.16.36.36v.36c0 .2-.16.36-.36.36z\"/></svg>";
const SVG_COPY: &str = "<svg viewBox=\"0 0 24 24\" width=\"16\" height=\"16\" fill=\"none\" stroke=\"currentColor\" stroke-width=\"2\" stroke-linecap=\"round\" stroke-linejoin=\"round\" aria-hidden=\"true\"><rect x=\"9\" y=\"9\" width=\"13\" height=\"13\" rx=\"2\" ry=\"2\"/><path d=\"M5 15H4a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2h9a2 2 0 0 1 2 2v1\"/></svg>";
const SVG_FACEBOOK: &str = "<svg viewBox=\"0 0 24 24\" width=\"16\" height=\"16\" fill=\"currentColor\" aria-hidden=\"true\"><path d=\"M24 12.073c0-6.627-5.373-12-12-12s-12 5.373-12 12c0 5.99 4.388 10.954 10.125 11.854v-8.385H7.078v-3.47h3.047V9.43c0-3.007 1.792-4.669 4.533-4.669 1.312 0 2.686.235 2.686.235v2.953H15.83c-1.491 0-1.956.925-1.956 1.874v2.25h3.328l-.532 3.47h-2.796v8.385C19.612 23.027 24 18.062 24 12.073z\"/></svg>";
const SVG_HATENA: &str = "<svg viewBox=\"0 0 24 24\" width=\"16\" height=\"16\" fill=\"currentColor\" aria-hidden=\"true\"><path d=\"M4.5 3h6.75c3.728 0 6.75 3.022 6.75 6.75 0 1.93-.81 3.673-2.11 4.908 1.972 1.127 3.31 3.25 3.31 5.692 0 3.672-2.978 6.65-6.65 6.65H4.5V3zm5.7 8.35h1.05c1.464 0 2.65-1.186 2.65-2.65S12.714 6.05 11.25 6.05h-1.05v5.3zm0 10.6h1.35c1.63 0 2.95-1.32 2.95-2.95 0-1.63-1.32-2.95-2.95-2.95h-1.35v5.9z\"/><circle cx=\"19.5\" cy=\"18\" r=\"1.8\"/></svg>";
const SVG_BLUESKY: &str = "<svg viewBox=\"0 0 24 24\" width=\"16\" height=\"16\" fill=\"currentColor\" aria-hidden=\"true\"><path d=\"M12 10.8c-1.087-2.114-4.046-6.053-6.798-7.995C2.566 1.01 1.25 1.758 1.25 3.968c0 2.21.375 7.95 1.5 10.02 1.34 2.463 3.97 2.97 6.13 2.502-3.14.75-4.8 2.22-3.37 4.23 2.11 2.97 5.86 1.74 6.49-1.28.63 3.02 4.38 4.25 6.49 1.28 1.43-2.01-.23-3.48-3.37-4.23 2.16.468 4.79-.039 6.13-2.502 1.125-2.07 1.5-7.81 1.5-10.02 0-2.21-1.316-2.958-3.952-1.163C16.046 4.747 13.087 8.686 12 10.8z\"/></svg>";
const SVG_THREADS: &str = "<svg viewBox=\"0 0 24 24\" width=\"16\" height=\"16\" fill=\"currentColor\" aria-hidden=\"true\"><path d=\"M12.186 24C5.466 24 0 18.625 0 12S5.466 0 12.186 0c6.602 0 11.97 5.213 12.174 11.754H21.41C21.21 6.71 17.18 2.95 12.186 2.95 7.14 2.95 3.02 7.012 3.02 12s4.12 9.05 9.166 9.05c3.784 0 7.037-2.183 8.358-5.394l2.71 1.22C21.48 21.106 17.135 24 12.186 24z\"/></svg>";
const SVG_POCKET: &str = "<svg viewBox=\"0 0 24 24\" width=\"16\" height=\"16\" fill=\"currentColor\" aria-hidden=\"true\"><path d=\"M12 2C6.477 2 2 6.477 2 12v3c0 5.523 4.477 10 10 10s10-4.477 10-10v-3c0-5.523-4.477-10-10-10zm4.53 9.47l-4 4a.75.75 0 0 1-1.06 0l-4-4a.75.75 0 0 1 1.06-1.06L12 13.94l3.47-3.53a.75.75 0 1 1 1.06 1.06z\"/></svg>";
const SVG_LINKEDIN: &str = "<svg viewBox=\"0 0 24 24\" width=\"16\" height=\"16\" fill=\"currentColor\" aria-hidden=\"true\"><path d=\"M19 3a2 2 0 0 1 2 2v14a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h14m-.5 15.5v-5.3a3.26 3.26 0 0 0-3.26-3.26c-.85 0-1.84.52-2.28 1.3v-1.11h-2.79v8.37h2.79v-4.93c0-.77.62-1.4 1.39-1.4a1.4 1.4 0 0 1 1.4 1.4v4.93h2.75M6.46 10.9h2.79v8.37H6.46V10.9M7.86 6.72a1.62 1.62 0 1 0 0 3.24 1.62 1.62 0 0 0 0-3.24z\"/></svg>";
const SVG_GENERIC_SHARE: &str = "<svg viewBox=\"0 0 24 24\" width=\"16\" height=\"16\" fill=\"none\" stroke=\"currentColor\" stroke-width=\"2\" stroke-linecap=\"round\" stroke-linejoin=\"round\" aria-hidden=\"true\"><circle cx=\"18\" cy=\"5\" r=\"3\"/><circle cx=\"6\" cy=\"12\" r=\"3\"/><circle cx=\"18\" cy=\"19\" r=\"3\"/><line x1=\"8.59\" y1=\"13.51\" x2=\"15.42\" y2=\"17.49\"/><line x1=\"15.41\" y1=\"6.51\" x2=\"8.59\" y2=\"10.49\"/></svg>";

struct ShareItem {
    id: String,
    label_ja: String,
    label_en: String,
    bg_color: String,
    fg_color: String,
    url_template: String,
    svg: String,
    is_copy: bool,
}

impl ShareItem {
    fn built_in(id: &str, x_via: &str, x_hashtags: &str) -> Option<Self> {
        match id {
            "x" => {
                let mut url = "https://x.com/intent/tweet?url=__URL__&text=__TITLE__".to_string();
                if !x_via.is_empty() {
                    url.push_str("&via=");
                    url.push_str(x_via);
                }
                if !x_hashtags.is_empty() {
                    url.push_str("&hashtags=");
                    url.push_str(x_hashtags);
                }
                Some(Self {
                    id: "x".to_string(),
                    label_ja: "X".to_string(),
                    label_en: "X".to_string(),
                    bg_color: "#000000".to_string(),
                    fg_color: "#ffffff".to_string(),
                    url_template: url,
                    svg: SVG_X.to_string(),
                    is_copy: false,
                })
            }
            "line" => Some(Self {
                id: "line".to_string(),
                label_ja: "LINE".to_string(),
                label_en: "LINE".to_string(),
                bg_color: "#06c755".to_string(),
                fg_color: "#ffffff".to_string(),
                url_template: "https://social-plugins.line.me/lineit/share?url=__URL__".to_string(),
                svg: SVG_LINE.to_string(),
                is_copy: false,
            }),
            "copy" => Some(Self {
                id: "copy".to_string(),
                label_ja: "URLコピー".to_string(),
                label_en: "Copy Link".to_string(),
                bg_color: "#374151".to_string(),
                fg_color: "#ffffff".to_string(),
                url_template: String::new(),
                svg: SVG_COPY.to_string(),
                is_copy: true,
            }),
            "facebook" => Some(Self {
                id: "facebook".to_string(),
                label_ja: "Facebook".to_string(),
                label_en: "Facebook".to_string(),
                bg_color: "#1877f2".to_string(),
                fg_color: "#ffffff".to_string(),
                url_template: "https://www.facebook.com/sharer/sharer.php?u=__URL__".to_string(),
                svg: SVG_FACEBOOK.to_string(),
                is_copy: false,
            }),
            "hatena" => Some(Self {
                id: "hatena".to_string(),
                label_ja: "はてなブックマーク".to_string(),
                label_en: "Hatena".to_string(),
                bg_color: "#00a4de".to_string(),
                fg_color: "#ffffff".to_string(),
                url_template: "https://b.hatena.ne.jp/entry/__URL__".to_string(),
                svg: SVG_HATENA.to_string(),
                is_copy: false,
            }),
            "bluesky" => Some(Self {
                id: "bluesky".to_string(),
                label_ja: "Bluesky".to_string(),
                label_en: "Bluesky".to_string(),
                bg_color: "#0285ff".to_string(),
                fg_color: "#ffffff".to_string(),
                url_template: "https://bsky.app/intent/compose?text=__TITLE__%20__URL__"
                    .to_string(),
                svg: SVG_BLUESKY.to_string(),
                is_copy: false,
            }),
            "threads" => Some(Self {
                id: "threads".to_string(),
                label_ja: "Threads".to_string(),
                label_en: "Threads".to_string(),
                bg_color: "#000000".to_string(),
                fg_color: "#ffffff".to_string(),
                url_template: "https://www.threads.net/intent/post?text=__TITLE__%20__URL__"
                    .to_string(),
                svg: SVG_THREADS.to_string(),
                is_copy: false,
            }),
            "pocket" => Some(Self {
                id: "pocket".to_string(),
                label_ja: "Pocket".to_string(),
                label_en: "Pocket".to_string(),
                bg_color: "#ef4056".to_string(),
                fg_color: "#ffffff".to_string(),
                url_template: "https://getpocket.com/edit?url=__URL__&title=__TITLE__".to_string(),
                svg: SVG_POCKET.to_string(),
                is_copy: false,
            }),
            "linkedin" => Some(Self {
                id: "linkedin".to_string(),
                label_ja: "LinkedIn".to_string(),
                label_en: "LinkedIn".to_string(),
                bg_color: "#0a66c2".to_string(),
                fg_color: "#ffffff".to_string(),
                url_template: "https://www.linkedin.com/sharing/share-offsite/?url=__URL__"
                    .to_string(),
                svg: SVG_LINKEDIN.to_string(),
                is_copy: false,
            }),
            _ => None,
        }
    }

    /// `Name|URL_TEMPLATE|COLOR` 形式のカスタム行を解釈
    fn from_custom_line(line: &str) -> Option<Self> {
        let parts: Vec<&str> = line.split('|').map(str::trim).collect();
        if parts.len() < 2 {
            return None;
        }
        let name = parts[0];
        let template = parts[1];
        if name.is_empty() || template.is_empty() {
            return None;
        }
        let bg_color = parts
            .get(2)
            .and_then(|c| css_color(c))
            .unwrap_or_else(|| "#4f46e5".to_string());

        let url_template = template
            .replace("{url}", "__URL__")
            .replace("{title}", "__TITLE__");

        Some(Self {
            id: format!("custom-{}", name.to_ascii_lowercase()),
            label_ja: name.to_string(),
            label_en: name.to_string(),
            bg_color,
            fg_color: "#ffffff".to_string(),
            url_template,
            svg: SVG_GENERIC_SHARE.to_string(),
            is_copy: false,
        })
    }
}

struct Settings {
    content_types: Vec<String>,
    position: &'static str,
    style: &'static str,
    shape: &'static str,
    heading: Option<String>,
    items: Vec<ShareItem>,
}

fn load_settings() -> Settings {
    let content_types = setting("content_types")
        .unwrap_or_else(|| "article".to_string())
        .split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();

    let position = setting_choice("position", &["bottom", "top", "both"], "bottom");
    let style = setting_choice("style", &["buttons", "icons"], "buttons");
    let shape = setting_choice("shape", &["rounded", "pill", "square"], "rounded");
    let heading = setting("heading");
    let x_via = setting("x_via").unwrap_or_default();
    let x_hashtags = setting("x_hashtags").unwrap_or_default();

    let platforms_str = setting("platforms").unwrap_or_else(|| "x,line,copy".to_string());
    let mut items = Vec::new();

    // 1. 組み込みプラットフォーム
    for p in platforms_str.split(',') {
        let key = p.trim().to_ascii_lowercase();
        if let Some(item) = ShareItem::built_in(&key, &x_via, &x_hashtags) {
            items.push(item);
        }
    }

    // 2. カスタム共有先（custom_services 設定から行単位で追加）
    if let Some(custom_raw) = setting("custom_services") {
        for line in custom_raw.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            if let Some(custom_item) = ShareItem::from_custom_line(line) {
                items.push(custom_item);
            }
        }
    }

    Settings {
        content_types,
        position,
        style,
        shape,
        heading,
        items,
    }
}

fn truthy(val: &serde_json::Value) -> bool {
    match val {
        serde_json::Value::Bool(b) => *b,
        serde_json::Value::String(s) => matches!(
            s.trim().to_ascii_lowercase().as_str(),
            "true" | "1" | "yes" | "on"
        ),
        serde_json::Value::Number(n) => n.as_i64().is_some_and(|i| i != 0),
        _ => false,
    }
}

fn render_buttons_html(settings: &Settings, is_ja: bool) -> String {
    if settings.items.is_empty() {
        return String::new();
    }

    let heading_text = match &settings.heading {
        Some(h) if !h.trim().is_empty() => escape_html(h),
        _ => if is_ja {
            "この記事をシェアする"
        } else {
            "Share this article"
        }
        .to_string(),
    };

    let shape_class = match settings.shape {
        "pill" => "osb-pill",
        "square" => "osb-square",
        _ => "osb-rounded",
    };

    let style_class = if settings.style == "icons" {
        "osb-icons-only"
    } else {
        "osb-with-labels"
    };

    let mut html = String::new();
    html.push_str("<div class=\"osb-container\">\n");
    if !heading_text.is_empty() {
        html.push_str(&format!(
            "  <div class=\"osb-heading\">{}</div>\n",
            heading_text
        ));
    }
    html.push_str(&format!(
        "  <div class=\"osb-list {} {}\">\n",
        style_class, shape_class
    ));

    for item in &settings.items {
        let label = if is_ja {
            &item.label_ja
        } else {
            &item.label_en
        };
        let copied_tip = if is_ja {
            "コピーしました！"
        } else {
            "Copied!"
        };

        if item.is_copy {
            html.push_str(&format!(
                "    <button type=\"button\" class=\"osb-btn osb-btn-copy osb-{}\" style=\"--osb-bg:{};--osb-fg:{};\" data-copied=\"{}\" title=\"{}\" aria-label=\"{}\">\n",
                item.id, item.bg_color, item.fg_color, copied_tip, label, label
            ));
            html.push_str(&format!(
                "      <span class=\"osb-ico\">{}</span>\n",
                item.svg
            ));
            if settings.style != "icons" {
                html.push_str(&format!(
                    "      <span class=\"osb-lbl\">{}</span>\n",
                    escape_html(label)
                ));
            }
            html.push_str("    </button>\n");
        } else {
            html.push_str(&format!(
                "    <a class=\"osb-btn osb-btn-{} osb-{}\" style=\"--osb-bg:{};--osb-fg:{};\" href=\"#\" data-tmpl=\"{}\" target=\"_blank\" rel=\"noopener noreferrer\" title=\"{}\" aria-label=\"{}\">\n",
                item.id, item.id, item.bg_color, item.fg_color, item.url_template, label, label
            ));
            html.push_str(&format!(
                "      <span class=\"osb-ico\">{}</span>\n",
                item.svg
            ));
            if settings.style != "icons" {
                html.push_str(&format!(
                    "      <span class=\"osb-lbl\">{}</span>\n",
                    escape_html(label)
                ));
            }
            html.push_str("    </a>\n");
        }
    }

    html.push_str("  </div>\n");
    html.push_str("</div>\n");
    html
}

fn css() -> &'static str {
    ".osb-container{margin:2rem 0;padding:1.25rem 0;clear:both}\
.osb-heading{font-size:0.95rem;font-weight:700;margin-bottom:0.75rem;color:var(--text-primary,#ffffff)}\
.osb-list{display:flex;flex-wrap:wrap;gap:0.5rem;align-items:center}\
.osb-btn{display:inline-flex;align-items:center;justify-content:center;gap:0.4rem;padding:0.45rem 0.85rem;\
font-size:0.82rem;font-weight:600;text-decoration:none;color:var(--osb-fg,#fff);background:var(--osb-bg,#4f46e5);\
border:none;cursor:pointer;line-height:1;transition:opacity 0.15s ease,transform 0.15s ease;-webkit-user-select:none;user-select:none}\
.osb-btn:hover{opacity:0.88;transform:translateY(-1px)}\
.osb-btn:active{transform:translateY(0)}\
.osb-rounded{border-radius:6px}\
.osb-pill{border-radius:9999px}\
.osb-square{border-radius:0}\
.osb-icons-only .osb-btn{padding:0.55rem;width:36px;height:36px}\
.osb-ico{display:inline-flex;align-items:center;justify-content:center}\
.osb-ico svg{display:block}\
.osb-lbl{white-space:nowrap}"
}

impl Plugin for ShareButtonsPlugin {
    fn on_render_page(&mut self, mut ctx: PageRenderContext) -> Result<PageRenderContext> {
        let settings = load_settings();

        if !settings.content_types.contains(&ctx.content_type)
            || ctx.fields.get("hide_share_buttons").is_some_and(truthy)
            || settings.items.is_empty()
        {
            return Ok(ctx);
        }

        let is_ja = ctx
            .theme_vars
            .get("locale")
            .map(|l| l.starts_with("ja"))
            .unwrap_or(true);

        let buttons_html = render_buttons_html(&settings, is_ja);

        // CSS とスクリプトの注入
        if !ctx.extra_css.contains(".osb-container") {
            if !ctx.extra_css.is_empty() {
                ctx.extra_css.push('\n');
            }
            ctx.extra_css.push_str(css());
        }

        let script_tag = format!("<script>{}</script>", SCRIPT);
        if !ctx.extra_head.contains("osb-btn") {
            if !ctx.extra_head.is_empty() {
                ctx.extra_head.push('\n');
            }
            ctx.extra_head.push_str(&script_tag);
        }

        // 位置に応じた挿入
        match settings.position {
            "top" => {
                ctx.content_html = format!("{}\n{}", buttons_html, ctx.content_html);
            }
            "both" => {
                ctx.content_html =
                    format!("{}\n{}\n{}", buttons_html, ctx.content_html, buttons_html);
            }
            _ => {
                ctx.content_html.push('\n');
                ctx.content_html.push_str(&buttons_html);
            }
        }

        Ok(ctx)
    }
}

aegis_plugin_sdk::register_plugin!(ShareButtonsPlugin);

#[cfg(test)]
mod tests {
    use super::*;
    use aegis_plugin_sdk::host::mock;

    fn page() -> PageRenderContext {
        PageRenderContext {
            content_type: "article".to_string(),
            content_html: "<p>Hello AegisCMS</p>".to_string(),
            title: "My Great Article".to_string(),
            slug: "my-great-article".to_string(),
            ..Default::default()
        }
    }

    fn render(ctx: PageRenderContext) -> PageRenderContext {
        ShareButtonsPlugin::default().on_render_page(ctx).unwrap()
    }

    #[test]
    fn default_renders_x_line_copy_at_bottom() {
        mock::reset();
        let out = render(page());
        assert!(out.content_html.contains("<div class=\"osb-container\">"));
        assert!(out.content_html.contains("osb-x"));
        assert!(out.content_html.contains("osb-line"));
        assert!(out.content_html.contains("osb-copy"));
        assert!(out
            .content_html
            .contains("data-tmpl=\"https://x.com/intent/tweet?url=__URL__&text=__TITLE__\""));
        assert!(out.extra_css.contains(".osb-container"));
        assert!(out.extra_head.contains("osb-btn"));
    }

    #[test]
    fn skips_rendering_when_hidden_or_different_type() {
        mock::reset();
        let mut p = page();
        p.content_type = "product".to_string();
        let out = render(p);
        assert!(!out.content_html.contains("osb-container"));
        assert!(out.extra_css.is_empty());

        let mut p_hide = page();
        p_hide.fields.insert(
            "hide_share_buttons".to_string(),
            serde_json::Value::Bool(true),
        );
        let out_hide = render(p_hide);
        assert!(!out_hide.content_html.contains("osb-container"));
    }

    #[test]
    fn custom_services_added_via_settings() {
        mock::reset();
        mock::set_config("platforms", "x,copy");
        mock::set_config(
            "custom_services",
            "Mastodon|https://mastodon.social/share?text={title}&url={url}|#6364ff\nMisskey|https://misskey-hub.net/share?text={title}&url={url}",
        );
        let out = render(page());
        assert!(out.content_html.contains("osb-custom-mastodon"));
        assert!(out.content_html.contains("osb-custom-misskey"));
        assert!(out
            .content_html
            .contains("style=\"--osb-bg:#6364ff;--osb-fg:#ffffff;\""));
        assert!(out
            .content_html
            .contains("data-tmpl=\"https://mastodon.social/share?text=__TITLE__&url=__URL__\""));
        assert!(out
            .content_html
            .contains("data-tmpl=\"https://misskey-hub.net/share?text=__TITLE__&url=__URL__\""));
        mock::reset();
    }

    #[test]
    fn position_top_and_shape_pill() {
        mock::reset();
        mock::set_config("position", "top");
        mock::set_config("shape", "pill");
        mock::set_config("style", "icons");
        let out = render(page());
        assert!(out
            .content_html
            .starts_with("<div class=\"osb-container\">"));
        assert!(out.content_html.contains("osb-pill"));
        assert!(out.content_html.contains("osb-icons-only"));
        mock::reset();
    }
}
