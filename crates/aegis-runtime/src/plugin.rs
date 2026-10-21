use crate::guard::CapabilityGuard;
use aegis_ast::BlockNode;
use aegis_core::{PluginManifest, PostContext, Result};
use async_trait::async_trait;
use std::sync::Arc;

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
}
