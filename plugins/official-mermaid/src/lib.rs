//! 公式プラグイン `official-mermaid`: ` ```mermaid ` のコードブロックを Mermaid の図にする（`on-render-page`）。
//!
//! - 本文に `language-mermaid` のブロックがあるページだけ、Mermaid を読み込む。図の無いページには、
//!   スクリプトも CSS も入れない（Mermaid は重いので、全ページには入れない）
//! - 図にできなかったブロック（構文の誤り・スクリプトが読めない）は、元のコードのまま残る
//! - テーマは `auto`（ページの背景色の明るさで、暗ければ `dark`、明るければ `default`）か、固定の名前
//! - `securityLevel` は既定で `strict`（図の中の HTML・クリック処理を許さない）
//! - Mermaid は CDN（既定は `cdn.jsdelivr.net`）から読む。マニフェストの `capabilities.csp.script_domains` に
//!   そのホストを宣言してあり、有効にしている間、サーバーが CSP の `script-src` に足す。自分のサーバーに
//!   置いた Mermaid を使うときは、`script_url` に `/` で始まる URL を設定する
//! - 設定は `[plugins.settings.official-mermaid]`（Admin Studio の Settings › Plugins）で変える

use aegis_plugin_sdk::prelude::*;

#[derive(Default)]
pub struct MermaidPlugin;

/// Mermaid 本体（バージョンを固定する。上げるときは、図の見た目が変わることがある）
const DEFAULT_SCRIPT_URL: &str = "https://cdn.jsdelivr.net/npm/mermaid@10.9.3/dist/mermaid.min.js";

const THEMES: &[&str] = &["auto", "default", "dark", "neutral", "forest"];
const SECURITY_LEVELS: &[&str] = &["strict", "loose", "antiscript", "sandbox"];

/// 色は、背景に対して薄い透明度だけを使う（ページが明るくても暗くても馴染む）
const CSS: &str = ".omd-diagram{display:flex;justify-content:center;align-items:center;margin:1.75rem 0;padding:1.5rem;\
border:1px solid rgba(127,127,127,.3);border-radius:12px;background:rgba(127,127,127,.08);overflow-x:auto}\
.omd-diagram svg{max-width:100%;height:auto}.omd-diagram,.omd-diagram svg{letter-spacing:normal;word-spacing:normal;text-transform:none;text-indent:0;text-align:start;line-height:normal;white-space:normal}";

const SCRIPT: &str = r#"(function(){var C=__CONFIG__;
function lum(c){var m=c.match(/[\d.]+/g);
if(m&&m.length>=3&&!(m.length>=4&&parseFloat(m[3])===0)){return 0.299*m[0]+0.587*m[1]+0.114*m[2]}return null}
function dark(){var start=document.querySelector('pre code.language-mermaid, pre.mermaid, div.mermaid')||document.body;
var box=start.closest('pre')||start;start=box.parentElement||document.body;
var els=[],e=start;while(e){els.push(e);e=e.parentElement}
for(var i=0;i<els.length;i++){var l=lum(getComputedStyle(els[i]).backgroundColor);if(l!==null)return l<128}
var t=lum(getComputedStyle(start).color);if(t!==null)return t>=128;
return !!(window.matchMedia&&matchMedia('(prefers-color-scheme: dark)').matches)}
function theme(){return C.theme==='auto'?(dark()?'dark':'default'):C.theme}
function run(){if(!window.mermaid)return;
try{window.mermaid.initialize({startOnLoad:false,theme:theme(),securityLevel:C.security})}catch(e){return}
var nodes=document.querySelectorAll('pre code.language-mermaid, pre.mermaid, div.mermaid');
Array.prototype.forEach.call(nodes,function(el,i){var box=el.closest('pre')||el;
var def=(el.textContent||'').trim();if(!def||box.getAttribute('data-omd'))return;box.setAttribute('data-omd','1');
var id='omd-'+i+'-'+Math.floor(Math.random()*1e6);
var card=document.createElement('div');card.className='omd-diagram';card.style.cssText='position:absolute;visibility:hidden;left:0;right:0';box.parentNode.insertBefore(card,box);
window.mermaid.render(id,def,card).then(function(res){card.style.cssText='';card.innerHTML=res.svg;box.remove()}).catch(function(e){
card.remove();var stray=document.getElementById('d'+id);if(stray&&stray.parentNode)stray.parentNode.removeChild(stray);
console.warn('Mermaid rendering failed:',e)})})}
document.readyState==='loading'?document.addEventListener('DOMContentLoaded',run):run()})();"#;

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

/// スクリプトの URL。`http(s)://` か、`/` で始まる（`//` ではない）相対 URL で、空白・制御文字を含まないもの
fn setting_script_url() -> String {
    setting("script_url")
        .filter(|url| {
            let lower = url.to_ascii_lowercase();
            // ホストが通すのは、このサイトのパスと、マニフェストで宣言したホスト（`cdn.jsdelivr.net`）だけ
            let allowed =
                lower.starts_with("https://") || (url.starts_with('/') && !url.starts_with("//"));
            allowed && !url.chars().any(|c| c.is_control() || c.is_whitespace())
        })
        .unwrap_or_else(|| DEFAULT_SCRIPT_URL.to_string())
}

// --- 出力 ---

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

fn push_line(target: &mut String, addition: &str) {
    if !target.is_empty() {
        target.push('\n');
    }
    target.push_str(addition);
}

