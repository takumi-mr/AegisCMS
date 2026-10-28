//! 設定ファイル全体（`AegisConfig`）と、サーバー自身の `[server]` / `[paths]`。
//! 各セクションの型はそれを使うクレートにある

use aegis_ast::SecurityConfig;
use aegis_audit::AuditConfig;
use aegis_auth::AuthConfig;
use aegis_cache::{CacheConfig, CdnConfig};
use aegis_comments::CommentsConfig;
use aegis_commerce::CommerceConfig;
use aegis_core::{AegisError, Result};
use aegis_db::DatabaseConfig;
use aegis_forum::ForumConfig;
use aegis_notify::NotificationsConfig;
use aegis_runtime::{PluginsConfig, RuntimeConfig};
use aegis_storage::StorageConfig;
use aegis_theme::ThemeConfig;
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
    pub cdn: CdnConfig,
    #[serde(default)]
    pub paths: PathsConfig,
    #[serde(default)]
    pub runtime: RuntimeConfig,
    #[serde(default)]
    pub plugins: PluginsConfig,
    #[serde(default)]
    pub security: SecurityConfig,
    #[serde(default)]
    pub audit: AuditConfig,
    #[serde(default)]
    pub auth: AuthConfig,
    #[serde(default)]
    pub theme: ThemeConfig,
    #[serde(default)]
    pub commerce: CommerceConfig,
    #[serde(default)]
    pub notifications: NotificationsConfig,
    #[serde(default)]
    pub comments: CommentsConfig,
    #[serde(default)]
    pub forum: ForumConfig,
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
    /// リバースプロキシのサブパス配下で公開する場合のパス（例: `/cms`）。空ならルートで公開。
    /// 読み出しは `normalized_base_path` を使う
    pub base_path: String,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            host: "127.0.0.1".to_string(),
            port: 8080,
            base_path: String::new(),
        }
    }
}

impl ServerConfig {
    /// `base_path` を `""`（ルート）または末尾 `/` なしの `/segment[/segment...]` に正規化する。
    /// URL や HTML 属性にそのまま書き込むため、使える文字は `A-Z a-z 0-9 - . _ ~` のみ
    pub fn normalized_base_path(&self) -> Result<String> {
        let trimmed = self.base_path.trim().trim_matches('/');
        if trimmed.is_empty() {
            return Ok(String::new());
        }
        for segment in trimmed.split('/') {
            let valid = !segment.is_empty()
                && segment != "."
                && segment != ".."
                && segment
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'.' | b'_' | b'~'));
            if !valid {
                return Err(AegisError::Config(format!(
                    "[server].base_path {:?} is invalid: use a path like \"/cms\" (letters, digits, '-', '.', '_', '~')",
                    self.base_path
                )));
            }
        }
        Ok(format!("/{}", trimmed))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct PathsConfig {
    pub schemas_dir: PathBuf,
    pub plugins_dir: PathBuf,
    pub admin_dist_dir: PathBuf,
    #[serde(default = "default_themes_dir")]
    pub themes_dir: PathBuf,
}

fn default_themes_dir() -> PathBuf {
    PathBuf::from("themes")
}

