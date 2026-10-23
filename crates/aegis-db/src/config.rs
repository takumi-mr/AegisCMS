//! `[database]` の設定

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DatabaseBackendKind {
    #[default]
    Sqlite,
    Postgres,
    Mysql,
}

impl DatabaseBackendKind {
    pub fn infer_from_url(url: &str) -> Self {
        let lower = url.to_ascii_lowercase();
        if lower.starts_with("postgres://") || lower.starts_with("postgresql://") {
            Self::Postgres
        } else if lower.starts_with("mysql://") || lower.starts_with("mariadb://") {
            Self::Mysql
        } else {
            Self::Sqlite
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct DatabaseConfig {
    #[serde(default)]
    pub backend: DatabaseBackendKind,
    pub url: String,
    pub max_connections: u32,
}

impl Default for DatabaseConfig {
    fn default() -> Self {
        Self {
            backend: DatabaseBackendKind::Sqlite,
            url: "sqlite://aegis.db?mode=rwc".to_string(),
            max_connections: 10,
        }
    }
}

impl DatabaseConfig {
    pub fn resolved_backend(&self) -> DatabaseBackendKind {
        // If explicit non-default or matches, or infer from URL
        if self.backend != DatabaseBackendKind::Sqlite {
            self.backend
        } else {
            DatabaseBackendKind::infer_from_url(&self.url)
        }
    }
}
