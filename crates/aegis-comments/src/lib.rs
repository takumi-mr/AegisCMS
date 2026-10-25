//! 読者のコメント（`[comments]`）: 投稿・審査（承認制・スパム対策）・スレッド表示・通知。
//! HTTP 層（API・公開ページ）はサーバー側にあり、ここは HTTP に依存しない
//!
//! 審査の仕組み（承認制の設定、入力の検査、IP ごとの投稿数、NG ワード・リンク数）は
//! [`moderation`] にまとめてあり、今後の商品レビューも同じものを使う。コメントの表は
//! `_aegis_comments`（`aegis_db::CommentStorage`）
//!
//! 通知（`[notifications]`）:
//!
//! - `comment.posted`: 承認済みのコメントが付いた → コンテンツの著者
//! - `comment.pending`: 承認待ちのコメントが来た → コンテンツの著者・運営者
//! - `comment.replied`: 返信が公開された → 返信先のコメントの投稿者（投稿時に希望した人だけ）

pub mod config;
pub mod moderation;

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use aegis_audit::{AuditOrigin, AuditService};
use aegis_core::{
    AegisError, AuditAction, AuditOutcome, CacheBackend, CommentSection, CommentSubmission,
    CommentVerdict, ModerationStatus, NotificationEvent, Notifier, PublicComment, Recipient,
};
use aegis_db::{CommentListQuery, CommentRecord, DatabaseAdapter};
use aegis_runtime::PluginSupervisor;
use aegis_schema::SchemaDefinition;
use chrono::{DateTime, SecondsFormat, Utc};
use serde::{Deserialize, Serialize};
use serde_json::json;
use tokio::sync::RwLock;
use uuid::Uuid;

pub use config::CommentsConfig;
pub use moderation::{ModerationConfig, ModerationPolicy, PolicyOverrides};

/// `_aegis_settings` の名前の接頭辞（`comments.require_approval` など）
pub const SETTINGS_PREFIX: &str = "comments";
/// IP ごとの投稿数を数えるキャッシュのキーの名前空間
const RATE_LIMIT_SCOPE: &str = "comments";
/// 公開ページに出すコメントの上限（古い順）
const MAX_PUBLIC_COMMENTS: u64 = 1000;
/// まとめて操作できるコメントの数
pub const MAX_BULK: usize = 200;

#[derive(Debug, thiserror::Error)]
pub enum CommentError {
    /// 入力の不備（400）
    #[error("{0}")]
    Invalid(String),
    /// コンテンツ・コメントが無い、コメントを受け付けない種類（404）
    #[error("{0}")]
    NotFound(String),
    /// 公開から日数が経ってコメントを閉じた（403）
    #[error("Comments are closed")]
    Closed,
    /// 同じ IP からの投稿が多すぎる（429）
    #[error("Too many comments from this address. Try again later.")]
    RateLimited,
    /// `check-comment` プラグインが投稿を断った（422。メッセージは投稿者に見せる）
    #[error("{0}")]
    Rejected(String),
    /// それ以外のサーバー側の失敗（500）
    #[error("{0}")]
    Internal(String),
}

impl From<AegisError> for CommentError {
    fn from(e: AegisError) -> Self {
        Self::Internal(e.to_string())
    }
}

pub type CommentResult<T> = std::result::Result<T, CommentError>;

/// 投稿日時（RFC 3339 UTC、マイクロ秒まで）。同じ秒の投稿もスレッドで投稿順に並ぶよう細かくする
fn now() -> String {
    Utc::now().to_rfc3339_opts(SecondsFormat::Micros, true)
}

/// コンテンツの公開ページの URL を作る（`content_type`, `slug` → 絶対 URL。作れなければ空）
pub type ContentUrl = Arc<dyn Fn(&str, &str) -> String + Send + Sync>;

/// コメントの処理が使うサーバーの部品
#[derive(Clone)]
pub struct CommentDeps {
    pub db: Arc<dyn DatabaseAdapter>,
    /// IP ごとの投稿数を数える
    pub cache: Arc<dyn CacheBackend>,
    pub schemas: Arc<RwLock<HashMap<String, SchemaDefinition>>>,
    /// `check-comment` プラグイン
    pub supervisor: Arc<RwLock<PluginSupervisor>>,
    pub audit: Arc<AuditService>,
    /// `comment.*` の通知（`[notifications]`）。無効なら `None`
    pub notifier: Option<Arc<dyn Notifier>>,
    pub links: Links,
}

/// 通知の本文に入れる URL
#[derive(Clone)]
pub struct Links {
    pub content_url: ContentUrl,
    /// 公開サイトの URL（返信通知の停止リンクに使う。空ならリンクを入れない）
    pub site_url: String,
    /// Admin Studio の URL（承認待ちの通知に入れる。空なら入れない）
    pub admin_url: String,
}

impl Default for Links {
    fn default() -> Self {
        Self {
            content_url: Arc::new(|_, _| String::new()),
            site_url: String::new(),
            admin_url: String::new(),
        }
    }
}

