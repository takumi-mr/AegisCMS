//! 公式プラグイン `official-ads`: 広告配信プラグイン。
//!
//! - `on-render-page` によるヘッダー注入（Google AdSense スクリプトタグ・カスタムヘッダータグ）と自動挿入
//! - カスタムブロック `official-ads/display`: 本文中に AdSense ユニットまたはカスタムバナーを挿入
//! - 動的 CSP (Content Security Policy) 設定との連携（`capabilities.csp`）
//! - Cookie 同意バナー対応: 同意が得られるまで広告スクリプトの実行・iframe の読み込みを保留（`data-aegis-consent`）

use aegis_plugin_sdk::prelude::*;

#[derive(Default)]
pub struct AdsPlugin;

// --- 設定構造体 ---

#[derive(Debug, Clone)]
struct Settings {
    client_id: String,
    auto_ads: bool,
    auto_insert_position: &'static str,
    default_slot: String,
    require_consent: bool,
    consent_category: String,
    custom_head_code: String,
    custom_ad_html: String,
    content_types: Vec<String>,
}

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

fn setting_bool(key: &str, default: bool) -> bool {
    setting(key)
        .map(|v| matches!(v.to_ascii_lowercase().as_str(), "true" | "1" | "yes" | "on"))
        .unwrap_or(default)
}

fn load_settings() -> Settings {
    let client_id = setting("client_id").unwrap_or_default();
    let auto_ads = setting_bool("auto_ads", false);
    let auto_insert_position = setting_choice(
        "auto_insert_position",
        &["none", "top", "bottom", "both"],
        "none",
    );
    let default_slot = setting("default_slot").unwrap_or_default();
    let require_consent = setting_bool("require_consent", true);
    let consent_category = setting("consent_category").unwrap_or_else(|| "marketing".to_string());
    let custom_head_code = setting("custom_head_code").unwrap_or_default();
    let custom_ad_html = setting("custom_ad_html").unwrap_or_default();
    let content_types = setting("content_types")
        .unwrap_or_else(|| "article".to_string())
        .split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();

    Settings {
        client_id,
        auto_ads,
        auto_insert_position,
        default_slot,
        require_consent,
        consent_category,
        custom_head_code,
        custom_ad_html,
        content_types,
    }
}

fn truthy(val: &serde_json::Value) -> bool {
    match val {
        serde_json::Value::Bool(b) => *b,
        serde_json::Value::String(s) => {
            matches!(
                s.trim().to_ascii_lowercase().as_str(),
                "true" | "1" | "yes" | "on"
            )
        }
        serde_json::Value::Number(n) => n.as_i64().is_some_and(|i| i != 0),
        _ => false,
    }
}

