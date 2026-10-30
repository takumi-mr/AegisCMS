//! タグ（`[tags]`）: 名前と slug を持つ管理されたタグを、どのコンテンツ（記事・固定ページ・商品など）にも
//! 付けられる。HTTP 層（API・公開ページ）はサーバー側にあり、ここは HTTP に依存しない。
//! タグの表は `_aegis_tags`・`_aegis_content_tags`（`aegis_db::TagStorage`）
//!
//! - slug は半角英数字とハイフン（`/tags/{slug}` の URL になる）。名前から作れるとき（半角英数字を含む）は
//!   省略でき、作れないとき（日本語だけの名前など）は `tag-` に乱数をつけた slug になる
//! - 名前は 50 文字まで。大文字小文字を区別せず、同じ名前のタグは作れない
//! - 1 つのコンテンツに付けられる数は [`TagsConfig::max_per_content`]

pub mod config;

use std::sync::Arc;

use aegis_core::AegisError;
use aegis_db::{DatabaseAdapter, TagListQuery, TagPage, TagRecord};
use chrono::{SecondsFormat, Utc};
use uuid::Uuid;

pub use config::TagsConfig;

/// 名前の最大文字数
pub const MAX_NAME_CHARS: usize = 50;
/// slug の最大文字数
pub const MAX_SLUG_CHARS: usize = 64;
/// 説明の最大文字数
pub const MAX_DESCRIPTION_CHARS: usize = 500;

#[derive(Debug, thiserror::Error)]
pub enum TagError {
    /// 入力の不備（400）
    #[error("{0}")]
    Invalid(String),
    /// タグが無い（404）
    #[error("{0}")]
    NotFound(String),
    /// 同じ slug・名前のタグがある（409）
    #[error("{0}")]
    Conflict(String),
    #[error("database error: {0}")]
    Database(#[from] AegisError),
}

pub type Result<T> = std::result::Result<T, TagError>;

/// 新しいタグ・更新で受け取る値
#[derive(Debug, Clone, Default)]
pub struct TagInput {
    pub name: String,
    /// 省略（空）なら名前から作る
    pub slug: Option<String>,
    pub description: Option<String>,
}

pub struct TagService {
    db: Arc<dyn DatabaseAdapter>,
    config: TagsConfig,
}

fn now() -> String {
    Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true)
}

/// 名前から slug の候補を作る（半角英数字を小文字にして、それ以外の並びを `-` にする）。作れなければ空
pub fn slugify(name: &str) -> String {
    let mut out = String::new();
    let mut pending_dash = false;
    for c in name.chars() {
        if c.is_ascii_alphanumeric() {
            if pending_dash && !out.is_empty() {
                out.push('-');
            }
            pending_dash = false;
            out.push(c.to_ascii_lowercase());
        } else {
            pending_dash = true;
        }
    }
    out.chars()
        .take(MAX_SLUG_CHARS)
        .collect::<String>()
        .trim_end_matches('-')
        .to_string()
}

