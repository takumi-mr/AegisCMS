use std::collections::{HashMap, HashSet};
use std::str::FromStr;

use aegis_core::{AegisError, Result};
use aegis_schema::{FieldType, SchemaDefinition, SchemaMigration, SqlDialect};
use async_trait::async_trait;
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::{Pool, Row, Sqlite};

use super::{is_standard_column, ContentRecord, DatabaseAdapter, ResolvedRelation};

#[derive(Debug, Clone)]
pub struct SqliteAdapter {
    pool: Pool<Sqlite>,
}

impl SqliteAdapter {
    pub async fn connect(database_url: &str, max_connections: u32) -> Result<Self> {
        let options = SqliteConnectOptions::from_str(database_url)
            .map_err(|e| AegisError::Database(e.to_string()))?
            .create_if_missing(true);

        let pool = SqlitePoolOptions::new()
            .max_connections(max_connections)
            .connect_with(options)
            .await
            .map_err(|e| AegisError::Database(e.to_string()))?;

        Ok(Self { pool })
    }

    #[allow(dead_code)]
    pub fn new(pool: Pool<Sqlite>) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl DatabaseAdapter for SqliteAdapter {
    async fn init(&self) -> Result<()> {
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS _aegis_migrations (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                content_type TEXT NOT NULL UNIQUE,
                migrated_at TEXT NOT NULL
            );
            "#,
        )
        .execute(&self.pool)
        .await
        .map_err(|e| AegisError::Database(e.to_string()))?;

        Ok(())
    }

