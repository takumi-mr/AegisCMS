pub mod mysql;
pub mod postgres;
pub mod sqlite;

use aegis_core::{DatabaseBackendKind, DatabaseConfig, Result};

use aegis_schema::SchemaDefinition;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;

pub use mysql::MySqlAdapter;
pub use postgres::PostgresAdapter;
pub use sqlite::SqliteAdapter;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResolvedRelation {
    pub id: String,
    pub content_type: String,
    pub title: String,
    pub slug: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContentRecord {
    pub id: String,
    pub content_type: String,
    pub title: String,
    pub slug: String,
    pub status: String,
    pub blocks_json: String,
    pub author_id: Option<String>,
    pub published_at: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub custom_fields: HashMap<String, serde_json::Value>,
}

#[async_trait]
pub trait DatabaseAdapter: Send + Sync {
    /// Initialize the database, checking connectivity and setting up migration history tables
    async fn init(&self) -> Result<()>;

    /// Apply schema migrations dynamically across dialects
    async fn run_migrations(&self, schemas: &HashMap<String, SchemaDefinition>) -> Result<()>;

    /// Insert a content record
    async fn insert_content(&self, schema: &SchemaDefinition, record: &ContentRecord)
        -> Result<()>;

    /// Fetch a single content record by slug or ID
    async fn get_content(
        &self,
        schema: &SchemaDefinition,
        slug_or_id: &str,
    ) -> Result<Option<ContentRecord>>;

    /// List content records ordered by creation date
    async fn list_contents(&self, schema: &SchemaDefinition) -> Result<Vec<ContentRecord>>;

    /// Resolve a relation item from a foreign schema table
    async fn resolve_relation(
        &self,
        target_schema: &SchemaDefinition,
        target_field: &str,
        ref_id: &str,
    ) -> Result<Option<ResolvedRelation>>;
}

pub async fn create_database_adapter(config: &DatabaseConfig) -> Result<Arc<dyn DatabaseAdapter>> {
    let backend = config.resolved_backend();
    match backend {
        DatabaseBackendKind::Sqlite => {
            tracing::info!(
                "Initializing SQLite database adapter (URL: {}, max_connections: {})",
                config.url,
                config.max_connections
            );
            let adapter = SqliteAdapter::connect(&config.url, config.max_connections).await?;
            adapter.init().await?;
            Ok(Arc::new(adapter))
        }
        DatabaseBackendKind::Postgres => {
            tracing::info!(
                "Initializing PostgreSQL database adapter (URL: {}, max_connections: {})",
                config.url,
                config.max_connections
            );
            let adapter = PostgresAdapter::connect(&config.url, config.max_connections).await?;
            adapter.init().await?;
            Ok(Arc::new(adapter))
        }
        DatabaseBackendKind::Mysql => {
            tracing::info!(
                "Initializing MySQL database adapter (URL: {}, max_connections: {})",
                config.url,
                config.max_connections
            );
            let adapter = MySqlAdapter::connect(&config.url, config.max_connections).await?;
            adapter.init().await?;
            Ok(Arc::new(adapter))
        }
    }
}

pub fn is_standard_column(name: &str) -> bool {
    matches!(
        name,
        "id" | "content_type"
            | "title"
            | "slug"
            | "status"
            | "blocks_json"
            | "author_id"
            | "published_at"
            | "created_at"
            | "updated_at"
    )
}
