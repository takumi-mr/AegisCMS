use std::collections::{HashMap, HashSet};

use aegis_core::{AegisError, AuditEvent, AuditQuery, Result};
use aegis_schema::{FieldType, SchemaDefinition, SchemaMigration, SqlDialect};
use async_trait::async_trait;
use sqlx::mysql::{MySqlDatabaseError, MySqlPoolOptions, MySqlRow};
use sqlx::{MySql, Pool, Row};

use super::{
    audit_insert_values, audit_row_from_sqlx, build_audit_delete_before, build_audit_insert,
    build_audit_select, build_content_list, build_content_update, build_identity_insert,
    build_identity_select, build_identity_table, build_relation_lookup, build_scheduled_publish,
    build_scheduled_unpublish, build_setting_delete, build_setting_upsert, build_settings_select,
    build_settings_table, build_user_delete, build_user_insert, build_user_select,
    build_user_update, custom_columns, is_standard_column, user_row_from_sqlx, ContentListQuery,
    ContentPage, ContentRecord, CustomFieldValue, DatabaseAdapter, ResolvedRelation, UserRecord,
    AUDIT_TABLE, USER_AVATAR_COLUMN, USER_PENDING_APPROVAL_COLUMN, USER_TABLE,
};

#[derive(Debug, Clone)]
pub struct MySqlAdapter {
    pool: Pool<MySql>,
}

impl MySqlAdapter {
    pub async fn connect(database_url: &str, max_connections: u32) -> Result<Self> {
        let pool = MySqlPoolOptions::new()
            .max_connections(max_connections)
            .connect(database_url)
            .await
            .map_err(|e| AegisError::Database(e.to_string()))?;

        Ok(Self { pool })
    }

    #[allow(dead_code)]
    pub fn new(pool: Pool<MySql>) -> Self {
        Self { pool }
    }

    pub fn pool(&self) -> &Pool<MySql> {
        &self.pool
    }
}

/// MySQL のエラー番号 1061 (ER_DUP_KEYNAME: 同名インデックスが既に存在する) か
fn is_duplicate_key_name(error: &sqlx::Error) -> bool {
    error
        .as_database_error()
        .and_then(|e| e.try_downcast_ref::<MySqlDatabaseError>())
        .is_some_and(|e| e.number() == 1061)
}

/// 行からスキーマ定義の列型に合わせてカスタムフィールドを読み出す（NULL の列は含めない）
fn custom_fields_from_row(
    schema: &SchemaDefinition,
    row: &MySqlRow,
) -> HashMap<String, serde_json::Value> {
    let mut custom_fields = HashMap::new();
    for field in &schema.fields {
        if is_standard_column(&field.name) {
            continue;
        }
        let name = field.name.as_str();
        let value = match field.field_type {
            FieldType::Jsonb => row
                .try_get::<Option<serde_json::Value>, _>(name)
                .ok()
                .flatten(),
            FieldType::Integer => row
                .try_get::<Option<i64>, _>(name)
                .ok()
                .flatten()
                .map(serde_json::Value::from),
            FieldType::Boolean => row
                .try_get::<Option<bool>, _>(name)
                .ok()
                .flatten()
                .map(serde_json::Value::Bool),
            _ => row
                .try_get::<Option<String>, _>(name)
                .ok()
                .flatten()
                .map(|s| serde_json::from_str(&s).unwrap_or(serde_json::Value::String(s))),
        };
        if let Some(value) = value {
            custom_fields.insert(field.name.clone(), value);
        }
    }
    custom_fields
}

fn record_from_row(schema: &SchemaDefinition, r: &MySqlRow) -> ContentRecord {
    ContentRecord {
        id: r.get("id"),
        content_type: r.get("content_type"),
        title: r.get("title"),
        slug: r.get("slug"),
        status: r.get("status"),
        blocks_json: r.get("blocks_json"),
        author_id: r.get("author_id"),
        published_at: r.get("published_at"),
        unpublish_at: r.get("unpublish_at"),
        created_at: r.get("created_at"),
        updated_at: r.get("updated_at"),
        custom_fields: custom_fields_from_row(schema, r),
    }
}