fn escape_html(text: &str) -> String {
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

fn is_valid_client_id(client_id: &str) -> bool {
    let trimmed = client_id.trim();
    trimmed.starts_with("ca-pub-")
        && !trimmed["ca-pub-".len()..].is_empty()
        && trimmed["ca-pub-".len()..]
            .chars()
            .all(|c| c.is_ascii_digit())
}

/// Google AdSense のスクリプト（client_id 設定時）。URL のホストは、マニフェストの `script_domains` にある。
/// タグはホストが作る（同意が要る設定のときは、同意のあとに実行する形で出す）
fn adsense_script(settings: &Settings) -> Option<InjectedScript> {
    if settings.client_id.is_empty() || !is_valid_client_id(&settings.client_id) {
        return None;
    }
    let src = format!(
        "https://pagead2.googlesyndication.com/pagead/js/adsbygoogle.js?client={}",
        settings.client_id.trim()
    );
    let script = InjectedScript::external(src).async_load().crossorigin();
    Some(if settings.require_consent {
        script.with_consent(settings.consent_category.clone())
    } else {
        script
    })
}

/// `custom_head_code`（管理者が書く `<head>` に足す HTML）。`<script>` と `on*` 属性は、ホストが取り除く
/// （スクリプトは構造化した注入でだけ足せる。docs/csp.md）。`<meta>`・`<link>`・`<style>` は使える
fn custom_head_html(settings: &Settings) -> String {
    let mut html = settings.custom_head_code.clone();
    if !html.is_empty() && !html.ends_with('\n') {
        html.push('\n');
    }
    html
}

fn build_adsense_block_html(
    client_id: &str,
    slot_id: &str,
    format: &str,
    require_consent: bool,
    consent_category: &str,
) -> String {
    let mut html = String::new();
    let escaped_client = escape_html(client_id);
    let escaped_slot = escape_html(slot_id);
    let escaped_format = match format {
        "rectangle" => "rectangle",
        "horizontal" => "horizontal",
        "vertical" => "vertical",
        _ => "auto",
    };

    html.push_str(&format!(
        "<ins class=\"adsbygoogle\" data-ad-client=\"{}\" data-ad-slot=\"{}\" data-ad-format=\"{}\" data-full-width-responsive=\"true\"></ins>\n",
        escaped_client, escaped_slot, escaped_format
    ));

    if require_consent {
        let cat = escape_html(consent_category);
        html.push_str(&format!(
            "<script type=\"text/plain\" data-aegis-consent=\"{}\">(adsbygoogle = window.adsbygoogle || []).push({{}});\n</script>",
            cat
        ));
    } else {
        html.push_str("<script>(adsbygoogle = window.adsbygoogle || []).push({});\n</script>");
    }

    html
}

/// カスタム HTML バナー内のスクリプトや iframe に Cookie 同意属性を適用
fn apply_consent_to_custom_html(html: &str, consent_category: &str) -> String {
    let cat = escape_html(consent_category);
    let with_scripts = html
        .replace(
            "<script ",
            &format!(
                "<script type=\"text/plain\" data-aegis-consent=\"{}\" ",
                cat
            ),
        )
        .replace(
            "<script>",
            &format!(
                "<script type=\"text/plain\" data-aegis-consent=\"{}\">",
                cat
            ),
        );

    with_scripts
        .replace(
            "<iframe ",
            &format!("<iframe data-aegis-consent=\"{}\" ", cat),
        )
        .replace(" src=", " data-src=")
}

fn wrap_ad_unit(inner_html: &str, align: &str, is_ja: bool) -> String {
    let label = if is_ja { "広告" } else { "Advertisement" };
    let align_class = match align {
        "left" => "oa-align-left",
        "right" => "oa-align-right",
        _ => "oa-align-center",
    };

    format!(
        "<div class=\"oa-ad-container {}\">\n  <div class=\"oa-ad-label\">{}</div>\n  <div class=\"oa-ad-unit\">\n{}\n  </div>\n</div>\n",
        align_class, label, inner_html
    )
}

fn build_ad_unit_for_settings(settings: &Settings, is_ja: bool, align: &str) -> Option<String> {
    if !settings.client_id.is_empty()
        && !settings.default_slot.is_empty()
        && is_valid_client_id(&settings.client_id)
    {
        let inner = build_adsense_block_html(
            &settings.client_id,
            &settings.default_slot,
            "auto",
            settings.require_consent,
            &settings.consent_category,
        );
        Some(wrap_ad_unit(&inner, align, is_ja))
    } else if !settings.custom_ad_html.is_empty() {
        let content = if settings.require_consent {
            apply_consent_to_custom_html(&settings.custom_ad_html, &settings.consent_category)
        } else {
            settings.custom_ad_html.clone()
        };
        Some(wrap_ad_unit(&content, align, is_ja))
    } else {
        None
    }
}

fn css() -> &'static str {
    ":where(.oa-ad-container){margin:2rem 0;clear:both;text-align:center}\
:where(.oa-ad-label){font-size:0.75rem;color:rgba(127,127,127,0.7);margin-bottom:0.35rem;text-transform:uppercase;letter-spacing:0.05em;line-height:1}\
:where(.oa-ad-unit){margin:0 auto;overflow:hidden;max-width:100%}\
:where(.oa-align-center){text-align:center;margin-left:auto;margin-right:auto}\
:where(.oa-align-left){text-align:left;margin-left:0;margin-right:auto}\
:where(.oa-align-right){text-align:right;margin-left:auto;margin-right:0}\
.oa-ad-unit iframe,.oa-ad-unit ins{display:block;margin:0 auto}"
}