/// コメントを付けるコンテンツ（公開中のもの）
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommentTarget {
    pub content_type: String,
    pub id: String,
    pub title: String,
    pub slug: String,
    /// スキーマの表示名（`Articles` など）
    pub label: String,
    pub author_id: Option<String>,
    pub published_at: Option<String>,
}

/// `POST /api/v1/contents/{type}/{slug}/comments` の本文
#[derive(Debug, Clone, Default, Deserialize)]
pub struct CommentRequest {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub email: String,
    #[serde(default)]
    pub body: String,
    #[serde(default)]
    pub parent_id: Option<String>,
    /// 返信が付いたらメールで知らせてほしい
    #[serde(default)]
    pub notify_replies: bool,
    /// ハニーポット（人には見えない欄）。値があればボットとして扱い、保存しない
    #[serde(default)]
    pub website: String,
}

/// 投稿者のクライアント情報
#[derive(Debug, Clone, Default)]
pub struct ClientInfo {
    pub ip: Option<String>,
    pub user_agent: Option<String>,
}

/// 投稿の結果。スパム・ボットと判定しても投稿者には承認待ちと伝える
#[derive(Debug, Clone, PartialEq)]
pub struct Submitted {
    /// 投稿者に伝える状態（`approved` / `pending`）
    pub status: ModerationStatus,
    /// 公開されたコメント（承認済みのときだけ）
    pub comment: Option<PublicComment>,
}

/// Admin Studio から返信するユーザー
#[derive(Debug, Clone)]
pub struct StaffAuthor {
    pub username: String,
    pub display_name: Option<String>,
    pub email: Option<String>,
}

/// Admin Studio の一覧の条件
#[derive(Debug, Clone, Default)]
pub struct AdminListQuery {
    pub status: Option<ModerationStatus>,
    pub content_type: Option<String>,
    pub content_id: Option<String>,
    pub limit: u64,
    pub offset: u64,
}

