use crate::audit::AuditService;
use crate::auth::{LoginRateLimiter, SessionService};
use crate::db::DatabaseAdapter;
use aegis_core::{AegisConfig, AuthProvider, CacheBackend, StorageBackend};
use aegis_runtime::PluginSupervisor;
use aegis_schema::SchemaDefinition;
use aegis_theme::ThemeEngine;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

#[derive(Clone)]
pub struct AppState {
    pub config: AegisConfig,
    pub db: Arc<dyn DatabaseAdapter>,
    pub storage: Arc<dyn StorageBackend>,
    #[allow(dead_code)]
    pub cache: Arc<dyn CacheBackend>,
    pub sessions: Arc<SessionService>,
    /// ログイン（`POST /api/v1/auth/session`）で資格情報を検証するプロバイダ
    pub auth_provider: Arc<dyn AuthProvider>,
    /// ログイン失敗回数の制限（`[auth.login_rate_limit]`）
    pub login_limiter: Arc<LoginRateLimiter>,
    pub schemas: Arc<RwLock<HashMap<String, SchemaDefinition>>>,
    pub supervisor: Arc<RwLock<PluginSupervisor>>,
    pub audit: Arc<AuditService>,
    pub theme: Arc<ThemeEngine>,
    /// 外部 IdP ログイン（`[auth.oidc]`）。無効なら `None`
    pub oidc: Option<OidcLogin>,
    /// EC（`[commerce]`）。無効なら `None`
    pub commerce: Option<Arc<crate::commerce::CommerceService>>,
}

/// OpenID Connect ログインの設定済みプロバイダと、ログイン画面・リダイレクトに使う値
#[derive(Clone)]
pub struct OidcLogin {
    /// `initiate_login` で IdP への URL を作り、`authenticate` でコールバックを検証する
    pub provider: Arc<dyn AuthProvider>,
    /// ログイン画面のボタンに表示する名前
    pub display_name: String,
    /// ログイン後（成功・失敗とも）に戻る Admin Studio の URL
    pub post_login_redirect: String,
}

impl AppState {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        config: AegisConfig,
        db: Arc<dyn DatabaseAdapter>,
        storage: Arc<dyn StorageBackend>,
        cache: Arc<dyn CacheBackend>,
        sessions: Arc<SessionService>,
        auth_provider: Arc<dyn AuthProvider>,
        login_limiter: Arc<LoginRateLimiter>,
        schemas: HashMap<String, SchemaDefinition>,
        supervisor: PluginSupervisor,
        audit: Arc<AuditService>,
        theme: Arc<ThemeEngine>,
    ) -> Self {
        Self {
            config,
            db,
            storage,
            cache,
            sessions,
            auth_provider,
            login_limiter,
            schemas: Arc::new(RwLock::new(schemas)),
            supervisor: Arc::new(RwLock::new(supervisor)),
            audit,
            theme,
            oidc: None,
            commerce: None,
        }
    }

    /// EC（`[commerce]`）を有効にする
    pub fn with_commerce(mut self, commerce: crate::commerce::CommerceService) -> Self {
        self.commerce = Some(Arc::new(commerce));
        self
    }

    /// OpenID Connect ログインを有効にする
    pub fn with_oidc(mut self, oidc: OidcLogin) -> Self {
        self.oidc = Some(oidc);
        self
    }
}
