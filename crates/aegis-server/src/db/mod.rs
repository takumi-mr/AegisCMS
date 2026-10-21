pub mod mysql;
pub mod postgres;
pub mod sqlite;

use aegis_core::{
    AegisError, AuditActor, AuditEvent, AuditQuery, DatabaseBackendKind, DatabaseConfig, Result,
};

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

    /// Persist an audit event into the audit log table
    async fn insert_audit_event(&self, event: &AuditEvent) -> Result<()>;

    /// Search audit events, newest first
    async fn query_audit_events(&self, query: &AuditQuery) -> Result<Vec<AuditEvent>>;
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

/// 監査ログテーブル名（コンテンツテーブル `content_*` と衝突しないシステムテーブル）
pub const AUDIT_TABLE: &str = "_aegis_audit_logs";

/// INSERT / SELECT で使う監査ログテーブルのカラム（順序固定）
pub const AUDIT_COLUMNS: [&str; 11] = [
    "id",
    "occurred_at",
    "action",
    "outcome",
    "actor_id",
    "actor_role",
    "resource_type",
    "resource_id",
    "ip",
    "user_agent",
    "details_json",
];

/// 監査ログ索引の (インデックス名, 対象カラム)
pub const AUDIT_INDEXES: [(&str, &str); 4] = [
    ("idx_aegis_audit_logs_occurred_at", "occurred_at"),
    ("idx_aegis_audit_logs_action", "action, occurred_at"),
    ("idx_aegis_audit_logs_actor", "actor_id, occurred_at"),
    (
        "idx_aegis_audit_logs_resource",
        "resource_type, resource_id",
    ),
];

/// `AUDIT_COLUMNS` と同じ順序で INSERT にバインドする値
pub fn audit_insert_values(event: &AuditEvent) -> Vec<Option<String>> {
    vec![
        Some(event.id.clone()),
        Some(event.occurred_at.clone()),
        Some(event.action.as_str().to_string()),
        Some(event.outcome.as_str().to_string()),
        event.actor.as_ref().map(|a| a.user_id.clone()),
        event.actor.as_ref().map(|a| a.role.clone()),
        event.resource_type.clone(),
        event.resource_id.clone(),
        event.ip.clone(),
        event.user_agent.clone(),
        Some(event.details.to_string()),
    ]
}

/// プレースホルダ生成関数（1 始まりの番号を受け取る）を使って INSERT 文を組み立てる
pub fn build_audit_insert(placeholder: impl Fn(usize) -> String) -> String {
    let placeholders: Vec<String> = (1..=AUDIT_COLUMNS.len()).map(placeholder).collect();
    format!(
        "INSERT INTO {} ({}) VALUES ({});",
        AUDIT_TABLE,
        AUDIT_COLUMNS.join(", "),
        placeholders.join(", ")
    )
}

/// 検索条件から SELECT 文とバインド値を組み立てる。値はすべてバインドし、LIMIT / OFFSET のみ数値を埋め込む
pub fn build_audit_select(
    query: &AuditQuery,
    placeholder: impl Fn(usize) -> String,
) -> (String, Vec<String>) {
    let mut conditions = Vec::new();
    let mut binds: Vec<String> = Vec::new();
    let mut push = |expr: &str, value: String| {
        binds.push(value);
        conditions.push(format!("{} {}", expr, placeholder(binds.len())));
    };

    if let Some(action) = query.action {
        push("action =", action.as_str().to_string());
    }
    if let Some(outcome) = query.outcome {
        push("outcome =", outcome.as_str().to_string());
    }
    if let Some(actor_id) = &query.actor_id {
        push("actor_id =", actor_id.clone());
    }
    if let Some(resource_type) = &query.resource_type {
        push("resource_type =", resource_type.clone());
    }
    if let Some(resource_id) = &query.resource_id {
        push("resource_id =", resource_id.clone());
    }
    if let Some(since) = &query.since {
        push("occurred_at >=", since.clone());
    }
    if let Some(until) = &query.until {
        push("occurred_at <", until.clone());
    }

    let where_clause = if conditions.is_empty() {
        String::new()
    } else {
        format!(" WHERE {}", conditions.join(" AND "))
    };

    let sql = format!(
        "SELECT {} FROM {}{} ORDER BY occurred_at DESC, id DESC LIMIT {} OFFSET {};",
        AUDIT_COLUMNS.join(", "),
        AUDIT_TABLE,
        where_clause,
        query.limit,
        query.offset
    );
    (sql, binds)
}

/// DB の行から読み出したカラム値（`AUDIT_COLUMNS` の順）
pub struct AuditRow {
    pub id: String,
    pub occurred_at: String,
    pub action: String,
    pub outcome: String,
    pub actor_id: Option<String>,
    pub actor_role: Option<String>,
    pub resource_type: Option<String>,
    pub resource_id: Option<String>,
    pub ip: Option<String>,
    pub user_agent: Option<String>,
    pub details_json: String,
}

impl AuditRow {
    pub fn into_event(self) -> Result<AuditEvent> {
        let action = self
            .action
            .parse()
            .map_err(|e: String| AegisError::Database(e))?;
        let outcome = self
            .outcome
            .parse()
            .map_err(|e: String| AegisError::Database(e))?;
        let actor = self.actor_id.map(|user_id| AuditActor {
            user_id,
            role: self.actor_role.unwrap_or_default(),
        });
        let details = serde_json::from_str(&self.details_json)
            .unwrap_or(serde_json::Value::String(self.details_json));

        Ok(AuditEvent {
            id: self.id,
            occurred_at: self.occurred_at,
            action,
            outcome,
            actor,
            resource_type: self.resource_type,
            resource_id: self.resource_id,
            ip: self.ip,
            user_agent: self.user_agent,
            details,
        })
    }
}

/// sqlx の行 (`SqliteRow` / `PgRow` / `MySqlRow`) を `AuditRow` に変換する
macro_rules! audit_row_from_sqlx {
    ($row:expr) => {{
        let row = $row;
        let get = |e: sqlx::Error| aegis_core::AegisError::Database(e.to_string());
        $crate::db::AuditRow {
            id: row.try_get("id").map_err(get)?,
            occurred_at: row.try_get("occurred_at").map_err(get)?,
            action: row.try_get("action").map_err(get)?,
            outcome: row.try_get("outcome").map_err(get)?,
            actor_id: row.try_get("actor_id").map_err(get)?,
            actor_role: row.try_get("actor_role").map_err(get)?,
            resource_type: row.try_get("resource_type").map_err(get)?,
            resource_id: row.try_get("resource_id").map_err(get)?,
            ip: row.try_get("ip").map_err(get)?,
            user_agent: row.try_get("user_agent").map_err(get)?,
            details_json: row.try_get("details_json").map_err(get)?,
        }
    }};
}
pub(crate) use audit_row_from_sqlx;
