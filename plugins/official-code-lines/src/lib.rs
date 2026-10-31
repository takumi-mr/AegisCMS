//! 公式プラグイン `official-code-lines`: コードブロックに行番号とファイル名を足す（`on-render-page`）。
//!
//! - 行番号: スクリプトが `<pre>` に行番号の列を足す（コードの文字は変えないので、コピーにも番号は入らない）。
//!   行数が `min_lines` 未満のブロックには付けない
//! - ファイル名: コードブロックの `filename` 属性（`<pre data-filename="...">`。Markdown では
//!   ` ```rust title="src/main.rs" `）を、ブロックの上の見出しとして出す
//! - 本文に `<pre` が無いページには何も足さない。スクリプトが動かなくてもコードはそのまま読める
//! - 見た目は設定（`[plugins.settings.official-code-lines]`）で変える。色は許可する形だけを CSS に入れる

use aegis_plugin_sdk::prelude::*;

#[derive(Default)]
pub struct CodeLinesPlugin;

const DEFAULT_SCOPE: &str = ".prose";

const SCRIPT: &str = "(function(){var C=__CONFIG__;\
function scopes(){return C.selector.split(',').map(function(s){return s.trim()}).filter(Boolean)}\
function init(){var seen=[];try{scopes().forEach(function(s){\
Array.prototype.forEach.call(document.querySelectorAll(s+' pre'),function(p){if(seen.indexOf(p)<0)seen.push(p)})})}catch(e){return}\
seen.forEach(function(pre){if(pre.getAttribute('data-ocl'))return;pre.setAttribute('data-ocl','1');\
var code=pre.querySelector('code');if(code&&/(^|\\s)language-mermaid(\\s|$)/.test(code.className))return;\
var text=(code||pre).textContent||'';var n=text.replace(/\\n$/,'').split('\\n').length;\
if(C.numbers&&n>=C.minLines){var rows=document.createElement('span');rows.className='ocl-rows';rows.setAttribute('aria-hidden','true');\
for(var i=0;i<n;i++)rows.appendChild(document.createElement('span'));pre.classList.add('ocl-numbered');pre.insertBefore(rows,pre.firstChild)}\
var name=pre.getAttribute('data-filename');\
if(C.filename&&name){var cap=document.createElement('div');cap.className='ocl-filename';cap.textContent=name;\
var box=pre.parentNode&&pre.parentNode.classList&&pre.parentNode.classList.contains('code-block-wrapper')?pre.parentNode:pre;\
box.parentNode.insertBefore(cap,box);pre.classList.add('ocl-titled')}})}\
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
    numbers: bool,
    filename: bool,
    min_lines: u32,
    number_color: String,
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
            numbers: setting_bool("line_numbers", true),
            filename: setting_bool("show_filename", true),
            min_lines: setting_int("min_lines", 2, 1, 1000),
            number_color: setting_color("number_color", "var(--text-muted,#8b949e)"),
        }
    }
}

// --- 出力 ---

fn css(s: &Settings) -> String {
    // 行番号の列は `<pre>` の左に絶対配置し、各行の高さは `<code>` と同じ（フォントと行の高さを引き継ぐ）。
    // 折り返すと行がずれるので、番号を付けたブロックは折り返さずに横へスクロールする
    format!(
        "pre.ocl-numbered{{position:relative;padding-left:3.6em;white-space:pre;overflow-x:auto;counter-reset:ocl}}\
         pre.ocl-numbered>.ocl-rows{{position:absolute;left:0;top:0;bottom:0;width:2.8em;padding:inherit;padding-left:0;padding-right:.4em;\
         box-sizing:border-box;text-align:right;color:{color};user-select:none;-webkit-user-select:none;pointer-events:none;\
         font:inherit;line-height:inherit}}\
         pre.ocl-numbered>.ocl-rows>span{{display:block}}\
         pre.ocl-numbered>.ocl-rows>span::before{{counter-increment:ocl;content:counter(ocl)}}\
         .ocl-filename{{margin:1em 0 0;padding:.35em .9em;font:600 .8rem/1.4 ui-monospace,SFMono-Regular,Menlo,Consolas,monospace;\
         background:rgba(127,127,127,.18);border:1px solid rgba(127,127,127,.3);border-bottom:0;border-radius:8px 8px 0 0;\
         overflow:hidden;text-overflow:ellipsis;white-space:nowrap}}\
         .ocl-filename+pre,.ocl-filename+.code-block-wrapper,.ocl-filename+.code-block-wrapper>pre{{margin-top:0;border-top-left-radius:0;border-top-right-radius:0}}",
        color = s.number_color,
    )
}

