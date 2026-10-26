use std::collections::{HashMap, HashSet};
use std::str::FromStr;

use aegis_core::{AegisError, AuditEvent, AuditQuery, Result};
use aegis_schema::{FieldType, SchemaDefinition, SchemaMigration, SqlDialect};
use async_trait::async_trait;
use sqlx::sqlite::{
    SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions, SqliteRow, SqliteSynchronous,
};
use sqlx::{Pool, Row, Sqlite, SqliteConnection};

use super::{
    audit_insert_values, audit_row_from_sqlx, build_audit_delete_before, build_audit_insert,
    build_audit_select, build_content_list, build_content_update, build_identity_insert,
    build_identity_select, build_identity_table, build_relation_lookup, build_scheduled_publish,
    build_scheduled_unpublish, build_setting_delete, build_setting_upsert, build_settings_select,
    build_settings_table, build_user_delete, build_user_insert, build_user_select,
    build_user_update, custom_columns, is_standard_column, user_row_from_sqlx, ContentListQuery,
    ContentPage, ContentPublicationChange, ContentRecord, ContentVersionRecord, DatabaseAdapter,
    ResolvedRelation, UserRecord, AUDIT_INDEXES, AUDIT_TABLE, USER_AVATAR_COLUMN,
    USER_PENDING_APPROVAL_COLUMN, USER_TABLE,
};

