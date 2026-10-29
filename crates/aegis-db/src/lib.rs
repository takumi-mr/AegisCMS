//! データベース（`[database]`: SQLite / PostgreSQL / MySQL）。コンテンツ・監査ログ・ユーザー・注文・コメント・通知の送信待ちの保存

pub mod analytics;
pub mod comments;
pub mod config;
pub mod contact;
pub mod customers;
pub mod forum;
pub mod mysql;
pub mod notifications;
pub mod orders;
pub mod postgres;
pub mod reviews;
pub mod sqlite;
pub mod sqlx_compat;
pub mod variants;

use aegis_core::{AegisError, AuditActor, AuditEvent, AuditQuery, Result};

use aegis_schema::{FieldDefinition, FieldType, SchemaDefinition};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;

pub use analytics::{AnalyticsCount, AnalyticsStorage, AnalyticsTotal, ANALYTICS_TABLE};
pub use comments::{CommentListQuery, CommentPage, CommentRecord, CommentStorage, COMMENT_TABLE};
pub use config::{DatabaseBackendKind, DatabaseConfig};
pub use contact::{
    ContactListQuery, ContactMessageRecord, ContactPage, ContactStorage, CONTACT_TABLE,
};
pub use customers::{
    build_customer_table, CustomerRecord, CustomerStorage, CUSTOMER_COLUMNS, CUSTOMER_TABLE,
};
pub use forum::{
    ForumCategoryRecord, ForumPostRecord, ForumReportRecord, ForumStorage, ForumThreadRecord,
    PostListQuery, ThreadListQuery, FORUM_CATEGORY_TABLE, FORUM_POST_TABLE, FORUM_REPORT_TABLE,
    FORUM_THREAD_TABLE,
};
pub use mysql::MySqlAdapter;
pub use notifications::{
    NotificationListQuery, NotificationPage, NotificationQueueStorage, NotificationRelease,
    QueuedNotification, NOTIFICATION_QUEUE_TABLE, QUEUE_FAILED, QUEUE_PENDING,
};
pub use orders::{
    OrderListQuery, OrderPage, OrderRecord, OrderRefundUpdate, OrderStorage, OrderTransition,
    OutOfStock, SalesReportRow, StockReservation,
};
pub use postgres::PostgresAdapter;
pub use reviews::{
    build_review_table, ReviewAggregateStats, ReviewListQuery, ReviewPage, ReviewRecord,
    ReviewStorage, REVIEW_COLUMNS, REVIEW_TABLE,
};
pub use sqlite::SqliteAdapter;
pub use variants::{ProductVariantRecord, ProductVariantStorage, PRODUCT_VARIANT_TABLE};

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
    pub unpublish_at: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub custom_fields: HashMap<String, serde_json::Value>,
}

/// Immutable editorial snapshot of a content record.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContentVersionRecord {
    pub content_type: String,
    pub content_id: String,
    pub revision: i64,
    pub snapshot_json: String,
    pub created_at: String,
    pub created_by: Option<String>,
    pub change_note: Option<String>,
}

pub const CONTENT_VERSIONS_TABLE: &str = "_aegis_content_versions";
pub const CONTENT_PUBLICATIONS_TABLE: &str = "_aegis_content_publications";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContentPublicationChange {
    Keep,
    /// Point the public view at one of the snapshots in this same transaction.
    PublishVersion(usize),
    /// Point at this snapshot only if the content has no published version yet.
    EnsureVersion(usize),
    Unpublish,
}

pub fn record_from_content_version(version: &ContentVersionRecord) -> Result<ContentRecord> {
    serde_json::from_str(&version.snapshot_json)
        .map_err(|e| AegisError::Database(format!("Invalid content version snapshot: {}", e)))
}

pub fn published_record_from_snapshot(
    snapshot_json: &str,
    status: String,
    published_at: Option<String>,
    unpublish_at: Option<String>,
    updated_at: String,
) -> Result<ContentRecord> {
    let mut record: ContentRecord = serde_json::from_str(snapshot_json)
        .map_err(|e| AegisError::Database(format!("Invalid published snapshot: {}", e)))?;
    record.status = status;
    record.published_at = published_at;
    record.unpublish_at = unpublish_at;
    record.updated_at = updated_at;
    Ok(record)
}