/// 本文に Mermaid の図（` ```mermaid ` のコードブロック、または `class="mermaid"` の要素）があるか
fn has_diagram(content_html: &str) -> bool {
    content_html.contains("language-mermaid") || content_html.contains("class=\"mermaid\"")
}

impl Plugin for MermaidPlugin {
    fn on_render_page(&mut self, mut ctx: PageRenderContext) -> Result<PageRenderContext> {
        if !has_diagram(&ctx.content_html) {
            return Ok(ctx);
        }
        let config = serde_json::json!({
            "theme": setting_choice("theme", THEMES, "auto"),
            "security": setting_choice("security_level", SECURITY_LEVELS, "strict"),
        });
        push_line(&mut ctx.extra_css, CSS);
        ctx.scripts
            .push(InjectedScript::external(setting_script_url()));
        ctx.scripts.push(InjectedScript::inline(
            SCRIPT.replace("__CONFIG__", &script_json(&config)),
        ));
        Ok(ctx)
    }
}

aegis_plugin_sdk::register_plugin!(MermaidPlugin);

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

    const DIAGRAM: &str = "<pre><code class=\"language-mermaid\">graph TD; A--&gt;B;</code></pre>";

    #[test]
    fn leaves_pages_without_a_diagram_alone() {
        mock::reset();
        let mut plugin = MermaidPlugin;
        for html in [
            "",
            "<p>No diagram</p>",
            "<pre><code class=\"language-rust\">fn main() {}</code></pre>",
        ] {
            let out = plugin.on_render_page(page(html)).unwrap();
            assert_eq!(out.content_html, html);
            assert!(
                out.extra_css.is_empty() && out.head_html().is_empty(),
                "{html:?}"
            );
        }
    }

    #[test]
    fn loads_mermaid_only_for_pages_with_a_diagram() {
        mock::reset();
        let out = MermaidPlugin.on_render_page(page(DIAGRAM)).unwrap();
        assert!(out.extra_css.contains(".omd-diagram"));
        assert!(out
            .head_html()
            .contains(&format!("<script src=\"{}\"></script>", DEFAULT_SCRIPT_URL)));
        assert!(out.head_html().contains("mermaid.render"));
        // 既定は auto と strict
        assert!(out
            .head_html()
            .contains(r#"{"security":"strict","theme":"auto"}"#));
        // 本文は変えない
        assert_eq!(out.content_html, DIAGRAM);
        // `class="mermaid"` の要素も図として扱う
        let by_class = MermaidPlugin
            .on_render_page(page("<div class=\"mermaid\">graph TD; A-->B;</div>"))
            .unwrap();
        assert!(by_class.head_html().contains("mermaid.render"));
    }

    #[test]
    fn settings_change_the_theme_the_security_level_and_the_script() {
        mock::reset();
        mock::set_config("theme", "Forest");
        mock::set_config("security_level", "sandbox");
        mock::set_config("script_url", "/assets/mermaid.min.js");
        let out = MermaidPlugin.on_render_page(page(DIAGRAM)).unwrap();
        assert!(out
            .head_html()
            .contains(r#"{"security":"sandbox","theme":"forest"}"#));
        assert!(out
            .head_html()
            .contains("<script src=\"/assets/mermaid.min.js\"></script>"));
        assert!(!out.head_html().contains("cdn.jsdelivr.net"));
        mock::reset();
    }

    #[test]
    fn unsafe_or_unknown_settings_fall_back_to_the_defaults() {
        mock::reset();
        mock::set_config("theme", "</script><script>alert(1)</script>");
        mock::set_config("security_level", "off");
        mock::set_config("script_url", "javascript:alert(1)");
        let out = MermaidPlugin.on_render_page(page(DIAGRAM)).unwrap();
        assert!(out
            .head_html()
            .contains(r#"{"security":"strict","theme":"auto"}"#));
        assert!(out.head_html().contains(DEFAULT_SCRIPT_URL));
        assert!(!out.head_html().contains("alert(1)"));

        for bad in [
            "//evil.example/m.js",
            "https://a.example/m.js\" onload=\"x()",
            "https://a.example/ m.js",
            "ftp://a.example/m.js",
        ] {
            mock::set_config("script_url", bad);
            let out = MermaidPlugin.on_render_page(page(DIAGRAM)).unwrap();
            assert!(out.head_html().contains(DEFAULT_SCRIPT_URL), "{bad}");
        }
        // 渡すのは URL そのもの（属性へのエスケープはホストがする。`https` は宣言したホストだけ通る）
        mock::set_config("script_url", "https://a.example/m.js?x=1&y=2");
        let out = MermaidPlugin.on_render_page(page(DIAGRAM)).unwrap();
        assert_eq!(
            out.scripts[0],
            InjectedScript::external("https://a.example/m.js?x=1&y=2")
        );
        mock::reset();
    }

    #[test]
    fn appends_to_what_other_plugins_added() {
        mock::reset();
        let mut ctx = page(DIAGRAM);
        ctx.extra_css = "a{}".to_string();
        ctx.extra_head = "<meta name=\"x\">".to_string();
        let out = MermaidPlugin.on_render_page(ctx).unwrap();
        assert!(out.extra_css.starts_with("a{}\n"));
        assert!(out.head_html().starts_with("<meta name=\"x\">\n"));
    }
}
