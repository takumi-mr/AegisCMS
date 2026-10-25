//! 掲示板（`[forum]`）の設定

use aegis_core::{AegisError, Result};
use serde::{Deserialize, Serialize};

/// タイトルの最大文字数の上限
const MAX_TITLE_LIMIT: usize = 1_000;
/// 本文の最大文字数の上限
const MAX_BODY_LIMIT: usize = 100_000;

/// 掲示板（`[forum]`）の設定
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct ForumConfig {
    pub enabled: bool,
    /// スレッドタイトルの最小文字数
    pub title_min_length: usize,
    /// スレッドタイトルの最大文字数
    pub title_max_length: usize,
    /// 投稿本文の最小文字数
    pub body_min_length: usize,
    /// 投稿本文の最大文字数
    pub body_max_length: usize,
    /// 連続投稿制限（秒）。同じ IP またはユーザーからの投稿間隔
    pub rate_limit_secs: u64,
    /// 匿名（未ログイン）投稿を許可するか
    pub allow_anonymous: bool,
    /// 指定レス数に達したら自動でスレッドをロックするか（0 は無制限、例: 1000）
    pub auto_lock_post_count: u64,
    /// 1ページあたりの既定の表示件数
    pub default_page_size: u64,
}

impl Default for ForumConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            title_min_length: 2,
            title_max_length: 200,
            body_min_length: 1,
            body_max_length: 10_000,
            rate_limit_secs: 5,
            allow_anonymous: true,
            auto_lock_post_count: 1000,
            default_page_size: 50,
        }
    }
}

impl ForumConfig {
    pub fn validate(&self) -> Result<()> {
        if !self.enabled {
            return Ok(());
        }
        if self.title_min_length == 0 {
            return Err(AegisError::Config(
                "[forum].title_min_length must be at least 1".to_string(),
            ));
        }
        if self.title_max_length < self.title_min_length {
            return Err(AegisError::Config(
                "[forum].title_max_length must be >= title_min_length".to_string(),
            ));
        }
        if self.title_max_length > MAX_TITLE_LIMIT {
            return Err(AegisError::Config(format!(
                "[forum].title_max_length must not exceed {}",
                MAX_TITLE_LIMIT
            )));
        }
        if self.body_min_length == 0 {
            return Err(AegisError::Config(
                "[forum].body_min_length must be at least 1".to_string(),
            ));
        }
        if self.body_max_length < self.body_min_length {
            return Err(AegisError::Config(
                "[forum].body_max_length must be >= body_min_length".to_string(),
            ));
        }
        if self.body_max_length > MAX_BODY_LIMIT {
            return Err(AegisError::Config(format!(
                "[forum].body_max_length must not exceed {}",
                MAX_BODY_LIMIT
            )));
        }
        if self.default_page_size == 0 || self.default_page_size > 200 {
            return Err(AegisError::Config(
                "[forum].default_page_size must be between 1 and 200".to_string(),
            ));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_forum_config_defaults_and_validation() {
        let mut cfg = ForumConfig::default();
        assert!(cfg.validate().is_ok());

        cfg.enabled = true;
        assert!(cfg.validate().is_ok());

        cfg.title_min_length = 0;
        assert!(cfg.validate().is_err());

        cfg.title_min_length = 50;
        cfg.title_max_length = 20;
        assert!(cfg.validate().is_err());

        cfg.title_max_length = 200;
        cfg.body_max_length = 150_000;
        assert!(cfg.validate().is_err());
    }
}
