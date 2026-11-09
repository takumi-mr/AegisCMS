use axum::{
    extract::{Path, State},
    http::{header, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use serde::Deserialize;
use serde_json::json;
use std::collections::HashMap;
use std::sync::Arc;

use crate::auth::authorize::RequireRole;
use crate::auth::ViewerSession;
use crate::render;
use crate::state::AppState;
use aegis_ast::BlockNode;

#[derive(Debug, Deserialize)]
pub struct PreviewRequest {
    pub blocks: Vec<BlockNode>,
    /// Schema whose theme template renders the page (`article` → `article.html`, `page` → `page.html`)
    #[serde(default = "default_content_type")]
    pub content_type: String,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub slug: String,
    /// Custom fields, e.g. `category_id` (relations are resolved like on the public page)
    #[serde(default)]
    pub fields: HashMap<String, serde_json::Value>,
    /// だれとして見るか: `staff`（既定。全文）・`guest`・`member:<レベル>`。`fields.access` のあるページで、
    /// その閲覧者に見える姿（本文の途中までと案内）を確かめる。本物の会員になりすますのではなく、種類だけを選ぶ
    #[serde(default)]
    pub viewer: Option<String>,
    /// テーマ適用ページを置く場所の名前（32 桁の小文字の 16 進。エディターが作って使い回す）。あれば、
    /// 描画したページをサーバーに置き、`page_url` で返す（`GET /api/v1/preview/frame/{key}`）
    #[serde(default)]
    pub frame_key: Option<String>,
}

/// テーマ適用ページを置いておく時間（秒）。エディターの 1 つのプレビューにつき 1 件だけを上書きする
const FRAME_TTL_SECS: u64 = 3600;

fn frame_cache_key(key: &str) -> String {
    format!("preview-frame:{key}")
}

/// 32 桁の小文字の 16 進か（推測できない名前。形を決めておき、キャッシュのキーを勝手に作らせない）
fn is_frame_key(key: &str) -> bool {
    key.len() == 32
        && key
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

fn default_content_type() -> String {
    "article".to_string()
}

type ApiError = (StatusCode, Json<serde_json::Value>);

/// `POST /api/v1/preview` — renders unsaved content exactly like a save followed by a public
/// page view. `html` is the body (built-in AST pipeline, the plugins' `transform-content-ast`
/// hook, `HtmlRenderer`, the `[security]` rewriter); `page_html` is the whole themed page
/// (plugins' `on-render-page` hook and the theme template). Nothing is stored and
/// `on-before-save` is not called.
pub async fn preview_content(
    State(state): State<Arc<AppState>>,
    RequireRole(session): ViewerSession,
    Json(payload): Json<PreviewRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let schema = state
        .schemas
        .read()
        .await
        .get(&payload.content_type)
        .cloned()
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                Json(json!({ "error": format!("Schema '{}' not found", payload.content_type) })),
            )
        })?;

    // だれとして見るか（公開ページと同じ判定 `access::decide` を通す）
    let levels = &state.config.members.levels;
    let viewer = match payload.viewer.as_deref() {
        None | Some("") | Some("staff") => aegis_members::Viewer::Staff,
        Some("guest") => aegis_members::Viewer::Guest,
        Some(other) => match other
            .strip_prefix("member:")
            .and_then(|level| levels.iter().find(|l| l.as_str() == level))
        {
            Some(level) => aegis_members::Viewer::Member { level },
            None => {
                return Err((
                    StatusCode::BAD_REQUEST,
                    Json(json!({
                        "error": "viewer must be staff, guest or member:<level>",
                        "code": "invalid_viewer"
                    })),
                ))
            }
        },
    };
    let public_path =
        crate::handlers::public::public_content_path(&state, &payload.content_type, &payload.slug);
    let gate = crate::handlers::access::decide(&state, &payload.fields, viewer, &public_path);
    if payload
        .frame_key
        .as_deref()
        .is_some_and(|k| !is_frame_key(k))
    {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(
                json!({ "error": "frame_key must be 32 lowercase hex digits", "code": "invalid_frame_key" }),
            ),
        ));
    }

    let transformed = render::transform_blocks(&state, payload.blocks).await;
    if let Some(e) = &transformed.plugin_error {
        // Previews run on every edit, so a failing hook is logged but not audited
        tracing::warn!(
            "Plugin hook 'transform-content-ast' failed in preview: {}",
            e
        );
    }
    // 見られない閲覧者には、本文の代わりに「見せてよい部分と案内」を出す（公開ページと同じ）
    let rendered = match &gate {
        crate::handlers::access::Gate::Denied { notice_html } => {
            let teaser =
                crate::handlers::access::shared_blocks(&payload.fields, transformed.blocks.clone());
            let mut rendered = render::render_content(&state, &teaser).await;
            rendered.html.push_str(notice_html);
            rendered
        }
        _ => render::render_content(&state, &transformed.blocks).await,
    };
    let html = rendered.html;
    let mut warnings: Vec<String> = transformed
        .plugin_error
        .map(|e| format!("Plugin hook 'transform-content-ast' failed: {}", e))
        .into_iter()
        .chain(
            rendered
                .warnings
                .into_iter()
                .map(|e| format!("Plugin hook 'render-blocks' failed: {}", e)),
        )
        .collect();

    let template = match payload.fields.get("layout").and_then(|v| v.as_str()) {
        Some("blank") => "blank.html".to_string(),
        Some(custom) if !custom.trim().is_empty() && custom != "default" => {
            let c = custom.trim();
            if c.ends_with(".html") {
                c.to_string()
            } else {
                format!("{}.html", c)
            }
        }
        _ => format!("{}.html", payload.content_type),
    };

    let page = render::PageInput {
        template,
        content_type: payload.content_type,
        title: payload.title,
        slug: payload.slug,
        path: None,
        content_html: html.clone(),
        author: Some(session.user_id),
        published_at: None,
        updated_at: None,
        fields: payload.fields,
        comments: None,
        related_articles: Vec::new(),
        prev_article: None,
        next_article: None,
    };
    let page_html = match render::render_page(&state, &schema, page).await {
        Ok(page_html) => Some(page_html),
        Err(e) => {
            warnings.push(format!("Theme template rendering failed: {}", e));
            None
        }
    };

    // ページを置き、読み込み先を返す。iframe を `srcdoc` にすると、Admin Studio の CSP（スクリプトは
    // 自分のオリジンだけ）を引き継ぎ、テーマ・プラグインのインラインのスクリプトが動かない。別の URL
    // （公開ページと同じ CSP）から読み込めば、公開ページと同じに動く
    let mut page_url = serde_json::Value::Null;
    if let (Some(key), Some(page)) = (payload.frame_key.as_deref(), page_html.as_deref()) {
        let stored = state
            .cache
            .set(&frame_cache_key(key), page, Some(FRAME_TTL_SECS))
            .await;
        match stored {
            // `v` は、同じ URL でも読み直させるための値
            Ok(()) => {
                page_url = json!(format!(
                    "/preview/frame/{key}?v={}",
                    chrono::Utc::now().timestamp_millis()
                ))
            }
            Err(e) => warnings.push(format!("The themed page could not be stored: {e}")),
        }
    }

    Ok(Json(json!({
        "html": html,
        "page_html": page_html,
        "page_url": page_url,
        "blocks": transformed.blocks,
        "warnings": warnings,
    })))
}

