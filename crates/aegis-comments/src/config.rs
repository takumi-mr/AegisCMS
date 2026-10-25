//! `[comments]` の設定

use aegis_core::{AegisError, Result};
use serde::{Deserialize, Serialize};

use crate::moderation::ModerationConfig;

/// 本文の最大文字数の上限（設定できる最大値）
const MAX_LENGTH_LIMIT: usize = 100_000;

/// Gravatar の `d`（メールアドレスに画像が無いときの画像）に使える値
const GRAVATAR_DEFAULTS: [&str; 8] = [
    "mp",
    "identicon",
    "monsterid",
    "wavatar",
    "retro",
    "robohash",
    "blank",
    "404",
];

/// 投稿者のアバター画像の取得元
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AvatarProvider {
    /// 画像を出さない（読者のブラウザが外部に接続しない）
    #[default]
    None,
    /// Gravatar（メールアドレスの SHA-256 でハッシュを作り、読者のブラウザが gravatar.com から取得する）
    Gravatar,
}

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
    /// 投稿者のアバター画像（`none` / `gravatar`）
    pub avatars: AvatarProvider,
    /// Gravatar に画像が無い人の画像（`mp` / `identicon` / `monsterid` / `wavatar` / `retro` /
    /// `robohash` / `blank` / `404`）
    pub avatar_default: String,
    /// 同じコンテンツに同じ人（メールアドレスか IP アドレスが同じ）が同じ本文を投稿したら断る
    pub reject_duplicates: bool,
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
            avatars: AvatarProvider::None,
            avatar_default: "identicon".to_string(),
            reject_duplicates: true,
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
        if !GRAVATAR_DEFAULTS.contains(&self.avatar_default.as_str()) {
            return Err(AegisError::Config(format!(
                "[comments].avatar_default must be one of: {}",
                GRAVATAR_DEFAULTS.join(", ")
            )));
        }
        self.moderation.validate("[comments.moderation]")
    }

    /// 投稿者のアバター画像の URL（`avatars = "none"`・メールアドレスが無いときは `None`）。
    /// `email` は小文字に揃えたもの
    pub fn avatar_url(&self, email: &str) -> Option<String> {
        use sha2::{Digest, Sha256};
        if self.avatars != AvatarProvider::Gravatar || email.is_empty() {
            return None;
        }
        let hash: String = Sha256::digest(email.as_bytes())
            .iter()
            .map(|b| format!("{:02x}", b))
            .collect();
        Some(format!(
            "https://www.gravatar.com/avatar/{}?s=96&d={}&r=g",
            hash, self.avatar_default
        ))
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
                avatar_default: "javascript:".to_string(),
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

    #[test]
    fn test_avatar_url() {
        let mut config = CommentsConfig::default();
        assert_eq!(config.avatar_url("hanako@example.com"), None);
        config.avatars = AvatarProvider::Gravatar;
        assert_eq!(config.avatar_url(""), None);
        let url = config.avatar_url("hanako@example.com").unwrap();
        assert!(
            url.starts_with("https://www.gravatar.com/avatar/"),
            "{}",
            url
        );
        // SHA-256 は 64 桁。メールアドレスそのものは URL に入らない
        let hash = url.rsplit('/').next().unwrap().split('?').next().unwrap();
        assert_eq!(hash.len(), 64);
        assert!(!url.contains("hanako"));
        assert!(url.ends_with("&d=identicon&r=g"));

        let parsed: CommentsConfig = toml::from_str(
            r#"avatars = "gravatar"
avatar_default = "retro""#,
        )
        .unwrap();
        assert_eq!(parsed.avatars, AvatarProvider::Gravatar);
        assert!(parsed.reject_duplicates);
    }
}