/// Includes pre-pointer published rows while returning records in the same order and page shape.
pub fn merge_published_content_records(
    mut published: Vec<ContentRecord>,
    legacy_working: Vec<ContentRecord>,
    query: &ContentListQuery,
) -> ContentPage {
    let mut ids: std::collections::HashSet<String> =
        published.iter().map(|record| record.id.clone()).collect();
    published.extend(
        legacy_working
            .into_iter()
            .filter(|record| record.status == "published" && ids.insert(record.id.clone())),
    );
    published.sort_by(|a, b| {
        b.created_at
            .cmp(&a.created_at)
            .then_with(|| b.id.cmp(&a.id))
    });
    let total = published.len() as u64;
    let offset = usize::try_from(query.offset).unwrap_or(usize::MAX);
    let records = published
        .into_iter()
        .skip(offset)
        .take(query.limit.map_or(usize::MAX, |limit| {
            usize::try_from(limit).unwrap_or(usize::MAX)
        }))
        .collect();
    ContentPage { records, total }
}

/// コンテンツ一覧の条件（`list_contents_page`）
#[derive(Debug, Clone, Default)]
pub struct ContentListQuery {
    /// 指定した公開状態（`published` など）のみ
    pub status: Option<String>,
    /// 取得する最大件数。`None` なら全件（`offset` は無視）
    pub limit: Option<u64>,
    pub offset: u64,
}

/// `list_contents_page` の結果: 条件に一致する行（作成日の新しい順）と、ページングしない場合の総件数
#[derive(Debug, Clone)]
pub struct ContentPage {
    pub records: Vec<ContentRecord>,
    pub total: u64,
}