fn push_line(target: &mut String, addition: &str) {
    if !target.is_empty() {
        target.push('\n');
    }
    target.push_str(addition);
}

impl Plugin for CodeLinesPlugin {
    fn on_render_page(&mut self, mut ctx: PageRenderContext) -> Result<PageRenderContext> {
        let settings = Settings::load();
        if !ctx.content_html.contains("<pre")
            || !settings.content_types.contains(&ctx.content_type)
            || !(settings.numbers || settings.filename)
        {
            return Ok(ctx);
        }
        let config = serde_json::json!({
            "selector": settings.selector,
            "numbers": settings.numbers,
            "filename": settings.filename,
            "minLines": settings.min_lines,
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

aegis_plugin_sdk::register_plugin!(CodeLinesPlugin);

#[cfg(test)]
mod tests {
    use super::*;
    use aegis_plugin_sdk::host::mock;

    fn page() -> PageRenderContext {
        PageRenderContext {
            content_type: "article".to_string(),
            content_html: "<p>Hi</p><pre><code class=\"language-rust\">fn main() {}</code></pre>"
                .to_string(),
            ..Default::default()
        }
    }

    fn render(ctx: PageRenderContext) -> PageRenderContext {
        CodeLinesPlugin.on_render_page(ctx).unwrap()
    }

    #[test]
    fn adds_styles_and_script_to_pages_with_code() {
        mock::reset();
        let out = render(page());
        assert_eq!(out.content_html, page().content_html);
        assert!(out.extra_css.contains("pre.ocl-numbered{"));
        assert!(out.extra_css.contains("color:var(--text-muted,#8b949e)"));
        assert!(out.extra_head.starts_with("<script>"));
        assert!(out.extra_head.contains(
            "{\"filename\":true,\"minLines\":2,\"numbers\":true,\"selector\":\".prose\"}"
        ));
        assert!(!out.extra_head.contains("__CONFIG__"));
    }

    #[test]
    fn leaves_pages_without_code_and_other_types_alone() {
        mock::reset();
        let mut plain = page();
        plain.content_html = "<p>No code</p>".to_string();
        assert_eq!(render(plain.clone()), plain);
        let mut index = page();
        index.content_type = "index".to_string();
        assert_eq!(render(index.clone()), index);
        mock::set_config("content_types", "article, product");
        let mut product = page();
        product.content_type = "product".to_string();
        assert!(render(product).extra_css.contains("ocl-numbered"));
        mock::reset();
    }

    #[test]
    fn both_features_can_be_turned_off() {
        mock::reset();
        mock::set_config("line_numbers", "false");
        mock::set_config("show_filename", "false");
        assert_eq!(render(page()), page());
        mock::set_config("show_filename", "true");
        assert!(render(page()).extra_head.contains("\"numbers\":false"));
        mock::reset();
    }

    #[test]
    fn settings_are_checked() {
        mock::reset();
        mock::set_config("min_lines", "5000");
        mock::set_config("number_color", "red;} body{display:none");
        mock::set_config("selector", "main\u{7}.x");
        let out = render(page());
        assert!(out.extra_head.contains("\"minLines\":2"));
        assert!(out.extra_head.contains("\"selector\":\".prose\""));
        assert!(!out.extra_css.contains("display:none"));
        mock::reset();
        mock::set_config("min_lines", "10");
        mock::set_config("number_color", "#888");
        mock::set_config("selector", "article .body");
        let out = render(page());
        assert!(out.extra_head.contains("\"minLines\":10"));
        assert!(out.extra_head.contains("\"selector\":\"article .body\""));
        assert!(out.extra_css.contains("color:#888"));
        mock::reset();
    }

    #[test]
    fn the_selector_cannot_close_the_script() {
        mock::reset();
        mock::set_config("selector", "</script><script>alert(1)</script>");
        let out = render(page());
        assert!(!out.extra_head.contains("</script><script>"));
        assert!(out.extra_head.contains("\\u003c/script\\u003e"));
        assert_eq!(out.extra_head.matches("</script>").count(), 1);
        mock::reset();
    }
}
