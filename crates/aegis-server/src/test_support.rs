//! Helpers shared by unit tests that need a full `AppState`

use std::sync::Arc;

use aegis_core::AegisConfig;
use aegis_runtime::PluginSupervisor;

use crate::audit::AuditService;
use crate::auth::{LocalAuthProvider, SessionService};
use crate::cache::MemoryCacheBackend;
use crate::db::{DatabaseAdapter, SqliteAdapter};
use crate::state::AppState;
use crate::storage::LocalStorageBackend;

/// App state backed by an in-memory SQLite database, a memory cache and no schemas
pub async fn test_state(supervisor: PluginSupervisor) -> Arc<AppState> {
    let config = AegisConfig::default();
    let adapter = SqliteAdapter::connect("sqlite::memory:", 1).await.unwrap();
    adapter.init().await.unwrap();
    let db: Arc<dyn DatabaseAdapter> = Arc::new(adapter);
    let cache = Arc::new(MemoryCacheBackend::new());
    let audit = Arc::new(AuditService::from_config(&config.audit, db.clone()));
    Arc::new(AppState::new(
        config,
        db.clone(),
        Arc::new(LocalStorageBackend::new("target/test_uploads", "/uploads")),
        cache.clone(),
        Arc::new(SessionService::new(cache, 3600)),
        Arc::new(LocalAuthProvider::new(db)),
        Default::default(),
        supervisor,
        audit,
        Arc::new(aegis_theme::ThemeEngine::new(
            std::path::Path::new("themes"),
            "default",
        )),
    ))
}