#[async_trait]
pub trait DatabaseAdapter:
    OrderStorage
    + ProductVariantStorage
    + CommentStorage
    + NotificationQueueStorage
    + ForumStorage
    + CustomerStorage
    + ReviewStorage
    + AnalyticsStorage
    + ContactStorage
    + Send
    + Sync
{
    /// Initialize the database, checking connectivity and setting up migration history tables
    async fn init(&self) -> Result<()>;

    /// Apply schema migrations dynamically across dialects
    async fn run_migrations(&self, schemas: &HashMap<String, SchemaDefinition>) -> Result<()>;

    /// Insert a content record
    async fn insert_content(&self, schema: &SchemaDefinition, record: &ContentRecord)
        -> Result<()>;

    /// Insert content and its immutable snapshots atomically.
    async fn insert_content_with_versions(
        &self,
        schema: &SchemaDefinition,
        record: &ContentRecord,
        versions: &[ContentVersionRecord],
        publication: ContentPublicationChange,
    ) -> Result<()>;

    /// Overwrite a content record's mutable columns (matched by `id`; `content_type` and
    /// `created_at` stay). Returns false when no record has that id
    async fn update_content(
        &self,
        schema: &SchemaDefinition,
        record: &ContentRecord,
    ) -> Result<bool>;

    /// Update content and append its immutable snapshots atomically.
    async fn update_content_with_versions(
        &self,
        schema: &SchemaDefinition,
        record: &ContentRecord,
        versions: &[ContentVersionRecord],
        publication: ContentPublicationChange,
    ) -> Result<bool>;

    /// Append an immutable content snapshot and return its assigned revision.
    async fn insert_content_version(&self, version: &ContentVersionRecord) -> Result<i64>;

    /// List versions for a content item, newest first.
    async fn list_content_versions(
        &self,
        content_type: &str,
        content_id: &str,
    ) -> Result<Vec<ContentVersionRecord>>;

    /// Fetch one immutable version.
    async fn get_content_version(
        &self,
        content_type: &str,
        content_id: &str,
        revision: i64,
    ) -> Result<Option<ContentVersionRecord>>;

    /// Fetch the immutable currently published snapshot by its published slug or content ID.
    async fn get_published_content(
        &self,
        schema: &SchemaDefinition,
        slug_or_id: &str,
    ) -> Result<Option<ContentRecord>>;

    /// List published snapshots with the standard content paging rules.
    async fn list_published_contents_page(
        &self,
        schema: &SchemaDefinition,
        query: &ContentListQuery,
    ) -> Result<ContentPage>;

    /// Publish a scheduled record (`status = 'scheduled'` → `'published'`, setting `updated_at`).
    /// Only matches while the record is still scheduled for `published_at`, so a record the
    /// editor rescheduled or unscheduled in the meantime is left alone. Returns false when nothing
    /// matched
    async fn publish_scheduled_content(
        &self,
        schema: &SchemaDefinition,
        id: &str,
        published_at: &str,
        updated_at: &str,
    ) -> Result<bool>;

    /// Unpublish (archive) a content record whose `unpublish_at` has arrived
    /// (`status = 'published'` → `'archived'`, setting `updated_at`).
    /// Matches while status is still 'published' and unpublish_at equals the expected timestamp.
    async fn unpublish_scheduled_content(
        &self,
        schema: &SchemaDefinition,
        id: &str,
        unpublish_at: &str,
        updated_at: &str,
    ) -> Result<bool>;

    /// Delete a content record by id. Returns false when no record has that id
    async fn delete_content(&self, schema: &SchemaDefinition, id: &str) -> Result<bool>;

    /// Fetch a single content record by slug or ID
    async fn get_content(
        &self,
        schema: &SchemaDefinition,
        slug_or_id: &str,
    ) -> Result<Option<ContentRecord>>;

    /// List all content records ordered by creation date (newest first)
    async fn list_contents(&self, schema: &SchemaDefinition) -> Result<Vec<ContentRecord>> {
        Ok(self
            .list_contents_page(schema, &ContentListQuery::default())
            .await?
            .records)
    }

    /// List content records matching `query` (newest first) with the total number of matches
    async fn list_contents_page(
        &self,
        schema: &SchemaDefinition,
        query: &ContentListQuery,
    ) -> Result<ContentPage>;

    /// Resolve many relation items with one query, keyed by their `target_field` value
    /// (values with no matching row are left out)
    async fn resolve_relations(
        &self,
        target_schema: &SchemaDefinition,
        target_field: &str,
        ref_ids: &[String],
    ) -> Result<HashMap<String, ResolvedRelation>>;

    /// Persist an audit event into the audit log table
    async fn insert_audit_event(&self, event: &AuditEvent) -> Result<()>;

    /// Search audit events, newest first
    async fn query_audit_events(&self, query: &AuditQuery) -> Result<Vec<AuditEvent>>;

    /// Delete audit events that occurred before `cutoff` (normalized timestamp) and return the count
    async fn delete_audit_events_before(&self, cutoff: &str) -> Result<u64>;

    /// Insert a local user account (fails if the username is already taken)
    async fn insert_user(&self, user: &UserRecord) -> Result<()>;

    /// Fetch a local user account by username
    async fn get_user_by_username(&self, username: &str) -> Result<Option<UserRecord>>;

    /// Overwrite a local user account's mutable columns (matched by `id`)
    async fn update_user(&self, user: &UserRecord) -> Result<()>;

    /// List local user accounts ordered by username
    async fn list_users(&self) -> Result<Vec<UserRecord>>;

    /// Delete a local user account (matched by `id`) and its external identity links
    async fn delete_user(&self, user: &UserRecord) -> Result<()>;

    /// Username linked to an external identity (`provider`, e.g. `oidc`, and the IdP's subject)
    async fn get_identity_link(&self, provider: &str, subject: &str) -> Result<Option<String>>;

    /// Link an external identity to a username (fails if the identity is already linked)
    async fn insert_identity_link(
        &self,
        provider: &str,
        subject: &str,
        username: &str,
    ) -> Result<()>;

    /// All site-wide settings saved from Admin Studio (`name` → `value`)
    async fn list_settings(&self) -> Result<HashMap<String, String>>;

    /// Save a setting (insert or overwrite)
    async fn set_setting(&self, name: &str, value: &str) -> Result<()>;

    /// Remove a setting (the configuration file's value applies again)
    async fn delete_setting(&self, name: &str) -> Result<()>;
}

/// ローカル認証のユーザーアカウント（`_aegis_users` テーブルの 1 行）
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UserRecord {
    pub id: String,
    /// ログイン名（一意・変更不可）。セッションの `user_id` や監査ログの actor に使う
    pub username: String,
    pub email: Option<String>,
    pub display_name: Option<String>,
    pub role: String,
    /// Argon2id の PHC 文字列
    pub password_hash: String,
    pub disabled: bool,
    /// アバター画像のストレージキー（`avatars/...`）。公開 URL は `StorageBackend::public_url` で求める
    pub avatar_key: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    /// 登録後の承認待ち（admin が有効化するまでログインできない）
    pub pending_approval: bool,
}

