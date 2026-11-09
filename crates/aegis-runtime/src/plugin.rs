use crate::guard::CapabilityGuard;
use crate::scripts;
use aegis_ast::BlockNode;
use aegis_core::{
    AegisError, BlockDefinition, CommentSubmission, CommentVerdict, Delivered, NotificationMessage,
    OrderContext, OrderStatusChange, PluginManifest, PostContext, PublishedContent, Result,
    SendError, UserAuthContext,
};
use async_trait::async_trait;
use std::sync::Arc;

use std::collections::HashMap;

/// 本文が変わったかを見るための安い指紋（長さとハッシュ）。プラグインに渡す前後の比較に使う
fn content_fingerprint(html: &str) -> (usize, u64) {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    html.hash(&mut hasher);
    (html.len(), hasher.finish())
}

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

    /// コンテンツ公開の通知（予約公開など）。結果は公開に影響しない
    async fn on_content_published(&self, content: PublishedContent) -> Result<()> {
        let guard = CapabilityGuard::new(self.manifest());
        guard.check_hook("on-content-published")?;
        let _ = content;
        Ok(())
    }

    /// 公開サイトから投稿されたコメントの審査（`check-comment`。スパム判定など）。
    /// `Err(AegisError::PluginRejected)` で投稿を断る（メッセージは投稿者に表示される）
    async fn check_comment(&self, comment: CommentSubmission) -> Result<CommentVerdict> {
        let guard = CapabilityGuard::new(self.manifest());
        guard.check_hook("check-comment")?;
        let _ = comment;
        Ok(CommentVerdict::Allow)
    }

    /// 通知の配信フック（`send-notification`）。`[notifications].channels` に `plugin:<名前>` で
    /// 加えたプラグインが、テンプレートから作った通知を Slack・SMS などへ送る。
    /// 扱わない通知は `Ok(Delivered::Skipped)`、`SendError::permanent` は再試行しない失敗
    async fn send_notification(
        &self,
        message: NotificationMessage,
    ) -> std::result::Result<Delivered, SendError> {
        let guard = CapabilityGuard::new(self.manifest());
        guard
            .check_hook("send-notification")
            .map_err(|e| SendError::permanent(e.to_string()))?;
        let _ = message;
        Ok(Delivered::Skipped)
    }

    /// 本文のカスタムブロック（マニフェストの `blocks`）を HTML にする（`render-blocks`）。
    /// 返す配列は `blocks` と同じ順で、`None` や足りない分のブロックは何も表示しない
    async fn render_blocks(&self, blocks: Vec<BlockNode>) -> Result<Vec<Option<String>>> {
        let guard = CapabilityGuard::new(self.manifest());
        guard.check_hook("render-blocks")?;
        let _ = blocks;
        Ok(Vec::new())
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

/// `check-comment` の結果（`PluginSupervisor::check_comment`）
#[derive(Debug, Default)]
pub struct CommentCheck {
    /// いちばん厳しい判定（スパム > 承認待ち > そのまま）
    pub verdict: CommentVerdict,
    /// その判定をしたプラグイン（`Allow` なら `None`）
    pub plugin: Option<String>,
    /// 失敗したプラグイン（タイムアウトなど）と、そのエラー。呼び出し側は承認待ちにする
    pub failures: Vec<(String, AegisError)>,
}

/// 有効なプラグインが要求する CSP (Content Security Policy) のドメイン一覧
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PluginCspSources {
    pub script_domains: Vec<String>,
    pub frame_domains: Vec<String>,
    pub connect_domains: Vec<String>,
    pub style_domains: Vec<String>,
    pub img_domains: Vec<String>,
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

    /// 本文に置けるカスタムブロックの宣言（`render-blocks` を宣言したプラグインのもの。実行順）。
    /// 組は（プラグイン名, 宣言）
    pub fn block_definitions(&self) -> Vec<(String, BlockDefinition)> {
        self.plugins
            .iter()
            .filter(|p| declares_hook(p.as_ref(), "render-blocks"))
            .flat_map(|p| {
                let name = p.manifest().name.clone();
                p.manifest()
                    .blocks
                    .iter()
                    .map(move |b| (name.clone(), b.clone()))
            })
            .collect()
    }

    /// カスタムブロックに埋め込める `<iframe>` の配信元（`render-blocks` を宣言したプラグインの
    /// `embed.frame_domains` の和。重複なし・並べ替え済み）。CSP の `frame-src` に足す
    pub fn frame_domains(&self) -> Vec<String> {
        let mut domains: Vec<String> = self
            .plugins
            .iter()
            .filter(|p| declares_hook(p.as_ref(), "render-blocks"))
            .flat_map(|p| p.manifest().frame_domains().to_vec())
            .collect();
        for p in &self.plugins {
            domains.extend(p.manifest().csp_frame_domains().iter().cloned());
        }
        domains.sort();
        domains.dedup();
        domains
    }

    /// 有効なプラグインが要求する CSP ソースの集計
    pub fn csp_sources(&self) -> PluginCspSources {
        let mut script = Vec::new();
        let frame = self.frame_domains();
        let mut connect = Vec::new();
        let mut style = Vec::new();
        let mut img = Vec::new();

        for p in &self.plugins {
            let m = p.manifest();
            script.extend(m.csp_script_domains().iter().cloned());
            connect.extend(m.csp_connect_domains().iter().cloned());
            style.extend(m.csp_style_domains().iter().cloned());
            img.extend(m.csp_img_domains().iter().cloned());
        }

        let dedup = |mut list: Vec<String>| {
            list.sort();
            list.dedup();
            list
        };

        PluginCspSources {
            script_domains: dedup(script),
            frame_domains: frame,
            connect_domains: dedup(connect),
            style_domains: dedup(style),
            img_domains: dedup(img),
        }
    }

    /// 本文のブロック名（`<プラグイン名>/<ブロック名>`）を描画するプラグイン
    pub fn block_renderer(&self, block_name: &str) -> Option<&Arc<dyn AegisPlugin>> {
        self.plugins.iter().find(|p| {
            declares_hook(p.as_ref(), "render-blocks") && p.manifest().declares_block(block_name)
        })
    }

    /// 名前でプラグインを探す
    pub fn plugin(&self, name: &str) -> Option<&Arc<dyn AegisPlugin>> {
        self.plugins.iter().find(|p| p.manifest().name == name)
    }

    /// プラグインが `hook` を宣言しているか（登録されていなければ `false`）
    pub fn plugin_declares_hook(&self, name: &str, hook: &str) -> bool {
        self.plugin(name)
            .is_some_and(|plugin| declares_hook(plugin.as_ref(), hook))
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
        // `prepend_content`/`append_content` は各プラグインの書き込み専用の枠。プラグインごとに
        // 空で渡して返った分を貯め、本文との結合は最後に 1 回だけ行う（本文が往復しない）
        let mut prepend = String::new();
        let mut append = String::new();
        // プラグインが構造化した注入（`scripts`）で足したスクリプトの、検査後のタグ。`extra_head` の
        // 後ろに最後にまとめて足す（次のプラグインの `<script>` の取り除きに巻き込まれないため）
        let mut script_tags = String::new();
        for plugin in &self.plugins {
            if plugin
                .manifest()
                .capabilities
                .hooks
                .iter()
                .any(|h| h == "on-render-page")
            {
                ctx.prepend_content.clear();
                ctx.append_content.clear();
                ctx.scripts.clear();
                ctx.content_length = ctx.content_html.len();
                let content_before = content_fingerprint(&ctx.content_html);
                match plugin.on_render_page(ctx).await {
                    Ok(updated) => ctx = updated,
                    Err(e) => {
                        tracing::warn!(
                            target: "aegis::plugin",
                            "Plugin '{}' failed on-render-page: {}",
                            plugin.manifest().name,
                            e
                        );
                        return Err(e);
                    }
                }
                let name = plugin.manifest().name.clone();
                // スクリプトは、構造化した注入でだけ足せる。HTML の断片の中の `<script>` と `on*` 属性は取り除く
                ctx.extra_head = scripts::strip_scripts(&ctx.extra_head);
                ctx.prepend_content = scripts::strip_scripts(&ctx.prepend_content);
                ctx.append_content = scripts::strip_scripts(&ctx.append_content);
                if content_fingerprint(&ctx.content_html) != content_before {
                    ctx.content_html = scripts::strip_scripts(&ctx.content_html);
                }
                let injected = scripts::render_script_tags(
                    plugin.manifest(),
                    &std::mem::take(&mut ctx.scripts),
                );
                for reason in &injected.rejected {
                    tracing::warn!(
                        target: "aegis::plugin",
                        "Plugin '{}' on-render-page: a script was dropped: {}",
                        name,
                        reason
                    );
                }
                script_tags.push_str(&injected.html);
                // 後のプラグインの前置きほど外側、後置きほど内側（本文を直接書き換えた場合と同じ順）
                prepend.insert_str(0, &std::mem::take(&mut ctx.prepend_content));
                append.push_str(&std::mem::take(&mut ctx.append_content));
            }
        }
        if !script_tags.is_empty() {
            if !ctx.extra_head.is_empty() && !ctx.extra_head.ends_with('\n') {
                ctx.extra_head.push('\n');
            }
            ctx.extra_head.push_str(&script_tags);
        }
        if !prepend.is_empty() || !append.is_empty() {
            prepend.push_str(&ctx.content_html);
            prepend.push_str(&append);
            ctx.content_html = prepend;
        }
        ctx.content_length = ctx.content_html.len();
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

    /// Notifies every `on-content-published` plugin. A failing plugin does not stop the
    /// others; the failures are returned (plugin name, error) for logging and auditing
    pub async fn notify_content_published(
        &self,
        content: &PublishedContent,
    ) -> Vec<(String, aegis_core::AegisError)> {
        let mut failures = Vec::new();
        for plugin in &self.plugins {
            if !declares_hook(plugin.as_ref(), "on-content-published") {
                continue;
            }
            if let Err(e) = plugin.on_content_published(content.clone()).await {
                failures.push((plugin.manifest().name.clone(), e));
            }
        }
        failures
    }

    /// Runs every `check-comment` plugin in turn and keeps the strictest verdict (spam over hold
    /// over allow). A spam verdict stops the chain. A refusal (`PluginRejected`) stops the chain
    /// and is returned as the error; any other failure is collected in `failures` and the
    /// remaining plugins still run
    pub async fn check_comment(&self, comment: &CommentSubmission) -> Result<CommentCheck> {
        let mut check = CommentCheck::default();
        for plugin in &self.plugins {
            if !declares_hook(plugin.as_ref(), "check-comment") {
                continue;
            }
            let name = plugin.manifest().name.clone();
            match plugin.check_comment(comment.clone()).await {
                Ok(CommentVerdict::Allow) => {}
                Ok(verdict @ CommentVerdict::Hold { .. }) => {
                    if check.verdict == CommentVerdict::Allow {
                        check.verdict = verdict;
                        check.plugin = Some(name);
                    }
                }
                Ok(verdict @ CommentVerdict::Spam { .. }) => {
                    check.verdict = verdict;
                    check.plugin = Some(name);
                    break;
                }
                Err(e @ AegisError::PluginRejected(_)) => return Err(e),
                Err(e) => check.failures.push((name, e)),
            }
        }
        Ok(check)
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
