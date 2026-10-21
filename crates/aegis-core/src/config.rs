use crate::cache::CacheBackendKind;
use crate::error::{AegisError, Result};
use crate::storage::StorageBackendKind;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AegisConfig {
    #[serde(default)]
    pub server: ServerConfig,
    #[serde(default)]
    pub database: DatabaseConfig,
    #[serde(default)]
    pub storage: StorageConfig,
    #[serde(default)]
    pub cache: CacheConfig,
    #[serde(default)]
    pub paths: PathsConfig,
    #[serde(default)]
    pub runtime: RuntimeConfig,
    #[serde(default)]
    pub security: SecurityConfig,
    #[serde(default)]
    pub audit: AuditConfig,
}

impl AegisConfig {
    pub fn load_from_file<P: AsRef<Path>>(path: P) -> Result<Self> {
        let content = std::fs::read_to_string(path.as_ref())
            .map_err(|e| AegisError::Config(format!("Failed to read config file: {}", e)))?;
        let config: AegisConfig = toml::from_str(&content)
            .map_err(|e| AegisError::Config(format!("Failed to parse config file: {}", e)))?;
        Ok(config)
    }

    /// 設定ファイルが存在しない場合のみ既定値を返す。
    /// 読み込み・パースに失敗した場合は黙ってフォールバックせずエラーを返す。
    pub fn load_or_default<P: AsRef<Path>>(path: P) -> Result<Self> {
        let path = path.as_ref();
        if !path.exists() {
            tracing::warn!(
                "Config file {:?} not found - starting with built-in defaults",
                path
            );
            return Ok(Self::default());
        }
        Self::load_from_file(path)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ServerConfig {
    pub host: String,
    pub port: u16,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            host: "127.0.0.1".to_string(),
            port: 8080,
        }
    }
}

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

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct StorageConfig {
    #[serde(default)]
    pub backend: StorageBackendKind,
    #[serde(default)]
    pub local: LocalStorageConfig,
    #[serde(default)]
    pub s3: S3StorageConfig,
}