super::orders::impl_order_storage!(MySqlAdapter, |_| "?".to_string(), "MySQL");
super::variants::impl_product_variant_storage!(MySqlAdapter, |_| "?".to_string(), "MySQL");
super::comments::impl_comment_storage!(MySqlAdapter, |_| "?".to_string(), "MySQL");
super::notifications::impl_notification_queue_storage!(MySqlAdapter, |_| "?".to_string(), "MySQL");
super::forum::impl_forum_storage!(MySqlAdapter, |_| "?".to_string(), "MySQL");

#[async_trait]
impl DatabaseAdapter for MySqlAdapter {
    async fn init(&self) -> Result<()> {
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS _aegis_migrations (
                id INT AUTO_INCREMENT PRIMARY KEY,
                content_type VARCHAR(255) NOT NULL UNIQUE,
                migrated_at VARCHAR(64) NOT NULL
            );
            "#,
        )
        .execute(&self.pool)
        .await
        .map_err(|e| AegisError::Database(e.to_string()))?;

        // 監査ログテーブル
        let audit_table_sql = format!(
            r#"
            CREATE TABLE IF NOT EXISTS {} (
                id VARCHAR(64) PRIMARY KEY,
                occurred_at VARCHAR(32) NOT NULL,
                action VARCHAR(64) NOT NULL,
                outcome VARCHAR(16) NOT NULL,
                actor_id VARCHAR(255),
                actor_role VARCHAR(64),
                resource_type VARCHAR(64),
                resource_id VARCHAR(255),
                ip VARCHAR(64),
                user_agent TEXT,
                details_json TEXT NOT NULL,
                INDEX idx_aegis_audit_logs_occurred_at (occurred_at),
                INDEX idx_aegis_audit_logs_action (action, occurred_at),
                INDEX idx_aegis_audit_logs_actor (actor_id, occurred_at),
                INDEX idx_aegis_audit_logs_resource (resource_type, resource_id)
            );
            "#,
            AUDIT_TABLE
        );
        sqlx::query(&audit_table_sql)
            .execute(&self.pool)
            .await
            .map_err(|e| AegisError::Database(e.to_string()))?;

        // ローカル認証のユーザーテーブル
        let user_table_sql = format!(
            r#"
            CREATE TABLE IF NOT EXISTS {} (
                id VARCHAR(64) PRIMARY KEY,
                username VARCHAR(255) NOT NULL UNIQUE,
                email VARCHAR(255),
                display_name VARCHAR(255),
                role VARCHAR(64) NOT NULL,
                password_hash TEXT NOT NULL,
                disabled BIGINT NOT NULL DEFAULT 0,
                created_at VARCHAR(64) NOT NULL,
                updated_at VARCHAR(64) NOT NULL,
                avatar_key VARCHAR(512),
                pending_approval BIGINT NOT NULL DEFAULT 0
            );
            "#,
            USER_TABLE
        );
        sqlx::query(&user_table_sql)
            .execute(&self.pool)
            .await
            .map_err(|e| AegisError::Database(e.to_string()))?;