impl Plugin for AdsPlugin {
    fn on_render_page(&mut self, mut ctx: PageRenderContext) -> Result<PageRenderContext> {
        let settings = load_settings();

        // 広告表示対象のコンテンツ種別か検査
        if !settings.content_types.contains(&ctx.content_type)
            || ctx.fields.get("hide_ads").is_some_and(truthy)
        {
            return Ok(ctx);
        }

        let is_ja = ctx
            .theme_vars
            .get("locale")
            .map(|l| l.starts_with("ja"))
            .unwrap_or(true);

        // 1. ヘッダー注入 (<head>)
        let head_html = custom_head_html(&settings);
        if !head_html.is_empty() {
            if !ctx.extra_head.is_empty() && !ctx.extra_head.ends_with('\n') {
                ctx.extra_head.push('\n');
            }
            ctx.extra_head.push_str(&head_html);
        }
        if let Some(script) = adsense_script(&settings) {
            ctx.scripts.push(script);
        }

        // 2. CSS スタイルの注入
        if !ctx.extra_css.contains(".oa-ad-container") {
            if !ctx.extra_css.is_empty() && !ctx.extra_css.ends_with('\n') {
                ctx.extra_css.push('\n');
            }
            ctx.extra_css.push_str(css());
        }

        // 3. 自動挿入 (本文の先頭・末尾)
        if settings.auto_insert_position != "none" || settings.auto_ads {
            if let Some(unit_html) = build_ad_unit_for_settings(&settings, is_ja, "center") {
                match settings.auto_insert_position {
                    "top" => ctx.prepend_content.push_str(&unit_html),
                    "bottom" => ctx.append_content.push_str(&unit_html),
                    "both" => {
                        ctx.prepend_content.push_str(&unit_html);
                        ctx.append_content.push_str(&unit_html);
                    }
                    _ => {}
                }
            }
        }

        Ok(ctx)
    }

    fn render_blocks(&mut self, blocks: Vec<BlockNode>) -> Result<Vec<Option<String>>> {
        let settings = load_settings();
        let mut results = Vec::with_capacity(blocks.len());

        for block in blocks {
            let custom_html = block
                .attributes
                .get("custom_html")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .trim();
            let align = block
                .attributes
                .get("align")
                .and_then(|v| v.as_str())
                .unwrap_or("center")
                .trim();
            let slot = block
                .attributes
                .get("slot")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .trim();
            let format = block
                .attributes
                .get("format")
                .and_then(|v| v.as_str())
                .unwrap_or("auto")
                .trim();

            let block_html = if !custom_html.is_empty() {
                let inner = if settings.require_consent {
                    apply_consent_to_custom_html(custom_html, &settings.consent_category)
                } else {
                    custom_html.to_string()
                };
                Some(wrap_ad_unit(&inner, align, true))
            } else {
                let target_slot = if !slot.is_empty() {
                    slot
                } else {
                    &settings.default_slot
                };

                if !settings.client_id.is_empty()
                    && !target_slot.is_empty()
                    && is_valid_client_id(&settings.client_id)
                {
                    let inner = build_adsense_block_html(
                        &settings.client_id,
                        target_slot,
                        format,
                        settings.require_consent,
                        &settings.consent_category,
                    );
                    Some(wrap_ad_unit(&inner, align, true))
                } else if !settings.custom_ad_html.is_empty() {
                    let inner = if settings.require_consent {
                        apply_consent_to_custom_html(
                            &settings.custom_ad_html,
                            &settings.consent_category,
                        )
                    } else {
                        settings.custom_ad_html.clone()
                    };
                    Some(wrap_ad_unit(&inner, align, true))
                } else {
                    None
                }
            };

            results.push(block_html);
        }

        Ok(results)
    }
}

aegis_plugin_sdk::register_plugin!(AdsPlugin);

#[cfg(test)]
mod tests {
    use super::*;
    use aegis_plugin_sdk::host::mock;
    use std::collections::HashMap;

    fn page() -> PageRenderContext {
        PageRenderContext {
            content_type: "article".to_string(),
            content_html: "<p>Hello AegisCMS Article</p>".to_string(),
            title: "Great Post".to_string(),
            slug: "great-post".to_string(),
            ..Default::default()
        }
    }

    fn render(ctx: PageRenderContext) -> PageRenderContext {
        AdsPlugin.on_render_page(ctx).unwrap()
    }

