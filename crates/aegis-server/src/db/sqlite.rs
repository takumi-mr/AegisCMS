use std::collections::{HashMap, HashSet};
use std::str::FromStr;

use aegis_core::{AegisError, AuditEvent, AuditQuery, Result};
use aegis_schema::{FieldType, SchemaDefinition, SchemaMigration, SqlDialect};
use async_trait::async_trait;
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions, SqliteRow};
use sqlx::{Pool, Row, Sqlite};

use super::{
    audit_insert_values, audit_row_from_sqlx, build_audit_delete_before, build_audit_insert,
    build_audit_select, build_content_list, build_identity_insert, build_identity_select,
    build_identity_table, build_relation_lookup, build_user_insert, build_user_select,
    build_user_update, is_standard_column, user_row_from_sqlx, ContentListQuery, ContentPage,
    ContentRecord, DatabaseAdapter, ResolvedRelation, UserRecord, AUDIT_INDEXES, AUDIT_TABLE,
    USER_TABLE,
};

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
                updated_at TEXT NOT NULL
            );
            "#,
            USER_TABLE
        );
        sqlx::query(&user_table_sql)
            .execute(&self.pool)
            .await
            .map_err(|e| AegisError::Database(e.to_string()))?;

        // 外部 IdP のアイデンティティとユーザーの対応表
        sqlx::query(&build_identity_table("TEXT"))
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

    async fn update_user(&self, user: &UserRecord) -> Result<()> {
        sqlx::query(&build_user_update(|i| format!("?{}", i)))
            .bind(&user.email)
            .bind(&user.display_name)
            .bind(&user.role)
            .bind(&user.password_hash)
            .bind(i64::from(user.disabled))
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
