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
    use sqlx::Row;
    use std::collections::HashSet;

    for (content_type, schema) in schemas {
        tracing::info!(
            "Applying declarative migration for schema: {}",
            content_type
        );
        let table = schema.table_name();

        // 1. テーブル作成
        let (create_table, indices) = SqliteMigration::generate_create_table(schema);
        sqlx::query(&create_table)
            .execute(pool)
            .await
            .map_err(|e| {
                AegisError::Database(format!(
                    "Table creation failed: {} - SQL: {}",
                    e, create_table
                ))
            })?;

        // 2. 既存カラム一覧を取得し、スキーマに定義されている未追加カラムを ALTER TABLE で追加
        let rows = sqlx::query(&format!("PRAGMA table_info({});", table))
            .fetch_all(pool)
            .await
            .map_err(|e| AegisError::Database(e.to_string()))?;

        let existing_columns: HashSet<String> = rows
            .into_iter()
            .map(|r| r.get::<String, _>("name"))
            .collect();

        for field in &schema.fields {
            if !existing_columns.contains(&field.name) {
                let sql_type = match field.field_type {
                    aegis_schema::FieldType::Integer | aegis_schema::FieldType::Boolean => {
                        "INTEGER"
                    }
                    _ => "TEXT",
                };
                let alter_sql = format!(
                    "ALTER TABLE {} ADD COLUMN {} {};",
                    table, field.name, sql_type
                );
                tracing::info!("Adding missing schema column: {}", alter_sql);
                sqlx::query(&alter_sql).execute(pool).await.map_err(|e| {
                    AegisError::Database(format!("Alter table failed: {} - SQL: {}", e, alter_sql))
                })?;
            }
        }

        // 3. インデックス作成
        for idx_sql in indices {
            sqlx::query(&idx_sql).execute(pool).await.map_err(|e| {
                AegisError::Database(format!("Index creation failed: {} - SQL: {}", e, idx_sql))
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
