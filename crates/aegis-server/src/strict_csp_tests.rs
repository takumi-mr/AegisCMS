//! 公開ページの Strict CSP（`[security].strict_csp`）の結合テスト（ルーター経由）。docs/csp.md

use std::collections::HashMap;
use std::sync::Arc;

use aegis_ast::{BlockNode, StrictCsp};
use aegis_core::manifest::CspCapability;
use aegis_core::{InjectedScript, PluginCapabilities, PluginManifest};
use aegis_db::ContentRecord;
use aegis_runtime::{AegisPlugin, PluginSupervisor};
use axum::body::Body;
use axum::http::{HeaderMap, Request, StatusCode};
use tower::ServiceExt;

use crate::config::AegisConfig;
use crate::state::AppState;
use crate::test_support::test_app_state;

/// インラインと外部のスクリプトを、構造化した注入で足すプラグイン。名前空間（`aegis-`）が本文にある
/// コードを返し、ハッシュが「置き換えたあとの本文」で取られることを確かめる
struct ScriptPlugin {
    manifest: PluginManifest,
}

const INLINE: &str = "window.aegis_marker = 1; document.title = 'x';";

impl ScriptPlugin {
    fn new() -> Arc<Self> {
        Arc::new(Self {
            manifest: PluginManifest {
                name: "scripts".to_string(),
                version: "1.0.0".to_string(),
                entrypoint: "plugin.wasm".to_string(),
                capabilities: PluginCapabilities {
                    hooks: vec!["on-render-page".to_string()],
                    inline_scripts: true,
                    csp: Some(CspCapability {
                        script_domains: vec!["cdn.example.com".to_string()],
                        ..Default::default()
                    }),
                    ..Default::default()
                },
                ..Default::default()
            },
        })
    }
}

#[async_trait::async_trait]
impl AegisPlugin for ScriptPlugin {
    fn manifest(&self) -> &PluginManifest {
        &self.manifest
    }

    async fn on_render_page(
        &self,
        mut ctx: aegis_core::PageRenderContext,
    ) -> aegis_core::Result<aegis_core::PageRenderContext> {
        ctx.scripts = vec![
            InjectedScript::inline(INLINE),
            InjectedScript::inline("track();").with_consent("analytics"),
            InjectedScript::external("https://cdn.example.com/lib.js").async_load(),
        ];
        Ok(ctx)
    }
}

struct Site {
    app: axum::Router,
    state: Arc<AppState>,
}

async fn site(configure: impl FnOnce(&mut AegisConfig)) -> Site {
    let mut config = AegisConfig::default();
    configure(&mut config);
    let mut supervisor = PluginSupervisor::new();
    supervisor.register(ScriptPlugin::new());
    let state = Arc::new(test_app_state(config.clone(), supervisor, Vec::new()).await);
    let schema = state.schemas.read().await["article"].clone();
    state
        .db
        .insert_content(
            &schema,
            &ContentRecord {
                id: "a1".to_string(),
                content_type: "article".to_string(),
                title: "Hello".to_string(),
                slug: "hello".to_string(),
                status: "published".to_string(),
                blocks_json: serde_json::to_string(&[BlockNode::paragraph("Body")]).unwrap(),
                author_id: None,
                published_at: Some("2026-10-22T00:00:00Z".to_string()),
                unpublish_at: None,
                created_at: "2026-10-22T00:00:00Z".to_string(),
                updated_at: "2026-10-22T00:00:00Z".to_string(),
                custom_fields: HashMap::new(),
            },
        )
        .await
        .unwrap();
    Site {
        app: crate::app::router(&config, state.clone()),
        state,
    }
}

impl Site {
    async fn get(&self, path: &str) -> (StatusCode, HeaderMap, String) {
        let res = self
            .app
            .clone()
            .oneshot(Request::get(path).body(Body::empty()).unwrap())
            .await
            .unwrap();
        let (status, headers) = (res.status(), res.headers().clone());
        let body = axum::body::to_bytes(res.into_body(), usize::MAX)
            .await
            .unwrap();
        (status, headers, String::from_utf8_lossy(&body).to_string())
    }
}

/// CSP の `script-src` の値（無ければ空）
fn script_src(csp: &str) -> &str {
    csp.split(';')
        .map(str::trim)
        .find(|d| d.starts_with("script-src"))
        .unwrap_or_default()
}

fn header<'a>(headers: &'a HeaderMap, name: &str) -> &'a str {
    headers
        .get(name)
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default()
}

