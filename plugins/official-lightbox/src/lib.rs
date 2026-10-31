//! 公式プラグイン `official-lightbox`: 本文の画像をクリックで拡大して見る（`on-render-page`）。
//!
//! - スクリプトが本文の範囲（`selector`）の画像を、`<dialog>` の拡大表示につなぐ。前後の画像への移動
//!   （ボタン・矢印キー）、Esc / 背景のクリックで閉じる、`alt` の説明文、フォーカスの戻しに対応する
//! - リンクの中の画像（クリックでリンク先へ行くもの）と、小さな画像（`min_width_px` 未満）は対象にしない
//! - 本文に `<img` が無いページには何も足さない。スクリプトが動かなくても、画像はそのまま見える
//! - 見た目と動きは設定（`[plugins.settings.official-lightbox]`）で変える。色は許可する形だけを CSS に入れる

use aegis_plugin_sdk::prelude::*;

#[derive(Default)]
pub struct LightboxPlugin;

const DEFAULT_SCOPE: &str = ".prose";

const SCRIPT: &str = "(function(){var C=__CONFIG__;if(typeof HTMLDialogElement==='undefined')return;\
var imgs=[],dlg,full,cap,prev,next,close,opener,index=0;\
function ok(i){return i.getBoundingClientRect().width>=C.min||!i.complete}\
function list(){return imgs.filter(ok)}\
function src(i){return i.currentSrc||i.src}\
function build(){dlg=document.createElement('dialog');dlg.className='olb';dlg.setAttribute('aria-label',C.t.viewer);\
full=document.createElement('img');full.className='olb-full';full.alt='';\
cap=document.createElement('p');cap.className='olb-caption';\
function btn(c,l,t){var b=document.createElement('button');b.type='button';b.className='olb-btn '+c;b.setAttribute('aria-label',l);b.textContent=t;return b}\
close=btn('olb-close',C.t.close,'\\u00d7');prev=btn('olb-prev',C.t.prev,'\\u2039');next=btn('olb-next',C.t.next,'\\u203a');\
dlg.appendChild(full);dlg.appendChild(cap);dlg.appendChild(close);dlg.appendChild(prev);dlg.appendChild(next);document.body.appendChild(dlg);\
close.addEventListener('click',function(){dlg.close()});prev.addEventListener('click',function(){go(-1)});next.addEventListener('click',function(){go(1)});\
dlg.addEventListener('click',function(e){if(e.target===dlg||e.target===cap)dlg.close()});\
dlg.addEventListener('keydown',function(e){if(e.key==='ArrowLeft'){go(-1);e.preventDefault()}else if(e.key==='ArrowRight'){go(1);e.preventDefault()}});\
dlg.addEventListener('close',function(){document.documentElement.classList.remove('olb-open');full.removeAttribute('src');\
if(opener&&document.contains(opener))opener.focus();opener=null})}\
function show(){var l=list(),i=l[index];if(!i)return;full.src=src(i);full.alt=i.alt||'';\
cap.textContent=C.caption?(i.alt||''):'';cap.hidden=!cap.textContent;prev.hidden=next.hidden=l.length<2}\
function go(d){var l=list();if(l.length<2)return;index=(index+d+l.length)%l.length;show()}\
function open(i){var l=list();index=l.indexOf(i);if(index<0)return;if(!dlg)build();opener=i;show();\
document.documentElement.classList.add('olb-open');dlg.showModal()}\
function init(){try{C.selector.split(',').forEach(function(s){s=s.trim();if(!s)return;\
Array.prototype.forEach.call(document.querySelectorAll(s+' img'),function(i){\
if(imgs.indexOf(i)<0&&!i.closest('a,button'))imgs.push(i)})})}catch(e){return}\
imgs.forEach(function(i){i.classList.add('olb-img');i.setAttribute('tabindex','0');i.setAttribute('aria-haspopup','dialog')});\
document.addEventListener('click',function(e){var i=e.target;if(i&&i.classList&&i.classList.contains('olb-img')&&ok(i)){e.preventDefault();open(i)}});\
document.addEventListener('keydown',function(e){var i=e.target;if((e.key==='Enter'||e.key===' ')&&i&&i.classList&&i.classList.contains('olb-img')&&ok(i)){e.preventDefault();open(i)}})}\
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