        // 以前のバージョンで作成したテーブルに後から追加したカラムを足す
        // （MySQL は ADD COLUMN IF NOT EXISTS 非対応）
        let has_avatar_column: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM information_schema.columns \
             WHERE table_schema = DATABASE() AND table_name = ? AND column_name = ?;",
        )
        .bind(USER_TABLE)
        .bind(USER_AVATAR_COLUMN)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| AegisError::Database(e.to_string()))?;
        if has_avatar_column == 0 {
            sqlx::query(&format!(
                "ALTER TABLE {} ADD COLUMN {} VARCHAR(512);",
                USER_TABLE, USER_AVATAR_COLUMN
            ))
            .execute(&self.pool)
            .await
            .map_err(|e| AegisError::Database(e.to_string()))?;
        }

        let has_pending_approval_column: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM information_schema.columns \
             WHERE table_schema = DATABASE() AND table_name = ? AND column_name = ?;",
        )
        .bind(USER_TABLE)
        .bind(USER_PENDING_APPROVAL_COLUMN)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| AegisError::Database(e.to_string()))?;
        if has_pending_approval_column == 0 {
            sqlx::query(&format!(
                "ALTER TABLE {} ADD COLUMN {} BIGINT NOT NULL DEFAULT 0;",
                USER_TABLE, USER_PENDING_APPROVAL_COLUMN
            ))
            .execute(&self.pool)
            .await
            .map_err(|e| AegisError::Database(e.to_string()))?;
        }

        // 外部 IdP のアイデンティティとユーザーの対応表
        sqlx::query(&build_identity_table("VARCHAR(255)"))
            .execute(&self.pool)
            .await
            .map_err(|e| AegisError::Database(e.to_string()))?;

        // Admin Studio から変更したサイト全体の設定
        sqlx::query(&build_settings_table("VARCHAR(255)", "TEXT"))
            .execute(&self.pool)
            .await
            .map_err(|e| AegisError::Database(e.to_string()))?;

        // EC の注文
        let (order_table, order_indexes) =
            super::orders::build_order_table("TEXT", "VARCHAR(255)", "BIGINT", true);
        sqlx::query(&order_table)
            .execute(&self.pool)
            .await
            .map_err(|e| AegisError::Database(e.to_string()))?;
        for index in order_indexes {
            sqlx::query(&index)
                .execute(&self.pool)
                .await
                .map_err(|e| AegisError::Database(e.to_string()))?;
        }

        // 以前のバージョンで作成した注文テーブルに後から追加したカラムを足す
        let order_cols: Vec<String> = sqlx::query_scalar(
            "SELECT column_name FROM information_schema.columns \
             WHERE table_schema = DATABASE() AND table_name = ?;",
        )
        .bind(super::orders::ORDER_TABLE)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| AegisError::Database(e.to_string()))?;

        if !order_cols.iter().any(|c| c == "transaction_id") {
            sqlx::query(&format!(
                "ALTER TABLE {} ADD COLUMN transaction_id VARCHAR(255);",
                super::orders::ORDER_TABLE
            ))
            .execute(&self.pool)
            .await
            .map_err(|e| AegisError::Database(e.to_string()))?;
        }
        if !order_cols.iter().any(|c| c == "refunded_amount") {
            sqlx::query(&format!(
                "ALTER TABLE {} ADD COLUMN refunded_amount BIGINT NOT NULL DEFAULT 0;",
                super::orders::ORDER_TABLE
            ))
            .execute(&self.pool)
            .await
            .map_err(|e| AegisError::Database(e.to_string()))?;
        }
        if !order_cols.iter().any(|c| c == "refunds_json") {
            sqlx::query(&format!(
                "ALTER TABLE {} ADD COLUMN refunds_json TEXT;",
                super::orders::ORDER_TABLE
            ))
            .execute(&self.pool)
            .await
            .map_err(|e| AegisError::Database(e.to_string()))?;
            sqlx::query(&format!(
                "UPDATE {} SET refunds_json = '[]' WHERE refunds_json IS NULL;",
                super::orders::ORDER_TABLE
            ))
            .execute(&self.pool)
            .await
            .map_err(|e| AegisError::Database(e.to_string()))?;
        }

        // 商品バリエーション
        let (variant_table, variant_indexes) =
            super::variants::build_product_variant_table("TEXT", "VARCHAR(255)", "BIGINT", true);
        sqlx::query(&variant_table)
            .execute(&self.pool)
            .await
            .map_err(|e| AegisError::Database(e.to_string()))?;
        for index in variant_indexes {
            sqlx::query(&index)
                .execute(&self.pool)
                .await
                .map_err(|e| AegisError::Database(e.to_string()))?;
        }

        // コメント
        let (comment_table, comment_indexes) =
            super::comments::build_comment_table("TEXT", "VARCHAR(255)", "BIGINT", true);
        sqlx::query(&comment_table)
            .execute(&self.pool)
            .await
            .map_err(|e| AegisError::Database(e.to_string()))?;
        for index in comment_indexes {
            sqlx::query(&index)
                .execute(&self.pool)
                .await
                .map_err(|e| AegisError::Database(e.to_string()))?;
        }

        // 通知の送信待ち
        let (queue_table, queue_indexes) = super::notifications::build_notification_queue_table(
            "LONGTEXT",
            "VARCHAR(255)",
            "BIGINT",
            true,
        );
        sqlx::query(&queue_table)
            .execute(&self.pool)
            .await
            .map_err(|e| AegisError::Database(e.to_string()))?;
        for index in queue_indexes {
            sqlx::query(&index)
                .execute(&self.pool)
                .await
                .map_err(|e| AegisError::Database(e.to_string()))?;
        }

        // 掲示板
        let (forum_tables, forum_indexes) =
            super::forum::build_forum_tables("TEXT", "VARCHAR(255)", "BIGINT", true);
        for table in forum_tables {
            sqlx::query(&table)
                .execute(&self.pool)
                .await
                .map_err(|e| AegisError::Database(e.to_string()))?;
        }
        for index in forum_indexes {
            sqlx::query(&index)
                .execute(&self.pool)
                .await
                .map_err(|e| AegisError::Database(e.to_string()))?;
        }

        Ok(())
    }

    async fn run_migrations(&self, schemas: &HashMap<String, SchemaDefinition>) -> Result<()> {
        for (content_type, schema) in schemas {
            tracing::info!(
                "Applying MySQL declarative migration for schema: {}",
                content_type
            );
            let table = schema.table_name();

            // 1. テーブル作成
            let (create_table, indices) =
                SchemaMigration::generate_create_table(schema, SqlDialect::MySql);
            sqlx::query(&create_table)
                .execute(&self.pool)
                .await
                .map_err(|e| {
                    AegisError::Database(format!(
                        "MySQL table creation failed: {} - SQL: {}",
                        e, create_table
                    ))
                })?;

            // 2. 既存カラム一覧を取得し、未追加カラムを ALTER TABLE で追加
            let rows = sqlx::query(
                "SELECT column_name AS column_name FROM information_schema.columns WHERE table_schema = DATABASE() AND table_name = ?;",
            )
            .bind(&table)
            .fetch_all(&self.pool)
            .await
            .map_err(|e| AegisError::Database(e.to_string()))?;

            let existing_columns: HashSet<String> = rows
                .into_iter()
                .map(|r| r.get::<String, _>("column_name"))
                .collect();

            if !existing_columns.contains("unpublish_at") {
                let alter_sql =
                    format!("ALTER TABLE {} ADD COLUMN unpublish_at VARCHAR(64);", table);
                tracing::info!("Adding missing unpublish_at column (MySQL): {}", alter_sql);
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

            for field in &schema.fields {
                if !existing_columns.contains(&field.name) {
                    let sql_type = match field.field_type {
                        FieldType::Integer => "BIGINT",
                        FieldType::Boolean => "BOOLEAN",
                        FieldType::Text => "LONGTEXT",
                        FieldType::Jsonb => "JSON",
                        _ => "VARCHAR(255)",
                    };
                    let alter_sql = format!(
                        "ALTER TABLE {} ADD COLUMN {} {};",
                        table, field.name, sql_type
                    );
                    tracing::info!("Adding missing schema column (MySQL): {}", alter_sql);
                    sqlx::query(&alter_sql)
                        .execute(&self.pool)
                        .await
                        .map_err(|e| {
                            AegisError::Database(format!(
                                "MySQL alter table failed: {} - SQL: {}",
                                e, alter_sql
                            ))
                        })?;
                }
            }

            // 3. インデックス作成（MySQL は CREATE INDEX IF NOT EXISTS 非対応のため、既存インデックスの重複エラーのみ無視）
            for idx_sql in indices {
                if let Err(e) = sqlx::query(&idx_sql).execute(&self.pool).await {
                    if !is_duplicate_key_name(&e) {
                        return Err(AegisError::Database(format!(
                            "MySQL index creation failed: {} - SQL: {}",
                            e, idx_sql
                        )));
                    }
                }
            }

            let now = chrono::Utc::now().to_rfc3339();
            sqlx::query(
                r#"
                INSERT INTO _aegis_migrations (content_type, migrated_at)
                VALUES (?, ?)
                ON DUPLICATE KEY UPDATE migrated_at = VALUES(migrated_at);
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
            "unpublish_at".to_string(),
            "created_at".to_string(),
            "updated_at".to_string(),
        ];
        let mut custom_values: Vec<CustomFieldValue> = Vec::new();

        for field in &schema.fields {
            if is_standard_column(&field.name) {
                continue;
            }
            columns.push(field.name.clone());
            custom_values.push(CustomFieldValue::from_json(
                field,
                record.custom_fields.get(&field.name),
            )?);
        }

        let placeholders: Vec<String> = vec!["?".to_string(); columns.len()];
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
            .bind(&record.unpublish_at)
            .bind(&record.created_at)
            .bind(&record.updated_at);

        for val in custom_values {
            query = match val {
                CustomFieldValue::Text(v) => query.bind(v),
                CustomFieldValue::BigInt(v) => query.bind(v),
                CustomFieldValue::Bool(v) => query.bind(v),
                CustomFieldValue::Json(v) => query.bind(v.map(sqlx::types::Json)),
            };
        }

        query
            .execute(&self.pool)
            .await
            .map_err(|e| AegisError::Database(format!("MySQL INSERT failed: {}", e)))?;

        Ok(())
    }

    async fn update_content(
        &self,
        schema: &SchemaDefinition,
        record: &ContentRecord,
    ) -> Result<bool> {
        let sql = build_content_update(&schema.table_name(), &custom_columns(schema), |_| {
            "?".to_string()
        });
        let mut query = sqlx::query(&sql)
            .bind(&record.title)
            .bind(&record.slug)
            .bind(&record.status)
            .bind(&record.blocks_json)
            .bind(&record.author_id)
            .bind(&record.published_at)
            .bind(&record.unpublish_at)
            .bind(&record.updated_at);
        for field in schema
            .fields
            .iter()
            .filter(|f| !is_standard_column(&f.name))
        {
            query = match CustomFieldValue::from_json(field, record.custom_fields.get(&field.name))?
            {
                CustomFieldValue::Text(v) => query.bind(v),
                CustomFieldValue::BigInt(v) => query.bind(v),
                CustomFieldValue::Bool(v) => query.bind(v),
                CustomFieldValue::Json(v) => query.bind(v.map(sqlx::types::Json)),
            };
        }
        let result = query
            .bind(&record.id)
            .execute(&self.pool)
            .await
            .map_err(|e| AegisError::Database(format!("MySQL UPDATE failed: {}", e)))?;
        Ok(result.rows_affected() > 0)
    }

    async fn publish_scheduled_content(
        &self,
        schema: &SchemaDefinition,
        id: &str,
        published_at: &str,
        updated_at: &str,
    ) -> Result<bool> {
        let sql = build_scheduled_publish(&schema.table_name(), |_| "?".to_string());
        let result = sqlx::query(&sql)
            .bind(updated_at)
            .bind(id)
            .bind(published_at)
            .execute(&self.pool)
            .await
            .map_err(|e| AegisError::Database(format!("MySQL UPDATE failed: {}", e)))?;
        Ok(result.rows_affected() > 0)
    }

    async fn unpublish_scheduled_content(
        &self,
        schema: &SchemaDefinition,
        id: &str,
        unpublish_at: &str,
        updated_at: &str,
    ) -> Result<bool> {
        let sql = build_scheduled_unpublish(&schema.table_name(), |_| "?".to_string());
        let result = sqlx::query(&sql)
            .bind(updated_at)
            .bind(id)
            .bind(unpublish_at)
            .execute(&self.pool)
            .await
            .map_err(|e| AegisError::Database(format!("MySQL UPDATE failed: {}", e)))?;
        Ok(result.rows_affected() > 0)
    }

    async fn delete_content(&self, schema: &SchemaDefinition, id: &str) -> Result<bool> {
        let result = sqlx::query(&format!(
            "DELETE FROM {} WHERE id = ?;",
            schema.table_name()
        ))
        .bind(id)
        .execute(&self.pool)
        .await
        .map_err(|e| AegisError::Database(e.to_string()))?;
        Ok(result.rows_affected() > 0)
    }

    async fn get_content(
        &self,
        schema: &SchemaDefinition,
        slug_or_id: &str,
    ) -> Result<Option<ContentRecord>> {
        let table = schema.table_name();
        let query = format!("SELECT * FROM {} WHERE slug = ? OR id = ? LIMIT 1;", table);

        let row = sqlx::query(&query)
            .bind(slug_or_id)
            .bind(slug_or_id)
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| AegisError::Database(e.to_string()))?;

        Ok(row.map(|r| record_from_row(schema, &r)))
    }

    async fn list_contents_page(
        &self,
        schema: &SchemaDefinition,
        query: &ContentListQuery,
    ) -> Result<ContentPage> {
        let (select, count, binds) =
            build_content_list(&schema.table_name(), query, |_| "?".to_string());

        let mut rows_query = sqlx::query(&select);
        let mut count_query = sqlx::query_scalar::<_, i64>(&count);
        for value in &binds {
            rows_query = rows_query.bind(value);
            count_query = count_query.bind(value);
        }
        let rows = rows_query
            .fetch_all(&self.pool)
            .await
            .map_err(|e| AegisError::Database(e.to_string()))?;
        let total = count_query
            .fetch_one(&self.pool)
            .await
            .map_err(|e| AegisError::Database(e.to_string()))?;

        Ok(ContentPage {
            records: rows.iter().map(|r| record_from_row(schema, r)).collect(),
            total: total.max(0) as u64,
        })
    }

    async fn resolve_relations(
        &self,
        target_schema: &SchemaDefinition,
        target_field: &str,
        ref_ids: &[String],
    ) -> Result<HashMap<String, ResolvedRelation>> {
        if ref_ids.is_empty() {
            return Ok(HashMap::new());
        }
        let query = build_relation_lookup(
            &target_schema.table_name(),
            target_field,
            ref_ids.len(),
            |_| "?".to_string(),
        );
        let mut lookup = sqlx::query(&query);
        for id in ref_ids {
            lookup = lookup.bind(id);
        }
        let rows = lookup
            .fetch_all(&self.pool)
            .await
            .map_err(|e| AegisError::Database(e.to_string()))?;

        Ok(rows
            .iter()
            .map(|r| {
                (
                    r.get("ref_key"),
                    ResolvedRelation {
                        id: r.get("id"),
                        content_type: r.get("content_type"),
                        title: r.get("title"),
                        slug: r.get("slug"),
                    },
                )
            })
            .collect())
    }

    async fn insert_audit_event(&self, event: &AuditEvent) -> Result<()> {
        let sql = build_audit_insert(|_| "?".to_string());
        let mut query = sqlx::query(&sql);
        for value in audit_insert_values(event) {
            query = query.bind(value);
        }
        query
            .execute(&self.pool)
            .await
            .map_err(|e| AegisError::Database(e.to_string()))?;
        Ok(())
    }

    async fn query_audit_events(&self, audit_query: &AuditQuery) -> Result<Vec<AuditEvent>> {
        let (sql, binds) = build_audit_select(audit_query, |_| "?".to_string());
        let mut query = sqlx::query(&sql);
        for value in binds {
            query = query.bind(value);
        }
        let rows = query
            .fetch_all(&self.pool)
            .await
            .map_err(|e| AegisError::Database(e.to_string()))?;

        let mut events = Vec::with_capacity(rows.len());
        for row in &rows {
            events.push(audit_row_from_sqlx!(row).into_event()?);
        }
        Ok(events)
    }

    async fn delete_audit_events_before(&self, cutoff: &str) -> Result<u64> {
        let result = sqlx::query(&build_audit_delete_before("?"))
            .bind(cutoff)
            .execute(&self.pool)
            .await
            .map_err(|e| AegisError::Database(e.to_string()))?;
        Ok(result.rows_affected())
    }

    async fn insert_user(&self, user: &UserRecord) -> Result<()> {
        sqlx::query(&build_user_insert(|_| "?".to_string()))
            .bind(&user.id)
            .bind(&user.username)
            .bind(&user.email)
            .bind(&user.display_name)
            .bind(&user.role)
            .bind(&user.password_hash)
            .bind(i64::from(user.disabled))
            .bind(&user.created_at)
            .bind(&user.updated_at)
            .bind(&user.avatar_key)
            .bind(i64::from(user.pending_approval))
            .execute(&self.pool)
            .await
            .map_err(|e| AegisError::Database(e.to_string()))?;
        Ok(())
    }

    async fn get_user_by_username(&self, username: &str) -> Result<Option<UserRecord>> {
        let row = sqlx::query(&build_user_select(Some("username = ?")))
            .bind(username)
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| AegisError::Database(e.to_string()))?;
        match row {
            Some(row) => Ok(Some(user_row_from_sqlx!(&row))),
            None => Ok(None),
        }
    }

    async fn get_identity_link(&self, provider: &str, subject: &str) -> Result<Option<String>> {
        let row = sqlx::query(&build_identity_select(|_| "?".to_string()))
            .bind(provider)
            .bind(subject)
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| AegisError::Database(e.to_string()))?;
        row.map(|row| row.try_get::<String, _>("username"))
            .transpose()
            .map_err(|e| AegisError::Database(e.to_string()))
    }

    async fn insert_identity_link(
        &self,
        provider: &str,
        subject: &str,
        username: &str,
    ) -> Result<()> {
        sqlx::query(&build_identity_insert(|_| "?".to_string()))
            .bind(provider)
            .bind(subject)
            .bind(username)
            .bind(chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true))
            .execute(&self.pool)
            .await
            .map_err(|e| AegisError::Database(e.to_string()))?;
        Ok(())
    }

    async fn list_settings(&self) -> Result<HashMap<String, String>> {
        let rows = sqlx::query(&build_settings_select())
            .fetch_all(&self.pool)
            .await
            .map_err(|e| AegisError::Database(e.to_string()))?;
        rows.into_iter()
            .map(|row| Ok((row.try_get("name")?, row.try_get("value")?)))
            .collect::<std::result::Result<_, sqlx::Error>>()
            .map_err(|e| AegisError::Database(e.to_string()))
    }

    async fn set_setting(&self, name: &str, value: &str) -> Result<()> {
        sqlx::query(&build_setting_upsert(|_| "?".to_string(), true))
            .bind(name)
            .bind(value)
            .bind(chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true))
            .execute(&self.pool)
            .await
            .map_err(|e| AegisError::Database(e.to_string()))?;
        Ok(())
    }

    async fn delete_setting(&self, name: &str) -> Result<()> {
        sqlx::query(&build_setting_delete(|_| "?".to_string()))
            .bind(name)
            .execute(&self.pool)
            .await
            .map_err(|e| AegisError::Database(e.to_string()))?;
        Ok(())
    }

    async fn update_user(&self, user: &UserRecord) -> Result<()> {
        sqlx::query(&build_user_update(|_| "?".to_string()))
            .bind(&user.email)
            .bind(&user.display_name)
            .bind(&user.role)
            .bind(&user.password_hash)
            .bind(i64::from(user.disabled))
            .bind(&user.avatar_key)
            .bind(i64::from(user.pending_approval))
            .bind(&user.updated_at)
            .bind(&user.id)
            .execute(&self.pool)
            .await
            .map_err(|e| AegisError::Database(e.to_string()))?;
        Ok(())
    }

    async fn list_users(&self) -> Result<Vec<UserRecord>> {
        let rows = sqlx::query(&build_user_select(None))
            .fetch_all(&self.pool)
            .await
            .map_err(|e| AegisError::Database(e.to_string()))?;
        let mut users = Vec::with_capacity(rows.len());
        for row in &rows {
            users.push(user_row_from_sqlx!(row));
        }
        Ok(users)
    }

    async fn delete_user(&self, user: &UserRecord) -> Result<()> {
        let (delete_user, delete_links) = build_user_delete(|_| "?".to_string());
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|e| AegisError::Database(e.to_string()))?;
        sqlx::query(&delete_links)
            .bind(&user.username)
            .execute(&mut *tx)
            .await
            .map_err(|e| AegisError::Database(e.to_string()))?;
        sqlx::query(&delete_user)
            .bind(&user.id)
            .execute(&mut *tx)
            .await
            .map_err(|e| AegisError::Database(e.to_string()))?;
        tx.commit()
            .await
            .map_err(|e| AegisError::Database(e.to_string()))
    }
}
