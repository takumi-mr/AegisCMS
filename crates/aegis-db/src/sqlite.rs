use std::collections::{HashMap, HashSet};
use std::str::FromStr;

use crate::sqlx_compat as sqlx;
use aegis_core::{AegisError, AuditEvent, AuditQuery, Result};
use aegis_schema::{FieldType, SchemaDefinition, SchemaMigration, SqlDialect};
use async_trait::async_trait;
use sqlx::sqlite::{
    SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions, SqliteRow, SqliteSynchronous,
};
use sqlx::{Pool, Row, Sqlite, SqliteConnection};

use super::{
    audit_insert_values, audit_row_from_sqlx, build_audit_delete_before, build_audit_insert,
    build_audit_select, build_column_probe, build_content_list, build_content_update,
    build_identity_insert, build_identity_select, build_published_relation_lookup,
    build_relation_lookup, build_scheduled_publish, build_scheduled_unpublish,
    build_setting_delete, build_setting_upsert, build_settings_select, build_user_delete,
    build_user_insert, build_user_select, build_user_update, custom_columns, is_standard_column,
    outdated_database_error, user_row_from_sqlx, ContentListQuery, ContentPage,
    ContentPublicationChange, ContentRecord, ContentVersionRecord, DatabaseAdapter,
    PublishedNeighborQuery, ResolvedRelation, UserRecord, REQUIRED_COLUMNS,
};

