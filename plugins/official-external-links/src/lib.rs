//! 公式プラグイン `official-external-links`: 本文の外部リンクの扱いをそろえる（`on-render-page`）。
//!
//! - スクリプトが本文の範囲（`selector`）の `http(s)` のリンクのうち、このサイトと `internal_domains` 以外のものに、
//!   新しいタブで開く（`target="_blank"`）、`rel="noopener nofollow"`、外部リンクのアイコンを付ける
//! - 本体の `[security]`（`enforce_external_link_rel` など）は、サーバーが HTML に直接 `rel="noopener noreferrer"` を付ける。
//!   このプラグインはそれに足す形で、`rel` は既存の値と合わせる
//! - ブラウザーで動くので、スクリプトを実行しない検索エンジンには `nofollow` は見えない
//! - 本文に `<a` が無いページには何も足さない。スクリプトが動かなくても、リンクは普通に開く

use aegis_plugin_sdk::prelude::*;

#[derive(Default)]
pub struct ExternalLinksPlugin;

const DEFAULT_SCOPE: &str = ".prose";

const SCRIPT: &str = "(function(){var C=__CONFIG__;\
function internal(host){host=host.toLowerCase();if(host===location.hostname.toLowerCase())return true;\
return C.internal.some(function(d){return host===d||host.slice(-d.length-1)==='.'+d})}\
function merge(a,tokens){var cur=(a.getAttribute('rel')||'').split(/\\s+/).filter(Boolean);\
tokens.forEach(function(t){if(cur.indexOf(t)<0)cur.push(t)});a.setAttribute('rel',cur.join(' '))}\
function init(){var seen=[];try{C.selector.split(',').forEach(function(s){s=s.trim();if(!s)return;\
Array.prototype.forEach.call(document.querySelectorAll(s+' a[href]'),function(a){if(seen.indexOf(a)<0)seen.push(a)})})}catch(e){return}\
seen.forEach(function(a){var url;try{url=new URL(a.getAttribute('href'),location.href)}catch(e){return}\
if(url.protocol!=='http:'&&url.protocol!=='https:')return;if(internal(url.hostname))return;\
var tokens=[];if(C.newTab&&!a.hasAttribute('target')){a.setAttribute('target','_blank')}\
if(a.getAttribute('target')==='_blank')tokens.push('noopener');if(C.nofollow)tokens.push('nofollow');\
if(tokens.length)merge(a,tokens);\
if(C.icon&&a.textContent.trim()){a.classList.add('oel-external')}\
if(a.getAttribute('target')==='_blank'&&a.textContent.trim()){var sr=document.createElement('span');sr.className='oel-sr';sr.textContent=C.t.newTab;a.appendChild(sr)}})}\
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

/// 本文の範囲を表す CSS セレクター。短い 1 行の文字だけ（スクリプトには JSON の文字列として渡す）
fn setting_selector(key: &str, default: &str) -> String {
    setting(key)
        .filter(|v| v.len() <= 200 && !v.chars().any(char::is_control))
        .unwrap_or_else(|| default.to_string())
}

/// ドメインの一覧（カンマ区切り）。ホスト名として使える文字だけ（先頭の `www.` や `.` は気にしない）
fn setting_domains(key: &str) -> Vec<String> {
    setting(key)
        .unwrap_or_default()
        .split(',')
        .map(|d| d.trim().trim_start_matches('.').to_ascii_lowercase())
        .filter(|d| {
            !d.is_empty()
                && d.len() <= 253
                && d.bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'.')
        })
        .collect()
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
    new_tab: bool,
    nofollow: bool,
    icon: bool,
    internal: Vec<String>,
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
            new_tab: setting_bool("new_tab", true),
            nofollow: setting_bool("nofollow", true),
            icon: setting_bool("icon", true),
            internal: setting_domains("internal_domains"),
            ja: setting("locale").as_deref() == Some("ja"),
        }
    }
}

// --- 出力 ---

