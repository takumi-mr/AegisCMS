//! `[comments]` の設定

use aegis_core::{AegisError, Result};
use serde::{Deserialize, Serialize};

use crate::moderation::ModerationConfig;

/// 本文の最大文字数の上限（設定できる最大値）
const MAX_LENGTH_LIMIT: usize = 100_000;

/// 読者のコメント（`[comments]`）
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct CommentsConfig {
    pub enabled: bool,
    /// コメントを受け付けるコンテンツの種類（スキーマの `content_type`）
    pub content_types: Vec<String>,
    /// 返信できる深さ（0 は返信なし。1 ならトップレベルへの返信まで）
    pub max_depth: u32,
    /// 本文の最大文字数
    pub max_length: usize,
    /// 公開からこの日数を過ぎたら新しいコメントを受け付けない（0 は閉じない）
    pub close_after_days: u32,
    /// コメントが付いたこと（承認待ちを含む）をコンテンツの著者に知らせる
    pub notify_author: bool,
    /// 承認待ちのコメントを運営者（`[notifications.email].staff_recipients` など）に知らせる
    pub notify_staff: bool,
    pub moderation: ModerationConfig,
}

impl Default for CommentsConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            content_types: vec!["article".to_string()],
            max_depth: 3,
            max_length: 5000,
            close_after_days: 0,
            notify_author: true,
            notify_staff: true,
            moderation: ModerationConfig::default(),
        }
    }
}

impl CommentsConfig {
    pub fn validate(&self) -> Result<()> {
        if !self.enabled {
            return Ok(());
        }
        if self.content_types.is_empty() {
            return Err(AegisError::Config(
                "[comments].content_types: list at least one content type".to_string(),
            ));
        }
        if self.content_types.iter().any(|t| t.trim().is_empty()) {
            return Err(AegisError::Config(
                "[comments].content_types must not contain empty names".to_string(),
            ));
        }
        if self.max_length == 0 || self.max_length > MAX_LENGTH_LIMIT {
            return Err(AegisError::Config(format!(
                "[comments].max_length must be between 1 and {}",
                MAX_LENGTH_LIMIT
            )));
        }
        if self.max_depth > 10 {
            return Err(AegisError::Config(
                "[comments].max_depth must be 10 or less".to_string(),
            ));
        }
        self.moderation.validate("[comments.moderation]")
    }

    pub fn accepts(&self, content_type: &str) -> bool {
        self.enabled && self.content_types.iter().any(|t| t == content_type)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_and_validate() {
        let config: CommentsConfig = toml::from_str(
            r#"
            enabled = true
            content_types = ["article", "product"]
            max_depth = 0

            [moderation]
            require_approval = false
            spam_words = ["casino"]
            "#,
        )
        .unwrap();
        assert!(config.validate().is_ok());
        assert!(config.accepts("product"));
        assert!(!config.accepts("page"));
        assert!(!config.moderation.require_approval);
        assert!(config.moderation.auto_approve_known);
        assert_eq!(config.max_length, 5000);

        let disabled = CommentsConfig::default();
        assert!(!disabled.accepts("article"));

        for bad in [
            CommentsConfig {
                enabled: true,
                content_types: vec![],
                ..Default::default()
            },
            CommentsConfig {
                enabled: true,
                max_length: 0,
                ..Default::default()
            },
            CommentsConfig {
                enabled: true,
                moderation: ModerationConfig {
                    hold_words: vec![" ".to_string()],
                    ..Default::default()
                },
                ..Default::default()
            },
        ] {
            assert!(bad.validate().is_err(), "{:?}", bad);
        }
    }
}
