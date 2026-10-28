//! 掲示板（`[forum]`）: カテゴリ・スレッド・投稿（レス）・モデレーション・通報・通知・レートリミット。
//! HTTP 層（API・公開画面）とは疎結合で、ビジネスロジックとストレージ連携をカプセル化する。

pub mod config;
#[cfg(test)]
mod tests;

use std::sync::Arc;

use aegis_audit::AuditService;
use aegis_comments::{moderation, ModerationConfig, ModerationPolicy, PolicyOverrides};
use aegis_core::{
    AegisError, AuditAction, AuditActor, AuditEvent, AuditOutcome, CacheBackend, ModerationStatus,
    NotificationEvent, Notifier, Recipient,
};
use aegis_db::{
    DatabaseAdapter, ForumCategoryRecord, ForumPostRecord, ForumReportRecord, ForumThreadRecord,
    PostListQuery, ThreadListQuery,
};
use chrono::{SecondsFormat, Utc};
use serde::{Deserialize, Serialize};
use serde_json::json;
use uuid::Uuid;

pub use config::ForumConfig;

/// レートリミットのキャッシュキー接頭辞
const RATE_LIMIT_PREFIX: &str = "forum:ratelimit:";
/// 時間単位のIP投稿数制限のスコープ
const IP_RATE_LIMIT_SCOPE: &str = "forum";
/// `_aegis_settings` に保存するフォーラムのモデレーション設定の接頭辞
const SETTINGS_PREFIX: &str = "forum";
/// 通報理由の最大文字数
const MAX_REPORT_REASON_CHARS: usize = 1_000;

#[derive(Debug, thiserror::Error)]
pub enum ForumError {
    #[error("{0}")]
    Invalid(String),
    #[error("{0}")]
    NotFound(String),
    #[error("{0}")]
    Forbidden(String),
    #[error("Thread is locked for new posts")]
    Locked,
    #[error("Posting too fast. Please wait a moment before trying again.")]
    RateLimited,
    #[error("Forum is currently disabled")]
    Disabled,
    #[error("{0}")]
    Internal(String),
}

impl From<AegisError> for ForumError {
    fn from(e: AegisError) -> Self {
        Self::Internal(e.to_string())
    }
}

pub type ForumResult<T> = std::result::Result<T, ForumError>;

fn now() -> String {
    Utc::now().to_rfc3339_opts(SecondsFormat::Micros, true)
}

/// 掲示板サービスの依存コンポーネント
#[derive(Clone)]
pub struct ForumDeps {
    pub db: Arc<dyn DatabaseAdapter>,
    pub cache: Arc<dyn CacheBackend>,
    pub audit: Arc<AuditService>,
    pub notifier: Option<Arc<dyn Notifier>>,
    pub site_url: String,
    pub admin_url: String,
}

