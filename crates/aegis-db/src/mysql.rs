use std::collections::{HashMap, HashSet};

use crate::sqlx_compat as sqlx;
use aegis_core::{AegisError, AuditEvent, AuditQuery, Result};
use aegis_schema::{FieldType, SchemaDefinition, SchemaMigration, SqlDialect};
use async_trait::async_trait;
use sqlx::mysql::{MySqlDatabaseError, MySqlPoolOptions, MySqlRow};
use sqlx::{MySql, MySqlConnection, Pool, Row};

use super::{
    audit_insert_values, audit_row_from_sqlx, build_audit_delete_before, build_audit_insert,
    build_audit_select, build_column_probe, build_content_list, build_content_update,
    build_identity_insert, build_identity_select, build_published_relation_lookup,
    build_relation_lookup, build_scheduled_publish, build_scheduled_unpublish,
    build_setting_delete, build_setting_upsert, build_settings_select, build_user_delete,
    build_user_insert, build_user_select, build_user_update, custom_columns, is_standard_column,
    outdated_database_error, user_row_from_sqlx, ContentListQuery, ContentPage,
    ContentPublicationChange, ContentRecord, ContentVersionRecord, CustomFieldValue,
    DatabaseAdapter, PublishedNeighborQuery, ResolvedRelation, UserRecord, REQUIRED_COLUMNS,
};

async fn apply_publication_change(
    connection: &mut MySqlConnection,
    versions: &[ContentVersionRecord],
    revisions: &[i64],
    change: ContentPublicationChange,
    plan: &crate::published_columns::PublishedColumnsPlan,
) -> Result<()> {
    let index = match change {
        ContentPublicationChange::Keep => return Ok(()),
        ContentPublicationChange::Unpublish => {
            if let Some(version) = versions.first() {
                sqlx::query("DELETE FROM _aegis_content_publications WHERE content_type = ? AND content_id = ?")
                    .bind(&version.content_type).bind(&version.content_id).execute(&mut *connection).await
                    .map_err(|e| AegisError::Database(e.to_string()))?;
                MySqlAdapter::delete_published_row(
                    connection,
                    plan,
                    &version.content_type,
                    &version.content_id,
                )
                .await?;
            }
            return Ok(());
        }
        ContentPublicationChange::PublishVersion(index) => index,
        ContentPublicationChange::EnsureVersion(index) => {
            let Some(version) = versions.get(index) else {
                return Err(AegisError::Database(
                    "Publication snapshot index is out of range".into(),
                ));
            };
            let exists: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM _aegis_content_publications WHERE content_type = ? AND content_id = ?")
                .bind(&version.content_type).bind(&version.content_id).fetch_one(&mut *connection).await
                .map_err(|e| AegisError::Database(e.to_string()))?;
            if exists > 0 {
                return Ok(());
            }
            index
        }
    };
    let version = versions
        .get(index)
        .ok_or_else(|| AegisError::Database("Publication snapshot index is out of range".into()))?;
    let revision = *revisions
        .get(index)
        .ok_or_else(|| AegisError::Database("Publication revision index is out of range".into()))?;
    let record = super::record_from_content_version(version)?;
    let (category_id, sort_at) = super::publication_columns(&record);
    sqlx::query("INSERT INTO _aegis_content_publications (content_type, content_id, revision, slug, title, status, published_at, unpublish_at, created_at, updated_at, category_id, sort_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?) ON DUPLICATE KEY UPDATE revision = VALUES(revision), slug = VALUES(slug), title = VALUES(title), status = VALUES(status), published_at = VALUES(published_at), unpublish_at = VALUES(unpublish_at), created_at = VALUES(created_at), updated_at = VALUES(updated_at), category_id = VALUES(category_id), sort_at = VALUES(sort_at)")
        .bind(&version.content_type).bind(&version.content_id).bind(revision).bind(&record.slug).bind(&record.title)
        .bind(&record.status).bind(&record.published_at).bind(&record.unpublish_at).bind(&record.created_at).bind(&record.updated_at)
        .bind(&category_id).bind(&sort_at)
        .execute(&mut *connection).await.map_err(|e| AegisError::Database(e.to_string()))?;
    MySqlAdapter::write_published_row(connection, plan, &record).await?;
    Ok(())
}

/// システムテーブルの手順（`migrations/mysql/NNNN_*.sql`）。コンパイル時にバイナリへ埋め込む
pub(crate) static MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!("migrations/mysql");

