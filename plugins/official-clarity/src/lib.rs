//! 公式プラグイン `official-clarity`: Microsoft Clarity（ヒートマップ・画面録画）を公開ページに足す。
//!
//! - `on-render-page` で `<head>` に Clarity の起動コードを入れる。
//! - Cookie の同意（`[consent]`）を待つ: 同意が要る形（`type="text/plain"` と `data-aegis-consent`）で出し、
//!   バナーのスクリプトが、同意のあとに実行する。同意を待たない設定も選べる。
//! - 録画に入れてはいけないページには付けない: 会員アカウント・カート・注文・問い合わせ・掲示板のページと、
//!   会員レベルを求める記事・ページ（全文が録画に入るため）。
//! - CSP の許可は `plugin.manifest.json` の `capabilities.csp` が宣言する（Clarity の公式の案内に合わせた個別のホスト）。
//!
//! 起動コードは Clarity が配る標準のもの。ただし、管理画面のプレビュー（`srcdoc` の iframe）や
//! `file:` で開いたときに記録しないよう、`http(s)` のページでだけ動かす。

use aegis_plugin_sdk::prelude::*;

#[derive(Default)]
pub struct ClarityPlugin;

/// 録画してはいけないページのテンプレート（会員・買い物・問い合わせ・掲示板。会員限定の掲示板のカテゴリもあるため）
const PRIVATE_TEMPLATES: &[&str] = &[
    "account.html",
    "cart.html",
    "order.html",
    "contact.html",
    "forum_index.html",
    "forum_category.html",
    "forum_thread.html",
];

fn setting(key: &str) -> Option<String> {
    config::get(key)
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
}

fn setting_bool(key: &str, default: bool) -> bool {
    setting(key)
        .map(|v| matches!(v.to_ascii_lowercase().as_str(), "true" | "1" | "yes" | "on"))
        .unwrap_or(default)
}

/// Clarity のプロジェクト ID（英数字 1〜32 文字）。HTML や JavaScript に差し込むので、これ以外は使わない
fn valid_project_id(id: &str) -> bool {
    (1..=32).contains(&id.len()) && id.bytes().all(|b| b.is_ascii_alphanumeric())
}

/// 同意の区分（小文字の英数字・`-`・`_` の 1〜32 文字。それ以外は既定の `analytics`）
fn consent_category(raw: Option<String>) -> String {
    raw.filter(|c| {
        (1..=32).contains(&c.len())
            && c.bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-' || b == b'_')
    })
    .unwrap_or_else(|| "analytics".to_string())
}

/// 会員レベルを求めるページか（`fields.access` が空・`public` 以外）
fn is_restricted(ctx: &PageRenderContext) -> bool {
    ctx.fields
        .get("access")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .is_some_and(|a| !a.is_empty() && a != "public")
}

/// Clarity の標準の起動コード。先頭の `if` は、`http(s)` 以外（プレビューの `srcdoc` など）では動かさない
fn snippet(project_id: &str) -> String {
    format!(
        "(function(c,l,a,r,i,t,y){{if(!/^https?:$/.test(l.location.protocol))return;\
         c[a]=c[a]||function(){{(c[a].q=c[a].q||[]).push(arguments)}};\
         t=l.createElement(r);t.async=1;t.src=\"https://www.clarity.ms/tag/\"+i;\
         y=l.getElementsByTagName(r)[0];y.parentNode.insertBefore(t,y);\
         }})(window,document,\"clarity\",\"script\",\"{project_id}\");"
    )
}

/// `<head>` に足すスクリプト。プロジェクト ID が無い・正しくないときは `None`。
/// タグはホストが作る（同意が要る設定のときは、同意のあとに実行する形で出す）
fn head_script(project_id: &str, consent: Option<&str>) -> Option<InjectedScript> {
    if !valid_project_id(project_id) {
        return None;
    }
    let script = InjectedScript::inline(snippet(project_id));
    Some(match consent {
        Some(category) => script.with_consent(category),
        None => script,
    })
}

impl Plugin for ClarityPlugin {
    fn on_render_page(&mut self, mut ctx: PageRenderContext) -> Result<PageRenderContext> {
        let Some(project_id) = setting("project_id") else {
            return Ok(ctx);
        };
        if PRIVATE_TEMPLATES.contains(&ctx.template.as_str())
            || (setting_bool("exclude_restricted", true) && is_restricted(&ctx))
        {
            return Ok(ctx);
        }
        let consent = setting_bool("require_consent", true)
            .then(|| consent_category(setting("consent_category")));
        if let Some(script) = head_script(&project_id, consent.as_deref()) {
            ctx.scripts.push(script);
        }
        Ok(ctx)
    }
}