async fn apply_publication_change(
    connection: &mut SqliteConnection,
    versions: &[ContentVersionRecord],
    revisions: &[i64],
    change: ContentPublicationChange,
    plan: &crate::published_columns::PublishedColumnsPlan,
) -> Result<()> {
    let index = match change {
        ContentPublicationChange::Keep => return Ok(()),
        ContentPublicationChange::Unpublish => {
            if let Some(version) = versions.first() {
                sqlx::query("DELETE FROM _aegis_content_publications WHERE content_type = ?1 AND content_id = ?2")
                    .bind(&version.content_type).bind(&version.content_id).execute(&mut *connection).await
                    .map_err(|e| AegisError::Database(e.to_string()))?;
                SqliteAdapter::delete_published_row(
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
            let exists: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM _aegis_content_publications WHERE content_type = ?1 AND content_id = ?2")
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
    sqlx::query("INSERT INTO _aegis_content_publications (content_type, content_id, revision, slug, title, status, published_at, unpublish_at, created_at, updated_at, category_id, sort_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12) ON CONFLICT(content_type, content_id) DO UPDATE SET revision = excluded.revision, slug = excluded.slug, title = excluded.title, status = excluded.status, published_at = excluded.published_at, unpublish_at = excluded.unpublish_at, created_at = excluded.created_at, updated_at = excluded.updated_at, category_id = excluded.category_id, sort_at = excluded.sort_at")
        .bind(&version.content_type).bind(&version.content_id).bind(revision).bind(&record.slug).bind(&record.title)
        .bind(&record.status).bind(&record.published_at).bind(&record.unpublish_at).bind(&record.created_at).bind(&record.updated_at)
        .bind(&category_id).bind(&sort_at)
        .execute(&mut *connection).await.map_err(|e| AegisError::Database(e.to_string()))?;
    SqliteAdapter::write_published_row(connection, plan, &record).await?;
    Ok(())
}

/// SQLite はカスタムフィールドを文字列で保存するため、JSON として読めればその値、読めなければ文字列にする。
/// NULL の列は含めない（`try_get::<String>` は NULL を空文字として返すため、`Option` で読む）
fn custom_fields_from_row(
    schema: &SchemaDefinition,
    row: &SqliteRow,
) -> HashMap<String, serde_json::Value> {
    let mut custom_fields = HashMap::new();
    for field in &schema.fields {
        if is_standard_column(&field.name) {
            continue;
        }
        let name = field.name.as_str();
        let value = match row.try_get::<Option<String>, _>(name) {
            Ok(text) => {
                text.map(|s| serde_json::from_str(&s).unwrap_or(serde_json::Value::String(s)))
            }
            // INTEGER の列に数値として保存された値
            Err(_) => row
                .try_get::<Option<i64>, _>(name)
                .ok()
                .flatten()
                .map(serde_json::Value::from),
        };
        if let Some(value) = value {
            let value = if field.field_type == FieldType::Boolean {
                boolean_value(value)
            } else {
                value
            };
            custom_fields.insert(field.name.clone(), value);
        }
    }
    custom_fields
}

/// 真偽値のフィールドの値。SQLite は `1` / `0` で保存するので、PostgreSQL・MySQL（`true` / `false`）と
/// 同じ形で返す。スキーマ（TypeScript・OpenAPI の `boolean`）が約束している形に合わせるため。
/// 真偽値として読めない値は、そのまま返す
fn boolean_value(value: serde_json::Value) -> serde_json::Value {
    use serde_json::Value;
    match &value {
        Value::Number(n) => match n.as_i64() {
            Some(1) => Value::Bool(true),
            Some(0) => Value::Bool(false),
            _ => value,
        },
        Value::String(text) => match text.trim().to_ascii_lowercase().as_str() {
            "1" | "true" => Value::Bool(true),
            "0" | "false" => Value::Bool(false),
            _ => value,
        },
        _ => value,
    }
}

/// カスタム列に保存する値（スキーマのカスタムフィールドの順）。SQLite は文字列で保存する（真偽値は
/// 1 / 0、値の無いフィールドは NULL）。null は数値・真偽値の列では NULL、それ以外は空文字
/// （数値の列に空文字を入れると、`stock - 1` のような計算で 0 として扱われてしまう）
fn custom_values(schema: &SchemaDefinition, record: &ContentRecord) -> Vec<Option<String>> {
    schema
        .fields
        .iter()
        .filter(|field| !is_standard_column(&field.name))
        .map(|field| {
            let numeric = matches!(field.field_type, FieldType::Integer | FieldType::Boolean);
            match record.custom_fields.get(&field.name) {
                None => None,
                Some(serde_json::Value::Null) if numeric => None,
                Some(val) => Some(match val {
                    serde_json::Value::String(s) => s.clone(),
                    serde_json::Value::Number(n) => n.to_string(),
                    serde_json::Value::Bool(b) => if *b { "1" } else { "0" }.to_string(),
                    serde_json::Value::Null => String::new(),
                    other => other.to_string(),
                }),
            }
        })
        .collect()
}

fn record_from_row(schema: &SchemaDefinition, r: &SqliteRow) -> ContentRecord {
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

/// システムテーブルの手順（`migrations/sqlite/NNNN_*.sql`）。コンパイル時にバイナリへ埋め込む
pub(crate) static MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!("migrations/sqlite");

#[derive(Debug, Clone)]
pub struct SqliteAdapter {
    pool: Pool<Sqlite>,
    published: crate::published_exec::SharedPlan,
}

impl SqliteAdapter {
    pub async fn connect(database_url: &str, max_connections: u32) -> Result<Self> {
        let options = SqliteConnectOptions::from_str(database_url)
            .map_err(|e| AegisError::Database(e.to_string()))?
            .create_if_missing(true)
            // WAL: 読み込みが書き込みを待たず、コミットごとの fsync も減る（`:memory:` では無視される）
            .journal_mode(SqliteJournalMode::Wal)
            .synchronous(SqliteSynchronous::Normal);

        let pool = SqlitePoolOptions::new()
            .max_connections(max_connections)
            .connect_with(options)
            .await
            .map_err(|e| AegisError::Database(e.to_string()))?;

        Ok(Self::new(pool))
    }

    #[allow(dead_code)]
    pub fn new(pool: Pool<Sqlite>) -> Self {
        Self {
            pool,
            published: crate::published_exec::new_shared_plan(),
        }
    }

    /// 公開の記録の一覧用の索引を用意する
    fn published_records(rows: &[SqliteRow]) -> Result<Vec<ContentRecord>> {
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

super::orders::impl_order_storage!(SqliteAdapter, |i| format!("?{}", i), "SQLite");
super::variants::impl_product_variant_storage!(SqliteAdapter, |i| format!("?{}", i), "SQLite");
super::comments::impl_comment_storage!(SqliteAdapter, |i| format!("?{}", i), "SQLite");
super::tags::impl_tag_storage!(SqliteAdapter, |i| format!("?{}", i), "SQLite");
super::contact::impl_contact_storage!(SqliteAdapter, |i| format!("?{}", i), "SQLite");
super::newsletter::impl_newsletter_storage!(SqliteAdapter, |i| format!("?{}", i), "SQLite");
super::analytics::impl_analytics_storage!(
    SqliteAdapter,
    |i| format!("?{}", i),
    super::analytics::UPSERT_ON_CONFLICT,
    "BIGINT",
    "SQLite"
);
super::notifications::impl_notification_queue_storage!(
    SqliteAdapter,
    |i| format!("?{}", i),
    "SQLite"
);
super::forum::impl_forum_storage!(SqliteAdapter, |i| format!("?{}", i), "SQLite");
super::members::impl_member_storage!(SqliteAdapter, |i| format!("?{}", i), "SQLite");
super::purchases::impl_purchase_storage!(SqliteAdapter, |i| format!("?{}", i), "SQLite");
super::reviews::impl_review_storage!(SqliteAdapter, |i| format!("?{}", i), "SQLite");

crate::published_exec::impl_published_columns!(
    SqliteAdapter,
    SqlDialect::Sqlite,
    SqliteConnection,
    |b: bool| i64::from(b),
    i64
);

#[async_trait]
impl DatabaseAdapter for SqliteAdapter {
    async fn init(&self) -> Result<()> {
        // システムテーブルは `migrations/sqlite/` の手順で作る・更新する（適用済みの手順は
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

            // 古い版で作ったテーブルには `unpublish_at` が無い。足さずに、移行の手順を示して止める
            sqlx::query(&build_column_probe(&table, &["unpublish_at"]))
                .execute(&self.pool)
                .await
                .map_err(|e| outdated_database_error(&table, &["unpublish_at"], &e.to_string()))?;

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
                    let sql_type =
                        SchemaMigration::column_type(field.field_type, SqlDialect::Sqlite);
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
        columns.extend(custom_columns(schema));
        let custom_values = custom_values(schema, record);

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
            .bind(&record.unpublish_at)
            .bind(&record.created_at)
            .bind(&record.updated_at);

        for val in &custom_values {
            query = query.bind(val);
        }

        query
            .execute(&self.pool)
            .await
            .map_err(|e| AegisError::Database(format!("SQLite INSERT failed: {}", e)))?;

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
        columns.extend(custom_columns(schema));
        let placeholders: Vec<String> = (1..=columns.len()).map(|i| format!("?{}", i)).collect();
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
        for value in custom_values(schema, record) {
            query = query.bind(value);
        }
        query
            .execute(&mut *tx)
            .await
            .map_err(|e| AegisError::Database(format!("SQLite INSERT failed: {}", e)))?;
        let mut revisions = Vec::with_capacity(versions.len());
        for version in versions {
            let revision: i64 = sqlx::query_scalar("SELECT COALESCE(MAX(revision), 0) + 1 FROM _aegis_content_versions WHERE content_type = ?1 AND content_id = ?2")
                .bind(&version.content_type).bind(&version.content_id).fetch_one(&mut *tx).await.map_err(|e| AegisError::Database(e.to_string()))?;
            sqlx::query("INSERT INTO _aegis_content_versions (content_type, content_id, revision, snapshot_json, created_at, created_by, change_note) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)")
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
        let sql = build_content_update(&schema.table_name(), &custom_columns(schema), |i| {
            format!("?{}", i)
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
        for val in custom_values(schema, record) {
            query = query.bind(val);
        }
        let result = query
            .bind(&record.id)
            .execute(&self.pool)
            .await
            .map_err(|e| AegisError::Database(format!("SQLite UPDATE failed: {}", e)))?;
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
        let sql = build_content_update(&schema.table_name(), &custom_columns(schema), |i| {
            format!("?{}", i)
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
        for value in custom_values(schema, record) {
            query = query.bind(value);
        }
        let result = query
            .bind(&record.id)
            .execute(&mut *tx)
            .await
            .map_err(|e| AegisError::Database(format!("SQLite UPDATE failed: {}", e)))?;
        if result.rows_affected() == 0 {
            tx.rollback()
                .await
                .map_err(|e| AegisError::Database(e.to_string()))?;
            return Ok(false);
        }
        let mut revisions = Vec::with_capacity(versions.len());
        for version in versions {
            let revision: i64 = sqlx::query_scalar("SELECT COALESCE(MAX(revision), 0) + 1 FROM _aegis_content_versions WHERE content_type = ?1 AND content_id = ?2")
                .bind(&version.content_type).bind(&version.content_id).fetch_one(&mut *tx).await.map_err(|e| AegisError::Database(e.to_string()))?;
            sqlx::query("INSERT INTO _aegis_content_versions (content_type, content_id, revision, snapshot_json, created_at, created_by, change_note) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)")
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
        let revision: i64 = sqlx::query_scalar("SELECT COALESCE(MAX(revision), 0) + 1 FROM _aegis_content_versions WHERE content_type = ?1 AND content_id = ?2")
            .bind(&version.content_type).bind(&version.content_id).fetch_one(&self.pool).await.map_err(|e| AegisError::Database(e.to_string()))?;
        sqlx::query("INSERT INTO _aegis_content_versions (content_type, content_id, revision, snapshot_json, created_at, created_by, change_note) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)")
            .bind(&version.content_type).bind(&version.content_id).bind(revision).bind(&version.snapshot_json).bind(&version.created_at).bind(&version.created_by).bind(&version.change_note).execute(&self.pool).await.map_err(|e| AegisError::Database(e.to_string()))?;
        Ok(revision)
    }

    async fn list_content_versions(
        &self,
        content_type: &str,
        content_id: &str,
    ) -> Result<Vec<ContentVersionRecord>> {
        let rows = sqlx::query("SELECT content_type, content_id, revision, snapshot_json, created_at, created_by, change_note FROM _aegis_content_versions WHERE content_type = ?1 AND content_id = ?2 ORDER BY revision DESC")
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
        let row = sqlx::query("SELECT content_type, content_id, revision, snapshot_json, created_at, created_by, change_note FROM _aegis_content_versions WHERE content_type = ?1 AND content_id = ?2 AND revision = ?3")
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
        let row = sqlx::query("SELECT v.snapshot_json, p.status, p.published_at, p.unpublish_at, p.updated_at FROM _aegis_content_publications p JOIN _aegis_content_versions v ON v.content_type = p.content_type AND v.content_id = p.content_id AND v.revision = p.revision WHERE p.content_type = ?1 AND (p.slug = ?2 OR p.content_id = ?2) LIMIT 1")
            .bind(&schema.content_type).bind(slug_or_id).fetch_optional(&self.pool).await
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
        let list = super::build_published_list(&schema.content_type, query, |i| format!("?{}", i));
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
                super::build_published_count(&schema.content_type, query, |i| format!("?{}", i));
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
            super::build_published_neighbors(&schema.content_type, query, |i| format!("?{}", i));
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
        let sql = build_scheduled_publish(&schema.table_name(), |i| format!("?{}", i));
        let result = sqlx::query(&sql)
            .bind(updated_at)
            .bind(id)
            .bind(published_at)
            .execute(&mut *tx)
            .await
            .map_err(|e| AegisError::Database(format!("SQLite UPDATE failed: {}", e)))?;
        if result.rows_affected() == 0 {
            tx.rollback()
                .await
                .map_err(|e| AegisError::Database(e.to_string()))?;
            return Ok(false);
        }
        let row = sqlx::query(&format!(
            "SELECT * FROM {} WHERE id = ?1",
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
        let revision: i64 = sqlx::query_scalar("SELECT COALESCE(MAX(revision), 0) + 1 FROM _aegis_content_versions WHERE content_type = ?1 AND content_id = ?2")
            .bind(&version.content_type).bind(&version.content_id).fetch_one(&mut *tx).await.map_err(|e| AegisError::Database(e.to_string()))?;
        sqlx::query("INSERT INTO _aegis_content_versions (content_type, content_id, revision, snapshot_json, created_at, created_by, change_note) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)")
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
        let sql = build_scheduled_unpublish(&schema.table_name(), |i| format!("?{}", i));
        let result = sqlx::query(&sql)
            .bind(updated_at)
            .bind(id)
            .bind(unpublish_at)
            .execute(&mut *tx)
            .await
            .map_err(|e| AegisError::Database(format!("SQLite UPDATE failed: {}", e)))?;
        if result.rows_affected() == 0 {
            tx.rollback()
                .await
                .map_err(|e| AegisError::Database(e.to_string()))?;
            return Ok(false);
        }
        sqlx::query(
            "DELETE FROM _aegis_content_publications WHERE content_type = ?1 AND content_id = ?2",
        )
        .bind(&schema.content_type)
        .bind(id)
        .execute(&mut *tx)
        .await
        .map_err(|e| AegisError::Database(e.to_string()))?;
        SqliteAdapter::delete_published_row(
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
            "DELETE FROM {} WHERE id = ?1;",
            schema.table_name()
        ))
        .bind(id)
        .execute(&mut *tx)
        .await
        .map_err(db_err)?;
        // 公開中の版も取り下げる（残すと削除したコンテンツが公開ページに出続ける）。版の履歴は残す
        sqlx::query(
            "DELETE FROM _aegis_content_publications WHERE content_type = ?1 AND content_id = ?2;",
        )
        .bind(&schema.content_type)
        .bind(id)
        .execute(&mut *tx)
        .await
        .map_err(db_err)?;
        SqliteAdapter::delete_published_row(
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
        let query = format!(
            "SELECT * FROM {} WHERE slug = ?1 OR id = ?1 LIMIT 1;",
            table
        );

        let row = sqlx::query(&query)
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
        let plan = build_content_list(&schema.table_name(), &schema.content_type, query, |i| {
            format!("?{}", i)
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
            |i| format!("?{}", i),
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
            build_published_relation_lookup(target_field, ref_ids.len(), |i| format!("?{}", i))
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
        let sql = build_audit_insert(|i| format!("?{}", i));
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
        let (sql, binds) = build_audit_select(audit_query, |i| format!("?{}", i));
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
        let result = sqlx::query(&build_audit_delete_before("?1"))
            .bind(cutoff)
            .execute(&self.pool)
            .await
            .map_err(|e| AegisError::Database(e.to_string()))?;
        Ok(result.rows_affected())
    }

    async fn insert_user(&self, user: &UserRecord) -> Result<()> {
        sqlx::query(&build_user_insert(|i| format!("?{}", i)))
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
        let row = sqlx::query(&build_user_select(Some("username = ?1")))
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
        let row = sqlx::query(&build_identity_select(|i| format!("?{}", i)))
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
        sqlx::query(&build_identity_insert(|i| format!("?{}", i)))
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
        sqlx::query(&build_setting_upsert(|i| format!("?{}", i), false))
            .bind(name)
            .bind(value)
            .bind(chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true))
            .execute(&self.pool)
            .await
            .map_err(|e| AegisError::Database(e.to_string()))?;
        Ok(())
    }

    async fn delete_setting(&self, name: &str) -> Result<()> {
        sqlx::query(&build_setting_delete(|i| format!("?{}", i)))
            .bind(name)
            .execute(&self.pool)
            .await
            .map_err(|e| AegisError::Database(e.to_string()))?;
        Ok(())
    }

    async fn update_user(&self, user: &UserRecord) -> Result<()> {
        sqlx::query(&build_user_update(|i| format!("?{}", i)))
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
        let (delete_user, delete_links) = build_user_delete(|i| format!("?{}", i));
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

#[cfg(test)]
mod tests {
    use super::*;
    use aegis_core::{AuditAction, AuditActor, AuditOutcome};

    async fn memory_adapter() -> SqliteAdapter {
        // インメモリ DB は接続ごとに別 DB になるため接続数を 1 に固定する
        let adapter = SqliteAdapter::connect("sqlite::memory:", 1).await.unwrap();
        adapter.init().await.unwrap();
        adapter
    }

    /// `category_id` を組み込みにする前に作った `content_page`（`category_id` の列が無い）が、
    /// 起動時の移行で列と索引を足され、既存の行を残したまま使えること
    #[tokio::test]
    async fn test_migration_adds_builtin_category_id_to_existing_table() {
        let adapter = memory_adapter().await;
        sqlx::query(
            "CREATE TABLE content_page (id TEXT PRIMARY KEY, content_type TEXT NOT NULL,              title TEXT NOT NULL, slug TEXT NOT NULL UNIQUE, status TEXT NOT NULL DEFAULT 'draft',              blocks_json TEXT NOT NULL DEFAULT '[]', author_id TEXT, published_at TEXT,              unpublish_at TEXT, created_at TEXT NOT NULL, updated_at TEXT NOT NULL, layout TEXT);",
        )
        .execute(&adapter.pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO content_page (id, content_type, title, slug, created_at, updated_at, layout)              VALUES ('p1', 'page', 'About', 'about', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z', 'blank');",
        )
        .execute(&adapter.pool)
        .await
        .unwrap();

        let schema = aegis_schema::SchemaLoader::load_from_str(
            "content_type: page
label: Page
fields:
  - name: layout
    type: string
    nullable: true
",
        )
        .unwrap();
        let schemas = HashMap::from([("page".to_string(), schema.clone())]);
        adapter.run_migrations(&schemas).await.unwrap();
        // 2 回目も同じ結果になる（起動のたびに走る）
        adapter.run_migrations(&schemas).await.unwrap();

        let columns: Vec<String> = sqlx::query("PRAGMA table_info(content_page);")
            .fetch_all(&adapter.pool)
            .await
            .unwrap()
            .into_iter()
            .map(|r| r.get::<String, _>("name"))
            .collect();
        assert!(columns.contains(&"category_id".to_string()));
        let indexes: Vec<String> = sqlx::query("PRAGMA index_list(content_page);")
            .fetch_all(&adapter.pool)
            .await
            .unwrap()
            .into_iter()
            .map(|r| r.get::<String, _>("name"))
            .collect();
        assert!(indexes.contains(&"idx_content_page_category_id".to_string()));

        let record = adapter
            .get_content(&schema, "about")
            .await
            .unwrap()
            .expect("the existing row is kept");
        assert_eq!(record.custom_fields["layout"], "blank");
        assert!(record
            .custom_fields
            .get("category_id")
            .is_none_or(|v| v.is_null()));
    }

    async fn applied_migrations(adapter: &SqliteAdapter) -> Vec<(i64, bool)> {
        sqlx::query("SELECT version, success FROM _sqlx_migrations ORDER BY version")
            .fetch_all(&adapter.pool)
            .await
            .unwrap()
            .into_iter()
            .map(|r| (r.get::<i64, _>("version"), r.get::<bool, _>("success")))
            .collect()
    }

    /// 新しい DB は手順を流して `_sqlx_migrations` に残し、2 回目の `init` は何もしない
    #[tokio::test]
    async fn test_init_applies_the_migrations_once() {
        let adapter = SqliteAdapter::connect("sqlite::memory:", 1).await.unwrap();
        adapter.init().await.unwrap();
        assert_eq!(
            applied_migrations(&adapter).await,
            vec![(1, true), (2, true), (3, true)]
        );
        adapter.set_setting("site.title", "kept").await.unwrap();

        adapter.init().await.unwrap();
        assert_eq!(
            applied_migrations(&adapter).await,
            vec![(1, true), (2, true), (3, true)]
        );
        assert_eq!(
            adapter.list_settings().await.unwrap().get("site.title"),
            Some(&"kept".to_string())
        );
    }

    /// v0.4 が作った DB（システムテーブルはあるが `_sqlx_migrations` が無い）は、データを
    /// そのままに、`0001_baseline` を適用済みとして採用する
    #[tokio::test]
    async fn test_init_adopts_a_database_created_before_the_migrations() {
        let adapter = SqliteAdapter::connect("sqlite::memory:", 1).await.unwrap();
        adapter.init().await.unwrap();
        adapter.set_setting("site.title", "kept").await.unwrap();
        // v0.4 の DB: 手順の記録が無く、0002 で足した列も無い
        sqlx::query("DROP TABLE _sqlx_migrations;")
            .execute(&adapter.pool)
            .await
            .unwrap();
        sqlx::query("ALTER TABLE _aegis_members DROP COLUMN level;")
            .execute(&adapter.pool)
            .await
            .unwrap();

        adapter.init().await.unwrap();
        // 0001 は採用し、0002 以降は流す（0002 は列を足す）
        assert_eq!(
            applied_migrations(&adapter).await,
            vec![(1, true), (2, true), (3, true)]
        );
        assert_eq!(
            adapter.list_settings().await.unwrap().get("site.title"),
            Some(&"kept".to_string())
        );
    }

    /// 新しい版が適用した手順を、古い版が知らないとき（新しい版で更新した DB を古い版で開く）は、
    /// 黙って動かさずに止める
    #[tokio::test]
    async fn test_init_refuses_a_database_updated_by_a_newer_version() {
        let adapter = SqliteAdapter::connect("sqlite::memory:", 1).await.unwrap();
        adapter.init().await.unwrap();
        sqlx::query(
            "INSERT INTO _sqlx_migrations (version, description, success, checksum, execution_time) \
             VALUES (9999, 'from a newer version', 1, x'00', 0);",
        )
        .execute(&adapter.pool)
        .await
        .unwrap();

        let message = adapter.init().await.unwrap_err().to_string();
        assert!(
            message.contains("9999") && message.contains("newer version"),
            "{message}"
        );
    }

    /// 適用済みの手順のファイルを後から書き換えたら（チェックサムが違う）、止める
    #[tokio::test]
    async fn test_init_refuses_a_migration_changed_after_it_was_applied() {
        let adapter = SqliteAdapter::connect("sqlite::memory:", 1).await.unwrap();
        adapter.init().await.unwrap();
        sqlx::query("UPDATE _sqlx_migrations SET checksum = x'00' WHERE version = 1;")
            .execute(&adapter.pool)
            .await
            .unwrap();

        let message = adapter.init().await.unwrap_err().to_string();
        assert!(
            message.contains("changed after it was applied") && message.contains('1'),
            "{message}"
        );
    }

    /// 古い版で作ったシステムテーブル（後から足した列が無い）は、黙って書き換えずに、
    /// 移行の手順を示すエラーで起動を止める
    #[tokio::test]
    async fn test_init_refuses_a_database_from_an_older_version() {
        let adapter = SqliteAdapter::connect("sqlite::memory:", 1).await.unwrap();
        sqlx::query(
            "CREATE TABLE _aegis_users (id TEXT PRIMARY KEY, username TEXT NOT NULL UNIQUE, \
             email TEXT, display_name TEXT, role TEXT NOT NULL, password_hash TEXT NOT NULL, \
             disabled INTEGER NOT NULL DEFAULT 0, created_at TEXT NOT NULL, updated_at TEXT NOT NULL);",
        )
        .execute(&adapter.pool)
        .await
        .unwrap();

        let message = adapter.init().await.unwrap_err().to_string();
        assert!(message.contains("older AegisCMS"), "{message}");
        assert!(message.contains("_aegis_users"), "{message}");
        assert!(
            message.contains("v0.3.x")
                && message.contains("task dump")
                && message.contains("task restore"),
            "{message}"
        );
        // 列を足して書き換えることはしない
        let columns: Vec<String> = sqlx::query("PRAGMA table_info(_aegis_users);")
            .fetch_all(&adapter.pool)
            .await
            .unwrap()
            .into_iter()
            .map(|r| r.get::<String, _>("name"))
            .collect();
        assert!(!columns.contains(&"avatar_key".to_string()));

        // 新しく作ったDBは通る
        memory_adapter().await;
    }

    /// コンテンツのテーブルも同じ: 古い版の `unpublish_at` の無いテーブルは足さずに止める
    #[tokio::test]
    async fn test_migrations_refuse_a_content_table_without_unpublish_at() {
        let adapter = memory_adapter().await;
        sqlx::query(
            "CREATE TABLE content_page (id TEXT PRIMARY KEY, content_type TEXT NOT NULL, \
             title TEXT NOT NULL, slug TEXT NOT NULL UNIQUE, status TEXT NOT NULL DEFAULT 'draft', \
             blocks_json TEXT NOT NULL DEFAULT '[]', author_id TEXT, published_at TEXT, \
             created_at TEXT NOT NULL, updated_at TEXT NOT NULL);",
        )
        .execute(&adapter.pool)
        .await
        .unwrap();
        let schema =
            aegis_schema::SchemaLoader::load_from_str("content_type: page\nlabel: Page\n").unwrap();
        let schemas = HashMap::from([("page".to_string(), schema)]);
        let message = adapter
            .run_migrations(&schemas)
            .await
            .unwrap_err()
            .to_string();
        assert!(
            message.contains("older AegisCMS") && message.contains("unpublish_at"),
            "{message}"
        );
    }

    #[tokio::test]
    async fn test_file_database_uses_wal() {
        let path = std::env::temp_dir().join(format!("aegis-wal-{}.db", std::process::id()));
        let url = format!("sqlite://{}?mode=rwc", path.display());
        let adapter = SqliteAdapter::connect(&url, 1).await.unwrap();
        let mode: String = sqlx::query_scalar("PRAGMA journal_mode")
            .fetch_one(&adapter.pool)
            .await
            .unwrap();
        adapter.pool.close().await;
        for suffix in ["", "-wal", "-shm"] {
            let _ = std::fs::remove_file(format!("{}{}", path.display(), suffix));
        }
        assert_eq!(mode, "wal");
    }

    #[tokio::test]
    async fn test_settings_set_overwrite_and_delete() {
        let adapter = memory_adapter().await;
        assert!(adapter.list_settings().await.unwrap().is_empty());

        adapter.set_setting("site.title", "First").await.unwrap();
        adapter.set_setting("site.title", "Second").await.unwrap();
        adapter
            .set_setting("site.favicon", "/uploads/icon.png")
            .await
            .unwrap();
        let settings = adapter.list_settings().await.unwrap();
        assert_eq!(settings.len(), 2);
        assert_eq!(settings["site.title"], "Second");

        adapter.delete_setting("site.title").await.unwrap();
        // 無い設定を消してもエラーにしない
        adapter.delete_setting("site.title").await.unwrap();
        let settings = adapter.list_settings().await.unwrap();
        assert_eq!(settings.len(), 1);
        assert_eq!(settings["site.favicon"], "/uploads/icon.png");
    }

    #[tokio::test]
    async fn test_user_optional_and_2fa_columns_round_trip() {
        let adapter = memory_adapter().await;
        // 任意の列（avatar_key・pending_approval・2FA）を省いて書いた行
        sqlx::query(
            "INSERT INTO _aegis_users (id, username, role, password_hash, created_at, updated_at)              VALUES ('u1', 'old', 'admin', 'x', 't', 't');",
        )
        .execute(&adapter.pool)
        .await
        .unwrap();

        // 2 回目の起動でも失敗しない
        adapter.init().await.unwrap();
        let user = adapter.get_user_by_username("old").await.unwrap().unwrap();
        assert_eq!(user.avatar_key, None);
        assert!(!user.pending_approval);
        assert_eq!(user.totp_secret, None);
        assert!(!user.totp_enabled);
        assert_eq!(user.backup_codes, None);

        // 2FA 情報を更新できることを確認
        let mut updated = user.clone();
        updated.totp_secret = Some("JBSWY3DPEHPK3PXP".to_string());
        updated.totp_enabled = true;
        updated.backup_codes = Some(r#"["hash1","hash2"]"#.to_string());
        adapter.update_user(&updated).await.unwrap();

        let reloaded = adapter.get_user_by_username("old").await.unwrap().unwrap();
        assert_eq!(reloaded.totp_secret.as_deref(), Some("JBSWY3DPEHPK3PXP"));
        assert!(reloaded.totp_enabled);
        assert_eq!(
            reloaded.backup_codes.as_deref(),
            Some(r#"["hash1","hash2"]"#)
        );
    }

    fn schemas() -> HashMap<String, SchemaDefinition> {
        let article: SchemaDefinition = serde_json::from_value(serde_json::json!({
            "content_type": "article",
            "label": "Articles",
            "fields": [
                { "name": "title", "type": "string", "required": true },
                { "name": "category_id", "type": "relation", "target": "category.id" },
                { "name": "custom_meta", "type": "jsonb" },
                { "name": "priority", "type": "integer" }
            ]
        }))
        .unwrap();
        let category: SchemaDefinition = serde_json::from_value(serde_json::json!({
            "content_type": "category",
            "label": "Categories",
            "fields": [{ "name": "title", "type": "string", "required": true }]
        }))
        .unwrap();
        HashMap::from([
            ("article".to_string(), article),
            ("category".to_string(), category),
        ])
    }

    fn record(
        content_type: &str,
        n: usize,
        status: &str,
        custom_fields: serde_json::Value,
    ) -> ContentRecord {
        ContentRecord {
            id: format!("{}-{}", content_type, n),
            content_type: content_type.to_string(),
            title: format!("Title {}", n),
            slug: format!("{}-slug-{}", content_type, n),
            status: status.to_string(),
            blocks_json: "[]".to_string(),
            author_id: None,
            published_at: None,
            unpublish_at: None,
            created_at: format!("2026-10-22T00:00:{:02}Z", n),
            updated_at: format!("2026-10-22T00:00:{:02}Z", n),
            custom_fields: serde_json::from_value(custom_fields).unwrap(),
        }
    }

    #[tokio::test]
    async fn test_null_custom_fields_are_left_out() {
        let adapter = memory_adapter().await;
        let schemas = schemas();
        adapter.run_migrations(&schemas).await.unwrap();
        let article = &schemas["article"];
        adapter
            .insert_content(
                article,
                &record("article", 1, "published", serde_json::json!({})),
            )
            .await
            .unwrap();
        adapter
            .insert_content(
                article,
                &record(
                    "article",
                    2,
                    "published",
                    serde_json::json!({ "category_id": "category-1", "custom_meta": { "seo": true } }),
                ),
            )
            .await
            .unwrap();

        // 以前は NULL の列が "" として返り、空文字でリレーションを問い合わせていた
        let empty = adapter
            .get_content(article, "article-1")
            .await
            .unwrap()
            .unwrap();
        assert!(empty.custom_fields.is_empty(), "{:?}", empty.custom_fields);
        let filled = adapter
            .get_content(article, "article-2")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(filled.custom_fields["category_id"], "category-1");
        assert_eq!(
            filled.custom_fields["custom_meta"],
            serde_json::json!({ "seo": true })
        );
    }

    /// 真偽値のカスタムフィールドは、PostgreSQL・MySQL と同じ `true` / `false` で返す
    /// （SQLite は `1` / `0` で保存するが、スキーマは `boolean` を約束している）。数値は数値のまま
    #[tokio::test]
    async fn test_boolean_custom_fields_are_read_as_booleans() {
        let adapter = memory_adapter().await;
        let event: SchemaDefinition = serde_json::from_value(serde_json::json!({
            "content_type": "event",
            "label": "Events",
            "fields": [
                { "name": "featured", "type": "boolean" },
                { "name": "capacity", "type": "integer" }
            ]
        }))
        .unwrap();
        let schemas = HashMap::from([("event".to_string(), event.clone())]);
        adapter.run_migrations(&schemas).await.unwrap();
        for (n, fields) in [
            (1, serde_json::json!({ "featured": true, "capacity": 1 })),
            (2, serde_json::json!({ "featured": false, "capacity": 0 })),
            (3, serde_json::json!({})),
            // 入力が `1` / `"true"` でも、保存して読めば真偽値
            (4, serde_json::json!({ "featured": 1 })),
            (5, serde_json::json!({ "featured": "true" })),
        ] {
            adapter
                .insert_content(&event, &record("event", n, "published", fields))
                .await
                .unwrap();
        }

        let get = |id: &str| {
            let adapter = adapter.clone();
            let event = event.clone();
            let id = id.to_string();
            async move {
                adapter
                    .get_content(&event, &id)
                    .await
                    .unwrap()
                    .unwrap()
                    .custom_fields
            }
        };
        let one = get("event-1").await;
        assert_eq!(one["featured"], serde_json::json!(true), "{one:?}");
        assert_eq!(
            one["capacity"],
            serde_json::json!(1),
            "an integer stays a number"
        );
        let two = get("event-2").await;
        assert_eq!(two["featured"], serde_json::json!(false), "{two:?}");
        assert_eq!(two["capacity"], serde_json::json!(0));
        assert!(
            !get("event-3").await.contains_key("featured"),
            "no value stays absent"
        );
        assert_eq!(get("event-4").await["featured"], serde_json::json!(true));
        assert_eq!(get("event-5").await["featured"], serde_json::json!(true));

        // 一覧も同じ
        let listed = adapter.list_contents(&event).await.unwrap();
        let featured: HashMap<&str, Option<&serde_json::Value>> = listed
            .iter()
            .map(|r| (r.id.as_str(), r.custom_fields.get("featured")))
            .collect();
        assert_eq!(featured["event-1"], Some(&serde_json::json!(true)));
        assert_eq!(featured["event-2"], Some(&serde_json::json!(false)));
    }

    #[tokio::test]
    async fn test_list_contents_page_filters_and_pages() {
        let adapter = memory_adapter().await;
        let schemas = schemas();
        adapter.run_migrations(&schemas).await.unwrap();
        let article = &schemas["article"];
        for n in 1..=5 {
            let status = if n == 3 { "draft" } else { "published" };
            adapter
                .insert_content(
                    article,
                    &record("article", n, status, serde_json::json!({})),
                )
                .await
                .unwrap();
        }

        let all = adapter.list_contents(article).await.unwrap();
        assert_eq!(all.len(), 5);
        assert_eq!(all[0].id, "article-5", "newest first");

        let page = adapter
            .list_contents_page(
                article,
                &ContentListQuery {
                    status: Some("published".to_string()),
                    limit: Some(2),
                    offset: 2,
                    ..Default::default()
                },
            )
            .await
            .unwrap();
        assert_eq!(page.total, 4);
        let ids: Vec<&str> = page.records.iter().map(|r| r.id.as_str()).collect();
        assert_eq!(ids, vec!["article-2", "article-1"]);
    }

    #[tokio::test]
    async fn test_content_update_and_version_insert_are_atomic() {
        let adapter = memory_adapter().await;
        let schemas = schemas();
        adapter.run_migrations(&schemas).await.unwrap();
        let schema = &schemas["article"];
        let original = record("article", 1, "draft", serde_json::json!({}));
        adapter.insert_content(schema, &original).await.unwrap();

        sqlx::query(
            "CREATE TRIGGER reject_content_version BEFORE INSERT ON _aegis_content_versions \
             BEGIN SELECT RAISE(ABORT, 'simulated version storage failure'); END;",
        )
        .execute(&adapter.pool)
        .await
        .unwrap();

        let mut changed = original.clone();
        changed.title = "Updated title".to_string();
        changed.updated_at = "2026-10-26T00:00:00Z".to_string();
        let version = ContentVersionRecord {
            content_type: changed.content_type.clone(),
            content_id: changed.id.clone(),
            revision: 0,
            snapshot_json: serde_json::to_string(&changed).unwrap(),
            created_at: changed.updated_at.clone(),
            created_by: Some("editor".to_string()),
            change_note: None,
        };

        assert!(adapter
            .update_content_with_versions(
                schema,
                &changed,
                &[version],
                ContentPublicationChange::Keep
            )
            .await
            .is_err());

        let actual = adapter
            .get_content(schema, &original.id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(actual.title, original.title);
        assert!(adapter
            .list_content_versions("article", &original.id)
            .await
            .unwrap()
            .is_empty());

        let new_record = record("article", 2, "draft", serde_json::json!({}));
        let new_version = ContentVersionRecord {
            content_type: new_record.content_type.clone(),
            content_id: new_record.id.clone(),
            revision: 0,
            snapshot_json: serde_json::to_string(&new_record).unwrap(),
            created_at: new_record.created_at.clone(),
            created_by: Some("editor".to_string()),
            change_note: None,
        };
        assert!(adapter
            .insert_content_with_versions(
                schema,
                &new_record,
                &[new_version],
                ContentPublicationChange::Keep
            )
            .await
            .is_err());
        assert!(adapter
            .get_content(schema, &new_record.id)
            .await
            .unwrap()
            .is_none());
    }

    #[tokio::test]
    async fn test_working_updates_do_not_change_published_snapshot() {
        let adapter = memory_adapter().await;
        let schemas = schemas();
        adapter.run_migrations(&schemas).await.unwrap();
        let schema = &schemas["article"];
        let mut published = record("article", 11, "published", serde_json::json!({}));
        published.published_at = Some(published.created_at.clone());
        let initial_version = ContentVersionRecord {
            content_type: published.content_type.clone(),
            content_id: published.id.clone(),
            revision: 0,
            snapshot_json: serde_json::to_string(&published).unwrap(),
            created_at: published.updated_at.clone(),
            created_by: None,
            change_note: None,
        };
        adapter
            .insert_content_with_versions(
                schema,
                &published,
                &[initial_version],
                ContentPublicationChange::PublishVersion(0),
            )
            .await
            .unwrap();

        let mut working = published.clone();
        working.title = "Working title".into();
        working.updated_at = "2026-10-26T00:01:00Z".into();
        let working_version = ContentVersionRecord {
            content_type: working.content_type.clone(),
            content_id: working.id.clone(),
            revision: 0,
            snapshot_json: serde_json::to_string(&working).unwrap(),
            created_at: working.updated_at.clone(),
            created_by: Some("editor".into()),
            change_note: None,
        };
        adapter
            .update_content_with_versions(
                schema,
                &working,
                std::slice::from_ref(&working_version),
                ContentPublicationChange::Keep,
            )
            .await
            .unwrap();
        let live = adapter
            .get_published_content(schema, &published.slug)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(live.title, published.title);

        adapter
            .update_content_with_versions(
                schema,
                &working,
                &[working_version],
                ContentPublicationChange::PublishVersion(0),
            )
            .await
            .unwrap();
        let live = adapter
            .get_published_content(schema, &published.slug)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(live.title, working.title);
    }

    #[tokio::test]
    async fn test_resolve_relations_in_one_query() {
        let adapter = memory_adapter().await;
        let schemas = schemas();
        adapter.run_migrations(&schemas).await.unwrap();
        for n in 1..=2 {
            adapter
                .insert_content(
                    &schemas["category"],
                    &record("category", n, "published", serde_json::json!({})),
                )
                .await
                .unwrap();
        }

        let ids = vec![
            "category-1".to_string(),
            "category-2".to_string(),
            "missing".to_string(),
        ];
        let found = adapter
            .resolve_relations(&schemas["category"], "id", &ids)
            .await
            .unwrap();
        assert_eq!(found.len(), 2);
        assert_eq!(found["category-2"].slug, "category-slug-2");
        assert!(adapter
            .resolve_relations(&schemas["category"], "id", &[])
            .await
            .unwrap()
            .is_empty());

        // 記事ごとの解決（同じ参照先はまとめて 1 回の問い合わせ）
        let records: Vec<HashMap<String, serde_json::Value>> = vec![
            serde_json::from_value(serde_json::json!({ "category_id": "category-1" })).unwrap(),
            HashMap::new(),
            serde_json::from_value(serde_json::json!({ "category_id": "  " })).unwrap(),
            serde_json::from_value(serde_json::json!({ "category_id": "category-1" })).unwrap(),
        ];
        let refs: Vec<_> = records.iter().collect();
        let resolved = super::super::resolve_relations_for_records(
            &adapter,
            &schemas,
            &schemas["article"],
            &refs,
        )
        .await;
        let titles: Vec<Option<&str>> = resolved
            .iter()
            .map(|r| r.get("category_id").map(|rel| rel.title.as_str()))
            .collect();
        assert_eq!(titles, vec![Some("Title 1"), None, None, Some("Title 1")]);
    }

    fn event_at(
        occurred_at: &str,
        action: AuditAction,
        outcome: AuditOutcome,
        user_id: &str,
    ) -> AuditEvent {
        let mut event = AuditEvent::new(action, outcome)
            .with_actor(Some(AuditActor {
                user_id: user_id.to_string(),
                role: "editor".to_string(),
            }))
            .with_resource("article", Some(format!("res-{}", user_id)))
            .with_client(Some("10.0.0.1".to_string()), Some("curl/8".to_string()))
            .with_details(serde_json::json!({ "slug": "hello" }));
        event.occurred_at = occurred_at.to_string();
        event
    }

    #[tokio::test]
    async fn test_audit_events_roundtrip_and_filters() {
        let adapter = memory_adapter().await;
        // init は冪等であること
        adapter.init().await.unwrap();

        let e1 = event_at(
            "2026-10-21T10:00:00.000000Z",
            AuditAction::ContentCreate,
            AuditOutcome::Success,
            "alice",
        );
        let e2 = event_at(
            "2026-10-21T11:00:00.000000Z",
            AuditAction::ContentCreate,
            AuditOutcome::Denied,
            "bob",
        );
        let mut e3 = AuditEvent::new(AuditAction::PluginError, AuditOutcome::Failure);
        e3.occurred_at = "2026-10-21T12:00:00.000000Z".to_string();

        for e in [&e1, &e2, &e3] {
            adapter.insert_audit_event(e).await.unwrap();
        }

        let all = adapter
            .query_audit_events(&AuditQuery {
                limit: 10,
                ..Default::default()
            })
            .await
            .unwrap();
        assert_eq!(all, vec![e3.clone(), e2.clone(), e1.clone()]);

        let by_action = adapter
            .query_audit_events(&AuditQuery {
                action: Some(AuditAction::ContentCreate),
                outcome: Some(AuditOutcome::Success),
                limit: 10,
                ..Default::default()
            })
            .await
            .unwrap();
        assert_eq!(by_action, vec![e1.clone()]);

        let by_actor_and_time = adapter
            .query_audit_events(&AuditQuery {
                actor_id: Some("bob".to_string()),
                resource_type: Some("article".to_string()),
                since: Some("2026-10-21T11:00:00.000000Z".to_string()),
                until: Some("2026-10-21T12:00:00.000000Z".to_string()),
                limit: 10,
                ..Default::default()
            })
            .await
            .unwrap();
        assert_eq!(by_actor_and_time, vec![e2.clone()]);

        let paged = adapter
            .query_audit_events(&AuditQuery {
                limit: 1,
                offset: 1,
                ..Default::default()
            })
            .await
            .unwrap();
        assert_eq!(paged, vec![e2]);
    }

    #[tokio::test]
    async fn test_audit_query_values_are_bound_not_interpolated() {
        let adapter = memory_adapter().await;
        adapter
            .insert_audit_event(&event_at(
                "2026-10-21T10:00:00.000000Z",
                AuditAction::SessionCreate,
                AuditOutcome::Success,
                "alice",
            ))
            .await
            .unwrap();

        let result = adapter
            .query_audit_events(&AuditQuery {
                actor_id: Some("x' OR '1'='1".to_string()),
                limit: 10,
                ..Default::default()
            })
            .await
            .unwrap();
        assert!(result.is_empty());
    }
}
