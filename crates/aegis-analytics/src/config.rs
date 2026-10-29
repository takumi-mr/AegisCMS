//! `[analytics]` の設定

use serde::{Deserialize, Serialize};

/// アクセス解析（`[analytics]`）
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct AnalyticsConfig {
    /// 公開ページにビーコンを入れて集計する
    pub enabled: bool,
    /// 集計を残す日数（0 は消さない）
    pub retention_days: u32,
    /// 溜めた集計を DB に書く間隔（秒）
    pub flush_interval_secs: u64,
    /// 集計しないパス（前方一致。`/preview` なら `/preview` と `/preview/...`）
    pub exclude_paths: Vec<String>,
}

impl Default for AnalyticsConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            retention_days: 400,
            flush_interval_secs: 30,
            exclude_paths: Vec::new(),
        }
    }
}

impl AnalyticsConfig {
    pub fn validate(&self) -> Result<(), String> {
        for path in &self.exclude_paths {
            if !path.starts_with('/') {
                return Err(format!(
                    "[analytics].exclude_paths entries must start with '/': {:?}",
                    path
                ));
            }
        }
        if self.flush_interval_secs == 0 {
            return Err("[analytics].flush_interval_secs must be 1 or more".to_string());
        }
        Ok(())
    }

    /// `path` を集計しないか
    pub fn excludes(&self, path: &str) -> bool {
        self.exclude_paths.iter().any(|prefix| {
            let prefix = prefix.trim_end_matches('/');
            prefix.is_empty()
                || path == prefix
                || path
                    .strip_prefix(prefix)
                    .is_some_and(|rest| rest.starts_with('/'))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_excludes_by_path_segment() {
        let config = AnalyticsConfig {
            exclude_paths: vec!["/preview/".to_string()],
            ..AnalyticsConfig::default()
        };
        assert!(config.excludes("/preview"));
        assert!(config.excludes("/preview/a"));
        assert!(!config.excludes("/previews"));
        assert!(!config.excludes("/"));
    }

    #[test]
    fn test_validate() {
        assert!(AnalyticsConfig::default().validate().is_ok());
        let bad = AnalyticsConfig {
            exclude_paths: vec!["preview".to_string()],
            ..AnalyticsConfig::default()
        };
        assert!(bad.validate().is_err());
    }
}