aegis_plugin_sdk::register_plugin!(ClarityPlugin);

#[cfg(test)]
mod tests {
    use super::*;
    use aegis_plugin_sdk::host::mock;

    fn page(template: &str) -> PageRenderContext {
        PageRenderContext {
            template: template.to_string(),
            content_type: "article".to_string(),
            ..Default::default()
        }
    }

    fn render(ctx: PageRenderContext) -> PageRenderContext {
        ClarityPlugin.on_render_page(ctx).unwrap()
    }

    #[test]
    fn nothing_is_added_without_a_project_id_or_with_a_bad_one() {
        mock::reset();
        assert!(render(page("article.html")).head_html().is_empty());
        for bad in [
            "abc\"><script>x</script>",
            "a b",
            "id;alert(1)",
            &"a".repeat(33),
        ] {
            mock::set_config("project_id", bad);
            assert!(render(page("article.html")).head_html().is_empty(), "{bad}");
        }
        mock::reset();
    }

    #[test]
    fn the_tag_waits_for_consent_by_default() {
        mock::reset();
        mock::set_config("project_id", "abc123xyz9");
        let head = render(page("article.html")).head_html();
        assert!(head.contains("type=\"text/plain\""), "{head}");
        assert!(head.contains("data-aegis-consent=\"analytics\""), "{head}");
        assert!(head.contains("https://www.clarity.ms/tag/"), "{head}");
        assert!(head.contains("\"abc123xyz9\""), "{head}");
        // 標準の起動コードに、http(s) 以外では動かさない検査が付く
        assert!(head.contains("^https?:$"), "{head}");
        mock::reset();
    }

    #[test]
    fn consent_can_be_turned_off_and_the_category_changed() {
        mock::reset();
        mock::set_config("project_id", "abc123xyz9");
        mock::set_config("require_consent", "false");
        let head = render(page("article.html")).head_html();
        assert!(!head.contains("text/plain") && !head.contains("data-aegis-consent"));
        assert!(head.starts_with("<script>"), "{head}");

        mock::set_config("require_consent", "true");
        mock::set_config("consent_category", "marketing");
        assert!(render(page("article.html"))
            .head_html()
            .contains("data-aegis-consent=\"marketing\""));
        // 区分に使えない文字は、既定の analytics に戻す（属性を壊せない）
        mock::set_config("consent_category", "x\" onload=\"y");
        let head = render(page("article.html")).head_html();
        assert!(head.contains("data-aegis-consent=\"analytics\""), "{head}");
        assert!(!head.contains("onload"), "{head}");
        mock::reset();
    }

    #[test]
    fn private_pages_are_never_recorded() {
        mock::reset();
        mock::set_config("project_id", "abc123xyz9");
        for template in PRIVATE_TEMPLATES {
            assert!(render(page(template)).head_html().is_empty(), "{template}");
        }
        assert!(!render(page("page.html")).head_html().is_empty());
        mock::reset();
    }

    #[test]
    fn members_only_content_is_skipped_unless_asked_not_to() {
        mock::reset();
        mock::set_config("project_id", "abc123xyz9");
        let restricted = || {
            let mut ctx = page("article.html");
            ctx.fields
                .insert("access".to_string(), serde_json::json!("paid"));
            ctx
        };
        assert!(render(restricted()).head_html().is_empty());
        // 「だれでも」の値は制限ではない
        for open in ["", "public"] {
            let mut ctx = page("article.html");
            ctx.fields
                .insert("access".to_string(), serde_json::json!(open));
            assert!(!render(ctx).head_html().is_empty(), "{open:?}");
        }
        mock::set_config("exclude_restricted", "false");
        assert!(!render(restricted()).head_html().is_empty());
        mock::reset();
    }

    #[test]
    fn the_script_is_structured_and_the_other_head_content_is_kept() {
        mock::reset();
        mock::set_config("project_id", "abc123xyz9");
        let mut ctx = page("article.html");
        ctx.extra_head = "<meta name=\"x\" content=\"y\">".to_string();
        let out = render(ctx);
        // 手を付けない。足すのは、構造化したスクリプト 1 つだけ（タグはホストが作る）
        assert_eq!(out.extra_head, "<meta name=\"x\" content=\"y\">");
        assert_eq!(out.scripts.len(), 1);
        assert!(out.head_html().starts_with("<meta name=\"x\""));
        assert_eq!(
            out.scripts[0].consent.as_deref(),
            Some("analytics"),
            "{:?}",
            out.scripts
        );
        mock::reset();
    }
}
