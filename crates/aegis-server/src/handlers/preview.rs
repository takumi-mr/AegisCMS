use axum::{extract::State, Json};
use serde::Deserialize;
use serde_json::json;
use std::sync::Arc;

use crate::auth::ViewerSession;
use crate::render;
use crate::state::AppState;
use aegis_ast::BlockNode;

#[derive(Debug, Deserialize)]
pub struct PreviewRequest {
    pub blocks: Vec<BlockNode>,
}

/// `POST /api/v1/preview` — renders unsaved blocks exactly like a save followed by a public
/// page view: built-in AST pipeline, the plugins' `transform-content-ast` hook, `HtmlRenderer`
/// and the `[security]` rewriter. Nothing is stored and `on-before-save` is not called.
pub async fn preview_content(
    State(state): State<Arc<AppState>>,
    _session: ViewerSession,
    Json(payload): Json<PreviewRequest>,
) -> Json<serde_json::Value> {
    let transformed = render::transform_blocks(&state, payload.blocks).await;
    if let Some(e) = &transformed.plugin_error {
        // Previews run on every edit, so a failing hook is logged but not audited
        tracing::warn!(
            "Plugin hook 'transform-content-ast' failed in preview: {}",
            e
        );
    }
    let html = render::render_blocks_html(&state.config.security, &transformed.blocks);
    let warnings: Vec<String> = transformed
        .plugin_error
        .map(|e| format!("Plugin hook 'transform-content-ast' failed: {}", e))
        .into_iter()
        .collect();

    Json(json!({
        "html": html,
        "blocks": transformed.blocks,
        "warnings": warnings,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth::authorize::RequireRole;
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
                        hooks: vec!["transform-content-ast".to_string()],
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

        let Json(body) = preview_content(State(state.clone()), viewer(), Json(request())).await;
        let html = body["html"].as_str().unwrap();
        assert!(html.contains("id=\"hello-world\""), "heading id: {}", html);
        assert!(html.contains("loading=\"lazy\""), "rewriter: {}", html);
        assert!(html.contains("Added by plugin"), "plugin hook: {}", html);
        assert_eq!(body["warnings"], json!([]));

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

        let Json(body) = preview_content(State(state), viewer(), Json(request())).await;
        let html = body["html"].as_str().unwrap();
        assert!(html.contains("id=\"hello-world\""), "{}", html);
        assert!(!html.contains("Added by plugin"));
        assert_eq!(body["warnings"].as_array().unwrap().len(), 1);
    }
}