/// 承認制の設定（`GET` / `PUT /api/v1/forum/settings`）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ForumModerationSettings {
    /// 実際に使う値
    pub policy: ModerationPolicy,
    /// Admin Studio で変えた値（`null` は設定ファイルの値）
    pub overrides: PolicyOverrides,
    /// 設定ファイル（`[forum.moderation]`）の値
    pub defaults: ModerationPolicy,
    /// NG ワード・リンク上限などの設定値（参照用）
    pub config: ModerationConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateCategoryRequest {
    pub slug: String,
    pub name: String,
    pub description: String,
    pub position: i64,
    pub is_private: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateCategoryRequest {
    pub slug: String,
    pub name: String,
    pub description: String,
    pub position: i64,
    pub is_private: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateThreadRequest {
    pub category_id: String,
    pub title: String,
    pub body: String,
    pub author_name: String,
    pub author_id: Option<String>,
    pub ip: Option<String>,
    pub user_agent: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreatePostRequest {
    pub thread_id: String,
    pub body: String,
    pub reply_to_post_id: Option<String>,
    pub reply_to_number: Option<u64>,
    pub author_name: String,
    pub author_id: Option<String>,
    pub ip: Option<String>,
    pub user_agent: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateReportRequest {
    /// `"thread"` or `"post"`
    pub target_type: String,
    pub target_id: String,
    pub reason: String,
    pub reporter_id: Option<String>,
    pub reporter_ip: Option<String>,
}

#[derive(Clone)]
pub struct ForumService {
    config: ForumConfig,
    deps: ForumDeps,
}

impl ForumService {
    pub fn new(config: ForumConfig, deps: ForumDeps) -> Result<Self, AegisError> {
        config.validate()?;
        Ok(Self { config, deps })
    }

    pub fn config(&self) -> &ForumConfig {
        &self.config
    }

    fn check_enabled(&self) -> ForumResult<()> {
        if !self.config.enabled {
            return Err(ForumError::Disabled);
        }
        Ok(())
    }

    /// 監査ログのヘルパー
    #[allow(clippy::too_many_arguments)]
    async fn record_audit(
        &self,
        action: AuditAction,
        outcome: AuditOutcome,
        actor_id: Option<String>,
        resource_type: &str,
        resource_id: Option<String>,
        ip: Option<String>,
        user_agent: Option<String>,
        details: serde_json::Value,
    ) {
        let actor = actor_id.map(|user_id| AuditActor {
            user_id,
            role: "user".to_string(),
        });
        let mut event = AuditEvent::new(action, outcome)
            .with_actor(actor)
            .with_resource(resource_type, resource_id)
            .with_details(details);
        if ip.is_some() || user_agent.is_some() {
            event = event.with_client(ip, user_agent);
        }
        self.deps.audit.record(event).await;
    }

    /// レートリミット（連投防止）のチェック
    async fn check_rate_limit(&self, identifier: &str, ip: Option<&str>) -> ForumResult<()> {
        if self.config.rate_limit_secs > 0 {
            let key = format!("{}{}", RATE_LIMIT_PREFIX, identifier);
            let exists = self
                .deps
                .cache
                .get(&key)
                .await
                .map_err(|e| ForumError::Internal(e.to_string()))?;
            if exists.is_some() {
                return Err(ForumError::RateLimited);
            }
        }
        if !moderation::within_rate_limit(
            self.deps.cache.as_ref(),
            IP_RATE_LIMIT_SCOPE,
            ip,
            self.config.moderation.max_per_ip_per_hour,
        )
        .await?
        {
            return Err(ForumError::RateLimited);
        }
        Ok(())
    }

    /// レートリミットキーの記録（TTL = rate_limit_secs）
    async fn record_rate_limit(&self, identifier: &str) -> ForumResult<()> {
        if self.config.rate_limit_secs == 0 {
            return Ok(());
        }
        let key = format!("{}{}", RATE_LIMIT_PREFIX, identifier);
        self.deps
            .cache
            .set(&key, "1", Some(self.config.rate_limit_secs))
            .await
            .map_err(|e| ForumError::Internal(e.to_string()))?;
        Ok(())
    }

    // ==========================================
    // 承認制・モデレーション設定
    // ==========================================

    async fn overrides(&self) -> ForumResult<PolicyOverrides> {
        let settings = self.deps.db.list_settings().await?;
        Ok(PolicyOverrides::from_settings(SETTINGS_PREFIX, &settings))
    }

    pub async fn settings(&self) -> ForumResult<ForumModerationSettings> {
        let overrides = self.overrides().await?;
        Ok(ForumModerationSettings {
            policy: self.config.moderation.policy(&overrides),
            defaults: self.config.moderation.policy(&PolicyOverrides::default()),
            overrides,
            config: self.config.moderation.clone(),
        })
    }

    pub async fn update_settings(
        &self,
        actor: Option<&str>,
        next: PolicyOverrides,
    ) -> ForumResult<ForumModerationSettings> {
        let previous = self.overrides().await?;
        let (approval, known) = moderation::setting_names(SETTINGS_PREFIX);
        for (name, value) in [
            (&approval, next.require_approval),
            (&known, next.auto_approve_known),
        ] {
            match value {
                Some(v) => self.deps.db.set_setting(name, &v.to_string()).await?,
                None => self.deps.db.delete_setting(name).await?,
            }
        }

        self.record_audit(
            AuditAction::ForumSettingsUpdate,
            AuditOutcome::Success,
            actor.map(|s| s.to_string()),
            "forum_settings",
            None,
            None,
            None,
            json!({
                "require_approval": { "from": previous.require_approval, "to": next.require_approval },
                "auto_approve_known": { "from": previous.auto_approve_known, "to": next.auto_approve_known },
            }),
        )
        .await;

        self.settings().await
    }

    /// 内容（NG ワード・リンク数）と承認制（`require_approval` / `auto_approve_known`）から最初の状態を決める
    async fn evaluate_moderation(
        &self,
        texts: &[&str],
        author_name: &str,
        author_id: Option<&str>,
    ) -> ForumResult<(ModerationStatus, Option<String>)> {
        if let Some((status, reason)) = self.config.moderation.screen(texts) {
            return Ok((status, Some(reason)));
        }
        let policy = self.config.moderation.policy(&self.overrides().await?);
        let known = if policy.require_approval && policy.auto_approve_known {
            self.deps
                .db
                .has_approved_forum_post(author_name, author_id)
                .await?
        } else {
            false
        };
        Ok((policy.status_for(known), None))
    }

    fn thread_url(&self, category_slug: &str, thread_id: &str) -> String {
        if self.deps.site_url.is_empty() {
            String::new()
        } else {
            format!(
                "{}/forum/{}/{}",
                self.deps.site_url.trim_end_matches('/'),
                category_slug,
                thread_id
            )
        }
    }

    fn moderation_url(&self) -> String {
        if self.deps.admin_url.is_empty() {
            String::new()
        } else {
            format!("{}#forum", self.deps.admin_url)
        }
    }

    async fn notify_post(
        &self,
        category: &ForumCategoryRecord,
        thread: &ForumThreadRecord,
        post: &ForumPostRecord,
        is_reply: bool,
        status: ModerationStatus,
    ) {
        if !self.config.notify_staff {
            return;
        }
        let Some(notifier) = &self.deps.notifier else {
            return;
        };
        let kind = match status {
            ModerationStatus::Approved => "forum.posted",
            ModerationStatus::Pending => "forum.pending",
            ModerationStatus::Spam | ModerationStatus::Trash => return,
        };
        let mut thread_url = self.thread_url(&category.slug, &thread.id);
        if is_reply && !thread_url.is_empty() {
            thread_url = format!("{}#post-{}", thread_url, post.post_number);
        }
        notifier.notify(NotificationEvent {
            kind: kind.to_string(),
            recipients: vec![Recipient::Staff],
            data: json!({
                "category": {
                    "id": category.id,
                    "name": category.name,
                    "slug": category.slug,
                },
                "thread": {
                    "id": thread.id,
                    "title": thread.title,
                },
                "post": {
                    "id": post.id,
                    "author_name": post.author_name,
                    "body": post.body,
                    "ip": post.ip,
                    "is_reply": is_reply,
                },
                "thread_url": thread_url,
                "moderation_url": self.moderation_url(),
            }),
        });
    }

    // ==========================================
    // カテゴリ管理
    // ==========================================

    pub async fn create_category(
        &self,
        req: CreateCategoryRequest,
        actor: Option<&str>,
    ) -> ForumResult<ForumCategoryRecord> {
        let slug = req.slug.trim().to_lowercase();
        if slug.is_empty() {
            return Err(ForumError::Invalid("Category slug cannot be empty".into()));
        }
        let name = req.name.trim().to_string();
        if name.is_empty() {
            return Err(ForumError::Invalid("Category name cannot be empty".into()));
        }

        if let Some(existing) = self.deps.db.get_category_by_slug(&slug).await? {
            return Err(ForumError::Invalid(format!(
                "Category with slug '{}' already exists (id: {})",
                slug, existing.id
            )));
        }

        let cat_id = Uuid::new_v4().to_string();
        let timestamp = now();
        let record = ForumCategoryRecord {
            id: cat_id.clone(),
            slug: slug.clone(),
            name,
            description: req.description.trim().to_string(),
            position: req.position,
            is_private: req.is_private,
            created_at: timestamp.clone(),
            updated_at: timestamp,
        };

        self.deps.db.insert_category(&record).await?;

        self.record_audit(
            AuditAction::ForumCategoryCreate,
            AuditOutcome::Success,
            actor.map(|s| s.to_string()),
            "forum_category",
            Some(cat_id),
            None,
            None,
            json!({ "slug": slug }),
        )
        .await;

        Ok(record)
    }

    pub async fn get_category(&self, id: &str) -> ForumResult<ForumCategoryRecord> {
        self.deps
            .db
            .get_category(id)
            .await?
            .ok_or_else(|| ForumError::NotFound(format!("Category {} not found", id)))
    }

    pub async fn get_category_by_slug(&self, slug: &str) -> ForumResult<ForumCategoryRecord> {
        self.deps
            .db
            .get_category_by_slug(slug)
            .await?
            .ok_or_else(|| ForumError::NotFound(format!("Category with slug '{}' not found", slug)))
    }

    pub async fn list_categories(
        &self,
        include_private: bool,
    ) -> ForumResult<Vec<ForumCategoryRecord>> {
        let all = self.deps.db.list_categories().await?;
        if include_private {
            Ok(all)
        } else {
            Ok(all.into_iter().filter(|c| !c.is_private).collect())
        }
    }

    pub async fn update_category(
        &self,
        id: &str,
        req: UpdateCategoryRequest,
        actor: Option<&str>,
    ) -> ForumResult<ForumCategoryRecord> {
        let mut current = self.get_category(id).await?;
        let slug = req.slug.trim().to_lowercase();
        if slug.is_empty() {
            return Err(ForumError::Invalid("Category slug cannot be empty".into()));
        }
        let name = req.name.trim().to_string();
        if name.is_empty() {
            return Err(ForumError::Invalid("Category name cannot be empty".into()));
        }

        if slug != current.slug {
            if let Some(existing) = self.deps.db.get_category_by_slug(&slug).await? {
                if existing.id != id {
                    return Err(ForumError::Invalid(format!(
                        "Category slug '{}' is taken",
                        slug
                    )));
                }
            }
        }

        current.slug = slug;
        current.name = name;
        current.description = req.description.trim().to_string();
        current.position = req.position;
        current.is_private = req.is_private;
        current.updated_at = now();

        self.deps.db.update_category(&current).await?;

        self.record_audit(
            AuditAction::ForumCategoryUpdate,
            AuditOutcome::Success,
            actor.map(|s| s.to_string()),
            "forum_category",
            Some(id.to_string()),
            None,
            None,
            json!({ "slug": current.slug }),
        )
        .await;

        Ok(current)
    }

    pub async fn delete_category(&self, id: &str, actor: Option<&str>) -> ForumResult<()> {
        let cat = self.get_category(id).await?;
        let deleted = self.deps.db.delete_category(id).await?;
        if !deleted {
            return Err(ForumError::NotFound(format!("Category {} not found", id)));
        }

        self.record_audit(
            AuditAction::ForumCategoryDelete,
            AuditOutcome::Success,
            actor.map(|s| s.to_string()),
            "forum_category",
            Some(id.to_string()),
            None,
            None,
            json!({ "slug": cat.slug }),
        )
        .await;

        Ok(())
    }

    // ==========================================
    // スレッド管理
    // ==========================================

    pub async fn create_thread(
        &self,
        req: CreateThreadRequest,
    ) -> ForumResult<(ForumThreadRecord, ForumPostRecord)> {
        self.check_enabled()?;

        if !self.config.allow_anonymous && req.author_id.is_none() {
            return Err(ForumError::Forbidden(
                "Anonymous posting is disabled. You must log in.".into(),
            ));
        }

        let title = req.title.trim().to_string();
        let title_len = title.chars().count();
        if title_len < self.config.title_min_length {
            return Err(ForumError::Invalid(format!(
                "Title is too short (minimum {} characters)",
                self.config.title_min_length
            )));
        }
        if title_len > self.config.title_max_length {
            return Err(ForumError::Invalid(format!(
                "Title is too long (maximum {} characters)",
                self.config.title_max_length
            )));
        }

        let body = req.body.trim().to_string();
        let body_len = body.chars().count();
        if body_len < self.config.body_min_length {
            return Err(ForumError::Invalid(format!(
                "Body is too short (minimum {} characters)",
                self.config.body_min_length
            )));
        }
        if body_len > self.config.body_max_length {
            return Err(ForumError::Invalid(format!(
                "Body is too long (maximum {} characters)",
                self.config.body_max_length
            )));
        }

        let author_name = if req.author_name.trim().is_empty() {
            "Anonymous".to_string()
        } else {
            moderation::normalize_name(&req.author_name).map_err(ForumError::Invalid)?
        };

        // レートリミットチェック
        let rate_id = req
            .author_id
            .as_deref()
            .or(req.ip.as_deref())
            .unwrap_or("unknown");
        self.check_rate_limit(rate_id, req.ip.as_deref()).await?;

        // カテゴリ確認
        let cat = self.get_category(&req.category_id).await?;
        if cat.is_private && req.author_id.is_none() {
            return Err(ForumError::Forbidden(
                "Cannot post to private category".into(),
            ));
        }

        let (mod_status, mod_reason) = self
            .evaluate_moderation(
                &[&title, &body, &author_name],
                &author_name,
                req.author_id.as_deref(),
            )
            .await?;
        let status_str = mod_status.as_str().to_string();

        let thread_id = Uuid::new_v4().to_string();
        let post_id = Uuid::new_v4().to_string();
        let timestamp = now();

        let thread = ForumThreadRecord {
            id: thread_id.clone(),
            category_id: req.category_id,
            title: title.clone(),
            author_id: req.author_id.clone(),
            author_name: author_name.clone(),
            is_pinned: false,
            is_locked: false,
            status: status_str.clone(),
            post_count: 1,
            last_post_at: timestamp.clone(),
            created_at: timestamp.clone(),
            updated_at: timestamp.clone(),
        };

        let first_post = ForumPostRecord {
            id: post_id,
            thread_id: thread_id.clone(),
            post_number: 1,
            author_id: req.author_id.clone(),
            author_name,
            body,
            reply_to_post_id: None,
            reply_to_number: None,
            status: status_str.clone(),
            is_deleted: false,
            ip: req.ip.clone(),
            user_agent: req.user_agent.clone(),
            created_at: timestamp.clone(),
            updated_at: timestamp,
        };

        self.deps.db.insert_thread(&thread, &first_post).await?;
        self.record_rate_limit(rate_id).await?;

        self.record_audit(
            AuditAction::ForumThreadCreate,
            AuditOutcome::Success,
            req.author_id,
            "forum_thread",
            Some(thread_id),
            req.ip,
            req.user_agent,
            json!({
                "title": title,
                "status": status_str,
                "reason": mod_reason,
            }),
        )
        .await;

        self.notify_post(&cat, &thread, &first_post, false, mod_status)
            .await;

        Ok((thread, first_post))
    }

    pub async fn get_thread(&self, id: &str) -> ForumResult<ForumThreadRecord> {
        self.deps
            .db
            .get_thread(id)
            .await?
            .ok_or_else(|| ForumError::NotFound(format!("Thread {} not found", id)))
    }

    pub async fn list_threads(
        &self,
        query: &ThreadListQuery,
    ) -> ForumResult<Vec<ForumThreadRecord>> {
        let mut q = query.clone();
        if q.limit == 0 {
            q.limit = self.config.default_page_size;
        }
        let threads = self.deps.db.list_threads(&q).await?;
        Ok(threads)
    }

    pub async fn set_thread_pinned(
        &self,
        id: &str,
        pinned: bool,
        actor: Option<&str>,
    ) -> ForumResult<()> {
        let updated = self.deps.db.set_thread_pinned(id, pinned, &now()).await?;
        if !updated {
            return Err(ForumError::NotFound(format!("Thread {} not found", id)));
        }

        self.record_audit(
            AuditAction::ForumThreadPin,
            AuditOutcome::Success,
            actor.map(|s| s.to_string()),
            "forum_thread",
            Some(id.to_string()),
            None,
            None,
            json!({ "pinned": pinned }),
        )
        .await;

        Ok(())
    }

    pub async fn set_thread_locked(
        &self,
        id: &str,
        locked: bool,
        actor: Option<&str>,
    ) -> ForumResult<()> {
        let updated = self.deps.db.set_thread_locked(id, locked, &now()).await?;
        if !updated {
            return Err(ForumError::NotFound(format!("Thread {} not found", id)));
        }

        self.record_audit(
            AuditAction::ForumThreadLock,
            AuditOutcome::Success,
            actor.map(|s| s.to_string()),
            "forum_thread",
            Some(id.to_string()),
            None,
            None,
            json!({ "locked": locked }),
        )
        .await;

        Ok(())
    }

    pub async fn moderate_thread(
        &self,
        id: &str,
        status: ModerationStatus,
        actor: Option<&str>,
    ) -> ForumResult<ForumThreadRecord> {
        let mut thread = self.get_thread(id).await?;
        let prev_status = thread.status.clone();
        let timestamp = now();
        let status_str = status.as_str().to_string();
        self.deps
            .db
            .set_thread_status(id, &status_str, &timestamp)
            .await?;
        thread.status = status_str.clone();
        thread.updated_at = timestamp;

        self.record_audit(
            AuditAction::ForumModerate,
            AuditOutcome::Success,
            actor.map(|s| s.to_string()),
            "forum_thread",
            Some(id.to_string()),
            None,
            None,
            json!({ "from": prev_status, "to": status_str, "title": thread.title }),
        )
        .await;

        Ok(thread)
    }

    pub async fn delete_thread(&self, id: &str, actor: Option<&str>) -> ForumResult<()> {
        let thread = self.get_thread(id).await?;
        let deleted = self.deps.db.delete_thread(id).await?;
        if !deleted {
            return Err(ForumError::NotFound(format!("Thread {} not found", id)));
        }

        self.record_audit(
            AuditAction::ForumThreadDelete,
            AuditOutcome::Success,
            actor.map(|s| s.to_string()),
            "forum_thread",
            Some(id.to_string()),
            None,
            None,
            json!({ "title": thread.title }),
        )
        .await;

        Ok(())
    }

    // ==========================================
    // 投稿（レス）管理
    // ==========================================

    pub async fn create_post(&self, req: CreatePostRequest) -> ForumResult<ForumPostRecord> {
        self.check_enabled()?;

        if !self.config.allow_anonymous && req.author_id.is_none() {
            return Err(ForumError::Forbidden(
                "Anonymous posting is disabled. You must log in.".into(),
            ));
        }

        let body = req.body.trim().to_string();
        let body_len = body.chars().count();
        if body_len < self.config.body_min_length {
            return Err(ForumError::Invalid(format!(
                "Body is too short (minimum {} characters)",
                self.config.body_min_length
            )));
        }
        if body_len > self.config.body_max_length {
            return Err(ForumError::Invalid(format!(
                "Body is too long (maximum {} characters)",
                self.config.body_max_length
            )));
        }

        let author_name = if req.author_name.trim().is_empty() {
            "Anonymous".to_string()
        } else {
            moderation::normalize_name(&req.author_name).map_err(ForumError::Invalid)?
        };

        // レートリミットチェック
        let rate_id = req
            .author_id
            .as_deref()
            .or(req.ip.as_deref())
            .unwrap_or("unknown");
        self.check_rate_limit(rate_id, req.ip.as_deref()).await?;

        // スレッドチェック（存在＆ロック状態）
        let thread = self.get_thread(&req.thread_id).await?;
        if thread.is_locked {
            return Err(ForumError::Locked);
        }
        let cat = self.get_category(&thread.category_id).await?;

        let (mod_status, mod_reason) = self
            .evaluate_moderation(
                &[&body, &author_name],
                &author_name,
                req.author_id.as_deref(),
            )
            .await?;
        let status_str = mod_status.as_str().to_string();

        let post_id = Uuid::new_v4().to_string();
        let timestamp = now();

        let mut post = ForumPostRecord {
            id: post_id.clone(),
            thread_id: req.thread_id.clone(),
            post_number: 0,
            author_id: req.author_id.clone(),
            author_name,
            body,
            reply_to_post_id: req.reply_to_post_id,
            reply_to_number: req.reply_to_number,
            status: status_str.clone(),
            is_deleted: false,
            ip: req.ip.clone(),
            user_agent: req.user_agent.clone(),
            created_at: timestamp.clone(),
            updated_at: timestamp.clone(),
        };

        let new_num = self.deps.db.insert_post(&mut post).await?;
        self.record_rate_limit(rate_id).await?;

        // 自動ロック判定（上限レス数に達した場合）
        if self.config.auto_lock_post_count > 0 && new_num >= self.config.auto_lock_post_count {
            let _ = self
                .deps
                .db
                .set_thread_locked(&req.thread_id, true, &timestamp)
                .await;
        }

        self.record_audit(
            AuditAction::ForumPostCreate,
            AuditOutcome::Success,
            req.author_id,
            "forum_post",
            Some(post_id),
            req.ip,
            req.user_agent,
            json!({
                "thread_id": req.thread_id,
                "post_number": new_num,
                "status": status_str,
                "reason": mod_reason,
            }),
        )
        .await;

        self.notify_post(&cat, &thread, &post, true, mod_status)
            .await;

        Ok(post)
    }

    pub async fn get_post(&self, id: &str) -> ForumResult<ForumPostRecord> {
        self.deps
            .db
            .get_post(id)
            .await?
            .ok_or_else(|| ForumError::NotFound(format!("Post {} not found", id)))
    }

    pub async fn list_posts(&self, query: &PostListQuery) -> ForumResult<Vec<ForumPostRecord>> {
        let mut q = query.clone();
        if q.limit == 0 {
            q.limit = self.config.default_page_size;
        }
        let posts = self.deps.db.list_posts(&q).await?;
        Ok(posts)
    }

    pub async fn moderate_post(
        &self,
        id: &str,
        status: ModerationStatus,
        actor: Option<&str>,
    ) -> ForumResult<ForumPostRecord> {
        let mut post = self.get_post(id).await?;
        let prev_status = post.status.clone();
        let timestamp = now();
        let status_str = status.as_str().to_string();
        self.deps
            .db
            .set_post_status(id, &status_str, &timestamp)
            .await?;
        post.status = status_str.clone();
        post.updated_at = timestamp;

        self.record_audit(
            AuditAction::ForumModerate,
            AuditOutcome::Success,
            actor.map(|s| s.to_string()),
            "forum_post",
            Some(id.to_string()),
            None,
            None,
            json!({
                "thread_id": post.thread_id,
                "post_number": post.post_number,
                "from": prev_status,
                "to": status_str,
            }),
        )
        .await;

        Ok(post)
    }

    pub async fn delete_post(&self, id: &str, actor: Option<&str>) -> ForumResult<()> {
        let post = self.get_post(id).await?;
        let deleted = self.deps.db.delete_post(id, &now()).await?;
        if !deleted {
            return Err(ForumError::NotFound(format!("Post {} not found", id)));
        }

        self.record_audit(
            AuditAction::ForumPostDelete,
            AuditOutcome::Success,
            actor.map(|s| s.to_string()),
            "forum_post",
            Some(id.to_string()),
            None,
            None,
            json!({ "thread_id": post.thread_id, "post_number": post.post_number }),
        )
        .await;

        Ok(())
    }

    // ==========================================
    // 通報（Report）管理
    // ==========================================

    pub async fn report(&self, req: CreateReportRequest) -> ForumResult<ForumReportRecord> {
        self.check_enabled()?;

        let reason = req.reason.trim().to_string();
        if reason.is_empty() {
            return Err(ForumError::Invalid("Report reason cannot be empty".into()));
        }
        if reason.chars().count() > MAX_REPORT_REASON_CHARS {
            return Err(ForumError::Invalid(format!(
                "Report reason is too long (maximum {} characters)",
                MAX_REPORT_REASON_CHARS
            )));
        }

        if !moderation::within_rate_limit(
            self.deps.cache.as_ref(),
            "forum_report",
            req.reporter_ip.as_deref(),
            self.config.moderation.max_per_ip_per_hour,
        )
        .await?
        {
            return Err(ForumError::RateLimited);
        }

        let (thread, post_author, post_body) = match req.target_type.as_str() {
            "thread" => {
                let th = self.get_thread(&req.target_id).await?;
                let posts = self
                    .deps
                    .db
                    .list_posts(&PostListQuery {
                        thread_id: th.id.clone(),
                        after_post_number: 0,
                        status: None,
                        limit: 1,
                    })
                    .await?;
                let body = posts
                    .first()
                    .map(|p| p.body.clone())
                    .unwrap_or_else(|| th.title.clone());
                let author = th.author_name.clone();
                (th, author, body)
            }
            "post" => {
                let p = self.get_post(&req.target_id).await?;
                let th = self.get_thread(&p.thread_id).await?;
                (th, p.author_name, p.body)
            }
            other => {
                return Err(ForumError::Invalid(format!(
                    "Invalid report target_type '{}'",
                    other
                )))
            }
        };
        let cat = self.get_category(&thread.category_id).await?;

        let timestamp = now();
        let report_id = Uuid::new_v4().to_string();
        let record = ForumReportRecord {
            id: report_id.clone(),
            target_type: req.target_type.clone(),
            target_id: req.target_id.clone(),
            thread_id: thread.id.clone(),
            reason: reason.clone(),
            reporter_id: req.reporter_id.clone(),
            reporter_ip: req.reporter_ip.clone(),
            status: "open".to_string(),
            resolved_by: None,
            created_at: timestamp.clone(),
            updated_at: timestamp,
        };

        self.deps.db.insert_report(&record).await?;

        self.record_audit(
            AuditAction::ForumReportCreate,
            AuditOutcome::Success,
            req.reporter_id,
            "forum_report",
            Some(report_id),
            req.reporter_ip.clone(),
            None,
            json!({
                "target_type": req.target_type,
                "target_id": req.target_id,
                "thread_id": thread.id,
                "reason": reason,
            }),
        )
        .await;

        if self.config.notify_staff {
            if let Some(notifier) = &self.deps.notifier {
                notifier.notify(NotificationEvent {
                    kind: "forum.reported".to_string(),
                    recipients: vec![Recipient::Staff],
                    data: json!({
                        "category": {
                            "id": cat.id,
                            "name": cat.name,
                            "slug": cat.slug,
                        },
                        "thread": {
                            "id": thread.id,
                            "title": thread.title,
                        },
                        "post": {
                            "author_name": post_author,
                            "body": post_body,
                        },
                        "report": {
                            "id": record.id,
                            "target_type": record.target_type,
                            "target_id": record.target_id,
                            "reason": record.reason,
                            "reporter_ip": record.reporter_ip,
                        },
                        "thread_url": self.thread_url(&cat.slug, &thread.id),
                        "moderation_url": self.moderation_url(),
                    }),
                });
            }
        }

        Ok(record)
    }

    pub async fn list_reports(
        &self,
        status: Option<&str>,
        limit: u64,
    ) -> ForumResult<Vec<ForumReportRecord>> {
        let limit = if limit == 0 { 100 } else { limit.min(500) };
        Ok(self.deps.db.list_reports(status, limit).await?)
    }

    pub async fn resolve_report(
        &self,
        id: &str,
        status: &str,
        actor: Option<&str>,
    ) -> ForumResult<ForumReportRecord> {
        if !matches!(status, "resolved" | "dismissed" | "open") {
            return Err(ForumError::Invalid(format!(
                "Invalid report status '{}' (expected 'resolved', 'dismissed', or 'open')",
                status
            )));
        }
        let mut report = self
            .deps
            .db
            .get_report(id)
            .await?
            .ok_or_else(|| ForumError::NotFound(format!("Report {} not found", id)))?;
        let prev_status = report.status.clone();
        let timestamp = now();
        self.deps
            .db
            .set_report_status(id, status, actor, &timestamp)
            .await?;
        report.status = status.to_string();
        report.resolved_by = actor.map(|s| s.to_string());
        report.updated_at = timestamp;

        self.record_audit(
            AuditAction::ForumReportResolve,
            AuditOutcome::Success,
            actor.map(|s| s.to_string()),
            "forum_report",
            Some(id.to_string()),
            None,
            None,
            json!({
                "from": prev_status,
                "to": status,
                "target_type": report.target_type,
                "target_id": report.target_id,
            }),
        )
        .await;

        Ok(report)
    }
}
