use axum::{extract::State, http::StatusCode, Json};
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

    let transformed = render::transform_blocks(&state, payload.blocks).await;
    if let Some(e) = &transformed.plugin_error {
        // Previews run on every edit, so a failing hook is logged but not audited
        tracing::warn!(
            "Plugin hook 'transform-content-ast' failed in preview: {}",
            e
        );
    }
    let html = render::render_blocks_html(&state.config.security, &transformed.blocks);
    let mut warnings: Vec<String> = transformed
        .plugin_error
        .map(|e| format!("Plugin hook 'transform-content-ast' failed: {}", e))
        .into_iter()
        .collect();

    let page = render::PageInput {
        template: format!("{}.html", payload.content_type),
        content_type: payload.content_type,
        title: payload.title,
        slug: payload.slug,
        content_html: html.clone(),
        author: Some(session.user_id),
        published_at: None,
        updated_at: None,
        fields: payload.fields,
    };
    let page_html = match render::render_page(&state, &schema, page).await {
        Ok(page_html) => Some(page_html),
        Err(e) => {
            warnings.push(format!("Theme template rendering failed: {}", e));
            None
        }
    };

    Ok(Json(json!({
        "html": html,
        "page_html": page_html,
        "blocks": transformed.blocks,
        "warnings": warnings,
    })))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth::SessionData;
    use crate::test_support::test_state;
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
                        network: None,
                        storage: None,
                        hooks: vec![
                            "transform-content-ast".to_string(),
                            "on-render-page".to_string(),
                        ],
                    },
                    resource_limits: PluginResourceLimits::default(),
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
}