    async fn run_migrations(&self, schemas: &HashMap<String, SchemaDefinition>) -> Result<()> {
        for (content_type, schema) in schemas {
            tracing::info!(
                "Applying SQLite declarative migration for schema: {}",
                content_type
            );
            let table = schema.table_name();

            // 1. テーブル作成
            let (create_table, indices) =
                SchemaMigration::generate_create_table(schema, SqlDialect::Sqlite);
            sqlx::query(&create_table)
                .execute(&self.pool)
                .await
                .map_err(|e| {
                    AegisError::Database(format!(
                        "Table creation failed: {} - SQL: {}",
                        e, create_table
                    ))
                })?;

            // 2. 既存カラム一覧を取得し、未追加カラムを ALTER TABLE で追加
            let rows = sqlx::query(&format!("PRAGMA table_info({});", table))
                .fetch_all(&self.pool)
                .await
                .map_err(|e| AegisError::Database(e.to_string()))?;

            let existing_columns: HashSet<String> = rows
                .into_iter()
                .map(|r| r.get::<String, _>("name"))
                .collect();

            for field in &schema.fields {
                if !existing_columns.contains(&field.name) {
                    let sql_type = match field.field_type {
                        FieldType::Integer | FieldType::Boolean => "INTEGER",
                        _ => "TEXT",
                    };
                    let alter_sql = format!(
                        "ALTER TABLE {} ADD COLUMN {} {};",
                        table, field.name, sql_type
                    );
                    tracing::info!("Adding missing schema column (SQLite): {}", alter_sql);
                    sqlx::query(&alter_sql)
                        .execute(&self.pool)
                        .await
                        .map_err(|e| {
                            AegisError::Database(format!(
                                "Alter table failed: {} - SQL: {}",
                                e, alter_sql
                            ))
                        })?;
                }
            }

            // 3. インデックス作成
            for idx_sql in indices {
                sqlx::query(&idx_sql)
                    .execute(&self.pool)
                    .await
                    .map_err(|e| {
                        AegisError::Database(format!(
                            "Index creation failed: {} - SQL: {}",
                            e, idx_sql
                        ))
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
            .execute(&self.pool)
            .await
            .map_err(|e| AegisError::Database(e.to_string()))?;
        }

        Ok(())
    }

    async fn insert_content(
        &self,
        schema: &SchemaDefinition,
        record: &ContentRecord,
    ) -> Result<()> {
        let table = schema.table_name();
        let mut columns = vec![
            "id".to_string(),
            "content_type".to_string(),
            "title".to_string(),
            "slug".to_string(),
            "status".to_string(),
            "blocks_json".to_string(),
            "author_id".to_string(),
            "published_at".to_string(),
            "created_at".to_string(),
            "updated_at".to_string(),
        ];
        let mut custom_values: Vec<Option<String>> = Vec::new();

        for field in &schema.fields {
            if is_standard_column(&field.name) {
                continue;
            }
            columns.push(field.name.clone());
            if let Some(val) = record.custom_fields.get(&field.name) {
                let s = match val {
                    serde_json::Value::String(s) => s.clone(),
                    serde_json::Value::Number(n) => n.to_string(),
                    serde_json::Value::Bool(b) => {
                        if *b {
                            "1".to_string()
                        } else {
                            "0".to_string()
                        }
                    }
                    serde_json::Value::Null => String::new(),
                    other => other.to_string(),
                };
                custom_values.push(Some(s));
            } else {
                custom_values.push(None);
            }
        }

        let placeholders: Vec<String> = (1..=columns.len()).map(|i| format!("?{}", i)).collect();
        let query_str = format!(
            "INSERT INTO {} ({}) VALUES ({});",
            table,
            columns.join(", "),
            placeholders.join(", ")
        );

        let mut query = sqlx::query(&query_str)
            .bind(&record.id)
            .bind(&record.content_type)
            .bind(&record.title)
            .bind(&record.slug)
            .bind(&record.status)
            .bind(&record.blocks_json)
            .bind(&record.author_id)
            .bind(&record.published_at)
            .bind(&record.created_at)
            .bind(&record.updated_at);

        for val in &custom_values {
            query = query.bind(val);
        }

        query
            .execute(&self.pool)
            .await
            .map_err(|e| AegisError::Database(format!("SQLite INSERT failed: {}", e)))?;

        Ok(())
    }

    async fn get_content(
        &self,
        schema: &SchemaDefinition,
        slug_or_id: &str,
    ) -> Result<Option<ContentRecord>> {
        let table = schema.table_name();
        let query = format!(
            "SELECT * FROM {} WHERE slug = ?1 OR id = ?1 LIMIT 1;",
            table
        );

        let row = sqlx::query(&query)
            .bind(slug_or_id)
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| AegisError::Database(e.to_string()))?;

        match row {
            Some(r) => {
                let mut custom_fields = HashMap::new();
                for field in &schema.fields {
                    if is_standard_column(&field.name) {
                        continue;
                    }
                    if let Ok(val) = r.try_get::<String, _>(field.name.as_str()) {
                        let parsed: serde_json::Value =
                            serde_json::from_str(&val).unwrap_or(serde_json::Value::String(val));
                        custom_fields.insert(field.name.clone(), parsed);
                    } else if let Ok(val) = r.try_get::<i64, _>(field.name.as_str()) {
                        custom_fields
                            .insert(field.name.clone(), serde_json::Value::Number(val.into()));
                    }
                }

                Ok(Some(ContentRecord {
                    id: r.get("id"),
                    content_type: r.get("content_type"),
                    title: r.get("title"),
                    slug: r.get("slug"),
                    status: r.get("status"),
                    blocks_json: r.get("blocks_json"),
                    author_id: r.get("author_id"),
                    published_at: r.get("published_at"),
                    created_at: r.get("created_at"),
                    updated_at: r.get("updated_at"),
                    custom_fields,
                }))
            }
            None => Ok(None),
        }
    }

    async fn list_contents(&self, schema: &SchemaDefinition) -> Result<Vec<ContentRecord>> {
        let table = schema.table_name();
        let query = format!("SELECT * FROM {} ORDER BY created_at DESC;", table);

        let rows = sqlx::query(&query)
            .fetch_all(&self.pool)
            .await
            .map_err(|e| AegisError::Database(e.to_string()))?;

        let mut results = Vec::new();
        for r in rows {
            let mut custom_fields = HashMap::new();
            for field in &schema.fields {
                if is_standard_column(&field.name) {
                    continue;
                }
                if let Ok(val) = r.try_get::<String, _>(field.name.as_str()) {
                    let parsed: serde_json::Value =
                        serde_json::from_str(&val).unwrap_or(serde_json::Value::String(val));
                    custom_fields.insert(field.name.clone(), parsed);
                } else if let Ok(val) = r.try_get::<i64, _>(field.name.as_str()) {
                    custom_fields.insert(field.name.clone(), serde_json::Value::Number(val.into()));
                }
            }

            results.push(ContentRecord {
                id: r.get("id"),
                content_type: r.get("content_type"),
                title: r.get("title"),
                slug: r.get("slug"),
                status: r.get("status"),
                blocks_json: r.get("blocks_json"),
                author_id: r.get("author_id"),
                published_at: r.get("published_at"),
                created_at: r.get("created_at"),
                updated_at: r.get("updated_at"),
                custom_fields,
            });
        }

        Ok(results)
    }

    async fn resolve_relation(
        &self,
        target_schema: &SchemaDefinition,
        target_field: &str,
        ref_id: &str,
    ) -> Result<Option<ResolvedRelation>> {
        let target_table = target_schema.table_name();
        let query = format!(
            "SELECT id, content_type, title, slug FROM {} WHERE {} = ?1 LIMIT 1;",
            target_table, target_field
        );

        let row = sqlx::query(&query)
            .bind(ref_id)
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| AegisError::Database(e.to_string()))?;

        match row {
            Some(r) => Ok(Some(ResolvedRelation {
                id: r.get("id"),
                content_type: r.get("content_type"),
                title: r.get("title"),
                slug: r.get("slug"),
            })),
            None => Ok(None),
        }
    }
}