/// ユーザーテーブル名（コンテンツテーブル `content_*` と衝突しないシステムテーブル）
pub const USER_TABLE: &str = "_aegis_users";

/// INSERT / SELECT で使うユーザーテーブルのカラム（順序固定）
pub const USER_COLUMNS: [&str; 11] = [
    "id",
    "username",
    "email",
    "display_name",
    "role",
    "password_hash",
    "disabled",
    "created_at",
    "updated_at",
    "avatar_key",
    "pending_approval",
];

/// 後から追加したユーザーテーブルのカラム（既存の DB には起動時に `ALTER TABLE` で追加する）
pub const USER_AVATAR_COLUMN: &str = "avatar_key";
pub const USER_PENDING_APPROVAL_COLUMN: &str = "pending_approval";

/// プレースホルダ生成関数（1 始まりの番号を受け取る）を使ってユーザーの INSERT 文を組み立てる。
/// バインド順は `USER_COLUMNS` の順
pub fn build_user_insert(placeholder: impl Fn(usize) -> String) -> String {
    let placeholders: Vec<String> = (1..=USER_COLUMNS.len()).map(placeholder).collect();
    format!(
        "INSERT INTO {} ({}) VALUES ({});",
        USER_TABLE,
        USER_COLUMNS.join(", "),
        placeholders.join(", ")
    )
}

/// ユーザーの UPDATE 文。バインド順は email, display_name, role, password_hash, disabled,
/// avatar_key, pending_approval, updated_at, id
pub fn build_user_update(placeholder: impl Fn(usize) -> String) -> String {
    format!(
        "UPDATE {} SET email = {}, display_name = {}, role = {}, password_hash = {}, \
         disabled = {}, avatar_key = {}, pending_approval = {}, updated_at = {} WHERE id = {};",
        USER_TABLE,
        placeholder(1),
        placeholder(2),
        placeholder(3),
        placeholder(4),
        placeholder(5),
        placeholder(6),
        placeholder(7),
        placeholder(8),
        placeholder(9)
    )
}

/// ユーザーの DELETE 文と、そのユーザーの ID 連携を消す DELETE 文。バインドはそれぞれ id / username
pub fn build_user_delete(placeholder: impl Fn(usize) -> String) -> (String, String) {
    (
        format!("DELETE FROM {} WHERE id = {};", USER_TABLE, placeholder(1)),
        format!(
            "DELETE FROM {} WHERE username = {};",
            IDENTITY_TABLE,
            placeholder(1)
        ),
    )
}

/// ユーザーの SELECT 文（`where_clause` を省略すると全件、username 順）
pub fn build_user_select(where_clause: Option<&str>) -> String {
    format!(
        "SELECT {} FROM {}{} ORDER BY username;",
        USER_COLUMNS.join(", "),
        USER_TABLE,
        where_clause
            .map(|w| format!(" WHERE {}", w))
            .unwrap_or_default()
    )
}

/// 外部 IdP のアイデンティティ（プロバイダ + subject）と `_aegis_users` のユーザー名の対応表。
/// ローカルユーザーと同名の IdP ユーザーがそのアカウントに入れないよう、紐付けはここでだけ行う
pub const IDENTITY_TABLE: &str = "_aegis_user_identities";

/// 対応表の CREATE 文。`text` は文字列カラムの型（SQLite は `TEXT`、PostgreSQL / MySQL は
/// 主キーに使えるよう `VARCHAR(255)`）
pub fn build_identity_table(text: &str) -> String {
    format!(
        "CREATE TABLE IF NOT EXISTS {} (\
         provider {text} NOT NULL, \
         subject {text} NOT NULL, \
         username {text} NOT NULL, \
         created_at {text} NOT NULL, \
         PRIMARY KEY (provider, subject));",
        IDENTITY_TABLE,
        text = text
    )
}

/// 対応表の INSERT 文。バインド順は provider, subject, username, created_at
pub fn build_identity_insert(placeholder: impl Fn(usize) -> String) -> String {
    format!(
        "INSERT INTO {} (provider, subject, username, created_at) VALUES ({}, {}, {}, {});",
        IDENTITY_TABLE,
        placeholder(1),
        placeholder(2),
        placeholder(3),
        placeholder(4)
    )
}

