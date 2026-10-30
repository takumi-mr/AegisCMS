//! `[tags]` の設定

use aegis_core::{AegisError, Result};
use serde::{Deserialize, Serialize};

/// 1 つのコンテンツに付けられる数の上限（設定できる最大値）
const MAX_PER_CONTENT_LIMIT: usize = 100;

/// タグ（`[tags]`）
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct TagsConfig {
    pub enabled: bool,
    /// 1 つのコンテンツに付けられるタグの数
    pub max_per_content: usize,
}

impl Default for TagsConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            max_per_content: 20,
        }
    }
}

impl TagsConfig {
    pub fn validate(&self) -> Result<()> {
        if self.max_per_content == 0 || self.max_per_content > MAX_PER_CONTENT_LIMIT {
            return Err(AegisError::Config(format!(
                "[tags].max_per_content must be between 1 and {}",
                MAX_PER_CONTENT_LIMIT
            )));
        }
        Ok(())
    }
}
