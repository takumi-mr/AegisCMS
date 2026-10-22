use crate::cache::CacheBackendKind;
use crate::error::{AegisError, Result};
use crate::manifest::PluginResourceLimits;
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
    pub plugins: PluginsConfig,
    #[serde(default)]
    pub security: SecurityConfig,
    #[serde(default)]
    pub audit: AuditConfig,
    #[serde(default)]
    pub auth: AuthConfig,
    #[serde(default)]
    pub theme: ThemeConfig,
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
    /// アップロード 1 回あたりの最大リクエストサイズ（MB）
    #[serde(default = "default_max_upload_mb")]
    pub max_upload_mb: u64,
}

fn default_max_upload_mb() -> u64 {
    10
}

impl StorageConfig {
    /// アップロード API に適用するリクエストボディの上限（バイト）
    pub fn max_upload_bytes(&self) -> usize {
        usize::try_from(self.max_upload_mb.saturating_mul(1024 * 1024)).unwrap_or(usize::MAX)
    }
}

impl Default for StorageConfig {
    fn default() -> Self {
        Self {
            backend: StorageBackendKind::Local,
            local: LocalStorageConfig::default(),
            s3: S3StorageConfig::default(),
            max_upload_mb: default_max_upload_mb(),
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThemeConfig {
    pub active: String,
    pub site_title: String,
    pub site_description: String,
    #[serde(default)]
    pub custom_css: Option<String>,
    /// トップページ（記事一覧）の 1 ページあたりの件数（`/?page=N` でページ送り）
    #[serde(default = "default_posts_per_page")]
    pub posts_per_page: u64,
    /// `true` でテンプレートをリクエストごとに読み直す（テーマ開発用）。
    /// `false`（既定）ではコンパイル済みのテンプレートを再利用し、テーマの切り替え時に読み直す
    #[serde(default)]
    pub reload_templates: bool,
}

fn default_posts_per_page() -> u64 {
    10
}

impl Default for ThemeConfig {
    fn default() -> Self {
        Self {
            active: "default".to_string(),
            site_title: "AegisCMS".to_string(),
            site_description: "Next-Generation Secure CMS with Sandboxed Extensibility".to_string(),
            custom_css: None,
            posts_per_page: default_posts_per_page(),
            reload_templates: false,
        }
    }
}

/// `[plugins]` — インストール済みプラグイン（`[paths].plugins_dir/<name>/`）のうち有効にするもの
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct PluginsConfig {
    /// 有効にするプラグイン名（マニフェストの `name`）。フックはこの順に実行する。
    /// 列挙したプラグインが見つからない・読み込めない場合はサーバーを起動しない
    pub enabled: Vec<String>,
}

impl PluginsConfig {
    /// 名前の形式（`is_valid_plugin_name`）と重複を検証する
    pub fn validate(&self) -> Result<()> {
        let mut seen = std::collections::HashSet::new();
        for name in &self.enabled {
            if !crate::manifest::is_valid_plugin_name(name) {
                return Err(AegisError::Config(format!(
                    "[plugins].enabled: {:?} is not a valid plugin name (lowercase letters, digits, '-', '_')",
                    name
                )));
            }
            if !seen.insert(name) {
                return Err(AegisError::Config(format!(
                    "[plugins].enabled: {:?} is listed more than once",
                    name
                )));
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct RuntimeConfig {
    pub enable_wasm_plugins: bool,
    pub default_max_memory_mb: u32,
    pub default_max_fuel_units: u64,
    pub default_timeout_ms: u64,
    /// プーリングアロケータの枠数（= 同時に実行できるフック呼び出しの数。超えた分は空きを待つ）。
    /// 0 で無効（呼び出しごとにメモリを確保・解放する）
    pub instance_pool_size: u32,
    /// プールの 1 枠あたりの線形メモリの上限（MiB）。`max_memory_mb` がこれを超えるプラグインは読み込めない
    pub pool_max_memory_mb: u32,
}

impl RuntimeConfig {
    /// Resource limits applied to plugins whose manifest omits them (field by field)
    pub fn default_resource_limits(&self) -> PluginResourceLimits {
        PluginResourceLimits {
            max_memory_mb: self.default_max_memory_mb,
            max_fuel_units: self.default_max_fuel_units,
            execution_timeout_ms: self.default_timeout_ms,
        }
    }
}

impl Default for RuntimeConfig {
    fn default() -> Self {
        Self {
            enable_wasm_plugins: true,
            default_max_memory_mb: 16,
            default_max_fuel_units: 100_000_000,
            default_timeout_ms: 250,
            instance_pool_size: 64,
            pool_max_memory_mb: 64,
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
    /// `X-Forwarded-For` の先頭アドレスをクライアント IP として扱う（監査ログとログインのレート制限に使用。
    /// 信頼できるリバースプロキシ配下でのみ有効化）
    pub trust_forwarded_for: bool,
    /// `database` 出力先に保存した監査ログの保持日数。これより古いログは定期的に削除する（0 で無期限）
    pub retention_days: u32,
    /// 保持期間切れのログを削除する間隔（秒）
    pub purge_interval_secs: u64,
}

impl Default for AuditConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            sinks: vec![AuditSinkKind::Database],
            trust_forwarded_for: false,
            retention_days: 0,
            purge_interval_secs: 3600,
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct AuthConfig {
    pub login_rate_limit: LoginRateLimitConfig,
    pub oidc: OidcConfig,
}

/// OpenID Connect（認可コードフロー + PKCE）による外部 IdP ログイン
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct OidcConfig {
    pub enabled: bool,
    /// ログイン画面のボタンに表示する名前
    pub display_name: String,
    /// IdP の Issuer（`{issuer_url}/.well-known/openid-configuration` からエンドポイントを取得する）
    pub issuer_url: String,
    pub client_id: String,
    pub client_secret: String,
    /// IdP に登録するコールバック URL（`https://cms.example.com/api/v1/auth/oidc/callback`）
    pub redirect_url: String,
    pub scopes: Vec<String>,
    /// AegisCMS のユーザー名にするクレーム（無ければ `email`）
    pub username_claim: String,
    /// 未登録のユーザーを初回ログイン時に作成する（`false` なら IdP と紐付け済みのユーザーのみ）
    pub auto_create_users: bool,
    /// クレームからロールが決まらない場合のロール（新規ユーザーの初期ロール）
    pub default_role: String,
    /// ロールの決定に使うクレーム（例: `groups`）。空ならロールは AegisCMS のユーザーアカウントで決まる
    pub role_claim: String,
    /// `role_claim` の値 → ロール。複数一致した場合は最も強いロール
    pub role_mapping: std::collections::HashMap<String, String>,
    /// ログイン後に戻る Admin Studio の URL（セッショントークンを `#session=` で渡す）
    pub post_login_redirect: String,
}

impl Default for OidcConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            display_name: "SSO".to_string(),
            issuer_url: String::new(),
            client_id: String::new(),
            client_secret: String::new(),
            redirect_url: String::new(),
            scopes: vec![
                "openid".to_string(),
                "profile".to_string(),
                "email".to_string(),
            ],
            username_claim: "preferred_username".to_string(),
            auto_create_users: true,
            default_role: "viewer".to_string(),
            role_claim: String::new(),
            role_mapping: std::collections::HashMap::new(),
            post_login_redirect: "/admin/".to_string(),
        }
    }
}

/// `POST /api/v1/auth/session` のログイン失敗回数の制限。
/// 失敗回数はキャッシュバックエンド（`[cache]`）で数えるため、`redis` なら全サーバーで共有される
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct LoginRateLimitConfig {
    pub enabled: bool,
    /// 同じユーザー名で `window_secs` 内にこの回数失敗するとロックする（0 で無制限）
    pub max_failures_per_user: u64,
    /// 同じクライアント IP から `window_secs` 内にこの回数失敗するとロックする（0 で無制限）
    pub max_failures_per_ip: u64,
    /// 失敗回数を数える期間（秒）。最初の失敗から数え、過ぎるとリセットされる
    pub window_secs: u64,
    /// ロックの継続時間（秒）。ロック中は正しいパスワードでもログインできない
    pub lockout_secs: u64,
}

impl Default for LoginRateLimitConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            max_failures_per_user: 5,
            max_failures_per_ip: 20,
            window_secs: 900,
            lockout_secs: 900,
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
    fn test_plugins_config() {
        let config: AegisConfig = toml::from_str("").unwrap();
        assert!(config.plugins.enabled.is_empty());
        assert!(config.plugins.validate().is_ok());

        let config: AegisConfig =
            toml::from_str("[plugins]\nenabled = [\"slack-notifier\", \"seo_tools\"]\n").unwrap();
        assert_eq!(config.plugins.enabled, vec!["slack-notifier", "seo_tools"]);
        assert!(config.plugins.validate().is_ok());

        for invalid in [r#"["../etc"]"#, r#"["Slack"]"#, r#"["a", "a"]"#] {
            let config: AegisConfig =
                toml::from_str(&format!("[plugins]\nenabled = {}\n", invalid)).unwrap();
            assert!(config.plugins.validate().is_err(), "{}", invalid);
        }
    }
}