    #[test]
    fn test_adsense_header_injection_with_consent() {
        mock::reset();
        mock::set_config("client_id", "ca-pub-1234567890123456");
        mock::set_config("require_consent", "true");
        mock::set_config("consent_category", "marketing");

        let out = render(page());
        assert!(out.head_html().contains("type=\"text/plain\""));
        assert!(out.head_html().contains("data-aegis-consent=\"marketing\""));
        assert!(out
            .head_html()
            .contains("https://pagead2.googlesyndication.com/pagead/js/adsbygoogle.js?client=ca-pub-1234567890123456"));
        assert!(out.extra_css.contains(".oa-ad-container"));
        mock::reset();
    }

    #[test]
    fn test_adsense_header_injection_without_consent() {
        mock::reset();
        mock::set_config("client_id", "ca-pub-9999999999999999");
        mock::set_config("require_consent", "false");

        let out = render(page());
        assert!(!out.head_html().contains("type=\"text/plain\""));
        assert!(!out.head_html().contains("data-aegis-consent"));
        // タグはホストが作る。プラグインは、URL と、async・crossorigin の頼みだけを返す
        assert_eq!(
            out.scripts,
            vec![InjectedScript::external(
                "https://pagead2.googlesyndication.com/pagead/js/adsbygoogle.js?client=ca-pub-9999999999999999"
            )
            .async_load()
            .crossorigin()]
        );
        mock::reset();
    }

    #[test]
    fn test_skip_ads_when_hidden() {
        mock::reset();
        mock::set_config("client_id", "ca-pub-1234567890123456");
        let mut p = page();
        p.fields
            .insert("hide_ads".to_string(), serde_json::json!(true));

        let out = render(p);
        assert!(out.head_html().is_empty());
        assert!(out.extra_css.is_empty());
        mock::reset();
    }

    #[test]
    fn test_auto_insert_bottom() {
        mock::reset();
        mock::set_config("client_id", "ca-pub-1234567890123456");
        mock::set_config("default_slot", "8888888888");
        mock::set_config("auto_insert_position", "bottom");

        let out = render(page());
        assert!(out
            .composed_html()
            .starts_with("<p>Hello AegisCMS Article</p>"));
        assert!(out
            .composed_html()
            .contains("<div class=\"oa-ad-container oa-align-center\">"));
        assert!(out.composed_html().contains("data-ad-slot=\"8888888888\""));
        mock::reset();
    }

    #[test]
    fn test_render_blocks_adsense_and_custom_html() {
        mock::reset();
        mock::set_config("client_id", "ca-pub-1234567890123456");
        mock::set_config("require_consent", "true");

        let mut b1_attrs = HashMap::new();
        b1_attrs.insert("slot".to_string(), serde_json::json!("1122334455"));
        b1_attrs.insert("format".to_string(), serde_json::json!("rectangle"));
        b1_attrs.insert("align".to_string(), serde_json::json!("center"));

        let mut b2_attrs = HashMap::new();
        b2_attrs.insert(
            "custom_html".to_string(),
            serde_json::json!("<iframe src=\"https://example.com/ad\"></iframe>"),
        );
        b2_attrs.insert("align".to_string(), serde_json::json!("left"));

        let blocks = vec![
            BlockNode {
                name: "official-ads/display".to_string(),
                text: None,
                attributes: b1_attrs,
                children: vec![],
            },
            BlockNode {
                name: "official-ads/display".to_string(),
                text: None,
                attributes: b2_attrs,
                children: vec![],
            },
        ];

        let results = AdsPlugin.render_blocks(blocks).unwrap();
        assert_eq!(results.len(), 2);

        let html1 = results[0].as_ref().unwrap();
        assert!(html1.contains("data-ad-slot=\"1122334455\""));
        assert!(html1.contains("data-ad-format=\"rectangle\""));
        assert!(html1.contains("data-aegis-consent=\"marketing\""));

        let html2 = results[1].as_ref().unwrap();
        assert!(html2.contains("<iframe data-aegis-consent=\"marketing\" data-src=\"https://example.com/ad\"></iframe>"));
        assert!(html2.contains("oa-align-left"));
        mock::reset();
    }
}