async fn apply_publication_change(
    connection: &mut SqliteConnection,
    versions: &[ContentVersionRecord],
    revisions: &[i64],
    change: ContentPublicationChange,
) -> Result<()> {
    let index = match change {
        ContentPublicationChange::Keep => return Ok(()),
        ContentPublicationChange::Unpublish => {
            if let Some(version) = versions.first() {
                sqlx::query("DELETE FROM _aegis_content_publications WHERE content_type = ?1 AND content_id = ?2")
                    .bind(&version.content_type).bind(&version.content_id).execute(connection).await
                    .map_err(|e| AegisError::Database(e.to_string()))?;
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
    sqlx::query("INSERT INTO _aegis_content_publications (content_type, content_id, revision, slug, title, status, published_at, unpublish_at, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10) ON CONFLICT(content_type, content_id) DO UPDATE SET revision = excluded.revision, slug = excluded.slug, title = excluded.title, status = excluded.status, published_at = excluded.published_at, unpublish_at = excluded.unpublish_at, created_at = excluded.created_at, updated_at = excluded.updated_at")
        .bind(&version.content_type).bind(&version.content_id).bind(revision).bind(&record.slug).bind(&record.title)
        .bind(&record.status).bind(&record.published_at).bind(&record.unpublish_at).bind(&record.created_at).bind(&record.updated_at)
        .execute(connection).await.map_err(|e| AegisError::Database(e.to_string()))?;
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
            custom_fields.insert(field.name.clone(), value);
        }
    }
    custom_fields
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

#[derive(Debug, Clone)]
pub struct SqliteAdapter {
    pool: Pool<Sqlite>,
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

        Ok(Self { pool })
    }

    #[allow(dead_code)]
    pub fn new(pool: Pool<Sqlite>) -> Self {
        Self { pool }
    }
}

super::orders::impl_order_storage!(SqliteAdapter, |i| format!("?{}", i), "SQLite");
super::variants::impl_product_variant_storage!(SqliteAdapter, |i| format!("?{}", i), "SQLite");
super::comments::impl_comment_storage!(SqliteAdapter, |i| format!("?{}", i), "SQLite");
super::notifications::impl_notification_queue_storage!(
    SqliteAdapter,
    |i| format!("?{}", i),
    "SQLite"
);
super::forum::impl_forum_storage!(SqliteAdapter, |i| format!("?{}", i), "SQLite");
super::customers::impl_customer_storage!(SqliteAdapter, |i| format!("?{}", i), "SQLite");
super::reviews::impl_review_storage!(SqliteAdapter, |i| format!("?{}", i), "SQLite");

#[async_trait]
impl DatabaseAdapter for SqliteAdapter {
    async fn init(&self) -> Result<()> {
        sqlx::query("CREATE TABLE IF NOT EXISTS _aegis_content_versions (content_type TEXT NOT NULL, content_id TEXT NOT NULL, revision INTEGER NOT NULL, snapshot_json TEXT NOT NULL, created_at TEXT NOT NULL, created_by TEXT, change_note TEXT, PRIMARY KEY(content_type, content_id, revision));")
            .execute(&self.pool).await.map_err(|e| AegisError::Database(e.to_string()))?;
        sqlx::query("CREATE TABLE IF NOT EXISTS _aegis_content_publications (content_type TEXT NOT NULL, content_id TEXT NOT NULL, revision INTEGER NOT NULL, slug TEXT NOT NULL, title TEXT NOT NULL, status TEXT NOT NULL, published_at TEXT, unpublish_at TEXT, created_at TEXT NOT NULL, updated_at TEXT NOT NULL, PRIMARY KEY(content_type, content_id), UNIQUE(content_type, slug));")
            .execute(&self.pool).await.map_err(|e| AegisError::Database(e.to_string()))?;
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

        // 監査ログテーブル
        let audit_table_sql = format!(
            r#"
            CREATE TABLE IF NOT EXISTS {} (
                id TEXT PRIMARY KEY,
                occurred_at TEXT NOT NULL,
                action TEXT NOT NULL,
                outcome TEXT NOT NULL,
                actor_id TEXT,
                actor_role TEXT,
                resource_type TEXT,
                resource_id TEXT,
                ip TEXT,
                user_agent TEXT,
                details_json TEXT NOT NULL DEFAULT '{{}}'
            );
            "#,
            AUDIT_TABLE
        );
        sqlx::query(&audit_table_sql)
            .execute(&self.pool)
            .await
            .map_err(|e| AegisError::Database(e.to_string()))?;

        for (index_name, columns) in AUDIT_INDEXES {
            let sql = format!(
                "CREATE INDEX IF NOT EXISTS {} ON {} ({});",
                index_name, AUDIT_TABLE, columns
            );
            sqlx::query(&sql)
                .execute(&self.pool)
                .await
                .map_err(|e| AegisError::Database(e.to_string()))?;
        }

        // ローカル認証のユーザーテーブル
        let user_table_sql = format!(
            r#"
            CREATE TABLE IF NOT EXISTS {} (
                id TEXT PRIMARY KEY,
                username TEXT NOT NULL UNIQUE,
                email TEXT,
                display_name TEXT,
                role TEXT NOT NULL,
                password_hash TEXT NOT NULL,
                disabled INTEGER NOT NULL DEFAULT 0,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL,
                avatar_key TEXT,
                pending_approval INTEGER NOT NULL DEFAULT 0
            );
            "#,
            USER_TABLE
        );
        sqlx::query(&user_table_sql)
            .execute(&self.pool)
            .await
            .map_err(|e| AegisError::Database(e.to_string()))?;

        // 以前のバージョンで作成したテーブルに後から追加したカラムを足す
        let user_columns: HashSet<String> =
            sqlx::query(&format!("PRAGMA table_info({});", USER_TABLE))
                .fetch_all(&self.pool)
                .await
                .map_err(|e| AegisError::Database(e.to_string()))?
                .into_iter()
                .map(|r| r.get::<String, _>("name"))
                .collect();
        if !user_columns.contains(USER_AVATAR_COLUMN) {
            sqlx::query(&format!(
                "ALTER TABLE {} ADD COLUMN {} TEXT;",
                USER_TABLE, USER_AVATAR_COLUMN
            ))
            .execute(&self.pool)
            .await
            .map_err(|e| AegisError::Database(e.to_string()))?;
        }
        if !user_columns.contains(USER_PENDING_APPROVAL_COLUMN) {
            sqlx::query(&format!(
                "ALTER TABLE {} ADD COLUMN {} INTEGER NOT NULL DEFAULT 0;",
                USER_TABLE, USER_PENDING_APPROVAL_COLUMN
            ))
            .execute(&self.pool)
            .await
            .map_err(|e| AegisError::Database(e.to_string()))?;
        }

        // 外部 IdP のアイデンティティとユーザーの対応表
        sqlx::query(&build_identity_table("TEXT"))
            .execute(&self.pool)
            .await
            .map_err(|e| AegisError::Database(e.to_string()))?;

        // Admin Studio から変更したサイト全体の設定
        sqlx::query(&build_settings_table("TEXT", "TEXT"))
            .execute(&self.pool)
            .await
            .map_err(|e| AegisError::Database(e.to_string()))?;

        // EC の注文
        let (order_table, order_indexes) =
            super::orders::build_order_table("TEXT", "TEXT", "INTEGER", false);
        sqlx::query(&order_table)
            .execute(&self.pool)
            .await
            .map_err(|e| AegisError::Database(e.to_string()))?;

        // 以前のバージョンで作成した注文テーブルに後から追加したカラムを足す
        let order_columns: HashSet<String> = sqlx::query(&format!(
            "PRAGMA table_info({});",
            super::orders::ORDER_TABLE
        ))
        .fetch_all(&self.pool)
        .await
        .map_err(|e| AegisError::Database(e.to_string()))?
        .into_iter()
        .map(|r| r.get::<String, _>("name"))
        .collect();
        if !order_columns.contains("transaction_id") {
            sqlx::query(&format!(
                "ALTER TABLE {} ADD COLUMN transaction_id TEXT;",
                super::orders::ORDER_TABLE
            ))
            .execute(&self.pool)
            .await
            .map_err(|e| AegisError::Database(e.to_string()))?;
        }
        if !order_columns.contains("refunded_amount") {
            sqlx::query(&format!(
                "ALTER TABLE {} ADD COLUMN refunded_amount INTEGER NOT NULL DEFAULT 0;",
                super::orders::ORDER_TABLE
            ))
            .execute(&self.pool)
            .await
            .map_err(|e| AegisError::Database(e.to_string()))?;
        }
        if !order_columns.contains("refunds_json") {
            sqlx::query(&format!(
                "ALTER TABLE {} ADD COLUMN refunds_json TEXT NOT NULL DEFAULT '[]';",
                super::orders::ORDER_TABLE
            ))
            .execute(&self.pool)
            .await
            .map_err(|e| AegisError::Database(e.to_string()))?;
        }
        if !order_columns.contains("customer_id") {
            sqlx::query(&format!(
                "ALTER TABLE {} ADD COLUMN customer_id TEXT;",
                super::orders::ORDER_TABLE
            ))
            .execute(&self.pool)
            .await
            .map_err(|e| AegisError::Database(e.to_string()))?;
        }

        for index in order_indexes {
            sqlx::query(&index)
                .execute(&self.pool)
                .await
                .map_err(|e| AegisError::Database(e.to_string()))?;
        }

        // 商品バリエーション
        let (variant_table, variant_indexes) =
            super::variants::build_product_variant_table("TEXT", "TEXT", "INTEGER", false);
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

        // 顧客
        let (customer_table, customer_indexes) =
            super::customers::build_customer_table("TEXT", "TEXT", false);
        sqlx::query(&customer_table)
            .execute(&self.pool)
            .await
            .map_err(|e| AegisError::Database(e.to_string()))?;
        for index in customer_indexes {
            sqlx::query(&index)
                .execute(&self.pool)
                .await
                .map_err(|e| AegisError::Database(e.to_string()))?;
        }

        // 商品レビュー
        let (review_table, review_indexes) =
            super::reviews::build_review_table("TEXT", "TEXT", "INTEGER", false);
        sqlx::query(&review_table)
            .execute(&self.pool)
            .await
            .map_err(|e| AegisError::Database(e.to_string()))?;
        for index in review_indexes {
            sqlx::query(&index)
                .execute(&self.pool)
                .await
                .map_err(|e| AegisError::Database(e.to_string()))?;
        }

        // コメント
        let (comment_table, comment_indexes) =
            super::comments::build_comment_table("TEXT", "TEXT", "INTEGER", false);
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
        let (queue_table, queue_indexes) =
            super::notifications::build_notification_queue_table("TEXT", "TEXT", "INTEGER", false);
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
            super::forum::build_forum_tables("TEXT", "TEXT", "INTEGER", false);
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

            if !existing_columns.contains("unpublish_at") {
                let alter_sql = format!("ALTER TABLE {} ADD COLUMN unpublish_at TEXT;", table);
                tracing::info!("Adding missing unpublish_at column (SQLite): {}", alter_sql);
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

        // Preserve content published before immutable publication pointers existed.
        for schema in schemas.values() {
            for record in self
                .list_contents(schema)
                .await?
                .into_iter()
                .filter(|record| record.status == "published")
            {
                let exists: i64 = sqlx::query_scalar(
                    "SELECT COUNT(*) FROM _aegis_content_publications WHERE content_type = ?1 AND content_id = ?2",
                )
                .bind(&record.content_type)
                .bind(&record.id)
                .fetch_one(&self.pool)
                .await
                .map_err(|e| AegisError::Database(e.to_string()))?;
                if exists == 0 {
                    let version = ContentVersionRecord {
                        content_type: record.content_type.clone(),
                        content_id: record.id.clone(),
                        revision: 0,
                        snapshot_json: serde_json::to_string(&record)
                            .map_err(|e| AegisError::Database(e.to_string()))?,
                        created_at: chrono::Utc::now().to_rfc3339(),
                        created_by: None,
                        change_note: Some("Initial published snapshot".into()),
                    };
                    self.update_content_with_versions(
                        schema,
                        &record,
                        &[version],
                        ContentPublicationChange::EnsureVersion(0),
                    )
                    .await?;
                }
            }
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
        apply_publication_change(&mut tx, versions, &revisions, publication).await?;
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
        Ok(result.rows_affected() > 0)
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
        apply_publication_change(&mut tx, versions, &revisions, publication).await?;
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
        let status = query.status.as_deref();
        let select = if status.is_some() {
            "SELECT v.snapshot_json, p.status, p.published_at, p.unpublish_at, p.updated_at FROM _aegis_content_publications p JOIN _aegis_content_versions v ON v.content_type = p.content_type AND v.content_id = p.content_id AND v.revision = p.revision WHERE p.content_type = ?1 AND p.status = ?2 ORDER BY p.created_at DESC, p.content_id DESC"
        } else {
            "SELECT v.snapshot_json, p.status, p.published_at, p.unpublish_at, p.updated_at FROM _aegis_content_publications p JOIN _aegis_content_versions v ON v.content_type = p.content_type AND v.content_id = p.content_id AND v.revision = p.revision WHERE p.content_type = ?1 ORDER BY p.created_at DESC, p.content_id DESC"
        };
        let mut rows_query = sqlx::query(select).bind(&schema.content_type);
        if let Some(status) = status {
            rows_query = rows_query.bind(status);
        }
        let rows = rows_query
            .fetch_all(&self.pool)
            .await
            .map_err(|e| AegisError::Database(e.to_string()))?;
        let records = rows
            .iter()
            .map(|row| {
                super::published_record_from_snapshot(
                    &row.get::<String, _>("snapshot_json"),
                    row.get("status"),
                    row.get("published_at"),
                    row.get("unpublish_at"),
                    row.get("updated_at"),
                )
            })
            .collect::<Result<Vec<_>>>()?;
        let legacy_working = if status.is_none() || status == Some("published") {
            self.list_contents_page(
                schema,
                &ContentListQuery {
                    status: Some("published".into()),
                    ..ContentListQuery::default()
                },
            )
            .await?
            .records
        } else {
            Vec::new()
        };
        Ok(super::merge_published_content_records(
            records,
            legacy_working,
            query,
        ))
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
        tx.commit()
            .await
            .map_err(|e| AegisError::Database(e.to_string()))?;
        Ok(true)
    }

    async fn delete_content(&self, schema: &SchemaDefinition, id: &str) -> Result<bool> {
        let result = sqlx::query(&format!(
            "DELETE FROM {} WHERE id = ?1;",
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
        let (select, count, binds) =
            build_content_list(&schema.table_name(), query, |i| format!("?{}", i));

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
    async fn test_init_adds_the_avatar_column_to_an_existing_user_table() {
        let adapter = SqliteAdapter::connect("sqlite::memory:", 1).await.unwrap();
        // ユーザーテーブルに avatar_key が無かったバージョンの DB
        sqlx::query(
            "CREATE TABLE _aegis_users (id TEXT PRIMARY KEY, username TEXT NOT NULL UNIQUE, \
             email TEXT, display_name TEXT, role TEXT NOT NULL, password_hash TEXT NOT NULL, \
             disabled INTEGER NOT NULL DEFAULT 0, created_at TEXT NOT NULL, \
             updated_at TEXT NOT NULL);",
        )
        .execute(&adapter.pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO _aegis_users VALUES ('u1', 'old', NULL, NULL, 'admin', 'x', 0, 't', 't');",
        )
        .execute(&adapter.pool)
        .await
        .unwrap();

        adapter.init().await.unwrap();
        // 2 回目の起動でも失敗しない
        adapter.init().await.unwrap();
        let user = adapter.get_user_by_username("old").await.unwrap().unwrap();
        assert_eq!(user.avatar_key, None);
        assert_eq!(user.pending_approval, false);
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
                &[working_version.clone()],
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
