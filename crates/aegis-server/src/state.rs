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
        }
    }
}

#[cfg(test)]
pub mod test_support {
    use super::AppState;
    use crate::audit::AuditService;
    use crate::auth::{LocalAuthProvider, LoginRateLimiter, SessionService};
    use crate::cache::MemoryCacheBackend;
    use crate::db::{DatabaseAdapter, SqliteAdapter};
    use crate::storage::LocalStorageBackend;
    use aegis_core::AegisConfig;
    use std::sync::Arc;

    /// インメモリ SQLite / キャッシュで動く、スキーマ・プラグインなしの `AppState`
    pub async fn test_state(config: AegisConfig) -> Arc<AppState> {
        let adapter = SqliteAdapter::connect("sqlite::memory:", 1).await.unwrap();
        adapter.init().await.unwrap();
        let db: Arc<dyn DatabaseAdapter> = Arc::new(adapter);
        let cache = Arc::new(MemoryCacheBackend::new());
        let audit = Arc::new(AuditService::from_config(&config.audit, db.clone()));
        let login_limiter = Arc::new(LoginRateLimiter::new(
            cache.clone(),
            config.auth.login_rate_limit.clone(),
        ));
        Arc::new(AppState::new(
            config,
            db.clone(),
            Arc::new(LocalStorageBackend::new("target/test_uploads", "/uploads")),
            cache.clone(),
            Arc::new(SessionService::new(cache, 3600)),
            Arc::new(LocalAuthProvider::new(db)),
            login_limiter,
            Default::default(),
            aegis_runtime::PluginSupervisor::new(),
            audit,
            Arc::new(aegis_theme::ThemeEngine::new(
                std::path::Path::new("themes"),
                "default",
            )),
        ))
    }
}
