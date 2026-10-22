use crate::audit::AuditService;
use crate::auth::SessionService;
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
            schemas: Arc::new(RwLock::new(schemas)),
            supervisor: Arc::new(RwLock::new(supervisor)),
            audit,
            theme,
        }
    }
}