/// 本文の範囲を表す CSS セレクター。短い 1 行の文字だけ（スクリプトには JSON の文字列として渡す）
fn setting_selector(key: &str, default: &str) -> String {
    setting(key)
        .filter(|v| v.len() <= 200 && !v.chars().any(char::is_control))
        .unwrap_or_else(|| default.to_string())
}

/// `<script>` の中に置ける JSON（`<` `>` `&` と行の区切りをエスケープする）
fn script_json(value: &serde_json::Value) -> String {
    value
        .to_string()
        .replace('<', "\\u003c")
        .replace('>', "\\u003e")
        .replace('&', "\\u0026")
        .replace('\u{2028}', "\\u2028")
        .replace('\u{2029}', "\\u2029")
}

struct Settings {
    content_types: Vec<String>,
    selector: String,
    min_width: u32,
    caption: bool,
    animate: bool,
    bg: String,
    ja: bool,
}

impl Settings {
    fn load() -> Self {
        Self {
            content_types: setting("content_types")
                .unwrap_or_else(|| "article,page".to_string())
                .split(',')
                .map(|v| v.trim().to_string())
                .filter(|v| !v.is_empty())
                .collect(),
            selector: setting_selector("selector", DEFAULT_SCOPE),
            min_width: setting_int("min_width_px", 120, 0, 2000),
            caption: setting_bool("show_caption", true),
            animate: setting_bool("animate", true),
            bg: setting_color("bg_color", "rgba(0,0,0,.88)"),
            ja: setting("locale").as_deref() == Some("ja"),
        }
    }

    /// 画面の読み上げに使う文言
    fn texts(&self) -> serde_json::Value {
        if self.ja {
            serde_json::json!({"viewer": "画像の拡大表示", "close": "閉じる", "prev": "前の画像", "next": "次の画像"})
        } else {
            serde_json::json!({"viewer": "Image viewer", "close": "Close", "prev": "Previous image", "next": "Next image"})
        }
    }
}

// --- 出力 ---

fn css(s: &Settings) -> String {
    let mut css = format!(
        ".olb-img{{cursor:zoom-in}}\
         html.olb-open{{overflow:hidden}}\
         dialog.olb{{position:fixed;inset:0;width:100%;height:100%;max-width:none;max-height:none;margin:0;padding:3rem 1rem 3.5rem;\
         border:0;background:{bg};color:#fff;box-sizing:border-box;overflow:hidden}}\
         dialog.olb[open]{{display:flex;flex-direction:column;align-items:center;justify-content:center;gap:.75rem}}\
         dialog.olb::backdrop{{background:transparent}}\
         .olb-full{{display:block;max-width:100%;max-height:calc(100% - 2.5rem);min-height:0;object-fit:contain;cursor:zoom-out}}\
         .olb-caption{{margin:0;max-width:60rem;text-align:center;font-size:.9rem;line-height:1.5;cursor:pointer}}\
         .olb-btn{{position:absolute;display:flex;align-items:center;justify-content:center;width:44px;height:44px;padding:0;border:0;\
         border-radius:50%;background:rgba(255,255,255,.16);color:#fff;font-size:1.8rem;line-height:1;cursor:pointer}}\
         .olb-btn:hover{{background:rgba(255,255,255,.3)}}\
         .olb-btn:focus-visible{{outline:2px solid #fff;outline-offset:2px}}\
         .olb-close{{top:.75rem;right:.75rem}}\
         .olb-prev,.olb-next{{top:50%;transform:translateY(-50%)}}\
         .olb-prev{{left:.75rem}}.olb-next{{right:.75rem}}\
         .olb-btn[hidden]{{display:none}}\
         @media print{{dialog.olb{{display:none}}}}",
        bg = s.bg,
    );
    if s.animate {
        css.push_str(
            "@media (prefers-reduced-motion:no-preference){dialog.olb[open]{animation:olb-fade .18s ease-out}\
             @keyframes olb-fade{from{opacity:0}to{opacity:1}}}",
        );
    }
    css
}

fn push_line(target: &mut String, addition: &str) {
    if !target.is_empty() {
        target.push('\n');
    }
    target.push_str(addition);
}