impl Default for StorageConfig {
    fn default() -> Self {
        Self {
            backend: StorageBackendKind::Local,
            local: LocalStorageConfig::default(),
            s3: S3StorageConfig::default(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct LocalStorageConfig {
    pub upload_dir: PathBuf,
    pub public_url_prefix: String,
}

impl Default for LocalStorageConfig {
    fn default() -> Self {
        Self {
            upload_dir: PathBuf::from("data/uploads"),
            public_url_prefix: "/uploads".to_string(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct S3StorageConfig {
    pub bucket: String,
    pub region: String,
    pub endpoint: Option<String>,
    pub access_key_id: String,
    pub secret_access_key: String,
    pub force_path_style: bool,
    pub public_url_prefix: Option<String>,
}

impl Default for S3StorageConfig {
    fn default() -> Self {
        Self {
            bucket: "aegis-bucket".to_string(),
            region: "auto".to_string(),
            endpoint: None,
            access_key_id: String::new(),
            secret_access_key: String::new(),
            force_path_style: false,
            public_url_prefix: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct CacheConfig {
    #[serde(default)]
    pub backend: CacheBackendKind,
    #[serde(default = "default_cache_ttl")]
    pub default_ttl_secs: u64,
    #[serde(default)]
    pub redis: RedisCacheConfig,
}

fn default_cache_ttl() -> u64 {
    3600 // 1 hour
}

impl Default for CacheConfig {
    fn default() -> Self {
        Self {
            backend: CacheBackendKind::Memory,
            default_ttl_secs: default_cache_ttl(),
            redis: RedisCacheConfig::default(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct RedisCacheConfig {
    pub url: String,
    #[serde(default = "default_redis_timeout")]
    pub timeout_ms: u64,
}

fn default_redis_timeout() -> u64 {
    2000
}

impl Default for RedisCacheConfig {
    fn default() -> Self {
        Self {
            url: "redis://127.0.0.1:6379".to_string(),
            timeout_ms: default_redis_timeout(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct PathsConfig {
    pub schemas_dir: PathBuf,
    pub plugins_dir: PathBuf,
    pub admin_dist_dir: PathBuf,
}

impl Default for PathsConfig {
    fn default() -> Self {
        Self {
            schemas_dir: PathBuf::from("schemas"),
            plugins_dir: PathBuf::from("plugins"),
            admin_dist_dir: PathBuf::from("admin/dist"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct RuntimeConfig {
    pub enable_wasm_plugins: bool,
    pub default_max_memory_mb: u32,
    pub default_max_fuel_units: u64,
    pub default_timeout_ms: u64,
}

impl Default for RuntimeConfig {
    fn default() -> Self {
        Self {
            enable_wasm_plugins: true,
            default_max_memory_mb: 16,
            default_max_fuel_units: 100_000_000,
            default_timeout_ms: 250,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct SecurityConfig {
    pub enforce_external_link_rel: bool,
    pub open_external_links_in_new_tab: bool,
    pub internal_domains: Vec<String>,
    pub enforce_image_lazy_loading: bool,
    pub enforce_image_async_decoding: bool,
}

impl Default for SecurityConfig {
    fn default() -> Self {
        Self {
            enforce_external_link_rel: true,
            open_external_links_in_new_tab: false,
            internal_domains: Vec::new(),
            enforce_image_lazy_loading: true,
            enforce_image_async_decoding: true,
        }
    }
}

/// 監査イベントの出力先
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AuditSinkKind {
    /// `_aegis_audit_logs` テーブルへ永続化し、`GET /api/v1/audit-logs` で検索可能にする
    Database,
    /// `aegis::audit` ターゲットの構造化ログ (1 行 1 JSON) として出力する
    Log,
    /// `aegis_audit_events_total` カウンタを `GET /metrics` で公開する
    Prometheus,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AuditConfig {
    pub enabled: bool,
    pub sinks: Vec<AuditSinkKind>,
    /// `X-Forwarded-For` の先頭アドレスをクライアント IP として記録する（信頼できるリバースプロキシ配下でのみ有効化）
    pub trust_forwarded_for: bool,
}

impl Default for AuditConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            sinks: vec![AuditSinkKind::Database],
            trust_forwarded_for: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bundled_config_files_parse() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../config");
        for name in ["default.toml", "docker.toml"] {
            let path = root.join(name);
            if let Err(e) = AegisConfig::load_from_file(&path) {
                panic!("{:?} failed to parse: {}", path, e);
            }
        }

        let docker = AegisConfig::load_from_file(root.join("docker.toml")).unwrap();
        assert_eq!(docker.server.host, "0.0.0.0");
        assert_eq!(
            docker.storage.local.upload_dir,
            PathBuf::from("/app/data/uploads")
        );
    }

    #[test]
    fn test_partial_sections_fall_back_to_field_defaults() {
        let config: AegisConfig = toml::from_str(
            r#"
            [server]
            port = 9000

            [storage.s3]
            bucket = "my-bucket"
            "#,
        )
        .unwrap();

        assert_eq!(config.server.port, 9000);
        assert_eq!(config.server.host, "127.0.0.1");
        assert_eq!(config.storage.s3.bucket, "my-bucket");
        assert_eq!(config.storage.s3.region, "auto");
        assert!(config.storage.s3.access_key_id.is_empty());
        assert!(!config.storage.s3.force_path_style);
    }

    #[test]
    fn test_audit_config_parsing() {
        let default = AegisConfig::default();
        assert!(default.audit.enabled);
        assert_eq!(default.audit.sinks, vec![AuditSinkKind::Database]);

        let config: AegisConfig = toml::from_str(
            r#"
            [audit]
            sinks = ["database", "log", "prometheus"]
            "#,
        )
        .unwrap();
        assert!(config.audit.enabled);
        assert_eq!(
            config.audit.sinks,
            vec![
                AuditSinkKind::Database,
                AuditSinkKind::Log,
                AuditSinkKind::Prometheus
            ]
        );

        let invalid: std::result::Result<AegisConfig, _> =
            toml::from_str("[audit]\nsinks = [\"cloudwatch\"]\n");
        assert!(invalid.is_err());
    }

    #[test]
    fn test_load_or_default_missing_file_uses_defaults() {
        let config = AegisConfig::load_or_default("does/not/exist.toml").unwrap();
        assert_eq!(config.server.port, 8080);
    }

    #[test]
    fn test_load_or_default_rejects_invalid_file() {
        let path =
            std::env::temp_dir().join(format!("aegis-invalid-config-{}.toml", std::process::id()));
        std::fs::write(&path, "[server]\nport = \"not-a-number\"\n").unwrap();
        let result = AegisConfig::load_or_default(&path);
        let _ = std::fs::remove_file(&path);
        assert!(result.is_err());
    }
}