/// ページの `<script>` のうち、実行されるインラインのもの（`src` なし。データのブロックは除く）の本文。
/// `text/plain`（同意を待つもの）も数える: 同意のあと、バナーのスクリプトが同じ本文で実行する
fn executable_inline_scripts(html: &str) -> Vec<String> {
    let mut found = Vec::new();
    let mut rest = html;
    while let Some(start) = rest.find("<script") {
        let tag_end = start + rest[start..].find('>').unwrap() + 1;
        let tag = &rest[start..tag_end];
        let close = tag_end + rest[tag_end..].find("</script>").unwrap();
        let data = ["application/json", "application/ld+json"]
            .iter()
            .any(|t| tag.contains(&format!("type=\"{t}\"")));
        if !tag.contains(" src=") && !data {
            found.push(rest[tag_end..close].to_string());
        }
        rest = &rest[close + "</script>".len()..];
    }
    found
}

#[tokio::test]
async fn test_strict_csp_allows_exactly_the_inline_scripts_the_host_emitted() {
    let site = site(|c| c.security.strict_csp = StrictCsp::On).await;
    let (status, headers, html) = site.get("/articles/hello").await;
    assert_eq!(status, StatusCode::OK);
    let csp = header(&headers, "content-security-policy");

    // `unsafe-inline` は無い。プラグインが宣言したホストは残る。`strict-dynamic` は使わない
    let script_src = csp
        .split(';')
        .map(str::trim)
        .find(|d| d.starts_with("script-src "))
        .unwrap();
    assert!(
        !script_src.contains("unsafe-inline") && !script_src.contains("unsafe-eval"),
        "{csp}"
    );
    assert!(
        script_src.contains("'self'") && script_src.contains("https://cdn.example.com"),
        "{csp}"
    );
    assert!(!csp.contains("strict-dynamic"), "{csp}");
    assert!(header(&headers, "content-security-policy-report-only").is_empty());

    // ページの実行されるインラインのスクリプトは、どれもハッシュが CSP にある（ブラウザーが実行する本文で）
    let inline = executable_inline_scripts(&html);
    assert_eq!(inline.len(), 2, "{html}");
    for body in &inline {
        let hash = aegis_core::csp::script_hash(body);
        assert!(script_src.contains(&format!("'{hash}'")), "{body}: {csp}");
    }
    // 名前空間（`aegis_` → `cms_`）の置き換えのあとの本文で、ハッシュを取っている
    assert!(
        inline
            .iter()
            .any(|b| b.starts_with("window.cms_marker = 1;")),
        "{inline:?}"
    );
    // ハッシュは、記録したものだけ（余分に許可していない）
    assert_eq!(script_src.matches("'sha256-").count(), 2, "{csp}");
    // 外部のスクリプトは、ホストが作ったタグ（`'self'` 以外は宣言したホスト）
    assert!(
        html.contains(r#"<script src="https://cdn.example.com/lib.js" async></script>"#),
        "{html}"
    );
}

#[tokio::test]
async fn test_strict_csp_report_only_keeps_the_current_policy() {
    let site = site(|c| c.security.strict_csp = StrictCsp::ReportOnly).await;
    let (_, headers, html) = site.get("/articles/hello").await;
    // 今の CSP は、そのまま強制される
    let enforced = header(&headers, "content-security-policy");
    assert!(
        script_src(enforced).contains("'unsafe-inline'"),
        "{enforced}"
    );
    assert!(!enforced.contains("sha256-"), "{enforced}");
    // Strict の CSP は、報告だけ（ハッシュつき）
    let report_only = header(&headers, "content-security-policy-report-only");
    assert!(script_src(report_only).contains("'self'"), "{report_only}");
    assert!(
        !script_src(report_only).contains("'unsafe-inline'"),
        "{report_only}"
    );
    for body in executable_inline_scripts(&html) {
        assert!(
            report_only.contains(&aegis_core::csp::script_hash(&body)),
            "{body}"
        );
    }
}

#[tokio::test]
async fn test_strict_csp_off_keeps_unsafe_inline_and_never_overrides_an_admin_csp() {
    // `off` は今までどおり
    let site_off = site(|c| c.security.strict_csp = StrictCsp::Off).await;
    let (_, headers, _) = site_off.get("/articles/hello").await;
    let csp = header(&headers, "content-security-policy");
    assert!(script_src(csp).contains("'unsafe-inline'"), "{csp}");
    assert!(csp.contains("style-src 'self' 'unsafe-inline'"), "{csp}");
    assert!(!csp.contains("sha256-"));
    assert!(header(&headers, "content-security-policy-report-only").is_empty());

    // 管理者が CSP を上書きしているときは、その値が優先
    let site_on = site(|c| c.security.strict_csp = StrictCsp::On).await;
    site_on
        .state
        .headers
        .write()
        .unwrap()
        .content_security_policy =
        Some("default-src 'self'; script-src 'self' 'unsafe-inline'".to_string());
    let (_, headers, _) = site_on.get("/articles/hello").await;
    let csp = header(&headers, "content-security-policy");
    assert!(
        csp.contains("'unsafe-inline'") && !csp.contains("sha256-"),
        "{csp}"
    );
    assert!(header(&headers, "content-security-policy-report-only").is_empty());
}

#[tokio::test]
async fn test_admin_studio_keeps_its_own_strict_policy() {
    let site = site(|c| c.security.strict_csp = StrictCsp::On).await;
    let (_, headers, _) = site.get("/admin/").await;
    let csp = header(&headers, "content-security-policy");
    // 公開ページの Strict CSP とは別に、もともとスクリプトにもスタイルにも `unsafe-inline` が無い。ハッシュも足さない
    assert!(!script_src(csp).contains("unsafe-inline"), "{csp}");
    let style_src = csp
        .split(';')
        .map(str::trim)
        .find(|d| d.starts_with("style-src"))
        .unwrap();
    assert_eq!(style_src, "style-src 'self'", "{csp}");
    assert!(!csp.contains("sha256-"), "{csp}");
}

/// 何も設定しなければ Strict（`on`）。`[security].strict_csp = "off"` で、今までどおりに戻せる
#[tokio::test]
async fn test_strict_csp_is_on_by_default() {
    assert_eq!(AegisConfig::default().security.strict_csp, StrictCsp::On);
    let site = site(|_| {}).await;
    let (_, headers, html) = site.get("/articles/hello").await;
    let csp = header(&headers, "content-security-policy");
    assert!(!script_src(csp).contains("unsafe-inline"), "{csp}");
    for body in executable_inline_scripts(&html) {
        assert!(
            script_src(csp).contains(&aegis_core::csp::script_hash(&body)),
            "{body}: {csp}"
        );
    }
    // 設定ファイルの書き方
    let config: AegisConfig = toml::from_str("[security]\nstrict_csp = \"report-only\"").unwrap();
    assert_eq!(config.security.strict_csp, StrictCsp::ReportOnly);
    let config: AegisConfig = toml::from_str("[security]\nstrict_csp = \"off\"").unwrap();
    assert_eq!(config.security.strict_csp, StrictCsp::Off);
}

/// ページの `<style>` 要素の本文
fn style_elements(html: &str) -> Vec<String> {
    let mut found = Vec::new();
    let mut rest = html;
    while let Some(start) = rest.find("<style") {
        let tag_end = start + rest[start..].find('>').unwrap() + 1;
        let close = tag_end + rest[tag_end..].find("</style>").unwrap();
        found.push(rest[tag_end..close].to_string());
        rest = &rest[close + "</style>".len()..];
    }
    found
}

#[tokio::test]
async fn test_strict_csp_allows_exactly_the_style_elements_the_host_emitted() {
    let site = site(|c| c.security.strict_csp = StrictCsp::On).await;
    let (status, headers, html) = site.get("/articles/hello").await;
    assert_eq!(status, StatusCode::OK);
    let csp = header(&headers, "content-security-policy");
    let style_src = csp
        .split(';')
        .map(str::trim)
        .find(|d| d.starts_with("style-src "))
        .unwrap();

    // `unsafe-inline` は無い（`style=""` 属性も止まる）
    assert!(!style_src.contains("unsafe-inline"), "{csp}");
    assert!(!csp.contains("style-src-attr"), "{csp}");
    // ページに `style=` 属性は無い（組み込みのテンプレート・ブロックはクラスを使う）
    assert!(!html.contains(" style=\""), "{html}");

    // ページの `<style>` は、どれもハッシュが CSP にある（組み込みのテンプレート・ブロックのスタイル）
    let styles = style_elements(&html);
    assert!(styles.len() >= 2, "{html}");
    for body in &styles {
        let hash = aegis_core::csp::script_hash(body);
        assert!(
            style_src.contains(&format!("'{hash}'")),
            "{}: {csp}",
            &body[..body.len().min(60)]
        );
    }
    // 名前空間の置き換えのあとの本文で、ハッシュを取っている
    assert!(styles.iter().all(|b| !b.contains("aegis-")), "{styles:?}");
    // 記録したものだけ（余分に許可していない）
    assert_eq!(
        style_src.matches("'sha256-").count(),
        styles
            .iter()
            .map(|b| aegis_core::csp::script_hash(b))
            .collect::<std::collections::BTreeSet<_>>()
            .len(),
        "{csp}"
    );
}
