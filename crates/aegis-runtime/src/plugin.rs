use crate::guard::CapabilityGuard;
use aegis_ast::BlockNode;
use aegis_core::{PluginManifest, PostContext, Result, UserAuthContext};
use async_trait::async_trait;
use std::sync::Arc;

use std::collections::HashMap;

#[async_trait]
pub trait AegisPlugin: Send + Sync {
    fn manifest(&self) -> &PluginManifest;

    async fn on_before_save(&self, post: PostContext) -> Result<PostContext> {
        let guard = CapabilityGuard::new(self.manifest());
        guard.check_hook("on-before-save")?;
        Ok(post)
    }

    async fn transform_content_ast(&self, blocks: Vec<BlockNode>) -> Result<Vec<BlockNode>> {
        let guard = CapabilityGuard::new(self.manifest());
        guard.check_hook("transform-content-ast")?;
        Ok(blocks)
    }

    async fn on_render_page(
        &self,
        ctx: aegis_core::PageRenderContext,
    ) -> Result<aegis_core::PageRenderContext> {
        let guard = CapabilityGuard::new(self.manifest());
        guard.check_hook("on-render-page")?;
        Ok(ctx)
    }

    async fn on_user_authenticated(&self, ctx: UserAuthContext) -> Result<UserAuthContext> {
        let guard = CapabilityGuard::new(self.manifest());
        guard.check_hook("on-user-authenticated")?;
        Ok(ctx)
    }

    fn provide_templates(&self) -> HashMap<String, String> {
        HashMap::new()
    }
}

pub struct PluginSupervisor {
    plugins: Vec<Arc<dyn AegisPlugin>>,
}

impl Default for PluginSupervisor {
    fn default() -> Self {
        Self::new()
    }
}

impl PluginSupervisor {
    pub fn new() -> Self {
        Self {
            plugins: Vec::new(),
        }
    }

    pub fn register(&mut self, plugin: Arc<dyn AegisPlugin>) {
        tracing::info!("Registered plugin: {}", plugin.manifest().name);
        self.plugins.push(plugin);
    }

    /// 登録済みのプラグイン（フックの実行順）
    pub fn plugins(&self) -> &[Arc<dyn AegisPlugin>] {
        &self.plugins
    }

    pub async fn apply_before_save(&self, mut post: PostContext) -> Result<PostContext> {
        for plugin in &self.plugins {
            if plugin
                .manifest()
                .capabilities
                .hooks
                .iter()
                .any(|h| h == "on-before-save")
            {
                post = plugin.on_before_save(post).await?;
            }
        }
        Ok(post)
    }

    pub async fn apply_ast_transforms(&self, mut blocks: Vec<BlockNode>) -> Result<Vec<BlockNode>> {
        for plugin in &self.plugins {
            if plugin
                .manifest()
                .capabilities
                .hooks
                .iter()
                .any(|h| h == "transform-content-ast")
            {
                blocks = plugin.transform_content_ast(blocks).await?;
            }
        }
        Ok(blocks)
    }

    pub async fn apply_render_page(
        &self,
        mut ctx: aegis_core::PageRenderContext,
    ) -> Result<aegis_core::PageRenderContext> {
        for plugin in &self.plugins {
            if plugin
                .manifest()
                .capabilities
                .hooks
                .iter()
                .any(|h| h == "on-render-page")
            {
                ctx = plugin.on_render_page(ctx).await?;
            }
        }
        Ok(ctx)
    }

    /// Runs every `on-user-authenticated` plugin in turn after a successful login. An error
    /// (a rejection or a failure) stops the chain; the login must then be refused
    pub async fn apply_user_authenticated(
        &self,
        mut ctx: UserAuthContext,
    ) -> Result<UserAuthContext> {
        for plugin in &self.plugins {
            if plugin
                .manifest()
                .capabilities
                .hooks
                .iter()
                .any(|h| h == "on-user-authenticated")
            {
                ctx = plugin.on_user_authenticated(ctx).await?;
            }
        }
        Ok(ctx)
    }

    pub fn collect_plugin_templates(&self) -> HashMap<String, String> {
        let mut all_templates = HashMap::new();
        for plugin in &self.plugins {
            let tpls = plugin.provide_templates();
            for (k, v) in tpls {
                all_templates.insert(k, v);
            }
        }
        all_templates
    }
}