/// 対応表の SELECT 文。バインド順は provider, subject
pub fn build_identity_select(placeholder: impl Fn(usize) -> String) -> String {
    format!(
        "SELECT username FROM {} WHERE provider = {} AND subject = {};",
        IDENTITY_TABLE,
        placeholder(1),
        placeholder(2)
    )
}

/// Admin Studio から変更したサイト全体の設定（`name` → `value`）。設定ファイルの値より優先する。
/// MySQL の予約語 `key` を避けて `name` / `value` にする
pub const SETTINGS_TABLE: &str = "_aegis_settings";

/// 設定テーブルの CREATE 文。`key_type` は `name` の型（SQLite は `TEXT`、PostgreSQL / MySQL は
/// 主キーに使えるよう `VARCHAR(255)`）、`text` は値の型
pub fn build_settings_table(key_type: &str, text: &str) -> String {
    format!(
        "CREATE TABLE IF NOT EXISTS {} (\
         name {key_type} PRIMARY KEY, \
         value {text} NOT NULL, \
         updated_at {key_type} NOT NULL);",
        SETTINGS_TABLE,
        key_type = key_type,
        text = text
    )
}

/// 設定の INSERT 文（同じ名前があれば上書き）。バインド順は name, value, updated_at。
/// `mysql` なら `ON DUPLICATE KEY UPDATE`、それ以外（SQLite / PostgreSQL）は `ON CONFLICT`
pub fn build_setting_upsert(placeholder: impl Fn(usize) -> String, mysql: bool) -> String {
    let on_conflict = if mysql {
        "ON DUPLICATE KEY UPDATE value = VALUES(value), updated_at = VALUES(updated_at)"
    } else {
        "ON CONFLICT (name) DO UPDATE SET value = excluded.value, updated_at = excluded.updated_at"
    };
    format!(
        "INSERT INTO {} (name, value, updated_at) VALUES ({}, {}, {}) {};",
        SETTINGS_TABLE,
        placeholder(1),
        placeholder(2),
        placeholder(3),
        on_conflict
    )
}

/// 設定の DELETE 文。バインドは name
pub fn build_setting_delete(placeholder: impl Fn(usize) -> String) -> String {
    format!(
        "DELETE FROM {} WHERE name = {};",
        SETTINGS_TABLE,
        placeholder(1)
    )
}

/// 設定の SELECT 文（全件）
pub fn build_settings_select() -> String {
    format!("SELECT name, value FROM {};", SETTINGS_TABLE)
}

/// sqlx の行 (`SqliteRow` / `PgRow` / `MySqlRow`) を `UserRecord` に変換する。
/// `disabled` は全ダイアレクトで 64bit 整数 (0 / 1) として保存する
macro_rules! user_row_from_sqlx {
    ($row:expr) => {{
        let row = $row;
        let get = |e: sqlx::Error| aegis_core::AegisError::Database(e.to_string());
        $crate::UserRecord {
            id: row.try_get("id").map_err(get)?,
            username: row.try_get("username").map_err(get)?,
            email: row.try_get("email").map_err(get)?,
            display_name: row.try_get("display_name").map_err(get)?,
            role: row.try_get("role").map_err(get)?,
            password_hash: row.try_get("password_hash").map_err(get)?,
            disabled: row.try_get::<i64, _>("disabled").map_err(get)? != 0,
            avatar_key: row.try_get("avatar_key").map_err(get)?,
            created_at: row.try_get("created_at").map_err(get)?,
            updated_at: row.try_get("updated_at").map_err(get)?,
            pending_approval: row.try_get::<i64, _>("pending_approval").unwrap_or(0) != 0,
        }
    }};
}
pub(crate) use user_row_from_sqlx;

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

/// `update_content` で上書きする標準カラム（バインド順）。この後にカスタム列、最後に `id`
pub const CONTENT_UPDATE_COLUMNS: [&str; 8] = [
    "title",
    "slug",
    "status",
    "blocks_json",
    "author_id",
    "published_at",
    "unpublish_at",
    "updated_at",
];

/// コンテンツの UPDATE 文。バインド順は `CONTENT_UPDATE_COLUMNS` → `custom_columns` → id
pub fn build_content_update(
    table: &str,
    custom_columns: &[String],
    placeholder: impl Fn(usize) -> String,
) -> String {
    let assignments: Vec<String> = CONTENT_UPDATE_COLUMNS
        .iter()
        .map(|c| c.to_string())
        .chain(custom_columns.iter().cloned())
        .enumerate()
        .map(|(i, column)| format!("{} = {}", column, placeholder(i + 1)))
        .collect();
    format!(
        "UPDATE {} SET {} WHERE id = {};",
        table,
        assignments.join(", "),
        placeholder(assignments.len() + 1)
    )
}