impl Default for PathsConfig {
    fn default() -> Self {
        Self {
            schemas_dir: PathBuf::from("schemas"),
            plugins_dir: PathBuf::from("plugins"),
            admin_dist_dir: PathBuf::from("admin/dist"),
            themes_dir: default_themes_dir(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use aegis_audit::AuditSinkKind;
    use aegis_cache::CdnProviderKind;

    #[test]
    fn test_plugin_settings() {
        let config: AegisConfig = toml::from_str(
            r#"
            [plugins]
            enabled = ["shop-rules"]

            [plugins.settings.shop-rules]
            shipping_fee = 500
            coupons = "SAVE10:10%"
            secret = "env:AEGIS_TEST_PLUGIN_SECRET"
            regions = ["Tokyo", "Osaka"]
            "#,
        )
        .unwrap();
        let plugins = &config.plugins;
        assert_eq!(
            plugins.setting("shop-rules", "shipping_fee").as_deref(),
            Some("500")
        );
        assert_eq!(
            plugins.setting("shop-rules", "coupons").as_deref(),
            Some("SAVE10:10%")
        );
        assert_eq!(
            plugins.setting("shop-rules", "regions").as_deref(),
            Some(r#"["Tokyo","Osaka"]"#)
        );
        assert_eq!(plugins.setting("shop-rules", "secret"), None);
        assert_eq!(plugins.setting("other", "coupons"), None);
    }

    #[test]
    fn test_cdn_config_parsing() {
        let default = AegisConfig::default();
        assert!(!default.cdn.enabled);
        assert_eq!(default.cdn.provider, CdnProviderKind::None);
        assert!(default.cdn.emit_headers);

        let config: AegisConfig = toml::from_str(
            r#"
            [cdn]
            enabled = true
            provider = "cloudflare"
            emit_headers = true

            [cdn.cloudflare]
            zone_id = "zone_123"
            api_token = "tok_abc"
            "#,
        )
        .unwrap();
        assert!(config.cdn.enabled);
        assert_eq!(config.cdn.provider, CdnProviderKind::Cloudflare);
        assert_eq!(config.cdn.cloudflare.zone_id, "zone_123");
        assert!(config.cdn.validate().is_ok());
    }

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
            retention_days = 90
            "#,
        )
        .unwrap();
        assert_eq!(config.audit.retention_days, 90);
        assert_eq!(config.audit.purge_interval_secs, 3600);
        assert_eq!(default.audit.retention_days, 0);
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
    fn test_login_rate_limit_config_parsing() {
        let default = AegisConfig::default();
        assert!(default.auth.login_rate_limit.enabled);
        assert_eq!(default.auth.login_rate_limit.max_failures_per_user, 5);

        let config: AegisConfig =
            toml::from_str("[auth.login_rate_limit]\nmax_failures_per_ip = 0\n").unwrap();
        assert_eq!(config.auth.login_rate_limit.max_failures_per_ip, 0);
        assert_eq!(config.auth.login_rate_limit.window_secs, 900);
    }

    #[test]
    fn test_storage_upload_limit() {
        let default = AegisConfig::default();
        assert_eq!(default.storage.max_upload_mb, 10);
        assert_eq!(default.storage.max_upload_bytes(), 10 * 1024 * 1024);

        let config: AegisConfig = toml::from_str("[storage]\nmax_upload_mb = 25\n").unwrap();
        assert_eq!(config.storage.max_upload_bytes(), 25 * 1024 * 1024);
    }

    #[test]
    fn test_load_or_default_missing_file_uses_defaults() {
        let config = AegisConfig::load_or_default("does/not/exist.toml").unwrap();
        assert_eq!(config.server.port, 8080);
        assert_eq!(config.server.base_path, "");
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

    #[test]
    fn test_normalized_base_path() {
        let base = |value: &str| {
            ServerConfig {
                base_path: value.to_string(),
                ..Default::default()
            }
            .normalized_base_path()
        };
        assert_eq!(base("").unwrap(), "");
        assert_eq!(base("/").unwrap(), "");
        assert_eq!(base("cms").unwrap(), "/cms");
        assert_eq!(base("/cms/").unwrap(), "/cms");
        assert_eq!(base(" /sites/cms_2.v1 ").unwrap(), "/sites/cms_2.v1");
        for invalid in ["/a//b", "/../cms", "/cms?x=1", "/c ms", "/cms#top", "/\"x"] {
            assert!(base(invalid).is_err(), "{} should be rejected", invalid);
        }
    }

    #[test]
    fn test_plugins_and_theme_signature_config() {
        let config: AegisConfig = toml::from_str("").unwrap();
        assert!(config.plugins.enabled.is_empty());
        assert!(config.plugins.validate().is_ok());
        assert!(config.theme.validate().is_ok());

        let config: AegisConfig =
            toml::from_str("[plugins]\nenabled = [\"slack-notifier\", \"seo_tools\"]\n").unwrap();
        assert_eq!(config.plugins.enabled, vec!["slack-notifier", "seo_tools"]);
        assert!(config.plugins.validate().is_ok());

        for invalid in [r#"["../etc"]"#, r#"["Slack"]"#, r#"["a", "a"]"#] {
            let config: AegisConfig =
                toml::from_str(&format!("[plugins]\nenabled = {}\n", invalid)).unwrap();
            assert!(config.plugins.validate().is_err(), "{}", invalid);
        }

        // require_signatures = true without trusted_public_keys is rejected
        let config: AegisConfig = toml::from_str("[plugins]\nrequire_signatures = true\n").unwrap();
        assert!(config.plugins.validate().is_err());

        let config: AegisConfig = toml::from_str(
            r#"
            [theme]
            active = "default"
            site_title = "Site"
            site_description = "Desc"
            require_signatures = true
            "#,
        )
        .unwrap();
        assert!(config.theme.validate().is_err());

        // require_signatures = true with valid 32-byte hex key succeeds
        let valid_key = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
        let config: AegisConfig = toml::from_str(&format!(
            "[plugins]\nrequire_signatures = true\ntrusted_public_keys = [\"{}\"]\n",
            valid_key
        ))
        .unwrap();
        assert!(config.plugins.validate().is_ok());
    }
}
