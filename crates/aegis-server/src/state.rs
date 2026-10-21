use aegis_core::AegisConfig;
use aegis_runtime::PluginSupervisor;
use aegis_schema::SchemaDefinition;
use sqlx::SqlitePool;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

#[derive(Clone)]
pub struct AppState {
    #[allow(dead_code)]
    pub config: AegisConfig,
    pub db: SqlitePool,
    pub schemas: Arc<RwLock<HashMap<String, SchemaDefinition>>>,
    pub supervisor: Arc<RwLock<PluginSupervisor>>,
}

impl AppState {
    pub fn new(
        config: AegisConfig,
        db: SqlitePool,
        schemas: HashMap<String, SchemaDefinition>,
        supervisor: PluginSupervisor,
    ) -> Self {
        Self {
            config,
            db,
            schemas: Arc::new(RwLock::new(schemas)),
            supervisor: Arc::new(RwLock::new(supervisor)),
        }
    }
}