/// 予約公開の UPDATE 文（`publish_scheduled_content`）。バインド順は updated_at → id → published_at
pub fn build_scheduled_publish(table: &str, placeholder: impl Fn(usize) -> String) -> String {
    format!(
        "UPDATE {} SET status = 'published', updated_at = {} \
         WHERE id = {} AND status = 'scheduled' AND published_at = {};",
        table,
        placeholder(1),
        placeholder(2),
        placeholder(3)
    )
}

/// 予約非公開の UPDATE 文（`unpublish_scheduled_content`）。バインド順は updated_at → id → unpublish_at
pub fn build_scheduled_unpublish(table: &str, placeholder: impl Fn(usize) -> String) -> String {
    format!(
        "UPDATE {} SET status = 'archived', updated_at = {} \
         WHERE id = {} AND status = 'published' AND unpublish_at = {};",
        table,
        placeholder(1),
        placeholder(2),
        placeholder(3)
    )
}

/// スキーマのカスタム列（標準カラム以外のフィールド）の名前
pub fn custom_columns(schema: &SchemaDefinition) -> Vec<String> {
    schema
        .fields
        .iter()
        .filter(|f| !is_standard_column(&f.name))
        .map(|f| f.name.clone())
        .collect()
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
            | "unpublish_at"
            | "created_at"
            | "updated_at"
    )
}

/// 列型（`SchemaMigration` の型対応）に合わせてバインドするカスタムフィールド値（PostgreSQL / MySQL）
#[derive(Debug, Clone, PartialEq)]
pub enum CustomFieldValue {
    Text(Option<String>),
    BigInt(Option<i64>),
    Bool(Option<bool>),
    Json(Option<serde_json::Value>),
}

