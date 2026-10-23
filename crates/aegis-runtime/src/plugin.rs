use crate::guard::CapabilityGuard;
use aegis_ast::BlockNode;
use aegis_core::{
    OrderContext, OrderStatusChange, PluginManifest, PostContext, Result, UserAuthContext,
};
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

    /// 見積もり・注文の確定時に送料・税・割引（`adjustments`）を加える。`Err` で注文を断る
    async fn calculate_order(&self, ctx: OrderContext) -> Result<OrderContext> {
        let guard = CapabilityGuard::new(self.manifest());
        guard.check_hook("calculate-order")?;
        Ok(ctx)
    }

    /// 注文の作成・状態の変化の通知（結果は注文に影響しない）
    async fn on_order_status_changed(&self, change: OrderStatusChange) -> Result<()> {
        let guard = CapabilityGuard::new(self.manifest());
        guard.check_hook("on-order-status-changed")?;
        let _ = change;
        Ok(())
    }

    fn provide_templates(&self) -> HashMap<String, String> {
        HashMap::new()
    }
}

fn declares_hook(plugin: &dyn AegisPlugin, hook: &str) -> bool {
    plugin
        .manifest()
        .capabilities
        .hooks
        .iter()
        .any(|h| h == hook)
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

    /// Runs every `calculate-order` plugin in turn. Plugins may only change `adjustments` and
    /// `metadata`: the lines, prices, customer and coupon stay as the host built them, and the
    /// totals are recomputed after each plugin. Adjustments a plugin adds are tagged with its
    /// name (`source`). An error (a refusal or a failure) stops the chain
    pub async fn apply_calculate_order(&self, mut ctx: OrderContext) -> Result<OrderContext> {
        ctx.recompute_totals();
        for plugin in &self.plugins {
            if !declares_hook(plugin.as_ref(), "calculate-order") {
                continue;
            }
            let before = ctx.clone();
            let returned = plugin.calculate_order(ctx).await?;
            let name = &plugin.manifest().name;
            let adjustments = returned
                .adjustments
                .into_iter()
                .map(|mut adjustment| {
                    if !before.adjustments.contains(&adjustment) {
                        adjustment.source = name.clone();
                    }
                    adjustment
                })
                .collect();
            ctx = OrderContext {
                adjustments,
                metadata: returned.metadata,
                ..before
            };
            ctx.recompute_totals();
        }
        Ok(ctx)
    }

    /// Notifies every `on-order-status-changed` plugin. A failing plugin does not stop the
    /// others; the failures are returned (plugin name, error) for logging and auditing
    pub async fn notify_order_status_changed(
        &self,
        change: &OrderStatusChange,
    ) -> Vec<(String, aegis_core::AegisError)> {
        let mut failures = Vec::new();
        for plugin in &self.plugins {
            if !declares_hook(plugin.as_ref(), "on-order-status-changed") {
                continue;
            }
            if let Err(e) = plugin.on_order_status_changed(change.clone()).await {
                failures.push((plugin.manifest().name.clone(), e));
            }
        }
        failures
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