/// Admin Studio に見せるコメント（返信通知のトークン以外すべて）
#[derive(Debug, Clone, Serialize)]
pub struct AdminComment {
    pub id: String,
    pub content_type: String,
    pub content_id: String,
    /// コメントが付いたコンテンツ（削除済みなら `None`）
    pub content: Option<ContentRef>,
    pub parent_id: Option<String>,
    /// 返信先の投稿者の名前
    pub parent_author: Option<String>,
    pub author_name: String,
    pub author_email: String,
    pub author_user_id: Option<String>,
    pub body: String,
    pub status: String,
    pub ip: Option<String>,
    pub user_agent: Option<String>,
    pub notify_replies: bool,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ContentRef {
    pub title: String,
    pub slug: String,
    /// 公開ページの URL（作れなければ空）
    pub url: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct AdminCommentPage {
    pub comments: Vec<AdminComment>,
    pub total: u64,
    /// 状態ごとの件数（すべてのコメント。`pending` / `approved` / `spam` / `trash`）
    pub counts: HashMap<String, u64>,
}

/// 承認制の設定（`GET` / `PUT /api/v1/comments/settings`）
#[derive(Debug, Clone, Serialize)]
pub struct ModerationSettings {
    /// 実際に使う値
    pub policy: ModerationPolicy,
    /// Admin Studio で変えた値（`null` は設定ファイルの値）
    pub overrides: PolicyOverrides,
    /// 設定ファイル（`[comments.moderation]`）の値
    pub defaults: ModerationPolicy,
}

pub struct CommentService {
    pub config: CommentsConfig,
    deps: CommentDeps,
}

impl CommentService {
    pub fn new(config: CommentsConfig, deps: CommentDeps) -> aegis_core::Result<Self> {
        config.validate()?;
        Ok(Self { config, deps })
    }

    /// `[comments].content_types` のスキーマがあるか（起動時に確かめる）
    pub fn check_content_types(
        &self,
        schemas: &HashMap<String, SchemaDefinition>,
    ) -> aegis_core::Result<()> {
        for content_type in &self.config.content_types {
            if !schemas.contains_key(content_type) {
                return Err(AegisError::Config(format!(
                    "[comments].content_types: no schema for '{}'",
                    content_type
                )));
            }
        }
        Ok(())
    }

    // --- 承認制の設定 ---

    async fn overrides(&self) -> CommentResult<PolicyOverrides> {
        let settings = self.deps.db.list_settings().await?;
        Ok(PolicyOverrides::from_settings(SETTINGS_PREFIX, &settings))
    }

    /// 今の承認制の設定。DB から毎回読むので、ほかのサーバーで変えた値もすぐに効く
    pub async fn settings(&self) -> CommentResult<ModerationSettings> {
        let overrides = self.overrides().await?;
        Ok(ModerationSettings {
            policy: self.config.moderation.policy(&overrides),
            defaults: self.config.moderation.policy(&PolicyOverrides::default()),
            overrides,
        })
    }

    /// 承認制の設定を変える（`None` は設定ファイルの値に戻す）
    pub async fn update_settings(
        &self,
        audit: &AuditOrigin,
        next: PolicyOverrides,
    ) -> CommentResult<ModerationSettings> {
        let previous = self.overrides().await?;
        let (approval, known) = moderation::setting_names(SETTINGS_PREFIX);
        let saved = async {
            for (name, value) in [
                (&approval, next.require_approval),
                (&known, next.auto_approve_known),
            ] {
                match value {
                    Some(value) => self.deps.db.set_setting(name, &value.to_string()).await?,
                    None => self.deps.db.delete_setting(name).await?,
                }
            }
            Ok::<_, AegisError>(())
        }
        .await;
        let outcome = if saved.is_ok() {
            AuditOutcome::Success
        } else {
            AuditOutcome::Failure
        };
        self.deps
            .audit
            .record(
                audit
                    .event(AuditAction::CommentSettingsUpdate, outcome)
                    .with_details(json!({
                        "require_approval": { "from": previous.require_approval, "to": next.require_approval },
                        "auto_approve_known": { "from": previous.auto_approve_known, "to": next.auto_approve_known },
                        "error": saved.as_ref().err().map(|e| e.to_string()),
                    })),
            )
            .await;
        saved?;
        self.settings().await
    }

    // --- 公開サイト ---

    /// コメントを付けられるコンテンツ（`[comments].content_types` の種類で、公開中のもの）。
    /// それ以外は `NotFound`
    pub async fn target(
        &self,
        content_type: &str,
        slug_or_id: &str,
    ) -> CommentResult<CommentTarget> {
        let not_found = || CommentError::NotFound(format!("Content '{}' not found", slug_or_id));
        if !self.config.accepts(content_type) {
            return Err(CommentError::NotFound(format!(
                "Comments are not enabled for '{}'",
                content_type
            )));
        }
        let schema = self
            .deps
            .schemas
            .read()
            .await
            .get(content_type)
            .cloned()
            .ok_or_else(not_found)?;
        let record = self
            .deps
            .db
            .get_content(&schema, slug_or_id)
            .await?
            .ok_or_else(not_found)?;
        if !is_currently_published(&record.status, record.unpublish_at.as_deref(), Utc::now()) {
            return Err(not_found());
        }
        Ok(CommentTarget {
            content_type: record.content_type,
            id: record.id,
            title: record.title,
            slug: record.slug,
            label: schema.label,
            author_id: record.author_id.filter(|a| !a.is_empty()),
            published_at: record.published_at,
        })
    }

    /// 新しいコメントを受け付けているか（`close_after_days`）
    pub fn is_open(&self, target: &CommentTarget, now: DateTime<Utc>) -> bool {
        if self.config.close_after_days == 0 {
            return true;
        }
        let Some(published) = target
            .published_at
            .as_deref()
            .and_then(|p| DateTime::parse_from_rfc3339(p).ok())
        else {
            return true;
        };
        now < published.with_timezone(&Utc)
            + chrono::Duration::days(i64::from(self.config.close_after_days))
    }

    /// テーマのコメント欄（承認済みのコメントをスレッドの順に）。`post_url` は投稿先
    pub async fn section(
        &self,
        target: &CommentTarget,
        post_url: String,
    ) -> CommentResult<CommentSection> {
        let page = self
            .deps
            .db
            .list_comments(&CommentListQuery {
                content_type: Some(target.content_type.clone()),
                content_id: Some(target.id.clone()),
                status: Some(ModerationStatus::Approved.as_str().to_string()),
                oldest_first: true,
                limit: Some(MAX_PUBLIC_COMMENTS),
                offset: 0,
            })
            .await?;
        Ok(CommentSection {
            content_id: target.id.clone(),
            open: self.is_open(target, Utc::now()),
            count: page.total,
            items: thread(page.comments, self.config.max_depth),
            post_url,
            max_depth: self.config.max_depth,
            max_length: self.config.max_length,
        })
    }

    /// 公開サイトからの投稿
    pub async fn submit(
        &self,
        target: &CommentTarget,
        request: CommentRequest,
        client: &ClientInfo,
    ) -> CommentResult<Submitted> {
        let author_name =
            moderation::normalize_name(&request.name).map_err(CommentError::Invalid)?;
        let author_email =
            moderation::normalize_email(&request.email).map_err(CommentError::Invalid)?;
        let body = moderation::normalize_body(&request.body, self.config.max_length)
            .map_err(CommentError::Invalid)?;
        if !self.is_open(target, Utc::now()) {
            return Err(CommentError::Closed);
        }
        let held = Submitted {
            status: ModerationStatus::Pending,
            comment: None,
        };
        // ボット: 保存も通知もせず、承認待ちになったように見せる
        if !request.website.trim().is_empty() {
            tracing::info!(
                "Dropped a comment on {}/{} that filled in the honeypot field",
                target.content_type,
                target.id
            );
            return Ok(held);
        }
        if !moderation::within_rate_limit(
            self.deps.cache.as_ref(),
            RATE_LIMIT_SCOPE,
            client.ip.as_deref(),
            self.config.moderation.max_per_ip_per_hour,
        )
        .await?
        {
            return Err(CommentError::RateLimited);
        }
        let (parent, depth) = match request.parent_id.as_deref().filter(|p| !p.is_empty()) {
            Some(parent_id) => {
                let (parent, parent_depth) = self.reply_parent(target, parent_id).await?;
                (Some(parent), parent_depth + 1)
            }
            None => (None, 0),
        };

        // 内容 → 承認制 → プラグインの順に、厳しい方を採る
        let policy = self.config.moderation.policy(&self.overrides().await?);
        let (mut status, mut reason) =
            match self
                .config
                .moderation
                .screen(&[&author_name, &author_email, &body])
            {
                Some((status, reason)) => (status, Some(reason)),
                None => {
                    let known = policy.require_approval
                        && policy.auto_approve_known
                        && self
                            .deps
                            .db
                            .has_approved_comment(&author_email, &author_name)
                            .await?;
                    (policy.status_for(known), None)
                }
            };
        if status != ModerationStatus::Spam {
            let submission = CommentSubmission {
                content_type: target.content_type.clone(),
                content_id: target.id.clone(),
                content_title: target.title.clone(),
                content_slug: target.slug.clone(),
                parent_id: parent.as_ref().map(|p| p.id.clone()),
                author_name: author_name.clone(),
                author_email: author_email.clone(),
                body: body.clone(),
                ip: client.ip.clone(),
                user_agent: client.user_agent.clone(),
                status: status.as_str().to_string(),
            };
            let (plugin_status, plugin_reason) = self.check_with_plugins(&submission).await?;
            if moderation::stricter(status, plugin_status) != status {
                status = plugin_status;
                reason = plugin_reason;
            }
        }

        let created_at = now();
        let record = CommentRecord {
            id: Uuid::new_v4().to_string(),
            content_type: target.content_type.clone(),
            content_id: target.id.clone(),
            parent_id: parent.as_ref().map(|p| p.id.clone()),
            author_name,
            author_email,
            author_user_id: None,
            body,
            status: status.as_str().to_string(),
            ip: client.ip.clone(),
            user_agent: client.user_agent.clone(),
            notify_replies: request.notify_replies,
            notify_token: request
                .notify_replies
                .then(|| Uuid::new_v4().simple().to_string()),
            created_at: created_at.clone(),
            updated_at: created_at,
        };
        self.deps.db.insert_comment(&record).await?;
        if let Some(reason) = &reason {
            tracing::info!(
                "Comment {} on {}/{} is {}: {}",
                record.id,
                target.content_type,
                target.id,
                status,
                reason
            );
        }

        match status {
            ModerationStatus::Approved => {
                if self.config.notify_author {
                    self.notify_staff_side("comment.posted", target, &record, false)
                        .await;
                }
                if let Some(parent) = &parent {
                    self.notify_reply(target, parent, &record);
                }
                let comment = public_comment(&record, depth);
                Ok(Submitted {
                    status,
                    comment: Some(comment),
                })
            }
            ModerationStatus::Pending => {
                self.notify_staff_side(
                    "comment.pending",
                    target,
                    &record,
                    self.config.notify_staff,
                )
                .await;
                Ok(held)
            }
            ModerationStatus::Spam | ModerationStatus::Trash => Ok(held),
        }
    }

    /// 返信先のコメントとその深さ: 同じコンテンツの承認済みのコメントで、`max_depth` より浅いこと
    async fn reply_parent(
        &self,
        target: &CommentTarget,
        parent_id: &str,
    ) -> CommentResult<(CommentRecord, u32)> {
        if self.config.max_depth == 0 {
            return Err(CommentError::Invalid("Replies are not enabled".to_string()));
        }
        let invalid =
            || CommentError::Invalid("The comment you replied to was not found".to_string());
        let parent = self
            .deps
            .db
            .get_comment(parent_id)
            .await?
            .filter(|p| {
                p.content_type == target.content_type
                    && p.content_id == target.id
                    && p.status == ModerationStatus::Approved.as_str()
            })
            .ok_or_else(invalid)?;
        // 親をたどって深さを数える（`max_depth` で打ち切る）
        let mut depth = 0;
        let mut ancestor = parent.parent_id.clone();
        while let Some(id) = ancestor {
            depth += 1;
            if depth >= self.config.max_depth {
                return Err(CommentError::Invalid(
                    "Replies cannot be nested any deeper".to_string(),
                ));
            }
            ancestor = self
                .deps
                .db
                .get_comment(&id)
                .await?
                .and_then(|c| c.parent_id);
        }
        Ok((parent, depth))
    }

    /// `check-comment` プラグインの判定（状態と理由）。プラグインが失敗したら承認待ちにする
    async fn check_with_plugins(
        &self,
        submission: &CommentSubmission,
    ) -> CommentResult<(ModerationStatus, Option<String>)> {
        let check = {
            let supervisor = self.deps.supervisor.read().await;
            supervisor.check_comment(submission).await
        };
        let check = match check {
            Ok(check) => check,
            Err(AegisError::PluginRejected(message)) => {
                return Err(CommentError::Rejected(message))
            }
            Err(e) => return Err(e.into()),
        };
        let mut status = ModerationStatus::Approved;
        let mut reason = None;
        for (plugin, error) in &check.failures {
            tracing::warn!("Plugin '{}' check-comment failed: {}", plugin, error);
            self.deps
                .audit
                .record(
                    AuditOrigin::default()
                        .event(AuditAction::PluginError, AuditOutcome::Failure)
                        .with_resource("plugin", Some(plugin.clone()))
                        .with_details(json!({
                            "hook": "check-comment",
                            "error": error.to_string(),
                        })),
                )
                .await;
            status = ModerationStatus::Pending;
            reason = Some(format!("plugin '{}' failed", plugin));
        }
        let plugin = check.plugin.unwrap_or_default();
        match check.verdict {
            CommentVerdict::Allow => {}
            CommentVerdict::Hold { reason: why } => {
                status = ModerationStatus::Pending;
                reason = Some(format!("plugin '{}': {}", plugin, why.unwrap_or_default()));
            }
            CommentVerdict::Spam { reason: why } => {
                status = ModerationStatus::Spam;
                reason = Some(format!("plugin '{}': {}", plugin, why.unwrap_or_default()));
            }
        }
        Ok((status, reason))
    }

    /// 返信通知を止める（通知のメールのリンク）。トークンが合わなければ `NotFound`
    pub async fn unsubscribe(&self, id: &str, token: &str) -> CommentResult<()> {
        let not_found = || CommentError::NotFound("The link is invalid".to_string());
        let comment = self.deps.db.get_comment(id).await?.ok_or_else(not_found)?;
        match &comment.notify_token {
            Some(expected) if !token.is_empty() && constant_time_eq(expected, token) => {}
            _ => return Err(not_found()),
        }
        if comment.notify_replies {
            self.deps.db.set_comment_notify_replies(id, false).await?;
        }
        Ok(())
    }

    // --- Admin Studio ---

    /// 一覧（新しい順）と状態ごとの件数
    pub async fn list(&self, query: &AdminListQuery) -> CommentResult<AdminCommentPage> {
        let page = self
            .deps
            .db
            .list_comments(&CommentListQuery {
                content_type: query.content_type.clone(),
                content_id: query.content_id.clone(),
                status: query.status.map(|s| s.as_str().to_string()),
                oldest_first: false,
                limit: Some(query.limit),
                offset: query.offset,
            })
            .await?;
        let mut counts: HashMap<String, u64> = ModerationStatus::ALL
            .iter()
            .map(|s| (s.as_str().to_string(), 0))
            .collect();
        for (status, n) in self.deps.db.count_comments_by_status().await? {
            counts.insert(status, n);
        }

        let contents = self.content_refs(&page.comments).await;
        let mut parent_authors = HashMap::new();
        for parent_id in page.comments.iter().filter_map(|c| c.parent_id.as_ref()) {
            if parent_authors.contains_key(parent_id) {
                continue;
            }
            let name = match page.comments.iter().find(|c| &c.id == parent_id) {
                Some(parent) => Some(parent.author_name.clone()),
                None => self
                    .deps
                    .db
                    .get_comment(parent_id)
                    .await?
                    .map(|p| p.author_name),
            };
            parent_authors.insert(parent_id.clone(), name);
        }

        let comments = page
            .comments
            .into_iter()
            .map(|c| AdminComment {
                content: contents
                    .get(&(c.content_type.clone(), c.content_id.clone()))
                    .cloned(),
                parent_author: c
                    .parent_id
                    .as_ref()
                    .and_then(|p| parent_authors.get(p).cloned().flatten()),
                id: c.id,
                content_type: c.content_type,
                content_id: c.content_id,
                parent_id: c.parent_id,
                author_name: c.author_name,
                author_email: c.author_email,
                author_user_id: c.author_user_id,
                body: c.body,
                status: c.status,
                ip: c.ip,
                user_agent: c.user_agent,
                notify_replies: c.notify_replies,
                created_at: c.created_at,
                updated_at: c.updated_at,
            })
            .collect();
        Ok(AdminCommentPage {
            comments,
            total: page.total,
            counts,
        })
    }

    /// コメントが付いたコンテンツのタイトル・URL（種類ごとに 1 回の問い合わせ）
    async fn content_refs(
        &self,
        comments: &[CommentRecord],
    ) -> HashMap<(String, String), ContentRef> {
        let mut by_type: HashMap<&str, Vec<String>> = HashMap::new();
        for c in comments {
            let ids = by_type.entry(c.content_type.as_str()).or_default();
            if !ids.contains(&c.content_id) {
                ids.push(c.content_id.clone());
            }
        }
        let schemas = self.deps.schemas.read().await;
        let mut refs = HashMap::new();
        for (content_type, ids) in by_type {
            let Some(schema) = schemas.get(content_type) else {
                continue;
            };
            match self.deps.db.resolve_relations(schema, "id", &ids).await {
                Ok(found) => {
                    for (id, rel) in found {
                        let url = (self.deps.links.content_url)(content_type, &rel.slug);
                        refs.insert(
                            (content_type.to_string(), id),
                            ContentRef {
                                title: rel.title,
                                slug: rel.slug,
                                url,
                            },
                        );
                    }
                }
                Err(e) => tracing::warn!(
                    "Could not look up the {} the comments belong to: {}",
                    content_type,
                    e
                ),
            }
        }
        refs
    }

    /// 状態を変える（承認・承認待ちに戻す・スパム・ゴミ箱）。承認待ちから承認した返信は、
    /// 返信先の投稿者に知らせる。変えた件数を返す
    pub async fn moderate(
        &self,
        audit: &AuditOrigin,
        ids: &[String],
        status: ModerationStatus,
    ) -> CommentResult<u64> {
        let ids = checked_ids(ids)?;
        let mut before = Vec::with_capacity(ids.len());
        for id in &ids {
            if let Some(comment) = self.deps.db.get_comment(id).await? {
                before.push(comment);
            }
        }
        let result = self
            .deps
            .db
            .set_comments_status(&ids, status.as_str(), &now())
            .await;
        self.record_admin_action(
            audit,
            AuditAction::CommentModerate,
            &ids,
            json!({ "status": status.as_str() }),
            &result,
        )
        .await;
        let changed = result?;

        if status == ModerationStatus::Approved {
            for reply in before
                .iter()
                .filter(|c| c.status != ModerationStatus::Approved.as_str())
            {
                let Some(parent_id) = &reply.parent_id else {
                    continue;
                };
                let (Ok(Some(parent)), Ok(target)) = (
                    self.deps.db.get_comment(parent_id).await,
                    self.target_of(reply).await,
                ) else {
                    continue;
                };
                self.notify_reply(&target, &parent, reply);
            }
        }
        Ok(changed)
    }

    /// 完全に削除する。削除した件数を返す（返信は残り、公開ページではトップレベルに出る）
    pub async fn delete(&self, audit: &AuditOrigin, ids: &[String]) -> CommentResult<u64> {
        let ids = checked_ids(ids)?;
        let result = self.deps.db.delete_comments(&ids).await;
        self.record_admin_action(audit, AuditAction::CommentDelete, &ids, json!({}), &result)
            .await;
        Ok(result?)
    }

    /// Admin Studio からの返信（承認済みで公開する）。返信先が承認待ちなら一緒に承認する
    pub async fn reply(
        &self,
        audit: &AuditOrigin,
        parent_id: &str,
        author: &StaffAuthor,
        body: &str,
    ) -> CommentResult<CommentRecord> {
        let body = moderation::normalize_body(body, self.config.max_length)
            .map_err(CommentError::Invalid)?;
        let mut parent =
            self.deps.db.get_comment(parent_id).await?.ok_or_else(|| {
                CommentError::NotFound(format!("Comment '{}' not found", parent_id))
            })?;
        if parent.status != ModerationStatus::Approved.as_str() {
            if parent.status != ModerationStatus::Pending.as_str() {
                return Err(CommentError::Invalid(
                    "Only approved or pending comments can be replied to".to_string(),
                ));
            }
            self.moderate(audit, &[parent.id.clone()], ModerationStatus::Approved)
                .await?;
            parent.status = ModerationStatus::Approved.as_str().to_string();
        }
        let target = self.target_of(&parent).await?;
        let created_at = now();
        let record = CommentRecord {
            id: Uuid::new_v4().to_string(),
            content_type: parent.content_type.clone(),
            content_id: parent.content_id.clone(),
            parent_id: Some(parent.id.clone()),
            author_name: author
                .display_name
                .clone()
                .filter(|n| !n.trim().is_empty())
                .unwrap_or_else(|| author.username.clone()),
            author_email: author.email.clone().unwrap_or_default().to_lowercase(),
            author_user_id: Some(author.username.clone()),
            body,
            status: ModerationStatus::Approved.as_str().to_string(),
            ip: audit.ip.clone(),
            user_agent: audit.user_agent.clone(),
            notify_replies: false,
            notify_token: None,
            created_at: created_at.clone(),
            updated_at: created_at,
        };
        let result = self.deps.db.insert_comment(&record).await;
        let outcome = if result.is_ok() {
            AuditOutcome::Success
        } else {
            AuditOutcome::Failure
        };
        self.deps
            .audit
            .record(
                audit
                    .event(AuditAction::CommentCreate, outcome)
                    .with_resource("comment", Some(record.id.clone()))
                    .with_details(json!({
                        "parent_id": parent.id,
                        "content_type": record.content_type,
                        "content_id": record.content_id,
                        "error": result.as_ref().err().map(|e| e.to_string()),
                    })),
            )
            .await;
        result?;
        self.notify_reply(&target, &parent, &record);
        Ok(record)
    }

    /// コンテンツを削除したときに、そのコメントも消す
    pub async fn delete_for_content(
        &self,
        content_type: &str,
        content_id: &str,
    ) -> CommentResult<u64> {
        Ok(self
            .deps
            .db
            .delete_content_comments(content_type, content_id)
            .await?)
    }

    /// コメントが付いたコンテンツ（公開中でなくてもよい。通知の本文用）
    async fn target_of(&self, comment: &CommentRecord) -> CommentResult<CommentTarget> {
        let not_found = || {
            CommentError::NotFound(format!(
                "The content of comment '{}' no longer exists",
                comment.id
            ))
        };
        let schema = self
            .deps
            .schemas
            .read()
            .await
            .get(&comment.content_type)
            .cloned()
            .ok_or_else(not_found)?;
        let record = self
            .deps
            .db
            .get_content(&schema, &comment.content_id)
            .await?
            .ok_or_else(not_found)?;
        Ok(CommentTarget {
            content_type: record.content_type,
            id: record.id,
            title: record.title,
            slug: record.slug,
            label: schema.label,
            author_id: record.author_id.filter(|a| !a.is_empty()),
            published_at: record.published_at,
        })
    }

    async fn record_admin_action(
        &self,
        audit: &AuditOrigin,
        action: AuditAction,
        ids: &[String],
        mut details: serde_json::Value,
        result: &aegis_core::Result<u64>,
    ) {
        details["ids"] = json!(ids);
        let outcome = match result {
            Ok(count) => {
                details["count"] = json!(count);
                AuditOutcome::Success
            }
            Err(e) => {
                details["error"] = json!(e.to_string());
                AuditOutcome::Failure
            }
        };
        let resource = (ids.len() == 1).then(|| ids[0].clone());
        self.deps
            .audit
            .record(
                audit
                    .event(action, outcome)
                    .with_resource("comment", resource)
                    .with_details(details),
            )
            .await;
    }

    // --- 通知 ---

    fn comment_url(&self, target: &CommentTarget, comment_id: &str) -> String {
        let url = (self.deps.links.content_url)(&target.content_type, &target.slug);
        if url.is_empty() {
            url
        } else {
            format!("{}#comment-{}", url, comment_id)
        }
    }

    /// 著者（`notify_author` のとき）と、`to_staff` なら運営者に `kind` を知らせる
    async fn notify_staff_side(
        &self,
        kind: &str,
        target: &CommentTarget,
        comment: &CommentRecord,
        to_staff: bool,
    ) {
        let Some(notifier) = &self.deps.notifier else {
            return;
        };
        let mut recipients = Vec::new();
        let mut author_data = json!({ "username": null, "display_name": null });
        if self.config.notify_author {
            if let Some(author) = self.content_author(target.author_id.as_deref()).await {
                author_data = json!({
                    "username": author.username,
                    "display_name": author.display_name,
                });
                if let Some(email) = author.email.as_deref() {
                    let name = author.display_name.as_deref().unwrap_or(&author.username);
                    recipients.push(Recipient::person(email, name));
                }
            }
        }
        if to_staff {
            recipients.push(Recipient::Staff);
        }
        if recipients.is_empty() {
            return;
        }
        let admin_url = self.deps.links.admin_url.trim();
        let mut data = json!({
            "comment": {
                "id": comment.id,
                "author_name": comment.author_name,
                "author_email": comment.author_email,
                "body": comment.body,
                "status": comment.status,
                "ip": comment.ip,
                "created_at": comment.created_at,
                "is_reply": comment.parent_id.is_some(),
            },
            "content": content_data(target),
            "content_url": self.comment_url(target, &comment.id),
            "moderation_url": if admin_url.is_empty() {
                String::new()
            } else {
                format!("{}#comments", admin_url)
            },
        });
        data["username"] = author_data["username"].take();
        data["display_name"] = author_data["display_name"].take();
        notifier.notify(NotificationEvent::new(kind, recipients, data));
    }

    /// 返信先の投稿者に `comment.replied` を知らせる（希望した人だけ。自分への返信は除く）
    fn notify_reply(&self, target: &CommentTarget, parent: &CommentRecord, reply: &CommentRecord) {
        let Some(notifier) = &self.deps.notifier else {
            return;
        };
        let Some(token) = parent
            .notify_token
            .as_deref()
            .filter(|_| parent.notify_replies)
        else {
            return;
        };
        if parent.author_email.is_empty()
            || parent.author_email == reply.author_email
            || parent.status != ModerationStatus::Approved.as_str()
        {
            return;
        }
        let site = self.deps.links.site_url.trim().trim_end_matches('/');
        let unsubscribe_url = if site.is_empty() {
            String::new()
        } else {
            format!(
                "{}/comments/unsubscribe?id={}&token={}",
                site, parent.id, token
            )
        };
        notifier.notify(NotificationEvent::new(
            "comment.replied",
            vec![Recipient::person(&parent.author_email, &parent.author_name)],
            json!({
                "name": parent.author_name,
                "comment": { "body": parent.body, "created_at": parent.created_at },
                "reply": {
                    "author_name": reply.author_name,
                    "body": reply.body,
                    "created_at": reply.created_at,
                    "staff": reply.author_user_id.is_some(),
                },
                "content": content_data(target),
                "content_url": self.comment_url(target, &reply.id),
                "unsubscribe_url": unsubscribe_url,
            }),
        ));
    }

    /// 通知を送れる著者（有効なアカウント）
    async fn content_author(&self, author_id: Option<&str>) -> Option<aegis_db::UserRecord> {
        let author_id = author_id?;
        match self.deps.db.get_user_by_username(author_id).await {
            Ok(Some(user)) if !user.disabled => Some(user),
            Ok(_) => None,
            Err(e) => {
                tracing::warn!(
                    "Could not look up the author '{}' to notify: {}",
                    author_id,
                    e
                );
                None
            }
        }
    }
}

fn content_data(target: &CommentTarget) -> serde_json::Value {
    json!({
        "id": target.id,
        "content_type": target.content_type,
        "label": target.label,
        "title": target.title,
        "slug": target.slug,
    })
}

/// まとめて操作する ID（空・多すぎるは `Invalid`。重複は除く）
fn checked_ids(ids: &[String]) -> CommentResult<Vec<String>> {
    let mut seen = HashSet::new();
    let ids: Vec<String> = ids
        .iter()
        .map(|id| id.trim().to_string())
        .filter(|id| !id.is_empty() && seen.insert(id.clone()))
        .collect();
    if ids.is_empty() {
        return Err(CommentError::Invalid("No comments were given".to_string()));
    }
    if ids.len() > MAX_BULK {
        return Err(CommentError::Invalid(format!(
            "Too many comments at once ({} at most)",
            MAX_BULK
        )));
    }
    Ok(ids)
}

/// 公開中か（`published` で、`unpublish_at` を迎えていない）
fn is_currently_published(status: &str, unpublish_at: Option<&str>, now: DateTime<Utc>) -> bool {
    status == "published"
        && unpublish_at
            .and_then(|u| DateTime::parse_from_rfc3339(u).ok())
            .is_none_or(|due| due > now)
}

fn constant_time_eq(a: &str, b: &str) -> bool {
    a.len() == b.len()
        && a.bytes()
            .zip(b.bytes())
            .fold(0u8, |acc, (x, y)| acc | (x ^ y))
            == 0
}

fn public_comment(record: &CommentRecord, depth: u32) -> PublicComment {
    PublicComment {
        id: record.id.clone(),
        parent_id: record.parent_id.clone(),
        author_name: record.author_name.clone(),
        body_html: moderation::body_html(&record.body),
        created_at: record.created_at.clone(),
        depth,
        staff: record.author_user_id.is_some(),
    }
}

/// 古い順のコメントをスレッドの順に並べる（返信は親の直後。深さは `max_depth` まで）。
/// 親が一覧に無い（承認されていない・削除された）返信はトップレベルとして扱う
pub fn thread(records: Vec<CommentRecord>, max_depth: u32) -> Vec<PublicComment> {
    let ids: HashSet<&str> = records.iter().map(|r| r.id.as_str()).collect();
    let mut children: HashMap<&str, Vec<usize>> = HashMap::new();
    let mut roots = Vec::new();
    for (i, r) in records.iter().enumerate() {
        match r
            .parent_id
            .as_deref()
            .filter(|p| ids.contains(p) && *p != r.id)
        {
            Some(parent) => children.entry(parent).or_default().push(i),
            None => roots.push(i),
        }
    }
    let mut out = Vec::with_capacity(records.len());
    let mut visited = vec![false; records.len()];
    // 深さ優先（古い順を保つため、子は逆順に積む）
    let mut stack: Vec<(usize, u32)> = roots.into_iter().rev().map(|i| (i, 0)).collect();
    loop {
        while let Some((i, depth)) = stack.pop() {
            if std::mem::replace(&mut visited[i], true) {
                continue;
            }
            out.push(public_comment(&records[i], depth.min(max_depth)));
            if let Some(kids) = children.get(records[i].id.as_str()) {
                stack.extend(kids.iter().rev().map(|&k| (k, depth + 1)));
            }
        }
        // 親をたどるとループになる行（通常は無い）もトップレベルとして出す
        match visited.iter().position(|v| !v) {
            Some(i) => stack.push((i, 0)),
            None => break,
        }
    }
    out
}

#[cfg(test)]
mod tests;