impl CustomFieldValue {
    pub fn from_json(field: &FieldDefinition, value: Option<&serde_json::Value>) -> Result<Self> {
        use serde_json::Value;

        let invalid = |v: &Value| {
            AegisError::Validation(format!(
                "Field '{}' expects {:?} but got {}",
                field.name, field.field_type, v
            ))
        };
        let present = value.filter(|v| !v.is_null());

        Ok(match field.field_type {
            FieldType::Jsonb => Self::Json(present.cloned()),
            FieldType::Integer => Self::BigInt(match present {
                None => None,
                Some(v @ Value::Number(n)) => Some(n.as_i64().ok_or_else(|| invalid(v))?),
                Some(v @ Value::String(s)) => Some(s.trim().parse().map_err(|_| invalid(v))?),
                Some(v) => return Err(invalid(v)),
            }),
            FieldType::Boolean => Self::Bool(match present {
                None => None,
                Some(Value::Bool(b)) => Some(*b),
                Some(v @ Value::String(s)) => match s.trim().to_ascii_lowercase().as_str() {
                    "true" | "1" => Some(true),
                    "false" | "0" => Some(false),
                    _ => return Err(invalid(v)),
                },
                Some(v @ Value::Number(n)) => match n.as_i64() {
                    Some(1) => Some(true),
                    Some(0) => Some(false),
                    _ => return Err(invalid(v)),
                },
                Some(v) => return Err(invalid(v)),
            }),
            // テキスト系の列は従来どおり文字列化して保存する（null は空文字列）
            _ => Self::Text(value.map(|v| match v {
                Value::String(s) => s.clone(),
                Value::Null => String::new(),
                other => other.to_string(),
            })),
        })
    }
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

/// 保持期間切れの監査ログを削除する DELETE 文を組み立てる（プレースホルダは 1 つ）
pub fn build_audit_delete_before(placeholder: &str) -> String {
    format!(
        "DELETE FROM {} WHERE occurred_at < {};",
        AUDIT_TABLE, placeholder
    )
}

/// リレーション列の値（参照先のキー）。空文字・文字列以外は参照なしとして扱う
pub fn relation_ref(value: &serde_json::Value) -> Option<&str> {
    value.as_str().map(str::trim).filter(|s| !s.is_empty())
}

/// 複数レコードのリレーションを、リレーション列ごとに 1 回の問い合わせでまとめて解決する。
/// 戻り値は `records` と同じ順序の「フィールド名 → 参照先」。問い合わせの失敗は警告ログを出して未解決とする
pub async fn resolve_relations_for_records(
    db: &dyn DatabaseAdapter,
    schemas: &HashMap<String, SchemaDefinition>,
    schema: &SchemaDefinition,
    records: &[&HashMap<String, serde_json::Value>],
) -> Vec<HashMap<String, ResolvedRelation>> {
    let mut resolved = vec![HashMap::new(); records.len()];
    for (field_def, target) in schema.relation_fields() {
        let Some(target_schema) = schemas.get(&target.content_type) else {
            continue;
        };
        let refs: Vec<Option<&str>> = records
            .iter()
            .map(|fields| fields.get(&field_def.name).and_then(relation_ref))
            .collect();
        let mut ids: Vec<String> = refs.iter().flatten().map(|id| id.to_string()).collect();
        ids.sort();
        ids.dedup();
        if ids.is_empty() {
            continue;
        }
        let found = match db
            .resolve_relations(target_schema, &target.field, &ids)
            .await
        {
            Ok(found) => found,
            Err(e) => {
                tracing::warn!(
                    "Failed to resolve relation '{}' to {}: {}",
                    field_def.name,
                    target.content_type,
                    e
                );
                continue;
            }
        };
        for (relations, id) in resolved.iter_mut().zip(&refs) {
            if let Some(relation) = id.and_then(|id| found.get(id)) {
                relations.insert(field_def.name.clone(), relation.clone());
            }
        }
    }
    resolved
}

/// コンテンツ一覧の SELECT 文・件数の SELECT 文と、両方に共通のバインド値を組み立てる。
/// 値はすべてバインドし、LIMIT / OFFSET のみ数値を埋め込む
pub fn build_content_list(
    table: &str,
    query: &ContentListQuery,
    placeholder: impl Fn(usize) -> String,
) -> (String, String, Vec<String>) {
    let mut binds = Vec::new();
    let where_clause = match &query.status {
        Some(status) => {
            binds.push(status.clone());
            format!(" WHERE status = {}", placeholder(1))
        }
        None => String::new(),
    };
    let paging = match query.limit {
        Some(limit) => format!(" LIMIT {} OFFSET {}", limit, query.offset),
        None => String::new(),
    };
    (
        format!(
            "SELECT * FROM {}{} ORDER BY created_at DESC, id DESC{};",
            table, where_clause, paging
        ),
        format!("SELECT COUNT(*) FROM {}{};", table, where_clause),
        binds,
    )
}

/// `resolve_relations` の SELECT 文（`ref_key` 列に `target_field` の値）。`count` 個の値を IN でバインドする
pub fn build_relation_lookup(
    table: &str,
    target_field: &str,
    count: usize,
    placeholder: impl Fn(usize) -> String,
) -> String {
    let placeholders: Vec<String> = (1..=count).map(placeholder).collect();
    format!(
        "SELECT id, content_type, title, slug, {} AS ref_key FROM {} WHERE {} IN ({});",
        target_field,
        table,
        target_field,
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
        $crate::AuditRow {
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

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn field(name: &str, field_type: FieldType) -> FieldDefinition {
        serde_json::from_value(json!({
            "name": name,
            "type": serde_json::to_value(field_type).unwrap(),
        }))
        .unwrap()
    }

    #[test]
    fn test_content_update_sql() {
        let sql = build_content_update(
            "content_product",
            &["price".to_string(), "stock".to_string()],
            |i| format!("${}", i),
        );
        assert_eq!(
            sql,
            "UPDATE content_product SET title = $1, slug = $2, status = $3, blocks_json = $4, \
             author_id = $5, published_at = $6, unpublish_at = $7, updated_at = $8, price = $9, stock = $10 \
             WHERE id = $11;"
        );
    }

    #[test]
    fn test_scheduled_publish_sql() {
        assert_eq!(
            build_scheduled_publish("content_article", |i| format!("${}", i)),
            "UPDATE content_article SET status = 'published', updated_at = $1 \
             WHERE id = $2 AND status = 'scheduled' AND published_at = $3;"
        );
        assert_eq!(
            build_scheduled_unpublish("content_article", |i| format!("${}", i)),
            "UPDATE content_article SET status = 'archived', updated_at = $1 \
             WHERE id = $2 AND status = 'published' AND unpublish_at = $3;"
        );
    }

    #[test]
    fn test_content_list_and_relation_sql() {
        let query = ContentListQuery {
            status: Some("published".to_string()),
            limit: Some(10),
            offset: 20,
        };
        let (select, count, binds) =
            build_content_list("content_article", &query, |i| format!("${}", i));
        assert_eq!(
            select,
            "SELECT * FROM content_article WHERE status = $1 ORDER BY created_at DESC, id DESC LIMIT 10 OFFSET 20;"
        );
        assert_eq!(
            count,
            "SELECT COUNT(*) FROM content_article WHERE status = $1;"
        );
        assert_eq!(binds, vec!["published"]);

        let (select, count, binds) =
            build_content_list("content_article", &ContentListQuery::default(), |_| {
                "?".into()
            });
        assert_eq!(
            select,
            "SELECT * FROM content_article ORDER BY created_at DESC, id DESC;"
        );
        assert_eq!(count, "SELECT COUNT(*) FROM content_article;");
        assert!(binds.is_empty());

        assert_eq!(
            build_relation_lookup("content_category", "id", 3, |_| "?".into()),
            "SELECT id, content_type, title, slug, id AS ref_key FROM content_category WHERE id IN (?, ?, ?);"
        );
    }

    #[test]
    fn test_jsonb_values_bind_as_json_and_null_as_sql_null() {
        let meta = field("custom_meta", FieldType::Jsonb);
        let object = json!({ "seo": { "title": "x" }, "tags": ["a"] });
        assert_eq!(
            CustomFieldValue::from_json(&meta, Some(&object)).unwrap(),
            CustomFieldValue::Json(Some(object))
        );
        assert_eq!(
            CustomFieldValue::from_json(&meta, Some(&json!("plain"))).unwrap(),
            CustomFieldValue::Json(Some(json!("plain")))
        );
        assert_eq!(
            CustomFieldValue::from_json(&meta, Some(&serde_json::Value::Null)).unwrap(),
            CustomFieldValue::Json(None)
        );
        assert_eq!(
            CustomFieldValue::from_json(&meta, None).unwrap(),
            CustomFieldValue::Json(None)
        );
    }

    #[test]
    fn test_integer_and_boolean_values_are_converted_or_rejected() {
        let count = field("count", FieldType::Integer);
        assert_eq!(
            CustomFieldValue::from_json(&count, Some(&json!(42))).unwrap(),
            CustomFieldValue::BigInt(Some(42))
        );
        assert_eq!(
            CustomFieldValue::from_json(&count, Some(&json!(" 7 "))).unwrap(),
            CustomFieldValue::BigInt(Some(7))
        );
        assert!(CustomFieldValue::from_json(&count, Some(&json!(1.5))).is_err());
        assert!(CustomFieldValue::from_json(&count, Some(&json!("many"))).is_err());

        let flag = field("featured", FieldType::Boolean);
        assert_eq!(
            CustomFieldValue::from_json(&flag, Some(&json!(true))).unwrap(),
            CustomFieldValue::Bool(Some(true))
        );
        assert_eq!(
            CustomFieldValue::from_json(&flag, Some(&json!("0"))).unwrap(),
            CustomFieldValue::Bool(Some(false))
        );
        assert!(CustomFieldValue::from_json(&flag, Some(&json!("yes"))).is_err());
        assert_eq!(
            CustomFieldValue::from_json(&flag, None).unwrap(),
            CustomFieldValue::Bool(None)
        );
    }

    #[test]
    fn test_text_values_keep_previous_stringification() {
        let title = field("subtitle", FieldType::String);
        assert_eq!(
            CustomFieldValue::from_json(&title, Some(&json!("hello"))).unwrap(),
            CustomFieldValue::Text(Some("hello".to_string()))
        );
        assert_eq!(
            CustomFieldValue::from_json(&title, Some(&json!(3))).unwrap(),
            CustomFieldValue::Text(Some("3".to_string()))
        );
        assert_eq!(
            CustomFieldValue::from_json(&title, Some(&serde_json::Value::Null)).unwrap(),
            CustomFieldValue::Text(Some(String::new()))
        );
        assert_eq!(
            CustomFieldValue::from_json(&title, None).unwrap(),
            CustomFieldValue::Text(None)
        );
    }
}
