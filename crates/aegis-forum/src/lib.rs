//! 掲示板（`[forum]`）: カテゴリ・スレッド・投稿（レス）・モデレーション・レートリミット。
//! HTTP 層（API・公開画面）とは疎結合で、ビジネスロジックとストレージ連携をカプセル化する。

pub mod config;
#[cfg(test)]
mod tests;

use std::sync::Arc;

use aegis_audit::AuditService;
use aegis_core::{AegisError, AuditAction, AuditActor, AuditEvent, AuditOutcome, CacheBackend};
use aegis_db::{
    DatabaseAdapter, ForumCategoryRecord, ForumPostRecord, ForumThreadRecord, PostListQuery,
    ThreadListQuery,
};
use chrono::{SecondsFormat, Utc};
use serde::{Deserialize, Serialize};
use serde_json::json;
use uuid::Uuid;

pub use config::ForumConfig;

/// レートリミットのキャッシュキー接頭辞
const RATE_LIMIT_PREFIX: &str = "forum:ratelimit:";

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
    async fn check_rate_limit(&self, identifier: &str) -> ForumResult<()> {
        if self.config.rate_limit_secs == 0 {
            return Ok(());
        }
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
            req.author_name.trim().to_string()
        };

        // レートリミットチェック
        let rate_id = req
            .author_id
            .as_deref()
            .or(req.ip.as_deref())
            .unwrap_or("unknown");
        self.check_rate_limit(rate_id).await?;

        // カテゴリ確認
        let cat = self.get_category(&req.category_id).await?;
        if cat.is_private && req.author_id.is_none() {
            return Err(ForumError::Forbidden(
                "Cannot post to private category".into(),
            ));
        }

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
            json!({ "title": title }),
        )
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
            req.author_name.trim().to_string()
        };

        // レートリミットチェック
        let rate_id = req
            .author_id
            .as_deref()
            .or(req.ip.as_deref())
            .unwrap_or("unknown");
        self.check_rate_limit(rate_id).await?;

        // スレッドチェック（存在＆ロック状態）
        let thread = self.get_thread(&req.thread_id).await?;
        if thread.is_locked {
            return Err(ForumError::Locked);
        }

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
            json!({ "thread_id": req.thread_id, "post_number": new_num }),
        )
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
}