/// `GET /api/v1/preview/frame/{key}` — `POST /api/v1/preview` が置いたテーマ適用ページ。iframe と別のウィンドウが
/// 読み込む。名前（128 ビットの乱数）を知る人だけが読める。ページの中のスクリプトが管理画面のセッション
/// （`sessionStorage`）に触れないよう、`Content-Security-Policy: sandbox allow-scripts`（不透明なオリジン）で返す
/// （`app::security_headers_middleware` が付ける）
pub async fn preview_frame(
    State(state): State<Arc<AppState>>,
    Path(key): Path<String>,
) -> Response {
    let page = if is_frame_key(&key) {
        state.cache.get(&frame_cache_key(&key)).await.ok().flatten()
    } else {
        None
    };
    match page {
        Some(html) => (
            [
                (
                    header::CONTENT_TYPE,
                    HeaderValue::from_static("text/html; charset=utf-8"),
                ),
                (header::CACHE_CONTROL, HeaderValue::from_static("no-store")),
                (
                    header::HeaderName::from_static("x-robots-tag"),
                    HeaderValue::from_static("noindex"),
                ),
            ],
            html,
        )
            .into_response(),
        None => (StatusCode::NOT_FOUND, "Preview not found or expired").into_response(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::test_state;
    use aegis_auth::SessionData;
    use aegis_core::{AegisError, PluginCapabilities, PluginManifest, PluginResourceLimits};
    use aegis_runtime::{AegisPlugin, PluginSupervisor};

    /// Appends a paragraph, or fails when `fail` is set
    struct FooterPlugin {
        manifest: PluginManifest,
        fail: bool,
    }

    impl FooterPlugin {
        fn new(fail: bool) -> Arc<Self> {
            Arc::new(Self {
                manifest: PluginManifest {
                    name: "footer".to_string(),
                    version: "1.0.0".to_string(),
                    entrypoint: "plugin.wasm".to_string(),
                    capabilities: PluginCapabilities {
                        csp: None,
                        render_page: None,
                        embed: None,
                        network: None,
                        storage: None,
                        hooks: vec![
                            "transform-content-ast".to_string(),
                            "on-render-page".to_string(),
                        ],
                    },
                    resource_limits: PluginResourceLimits::default(),
                    ..Default::default()
                },
                fail,
            })
        }
    }

    #[async_trait::async_trait]
    impl AegisPlugin for FooterPlugin {
        fn manifest(&self) -> &PluginManifest {
            &self.manifest
        }

        async fn transform_content_ast(
            &self,
            mut blocks: Vec<BlockNode>,
        ) -> aegis_core::Result<Vec<BlockNode>> {
            if self.fail {
                return Err(AegisError::PluginRuntime("trap".to_string()));
            }
            let mut footer = BlockNode::new("core/paragraph");
            footer.text = Some("Added by plugin".to_string());
            blocks.push(footer);
            Ok(blocks)
        }

        async fn on_render_page(
            &self,
            mut ctx: aegis_core::PageRenderContext,
        ) -> aegis_core::Result<aegis_core::PageRenderContext> {
            ctx.extra_css.push_str(".preview-theme-marker{}");
            Ok(ctx)
        }
    }

    fn viewer() -> RequireRole<{ crate::auth::authorize::VIEWER }> {
        RequireRole(SessionData {
            id: "ses_test".to_string(),
            user_id: "viewer1".to_string(),
            role: "viewer".to_string(),
            created_at: String::new(),
            expires_at: String::new(),
            metadata: Default::default(),
        })
    }

    fn request() -> PreviewRequest {
        serde_json::from_value(json!({
            "title": "Preview Title",
            "slug": "preview-title",
            "blocks": [
                { "name": "core/heading", "attributes": { "level": 2 }, "text": "Hello World" },
                { "name": "core/image", "attributes": { "src": "/uploads/a.png", "alt": "A" } }
            ]
        }))
        .unwrap()
    }

    #[tokio::test]
    async fn test_preview_matches_the_saved_html_pipeline() {
        let mut supervisor = PluginSupervisor::new();
        supervisor.register(FooterPlugin::new(false));
        let state = test_state(supervisor).await;

        let Json(body) = preview_content(State(state.clone()), viewer(), Json(request()))
            .await
            .unwrap();
        let html = body["html"].as_str().unwrap();
        assert!(html.contains("id=\"hello-world\""), "heading id: {}", html);
        assert!(html.contains("loading=\"lazy\""), "rewriter: {}", html);
        assert!(html.contains("Added by plugin"), "plugin hook: {}", html);
        assert_eq!(body["warnings"], json!([]));

        // Full themed page: template, title, body and the on-render-page hook
        let page_html = body["page_html"].as_str().unwrap();
        assert!(
            page_html.contains("<title>Preview Title |"),
            "{}",
            page_html
        );
        assert!(page_html.contains(html), "page embeds the body HTML");
        assert!(
            page_html.contains(".preview-theme-marker{}"),
            "on-render-page hook"
        );

        // The same HTML a save + public view produces
        let transformed = render::transform_blocks(&state, request().blocks).await;
        assert_eq!(
            html,
            render::render_blocks_html(&state.config.security, &transformed.blocks)
        );
    }

    #[tokio::test]
    async fn test_preview_reports_a_failing_plugin_hook() {
        let mut supervisor = PluginSupervisor::new();
        supervisor.register(FooterPlugin::new(true));
        let state = test_state(supervisor).await;

        let Json(body) = preview_content(State(state), viewer(), Json(request()))
            .await
            .unwrap();
        let html = body["html"].as_str().unwrap();
        assert!(html.contains("id=\"hello-world\""), "{}", html);
        assert!(!html.contains("Added by plugin"));
        assert_eq!(body["warnings"].as_array().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn test_preview_rejects_an_unknown_content_type() {
        let state = test_state(PluginSupervisor::new()).await;
        let mut req = request();
        req.content_type = "missing".to_string();
        let err = preview_content(State(state), viewer(), Json(req))
            .await
            .unwrap_err();
        assert_eq!(err.0, StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn test_preview_supports_blank_layout() {
        let state = test_state(PluginSupervisor::new()).await;
        let mut req = request();
        req.content_type = "page".to_string();
        req.fields
            .insert("layout".to_string(), serde_json::json!("blank"));

        let Json(body) = preview_content(State(state), viewer(), Json(req))
            .await
            .expect("Blank preview must succeed");
        let page_html = body["page_html"].as_str().expect("page_html should exist");
        assert!(page_html.contains("<title>Preview Title |"));
        assert!(page_html.contains("id=\"hello-world\""));
        // ブランクレイアウトなので site-header や site-footer を含まない
        assert!(!page_html.contains("site-header"));
        assert!(!page_html.contains("site-footer"));
    }

    /// 閲覧者を選んで見る: 区切りより前と案内（ゲスト・足りない会員）、全文（読める会員・スタッフ）
    #[tokio::test]
    async fn test_preview_as_a_viewer_applies_the_members_only_access() {
        let state = test_state(PluginSupervisor::new()).await;
        let request_as = |who: Option<&str>| {
            let mut req: PreviewRequest = serde_json::from_value(json!({
                "title": "Members article",
                "slug": "members-article",
                "fields": { "access": "paid" },
                "viewer": who,
                "blocks": [
                    { "name": "core/paragraph", "text": "Free teaser zxqteaser." },
                    { "name": "core/paywall" },
                    { "name": "core/paragraph", "text": "Paid tail zxqtail." }
                ]
            }))
            .unwrap();
            req.viewer = who.map(str::to_string);
            req
        };
        let preview = |who: Option<&'static str>| {
            let state = state.clone();
            let req = request_as(who);
            async move {
                preview_content(State(state), viewer(), Json(req))
                    .await
                    .map(|Json(body)| body)
            }
        };

        // 既定（スタッフ）は全文
        for viewer in [None, Some("staff")] {
            let body = preview(viewer).await.unwrap();
            let html = body["html"].as_str().unwrap();
            assert!(
                html.contains("zxqteaser") && html.contains("zxqtail"),
                "{html}"
            );
            assert!(!html.contains("cms-access-notice"));
        }
        // ゲストと足りない会員は、区切りの前と案内だけ。ページ全体も同じ
        for (viewer, notice) in [
            (Some("guest"), "This content is for members"),
            (Some("member:free"), "paid"),
        ] {
            let body = preview(viewer).await.unwrap();
            for key in ["html", "page_html"] {
                let html = body[key].as_str().unwrap();
                assert!(html.contains("zxqteaser"), "{viewer:?} {key}");
                assert!(!html.contains("zxqtail"), "{viewer:?} {key}: {html}");
                assert!(html.contains("aegis-access-notice"), "{viewer:?} {key}");
            }
            assert!(
                body["html"].as_str().unwrap().contains(notice),
                "{viewer:?}"
            );
        }
        // 読める会員は全文
        let body = preview(Some("member:paid")).await.unwrap();
        assert!(body["html"].as_str().unwrap().contains("zxqtail"));
        // 知らない閲覧者・設定にないレベルは 400
        for bad in ["admin", "member:gold", "member:", "Guest"] {
            let err = preview(Some(bad)).await.unwrap_err();
            assert_eq!(err.0, StatusCode::BAD_REQUEST, "{bad}");
        }
    }

    const KEY: &str = "0123456789abcdef0123456789abcdef";

    /// `frame_key` があれば、テーマ適用ページを置いて、読み込み先（`page_url`）を返す。同じ名前は上書きする
    #[tokio::test]
    async fn test_preview_keeps_the_themed_page_for_the_iframe() {
        let state = test_state(PluginSupervisor::new()).await;
        let mut req = request();
        req.frame_key = Some(KEY.to_string());
        let Json(body) = preview_content(State(state.clone()), viewer(), Json(req))
            .await
            .unwrap();
        let url = body["page_url"].as_str().unwrap();
        assert!(
            url.starts_with(&format!("/preview/frame/{KEY}?v=")),
            "{url}"
        );

        let stored = state
            .cache
            .get(&frame_cache_key(KEY))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(stored, body["page_html"].as_str().unwrap());
        assert_eq!(
            preview_frame(State(state.clone()), Path(KEY.to_string()))
                .await
                .status(),
            StatusCode::OK
        );

        // 名前が無いときは置かない。形の違う名前は 400、置いていない名前・形の違う名前の読み出しは 404
        let Json(none) = preview_content(State(state.clone()), viewer(), Json(request()))
            .await
            .unwrap();
        assert!(none["page_url"].is_null());
        for bad in [
            "short",
            "0123456789ABCDEF0123456789ABCDEF",
            "../../etc/passwd",
        ] {
            let mut req = request();
            req.frame_key = Some(bad.to_string());
            let err = preview_content(State(state.clone()), viewer(), Json(req))
                .await
                .unwrap_err();
            assert_eq!(err.0, StatusCode::BAD_REQUEST, "{bad}");
            assert_eq!(
                preview_frame(State(state.clone()), Path(bad.to_string()))
                    .await
                    .status(),
                StatusCode::NOT_FOUND
            );
        }
        assert_eq!(
            preview_frame(State(state), Path("f".repeat(32)))
                .await
                .status(),
            StatusCode::NOT_FOUND
        );
    }

    /// 読み込むページは、公開ページと同じ CSP に `sandbox allow-scripts` を足して返す（不透明なオリジン。
    /// 管理画面のセッションに触れない）。どのオリジンの iframe にも入れられる
    #[tokio::test]
    async fn test_the_preview_frame_is_sandboxed_by_its_csp() {
        use axum::body::Body;
        use axum::http::Request;
        use tower::ServiceExt;

        let state = test_state(PluginSupervisor::new()).await;
        state
            .cache
            .set(&frame_cache_key(KEY), "<p>page</p>", Some(60))
            .await
            .unwrap();
        let app = crate::app::router(&state.config, state.clone());
        let res = app
            .clone()
            .oneshot(
                Request::get(format!("/api/v1/preview/frame/{KEY}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let csp = res.headers()["content-security-policy"].to_str().unwrap();
        assert!(csp.contains("sandbox allow-scripts"), "{csp}");
        // 公開ページと同じ（インラインのスクリプトが動く）。管理画面の CSP ではない
        assert!(csp.contains("script-src 'self' 'unsafe-inline'"), "{csp}");
        assert!(csp.contains("frame-ancestors *"), "{csp}");
        assert!(!res.headers().contains_key("x-frame-options"));
        assert_eq!(res.headers()["cache-control"], "no-store");

        // 管理画面（/admin/）は、スクリプトを自分のオリジンだけに限る
        let admin = app
            .oneshot(Request::get("/admin/").body(Body::empty()).unwrap())
            .await
            .unwrap();
        let csp = admin.headers()["content-security-policy"].to_str().unwrap();
        assert!(csp.contains("script-src 'self';"), "{csp}");
        assert!(!csp.contains("script-src 'self' 'unsafe-inline'"), "{csp}");
    }
}