fn css(s: &Settings) -> String {
    let mut css = String::from(
        // 画面には出さず、読み上げにだけ使う文
        ".oel-sr{position:absolute;width:1px;height:1px;margin:-1px;padding:0;overflow:hidden;clip:rect(0,0,0,0);white-space:nowrap;border:0}",
    );
    if s.icon {
        // アイコンは飾り（読み上げない）。`content: "…" / ""` を解さない古いブラウザーでは読み上げられてもよい
        css.push_str(
            "a.oel-external::after{content:\"\\2197\";content:\"\\2197\" / \"\";display:inline-block;margin-left:.15em;\
             font-size:.8em;line-height:1;text-decoration:none;opacity:.7}",
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

impl Plugin for ExternalLinksPlugin {
    fn on_render_page(&mut self, mut ctx: PageRenderContext) -> Result<PageRenderContext> {
        let settings = Settings::load();
        if !ctx.content_html.contains("<a ")
            || !settings.content_types.contains(&ctx.content_type)
            || !(settings.new_tab || settings.nofollow || settings.icon)
        {
            return Ok(ctx);
        }
        let new_tab_text = if settings.ja {
            "（新しいタブで開きます）"
        } else {
            " (opens in a new tab)"
        };
        let config = serde_json::json!({
            "selector": settings.selector,
            "newTab": settings.new_tab,
            "nofollow": settings.nofollow,
            "icon": settings.icon,
            "internal": settings.internal,
            "t": { "newTab": new_tab_text },
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

aegis_plugin_sdk::register_plugin!(ExternalLinksPlugin);

#[cfg(test)]
mod tests {
    use super::*;
    use aegis_plugin_sdk::host::mock;

    fn page() -> PageRenderContext {
        PageRenderContext {
            content_type: "article".to_string(),
            content_html: "<p><a href=\"https://example.org/\">Example</a></p>".to_string(),
            ..Default::default()
        }
    }

    fn render(ctx: PageRenderContext) -> PageRenderContext {
        ExternalLinksPlugin.on_render_page(ctx).unwrap()
    }

    #[test]
    fn adds_the_script_and_icon_to_pages_with_links() {
        mock::reset();
        let out = render(page());
        assert_eq!(out.content_html, page().content_html);
        assert!(out.extra_css.contains("a.oel-external::after"));
        assert!(out.extra_css.contains(".oel-sr{"));
        assert!(out.extra_head.starts_with("<script>"));
        assert!(out.extra_head.contains("\"newTab\":true"));
        assert!(out.extra_head.contains("\"nofollow\":true"));
        assert!(out.extra_head.contains("\"internal\":[]"));
        assert!(out.extra_head.contains("opens in a new tab"));
        assert!(!out.extra_head.contains("__CONFIG__"));
    }

    #[test]
    fn leaves_pages_without_links_and_other_types_alone() {
        mock::reset();
        let mut plain = page();
        plain.content_html = "<p>No link</p>".to_string();
        assert_eq!(render(plain.clone()), plain);
        let mut index = page();
        index.content_type = "index".to_string();
        assert_eq!(render(index.clone()), index);
        mock::reset();
    }

    #[test]
    fn each_option_can_be_turned_off() {
        mock::reset();
        mock::set_config("icon", "false");
        let out = render(page());
        assert!(!out.extra_css.contains("oel-external"));
        assert!(out.extra_head.contains("\"icon\":false"));
        mock::set_config("new_tab", "false");
        mock::set_config("nofollow", "false");
        // nothing left to do
        assert_eq!(render(page()), page());
        mock::reset();
    }

    #[test]
    fn internal_domains_are_cleaned() {
        mock::reset();
        mock::set_config(
            "internal_domains",
            " Example.com, .cdn.example.net ,bad domain,<x>,, ",
        );
        let out = render(page());
        assert!(out
            .extra_head
            .contains("\"internal\":[\"example.com\",\"cdn.example.net\"]"));
        mock::reset();
    }

    #[test]
    fn the_text_follows_the_locale() {
        mock::reset();
        mock::set_config("locale", "ja");
        assert!(render(page()).extra_head.contains("新しいタブで開きます"));
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
