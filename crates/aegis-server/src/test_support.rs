//! Helpers shared by unit tests that need a full `AppState`

use std::collections::HashMap;
use std::sync::Arc;

use aegis_core::AegisConfig;
use aegis_runtime::PluginSupervisor;
use aegis_schema::SchemaDefinition;

use crate::audit::AuditService;
use crate::auth::{LocalAuthProvider, LoginRateLimiter, SessionService};
use crate::cache::MemoryCacheBackend;
use crate::db::{DatabaseAdapter, SqliteAdapter};
use crate::state::AppState;
use crate::storage::LocalStorageBackend;

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
    let adapter = SqliteAdapter::connect("sqlite::memory:", 1).await.unwrap();
    adapter.init().await.unwrap();
    let db: Arc<dyn DatabaseAdapter> = Arc::new(adapter);
    let article: SchemaDefinition = serde_json::from_value(serde_json::json!({
        "content_type": "article",
        "label": "Articles",
        "fields": [{ "name": "title", "type": "string", "required": true }]
    }))
    .unwrap();
    let schemas = HashMap::from([("article".to_string(), article)]);
    db.run_migrations(&schemas).await.unwrap();
    let cache = Arc::new(MemoryCacheBackend::new());
    let audit = Arc::new(AuditService::from_config(&config.audit, db.clone()));
    let login_limiter = Arc::new(LoginRateLimiter::new(
        cache.clone(),
        config.auth.login_rate_limit.clone(),
    ));
    let base_path = config.server.normalized_base_path().unwrap();
    Arc::new(AppState::new(
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
    ))
}
