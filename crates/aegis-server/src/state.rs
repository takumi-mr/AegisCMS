use crate::auth::SessionService;
use crate::db::DatabaseAdapter;
use aegis_core::{AegisConfig, CacheBackend, StorageBackend};
use aegis_runtime::PluginSupervisor;
use aegis_schema::SchemaDefinition;
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
    pub schemas: Arc<RwLock<HashMap<String, SchemaDefinition>>>,
    pub supervisor: Arc<RwLock<PluginSupervisor>>,
}

impl AppState {
    pub fn new(
        config: AegisConfig,
        db: Arc<dyn DatabaseAdapter>,
        storage: Arc<dyn StorageBackend>,
        cache: Arc<dyn CacheBackend>,
        sessions: Arc<SessionService>,
        schemas: HashMap<String, SchemaDefinition>,
        supervisor: PluginSupervisor,
    ) -> Self {
        Self {
            config,
            db,
            storage,
            cache,
            sessions,
            schemas: Arc::new(RwLock::new(schemas)),
            supervisor: Arc::new(RwLock::new(supervisor)),
        }
    }
}