impl Plugin for LightboxPlugin {
    fn on_render_page(&mut self, mut ctx: PageRenderContext) -> Result<PageRenderContext> {
        let settings = Settings::load();
        if !ctx.content_html.contains("<img") || !settings.content_types.contains(&ctx.content_type)
        {
            return Ok(ctx);
        }
        let config = serde_json::json!({
            "selector": settings.selector,
            "min": settings.min_width,
            "caption": settings.caption,
            "t": settings.texts(),
        });
        push_line(&mut ctx.extra_css, &css(&settings));
        push_line(
            &mut ctx.extra_head,
            &format!(
                "<script>{}</script>",
                SCRIPT.replace("__CONFIG__", &script_json(&config))
            ),
        );
        Ok(ctx)
    }
}

aegis_plugin_sdk::register_plugin!(LightboxPlugin);

#[cfg(test)]
mod tests {
    use super::*;
    use aegis_plugin_sdk::host::mock;

    fn page() -> PageRenderContext {
        PageRenderContext {
            content_type: "article".to_string(),
            content_html: "<figure><img src=\"/uploads/a.png\" alt=\"A cat\"></figure>".to_string(),
            ..Default::default()
        }
    }

    fn render(ctx: PageRenderContext) -> PageRenderContext {
        LightboxPlugin.on_render_page(ctx).unwrap()
    }

    #[test]
    fn adds_the_viewer_to_pages_with_images() {
        mock::reset();
        let out = render(page());
        assert_eq!(out.content_html, page().content_html);
        assert!(out.extra_css.contains("dialog.olb{"));
        assert!(out.extra_css.contains("background:rgba(0,0,0,.88)"));
        assert!(out.extra_css.contains("olb-fade"));
        assert!(out.extra_head.starts_with("<script>"));
        assert!(out.extra_head.contains("\"min\":120"));
        assert!(out.extra_head.contains("\"selector\":\".prose\""));
        assert!(out.extra_head.contains("\"close\":\"Close\""));
        assert!(!out.extra_head.contains("__CONFIG__"));
    }

    #[test]
    fn leaves_pages_without_images_and_other_types_alone() {
        mock::reset();
        let mut plain = page();
        plain.content_html = "<p>No image</p>".to_string();
        assert_eq!(render(plain.clone()), plain);
        let mut index = page();
        index.content_type = "index".to_string();
        assert_eq!(render(index.clone()), index);
        mock::reset();
    }

    #[test]
    fn texts_follow_the_locale() {
        mock::reset();
        mock::set_config("locale", "ja");
        let out = render(page());
        assert!(out.extra_head.contains("\"close\":\"閉じる\""));
        assert!(out.extra_head.contains("\"next\":\"次の画像\""));
        mock::reset();
    }

    #[test]
    fn options_change_the_output() {
        mock::reset();
        mock::set_config("animate", "false");
        mock::set_config("show_caption", "false");
        mock::set_config("min_width_px", "300");
        mock::set_config("bg_color", "#000");
        let out = render(page());
        assert!(!out.extra_css.contains("olb-fade"));
        assert!(out.extra_css.contains("background:#000;"));
        assert!(out.extra_head.contains("\"caption\":false"));
        assert!(out.extra_head.contains("\"min\":300"));
        mock::reset();
    }

    #[test]
    fn settings_are_checked() {
        mock::reset();
        mock::set_config("bg_color", "red;} body{display:none");
        mock::set_config("min_width_px", "-5");
        mock::set_config("selector", &"x".repeat(300));
        let out = render(page());
        assert!(!out.extra_css.contains("body{display:none"));
        assert!(out.extra_css.contains("background:rgba(0,0,0,.88)"));
        assert!(out.extra_head.contains("\"min\":120"));
        assert!(out.extra_head.contains("\"selector\":\".prose\""));
        mock::reset();
    }

    #[test]
    fn the_selector_cannot_close_the_script() {
        mock::reset();
        mock::set_config("selector", "</script><script>alert(1)</script>");
        let out = render(page());
        assert_eq!(out.extra_head.matches("</script>").count(), 1);
        mock::reset();
    }
}
