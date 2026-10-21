use aegis_core::{AegisError, Result};
use aegis_schema::{SchemaDefinition, SqliteMigration};
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::{Pool, Sqlite};
use std::collections::HashMap;
use std::str::FromStr;

pub async fn init_db(database_url: &str, max_connections: u32) -> Result<Pool<Sqlite>> {
    let options = SqliteConnectOptions::from_str(database_url)
        .map_err(|e| AegisError::Database(e.to_string()))?
        .create_if_missing(true);

    let pool = SqlitePoolOptions::new()
        .max_connections(max_connections)
        .connect_with(options)
        .await
        .map_err(|e| AegisError::Database(e.to_string()))?;

    // マイグレーション履歴管理テーブル
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS _aegis_migrations (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            content_type TEXT NOT NULL UNIQUE,
            migrated_at TEXT NOT NULL
        );
        "#,
    )
    .execute(&pool)
    .await
    .map_err(|e| AegisError::Database(e.to_string()))?;

    Ok(pool)
}

pub async fn run_migrations(
    pool: &Pool<Sqlite>,
    schemas: &HashMap<String, SchemaDefinition>,
) -> Result<()> {
    for (content_type, schema) in schemas {
        tracing::info!(
            "Applying declarative migration for schema: {}",
            content_type
        );
        let statements = SqliteMigration::generate_schema_migration_sql(schema);

        for sql in statements {
            sqlx::query(&sql).execute(pool).await.map_err(|e| {
                AegisError::Database(format!("Migration failed: {} - SQL: {}", e, sql))
            })?;
        }

        let now = chrono::Utc::now().to_rfc3339();
        sqlx::query(
            r#"
            INSERT INTO _aegis_migrations (content_type, migrated_at)
            VALUES (?1, ?2)
            ON CONFLICT(content_type) DO UPDATE SET migrated_at = excluded.migrated_at;
            "#,
        )
        .bind(content_type)
        .bind(now)
        .execute(pool)
        .await
        .map_err(|e| AegisError::Database(e.to_string()))?;
    }

    Ok(())
}