#[derive(Debug, Clone)]
pub struct MySqlAdapter {
    pool: Pool<MySql>,
    published: crate::published_exec::SharedPlan,
}

impl MySqlAdapter {
    pub async fn connect(database_url: &str, max_connections: u32) -> Result<Self> {
        let pool = MySqlPoolOptions::new()
            .max_connections(max_connections)
            .connect(database_url)
            .await
            .map_err(|e| AegisError::Database(e.to_string()))?;

        Ok(Self::new(pool))
    }

    #[allow(dead_code)]
    pub fn new(pool: Pool<MySql>) -> Self {
        Self {
            pool,
            published: crate::published_exec::new_shared_plan(),
        }
    }

    /// 公開の記録の一覧用の索引を用意する
    pub fn pool(&self) -> &Pool<MySql> {
        &self.pool
    }

    fn published_records(rows: &[MySqlRow]) -> Result<Vec<ContentRecord>> {
        rows.iter()
            .map(|row| {
                super::published_record_from_snapshot(
                    &row.get::<String, _>("snapshot_json"),
                    row.get("status"),
                    row.get("published_at"),
                    row.get("unpublish_at"),
                    row.get("updated_at"),
                )
            })
            .collect()
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
super::tags::impl_tag_storage!(MySqlAdapter, |_| "?".to_string(), "MySQL");
super::contact::impl_contact_storage!(MySqlAdapter, |_| "?".to_string(), "MySQL");
super::newsletter::impl_newsletter_storage!(MySqlAdapter, |_| "?".to_string(), "MySQL");
super::analytics::impl_analytics_storage!(
    MySqlAdapter,
    |_| "?".to_string(),
    super::analytics::UPSERT_MYSQL,
    "SIGNED",
    "MySQL"
);
super::notifications::impl_notification_queue_storage!(MySqlAdapter, |_| "?".to_string(), "MySQL");
super::forum::impl_forum_storage!(MySqlAdapter, |_| "?".to_string(), "MySQL");
super::members::impl_member_storage!(MySqlAdapter, |_| "?".to_string(), "MySQL");
super::purchases::impl_purchase_storage!(MySqlAdapter, |_| "?".to_string(), "MySQL");
super::subscriptions::impl_subscription_storage!(MySqlAdapter, |_| "?".to_string(), "MySQL");
super::api_tokens::impl_api_token_storage!(MySqlAdapter, |_| "?".to_string(), "MySQL");
super::reviews::impl_review_storage!(MySqlAdapter, |_| "?".to_string(), "MySQL");

crate::published_exec::impl_published_columns!(
    MySqlAdapter,
    SqlDialect::MySql,
    MySqlConnection,
    |b: bool| b,
    bool
);

#[async_trait]
impl DatabaseAdapter for MySqlAdapter {
    async fn init(&self) -> Result<()> {
        // システムテーブルは `migrations/mysql/` の手順で作る・更新する（適用済みの手順は
        // `_sqlx_migrations` に残る）。手順は番号順に、まだ適用していないものだけを流す
        MIGRATOR
            .run(&self.pool)
            .await
            .map_err(|e| super::migration_error(&e))?;

        // v0.4 より前の版で作った DB は、`0001_baseline` が「作成済み」として何もしないので、
        // 足りない列を黙って足さずに、移行の手順を示して止める
        for (table, columns) in REQUIRED_COLUMNS {
            sqlx::query(&build_column_probe(table, columns))
                .execute(&self.pool)
                .await
                .map_err(|e| outdated_database_error(table, columns, &e.to_string()))?;
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

            // 古い版で作ったテーブルには `unpublish_at` が無い。足さずに、移行の手順を示して止める
            sqlx::query(&build_column_probe(&table, &["unpublish_at"]))
                .execute(&self.pool)
                .await
                .map_err(|e| outdated_database_error(&table, &["unpublish_at"], &e.to_string()))?;

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

            for field in &schema.fields {
                if !existing_columns.contains(&field.name) {
                    let sql_type =
                        SchemaMigration::column_type(field.field_type, SqlDialect::MySql);
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
        }
        Ok(())
    }

    async fn configure_published_columns(
        &self,
        plan: crate::published_columns::PublishedColumnsPlan,
    ) -> Result<()> {
        self.configure_published(plan, false).await
    }

    async fn rebuild_published_columns(
        &self,
        plan: crate::published_columns::PublishedColumnsPlan,
    ) -> Result<()> {
        self.configure_published(plan, true).await
    }

    async fn run_query(
        &self,
        query: &aegis_schema::QueryDefinition,
        schema: &SchemaDefinition,
        max_limit: usize,
    ) -> Result<crate::published_exec::QueryRows> {
        self.run_published_query(query, schema, max_limit).await
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

        super::ensure_publication(self, schema, record).await?;
        Ok(())
    }

    async fn insert_content_with_versions(
        &self,
        schema: &SchemaDefinition,
        record: &ContentRecord,
        versions: &[ContentVersionRecord],
        publication: ContentPublicationChange,
    ) -> Result<()> {
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|e| AegisError::Database(e.to_string()))?;
        let mut columns = vec![
            "id",
            "content_type",
            "title",
            "slug",
            "status",
            "blocks_json",
            "author_id",
            "published_at",
            "unpublish_at",
            "created_at",
            "updated_at",
        ]
        .into_iter()
        .map(str::to_owned)
        .collect::<Vec<_>>();
        let mut values = Vec::new();
        for field in schema
            .fields
            .iter()
            .filter(|f| !is_standard_column(&f.name))
        {
            columns.push(field.name.clone());
            values.push(CustomFieldValue::from_json(
                field,
                record.custom_fields.get(&field.name),
            )?);
        }
        let placeholders = vec!["?".to_string(); columns.len()];
        let sql = format!(
            "INSERT INTO {} ({}) VALUES ({})",
            schema.table_name(),
            columns.join(", "),
            placeholders.join(", ")
        );
        let mut query = sqlx::query(&sql)
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
        for value in values {
            query = match value {
                CustomFieldValue::Text(v) => query.bind(v),
                CustomFieldValue::BigInt(v) => query.bind(v),
                CustomFieldValue::Bool(v) => query.bind(v),
                CustomFieldValue::Json(v) => query.bind(v.map(sqlx::types::Json)),
            };
        }
        query
            .execute(&mut *tx)
            .await
            .map_err(|e| AegisError::Database(format!("MySQL INSERT failed: {}", e)))?;
        let mut revisions = Vec::with_capacity(versions.len());
        for version in versions {
            let revision: i64 = sqlx::query_scalar("SELECT COALESCE(MAX(revision), 0) + 1 FROM _aegis_content_versions WHERE content_type = ? AND content_id = ?")
                .bind(&version.content_type).bind(&version.content_id).fetch_one(&mut *tx).await.map_err(|e| AegisError::Database(e.to_string()))?;
            sqlx::query("INSERT INTO _aegis_content_versions (content_type, content_id, revision, snapshot_json, created_at, created_by, change_note) VALUES (?, ?, ?, ?, ?, ?, ?)")
                .bind(&version.content_type).bind(&version.content_id).bind(revision).bind(&version.snapshot_json)
                .bind(&version.created_at).bind(&version.created_by).bind(&version.change_note).execute(&mut *tx).await
                .map_err(|e| AegisError::Database(e.to_string()))?;
            revisions.push(revision);
        }
        apply_publication_change(
            &mut tx,
            versions,
            &revisions,
            publication,
            &crate::published_exec::current_plan(&self.published),
        )
        .await?;
        tx.commit()
            .await
            .map_err(|e| AegisError::Database(e.to_string()))?;
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
        let updated = result.rows_affected() > 0;
        if updated {
            super::ensure_publication(self, schema, record).await?;
        }
        Ok(updated)
    }

    async fn update_content_with_versions(
        &self,
        schema: &SchemaDefinition,
        record: &ContentRecord,
        versions: &[ContentVersionRecord],
        publication: ContentPublicationChange,
    ) -> Result<bool> {
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|e| AegisError::Database(e.to_string()))?;
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
            .execute(&mut *tx)
            .await
            .map_err(|e| AegisError::Database(format!("MySQL UPDATE failed: {}", e)))?;
        if result.rows_affected() == 0 {
            tx.rollback()
                .await
                .map_err(|e| AegisError::Database(e.to_string()))?;
            return Ok(false);
        }
        let mut revisions = Vec::with_capacity(versions.len());
        for version in versions {
            let revision: i64 = sqlx::query_scalar("SELECT COALESCE(MAX(revision), 0) + 1 FROM _aegis_content_versions WHERE content_type = ? AND content_id = ?")
                .bind(&version.content_type).bind(&version.content_id).fetch_one(&mut *tx).await.map_err(|e| AegisError::Database(e.to_string()))?;
            sqlx::query("INSERT INTO _aegis_content_versions (content_type, content_id, revision, snapshot_json, created_at, created_by, change_note) VALUES (?, ?, ?, ?, ?, ?, ?)")
                .bind(&version.content_type).bind(&version.content_id).bind(revision).bind(&version.snapshot_json)
                .bind(&version.created_at).bind(&version.created_by).bind(&version.change_note).execute(&mut *tx).await
                .map_err(|e| AegisError::Database(e.to_string()))?;
            revisions.push(revision);
        }
        apply_publication_change(
            &mut tx,
            versions,
            &revisions,
            publication,
            &crate::published_exec::current_plan(&self.published),
        )
        .await?;
        tx.commit()
            .await
            .map_err(|e| AegisError::Database(e.to_string()))?;
        Ok(true)
    }

    async fn insert_content_version(&self, version: &ContentVersionRecord) -> Result<i64> {
        let revision: i64 = sqlx::query_scalar("SELECT COALESCE(MAX(revision), 0) + 1 FROM _aegis_content_versions WHERE content_type = ? AND content_id = ?")
            .bind(&version.content_type).bind(&version.content_id).fetch_one(&self.pool).await.map_err(|e| AegisError::Database(e.to_string()))?;
        sqlx::query("INSERT INTO _aegis_content_versions (content_type, content_id, revision, snapshot_json, created_at, created_by, change_note) VALUES (?, ?, ?, ?, ?, ?, ?)")
            .bind(&version.content_type).bind(&version.content_id).bind(revision).bind(&version.snapshot_json).bind(&version.created_at).bind(&version.created_by).bind(&version.change_note).execute(&self.pool).await.map_err(|e| AegisError::Database(e.to_string()))?;
        Ok(revision)
    }

    async fn list_content_versions(
        &self,
        content_type: &str,
        content_id: &str,
    ) -> Result<Vec<ContentVersionRecord>> {
        let rows = sqlx::query("SELECT content_type, content_id, revision, snapshot_json, created_at, created_by, change_note FROM _aegis_content_versions WHERE content_type = ? AND content_id = ? ORDER BY revision DESC")
            .bind(content_type).bind(content_id).fetch_all(&self.pool).await.map_err(|e| AegisError::Database(e.to_string()))?;
        Ok(rows
            .iter()
            .map(|r| ContentVersionRecord {
                content_type: r.get("content_type"),
                content_id: r.get("content_id"),
                revision: r.get("revision"),
                snapshot_json: r.get("snapshot_json"),
                created_at: r.get("created_at"),
                created_by: r.get("created_by"),
                change_note: r.get("change_note"),
            })
            .collect())
    }

    async fn get_content_version(
        &self,
        content_type: &str,
        content_id: &str,
        revision: i64,
    ) -> Result<Option<ContentVersionRecord>> {
        let row = sqlx::query("SELECT content_type, content_id, revision, snapshot_json, created_at, created_by, change_note FROM _aegis_content_versions WHERE content_type = ? AND content_id = ? AND revision = ?")
            .bind(content_type).bind(content_id).bind(revision).fetch_optional(&self.pool).await.map_err(|e| AegisError::Database(e.to_string()))?;
        Ok(row.map(|r| ContentVersionRecord {
            content_type: r.get("content_type"),
            content_id: r.get("content_id"),
            revision: r.get("revision"),
            snapshot_json: r.get("snapshot_json"),
            created_at: r.get("created_at"),
            created_by: r.get("created_by"),
            change_note: r.get("change_note"),
        }))
    }

    async fn get_published_content(
        &self,
        schema: &SchemaDefinition,
        slug_or_id: &str,
    ) -> Result<Option<ContentRecord>> {
        let row = sqlx::query("SELECT v.snapshot_json, p.status, p.published_at, p.unpublish_at, p.updated_at FROM _aegis_content_publications p JOIN _aegis_content_versions v ON v.content_type = p.content_type AND v.content_id = p.content_id AND v.revision = p.revision WHERE p.content_type = ? AND (p.slug = ? OR p.content_id = ?) LIMIT 1")
            .bind(&schema.content_type).bind(slug_or_id).bind(slug_or_id).fetch_optional(&self.pool).await
            .map_err(|e| AegisError::Database(e.to_string()))?;
        if let Some(row) = row {
            return super::published_record_from_snapshot(
                &row.get::<String, _>("snapshot_json"),
                row.get("status"),
                row.get("published_at"),
                row.get("unpublish_at"),
                row.get("updated_at"),
            )
            .map(Some);
        }
        Ok(self
            .get_content(schema, slug_or_id)
            .await?
            .filter(|record| record.status == "published"))
    }

    async fn list_published_contents_page(
        &self,
        schema: &SchemaDefinition,
        query: &ContentListQuery,
    ) -> Result<ContentPage> {
        let list = super::build_published_list(&schema.content_type, query, |_| "?".to_string());
        let mut rows_query = sqlx::query(&list.sql);
        for value in &list.binds {
            rows_query = rows_query.bind(value);
        }
        let rows = rows_query
            .fetch_all(&self.pool)
            .await
            .map_err(|e| AegisError::Database(e.to_string()))?;
        let records = Self::published_records(&rows)?;
        let total = if query.limit.is_some() {
            let count =
                super::build_published_count(&schema.content_type, query, |_| "?".to_string());
            let mut count_query = sqlx::query_scalar::<_, i64>(&count.sql);
            for value in &count.binds {
                count_query = count_query.bind(value);
            }
            count_query
                .fetch_one(&self.pool)
                .await
                .map_err(|e| AegisError::Database(e.to_string()))? as u64
        } else {
            records.len() as u64
        };
        Ok(ContentPage { records, total })
    }

    async fn list_published_neighbors(
        &self,
        schema: &SchemaDefinition,
        query: &PublishedNeighborQuery,
    ) -> Result<Vec<ContentRecord>> {
        let neighbors =
            super::build_published_neighbors(&schema.content_type, query, |_| "?".to_string());
        let mut rows_query = sqlx::query(&neighbors.sql);
        for value in &neighbors.binds {
            rows_query = rows_query.bind(value);
        }
        let rows = rows_query
            .fetch_all(&self.pool)
            .await
            .map_err(|e| AegisError::Database(e.to_string()))?;
        Self::published_records(&rows)
    }

    async fn publish_scheduled_content(
        &self,
        schema: &SchemaDefinition,
        id: &str,
        published_at: &str,
        updated_at: &str,
    ) -> Result<bool> {
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|e| AegisError::Database(e.to_string()))?;
        let sql = build_scheduled_publish(&schema.table_name(), |_| "?".to_string());
        let result = sqlx::query(&sql)
            .bind(updated_at)
            .bind(id)
            .bind(published_at)
            .execute(&mut *tx)
            .await
            .map_err(|e| AegisError::Database(format!("MySQL UPDATE failed: {}", e)))?;
        if result.rows_affected() == 0 {
            tx.rollback()
                .await
                .map_err(|e| AegisError::Database(e.to_string()))?;
            return Ok(false);
        }
        let row = sqlx::query(&format!(
            "SELECT * FROM {} WHERE id = ?",
            schema.table_name()
        ))
        .bind(id)
        .fetch_one(&mut *tx)
        .await
        .map_err(|e| AegisError::Database(e.to_string()))?;
        let record = record_from_row(schema, &row);
        let version = ContentVersionRecord {
            content_type: record.content_type.clone(),
            content_id: record.id.clone(),
            revision: 0,
            snapshot_json: serde_json::to_string(&record)
                .map_err(|e| AegisError::Database(e.to_string()))?,
            created_at: updated_at.to_string(),
            created_by: None,
            change_note: Some("Scheduled publication".into()),
        };
        let revision: i64 = sqlx::query_scalar("SELECT COALESCE(MAX(revision), 0) + 1 FROM _aegis_content_versions WHERE content_type = ? AND content_id = ?")
            .bind(&version.content_type).bind(&version.content_id).fetch_one(&mut *tx).await.map_err(|e| AegisError::Database(e.to_string()))?;
        sqlx::query("INSERT INTO _aegis_content_versions (content_type, content_id, revision, snapshot_json, created_at, created_by, change_note) VALUES (?, ?, ?, ?, ?, ?, ?)")
            .bind(&version.content_type).bind(&version.content_id).bind(revision).bind(&version.snapshot_json)
            .bind(&version.created_at).bind(&version.created_by).bind(&version.change_note).execute(&mut *tx).await
            .map_err(|e| AegisError::Database(e.to_string()))?;
        apply_publication_change(
            &mut tx,
            &[version],
            &[revision],
            ContentPublicationChange::PublishVersion(0),
            &crate::published_exec::current_plan(&self.published),
        )
        .await?;
        tx.commit()
            .await
            .map_err(|e| AegisError::Database(e.to_string()))?;
        Ok(true)
    }

    async fn unpublish_scheduled_content(
        &self,
        schema: &SchemaDefinition,
        id: &str,
        unpublish_at: &str,
        updated_at: &str,
    ) -> Result<bool> {
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|e| AegisError::Database(e.to_string()))?;
        let sql = build_scheduled_unpublish(&schema.table_name(), |_| "?".to_string());
        let result = sqlx::query(&sql)
            .bind(updated_at)
            .bind(id)
            .bind(unpublish_at)
            .execute(&mut *tx)
            .await
            .map_err(|e| AegisError::Database(format!("MySQL UPDATE failed: {}", e)))?;
        if result.rows_affected() == 0 {
            tx.rollback()
                .await
                .map_err(|e| AegisError::Database(e.to_string()))?;
            return Ok(false);
        }
        sqlx::query(
            "DELETE FROM _aegis_content_publications WHERE content_type = ? AND content_id = ?",
        )
        .bind(&schema.content_type)
        .bind(id)
        .execute(&mut *tx)
        .await
        .map_err(|e| AegisError::Database(e.to_string()))?;
        MySqlAdapter::delete_published_row(
            &mut tx,
            &crate::published_exec::current_plan(&self.published),
            &schema.content_type,
            id,
        )
        .await?;
        tx.commit()
            .await
            .map_err(|e| AegisError::Database(e.to_string()))?;
        Ok(true)
    }