/// slug として使える形か（半角英数字と、その間のハイフン）
pub fn is_valid_slug(slug: &str) -> bool {
    !slug.is_empty()
        && slug.chars().count() <= MAX_SLUG_CHARS
        && !slug.starts_with('-')
        && !slug.ends_with('-')
        && !slug.contains("--")
        && slug
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

fn clean_name(name: &str) -> Result<String> {
    let name = name.split_whitespace().collect::<Vec<_>>().join(" ");
    if name.is_empty() {
        return Err(TagError::Invalid("The tag name is required".to_string()));
    }
    if name.chars().count() > MAX_NAME_CHARS {
        return Err(TagError::Invalid(format!(
            "The tag name is too long ({} characters at most)",
            MAX_NAME_CHARS
        )));
    }
    if name.chars().any(char::is_control) {
        return Err(TagError::Invalid(
            "The tag name has control characters".to_string(),
        ));
    }
    Ok(name)
}

fn clean_description(description: Option<&str>) -> Result<Option<String>> {
    let Some(description) = description.map(str::trim).filter(|d| !d.is_empty()) else {
        return Ok(None);
    };
    if description.chars().count() > MAX_DESCRIPTION_CHARS {
        return Err(TagError::Invalid(format!(
            "The description is too long ({} characters at most)",
            MAX_DESCRIPTION_CHARS
        )));
    }
    Ok(Some(description.to_string()))
}

impl TagService {
    pub fn new(db: Arc<dyn DatabaseAdapter>, config: TagsConfig) -> Self {
        Self { db, config }
    }

    pub fn config(&self) -> &TagsConfig {
        &self.config
    }

    /// 名前で探して、大文字小文字を区別せず同じ名前のタグ（`except` 以外）があるか
    async fn name_taken(&self, name: &str, except: Option<&str>) -> Result<bool> {
        let page = self
            .db
            .list_tags(&TagListQuery {
                search: Some(name.to_string()),
                ..Default::default()
            })
            .await?;
        let wanted = name.to_lowercase();
        Ok(page
            .tags
            .iter()
            .any(|t| t.tag.name.to_lowercase() == wanted && Some(t.tag.id.as_str()) != except))
    }

    async fn resolve_slug(&self, input: &TagInput, name: &str) -> Result<String> {
        match input
            .slug
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
        {
            Some(slug) => {
                if !is_valid_slug(slug) {
                    return Err(TagError::Invalid(format!(
                        "The slug must be lowercase letters, digits and hyphens ({} characters at most)",
                        MAX_SLUG_CHARS
                    )));
                }
                Ok(slug.to_string())
            }
            None => {
                let slug = slugify(name);
                if slug.is_empty() {
                    Ok(format!("tag-{}", &Uuid::new_v4().simple().to_string()[..8]))
                } else {
                    Ok(slug)
                }
            }
        }
    }

    pub async fn create(&self, input: TagInput) -> Result<TagRecord> {
        let name = clean_name(&input.name)?;
        let slug = self.resolve_slug(&input, &name).await?;
        let description = clean_description(input.description.as_deref())?;
        if self.name_taken(&name, None).await? {
            return Err(TagError::Conflict(format!(
                "A tag named '{}' already exists",
                name
            )));
        }
        if self.db.get_tag_by_slug(&slug).await?.is_some() {
            return Err(TagError::Conflict(format!(
                "A tag with the slug '{}' already exists",
                slug
            )));
        }
        let at = now();
        let tag = TagRecord {
            id: Uuid::new_v4().to_string(),
            slug,
            name,
            description,
            created_at: at.clone(),
            updated_at: at,
        };
        self.db.insert_tag(&tag).await?;
        Ok(tag)
    }

    pub async fn update(&self, id: &str, input: TagInput) -> Result<TagRecord> {
        let existing = self.get(id).await?;
        let name = clean_name(&input.name)?;
        // slug を省略したら、今の slug のまま（名前を変えても URL は変わらない）
        let slug = match input
            .slug
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
        {
            Some(_) => self.resolve_slug(&input, &name).await?,
            None => existing.slug.clone(),
        };
        let description = clean_description(input.description.as_deref())?;
        if self.name_taken(&name, Some(id)).await? {
            return Err(TagError::Conflict(format!(
                "A tag named '{}' already exists",
                name
            )));
        }
        if slug != existing.slug {
            if let Some(other) = self.db.get_tag_by_slug(&slug).await? {
                if other.id != id {
                    return Err(TagError::Conflict(format!(
                        "A tag with the slug '{}' already exists",
                        slug
                    )));
                }
            }
        }
        let tag = TagRecord {
            slug,
            name,
            description,
            updated_at: now(),
            ..existing
        };
        if !self.db.update_tag(&tag).await? {
            return Err(TagError::NotFound(format!("Tag '{}' not found", id)));
        }
        Ok(tag)
    }

    pub async fn get(&self, id: &str) -> Result<TagRecord> {
        self.db
            .get_tag(id)
            .await?
            .ok_or_else(|| TagError::NotFound(format!("Tag '{}' not found", id)))
    }

    pub async fn get_by_slug(&self, slug: &str) -> Result<TagRecord> {
        self.db
            .get_tag_by_slug(slug)
            .await?
            .ok_or_else(|| TagError::NotFound(format!("Tag '{}' not found", slug)))
    }

    pub async fn list(&self, query: &TagListQuery) -> Result<TagPage> {
        Ok(self.db.list_tags(query).await?)
    }

    /// タグを消す。付いていたコンテンツからも外れる
    pub async fn delete(&self, id: &str) -> Result<TagRecord> {
        let tag = self.get(id).await?;
        self.db.delete_tag(id).await?;
        Ok(tag)
    }

    pub async fn content_tags(
        &self,
        content_type: &str,
        content_id: &str,
    ) -> Result<Vec<TagRecord>> {
        Ok(self.db.list_content_tags(content_type, content_id).await?)
    }

    /// コンテンツのタグを `tag_ids` にする（重複は 1 つにまとめ、存在しないタグ・多すぎる数は断る）
    pub async fn set_content_tags(
        &self,
        content_type: &str,
        content_id: &str,
        tag_ids: &[String],
    ) -> Result<Vec<TagRecord>> {
        let mut unique: Vec<String> = Vec::new();
        for id in tag_ids {
            if !unique.contains(id) {
                unique.push(id.clone());
            }
        }
        if unique.len() > self.config.max_per_content {
            return Err(TagError::Invalid(format!(
                "Too many tags ({} at most per content)",
                self.config.max_per_content
            )));
        }
        for id in &unique {
            if self.db.get_tag(id).await?.is_none() {
                return Err(TagError::Invalid(format!("Tag '{}' does not exist", id)));
            }
        }
        self.db
            .set_content_tags(content_type, content_id, &unique, &now())
            .await?;
        self.content_tags(content_type, content_id).await
    }

    /// コンテンツを消したとき、付いていたタグを外す
    pub async fn forget_content(&self, content_type: &str, content_id: &str) -> Result<()> {
        self.db
            .delete_content_tags(content_type, content_id)
            .await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests;
