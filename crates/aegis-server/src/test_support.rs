//! Helpers shared by unit tests that need a full `AppState`

use std::collections::HashMap;
use std::sync::Arc;

use crate::config::AegisConfig;
use aegis_runtime::PluginSupervisor;
use aegis_schema::SchemaDefinition;

use crate::state::AppState;
use aegis_audit::AuditService;
use aegis_auth::{LocalAuthProvider, LoginRateLimiter, SessionService};
use aegis_cache::MemoryCacheBackend;
use aegis_db::{DatabaseAdapter, SqliteAdapter};
use aegis_storage::LocalStorageBackend;

/// App state backed by an in-memory SQLite database and a memory cache, with a minimal
/// `article` schema (title only) migrated
pub async fn test_state(supervisor: PluginSupervisor) -> Arc<AppState> {
    test_state_with_config(AegisConfig::default(), supervisor).await
}

/// `test_state` with a custom configuration (e.g. `[auth.login_rate_limit]`)
pub async fn test_state_with_config(
    config: AegisConfig,
    supervisor: PluginSupervisor,
) -> Arc<AppState> {
    Arc::new(test_app_state(config, supervisor, Vec::new()).await)
}

/// App state with supervisor and extra schemas
pub async fn test_state_with_plugins(
    supervisor: PluginSupervisor,
    extra_schemas: Vec<SchemaDefinition>,
) -> Arc<AppState> {
    Arc::new(test_app_state(AegisConfig::default(), supervisor, extra_schemas).await)
}

#[derive(Default)]
pub struct RecordingNotifier {
    pub events: std::sync::Mutex<Vec<aegis_core::NotificationEvent>>,
}

impl aegis_core::Notifier for RecordingNotifier {
    fn notify(&self, event: aegis_core::NotificationEvent) {
        self.events.lock().unwrap().push(event);
    }
}

impl RecordingNotifier {
    pub fn take(&self) -> Vec<aegis_core::NotificationEvent> {
        let mut guard = self.events.lock().unwrap();
        std::mem::take(&mut *guard)
    }

    #[allow(dead_code)]
    pub fn events(&self) -> Vec<aegis_core::NotificationEvent> {
        self.events.lock().unwrap().clone()
    }
}

pub async fn test_app_state(
    config: AegisConfig,
    supervisor: PluginSupervisor,
    extra_schemas: Vec<SchemaDefinition>,
) -> AppState {
    let adapter = SqliteAdapter::connect("sqlite::memory:", 1).await.unwrap();
    adapter.init().await.unwrap();
    test_app_state_with_db(Arc::new(adapter), config, supervisor, extra_schemas).await
}

/// The `test_app_state` backed by any `DatabaseAdapter` implementation (e.g. Postgres, MySQL)
pub async fn test_app_state_with_db(
    db: Arc<dyn DatabaseAdapter>,
    config: AegisConfig,
    supervisor: PluginSupervisor,
    extra_schemas: Vec<SchemaDefinition>,
) -> AppState {
    let article: SchemaDefinition = serde_json::from_value(serde_json::json!({
        "content_type": "article",
        "label": "Articles",
        "fields": [{ "name": "title", "type": "string", "required": true }]
    }))
    .unwrap();
    let mut schemas = HashMap::from([("article".to_string(), article)]);
    for schema in extra_schemas {
        schemas.insert(schema.content_type.clone(), schema);
    }
    db.run_migrations(&schemas).await.unwrap();
    let cache = Arc::new(MemoryCacheBackend::new());
    let audit = Arc::new(AuditService::from_config(&config.audit, db.clone()));
    let login_limiter = Arc::new(LoginRateLimiter::new(
        cache.clone(),
        config.auth.login_rate_limit.clone(),
    ));
    let base_path = config.server.normalized_base_path().unwrap();
    AppState::new(
        config,
        db.clone(),
        Arc::new(LocalStorageBackend::new("target/test_uploads", "/uploads")),
        cache.clone(),
        Arc::new(SessionService::new(cache, 3600)),
        Arc::new(LocalAuthProvider::new(db)),
        login_limiter,
        schemas,
        supervisor,
        audit,
        Arc::new(
            aegis_theme::ThemeEngine::new(std::path::Path::new("themes"), "default")
                .with_base_path(&base_path),
        ),
    )
}