    async fn delete_content(&self, schema: &SchemaDefinition, id: &str) -> Result<bool> {
        let db_err = |e: sqlx::Error| AegisError::Database(e.to_string());
        let mut tx = self.pool.begin().await.map_err(db_err)?;
        let result = sqlx::query(&format!(
            "DELETE FROM {} WHERE id = ?;",
            schema.table_name()
        ))
        .bind(id)
        .execute(&mut *tx)
        .await
        .map_err(db_err)?;
        // 公開中の版も取り下げる（残すと削除したコンテンツが公開ページに出続ける）。版の履歴は残す
        sqlx::query(
            "DELETE FROM _aegis_content_publications WHERE content_type = ? AND content_id = ?;",
        )
        .bind(&schema.content_type)
        .bind(id)
        .execute(&mut *tx)
        .await
        .map_err(db_err)?;
        MySqlAdapter::delete_published_row(
            &mut tx,
            &crate::published_exec::current_plan(&self.published),
            &schema.content_type,
            id,
        )
        .await?;
        tx.commit().await.map_err(db_err)?;
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
        let plan = build_content_list(&schema.table_name(), &schema.content_type, query, |_| {
            "?".to_string()
        });

        let mut rows_query = sqlx::query(&plan.select);
        for value in &plan.select_binds {
            rows_query = rows_query.bind(value);
        }
        let mut count_query = sqlx::query_scalar::<_, i64>(&plan.count);
        for value in &plan.count_binds {
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

    async fn resolve_published_relations(
        &self,
        target_schema: &SchemaDefinition,
        target_field: &str,
        ref_ids: &[String],
    ) -> Result<HashMap<String, ResolvedRelation>> {
        if ref_ids.is_empty() {
            return Ok(HashMap::new());
        }
        let Some(query) =
            build_published_relation_lookup(target_field, ref_ids.len(), |_| "?".to_string())
        else {
            return Ok(HashMap::new());
        };
        let mut lookup = sqlx::query(&query).bind(&target_schema.content_type);
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
            .bind(&user.totp_secret)
            .bind(i64::from(user.totp_enabled))
            .bind(&user.backup_codes)
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

    async fn list_identity_links(&self) -> Result<Vec<crate::UserIdentityRecord>> {
        let rows = sqlx::query(&crate::build_identity_list())
            .fetch_all(&self.pool)
            .await
            .map_err(|e| AegisError::Database(e.to_string()))?;
        let mut links = Vec::with_capacity(rows.len());
        for row in rows {
            links.push(crate::UserIdentityRecord {
                provider: row
                    .try_get("provider")
                    .map_err(|e| AegisError::Database(e.to_string()))?,
                subject: row
                    .try_get("subject")
                    .map_err(|e| AegisError::Database(e.to_string()))?,
                username: row
                    .try_get("username")
                    .map_err(|e| AegisError::Database(e.to_string()))?,
                created_at: row
                    .try_get("created_at")
                    .map_err(|e| AegisError::Database(e.to_string()))?,
            });
        }
        Ok(links)
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
            .bind(&user.totp_secret)
            .bind(i64::from(user.totp_enabled))
            .bind(&user.backup_codes)
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
