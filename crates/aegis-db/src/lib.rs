//! データベース（`[database]`: SQLite / PostgreSQL / MySQL）。コンテンツ・監査ログ・ユーザー・注文・コメント・通知の送信待ちの保存

pub mod analytics;
pub mod comments;
pub mod config;
pub mod contact;
pub mod forum;
pub mod members;
pub mod mysql;
pub mod newsletter;
pub mod notifications;
pub mod orders;
pub mod postgres;
pub mod published_columns;
pub mod published_exec;
pub mod reviews;
pub mod sqlite;
pub mod sqlx_compat;
pub mod tags;
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
pub use forum::{
    ForumCategoryRecord, ForumPostRecord, ForumReportRecord, ForumStorage, ForumThreadRecord,
    PostListQuery, ThreadListQuery, FORUM_CATEGORY_TABLE, FORUM_POST_TABLE, FORUM_REPORT_TABLE,
    FORUM_THREAD_TABLE,
};
pub use members::{MemberRecord, MemberStorage, MEMBER_COLUMNS, MEMBER_TABLE};
pub use mysql::MySqlAdapter;
pub use newsletter::{
    NewsletterIssueRecord, NewsletterListQuery, NewsletterPage, NewsletterStorage,
    NewsletterSubscriberRecord, NEWSLETTER_ISSUE_TABLE, NEWSLETTER_SUBSCRIBER_TABLE,
};
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
    ReviewAggregateStats, ReviewListQuery, ReviewPage, ReviewRecord, ReviewStorage, REVIEW_COLUMNS,
    REVIEW_TABLE,
};
pub use sqlite::SqliteAdapter;
pub use tags::{
    ContentTagRecord, TagListQuery, TagPage, TagRecord, TagStorage, TagWithUsage,
    CONTENT_TAG_TABLE, TAG_TABLE,
};
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

/// 記事・ページの `access`（必要な会員レベル）。未設定・空は `None`（docs/members.md 15）
pub fn access_of(fields: &HashMap<String, serde_json::Value>) -> Option<&str> {
    fields
        .get(aegis_schema::ACCESS_FIELD)
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
}

/// 閲覧を制限している（`access` がレベルを求めている）か。`public` は制限なし
pub fn is_restricted(fields: &HashMap<String, serde_json::Value>) -> bool {
    !aegis_core::is_public_access(access_of(fields))
}

impl ContentRecord {
    /// 閲覧を制限している記事・ページか。制限している本文は、フィード・検索・一覧・API に全文を出さない
    pub fn is_restricted(&self) -> bool {
        is_restricted(&self.custom_fields)
    }
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

/// 公開中の行を公開の記録なしに書いたとき（`insert_content` / `update_content`）に、その場の内容を
/// 公開版にする（公開の記録がまだ無い場合だけポインターを張る）。本番の書き込みは
/// `*_with_versions` で公開の記録を作るので、これを通るのはテストと、版の無いバックアップの復元だけ
pub async fn ensure_publication<D: DatabaseAdapter + ?Sized>(
    db: &D,
    schema: &SchemaDefinition,
    record: &ContentRecord,
) -> Result<()> {
    if record.status != "published" {
        return Ok(());
    }
    let version = ContentVersionRecord {
        content_type: record.content_type.clone(),
        content_id: record.id.clone(),
        revision: 0,
        snapshot_json: serde_json::to_string(record)
            .map_err(|e| AegisError::Database(e.to_string()))?,
        created_at: chrono::Utc::now().to_rfc3339(),
        created_by: None,
        change_note: Some("Initial published snapshot".into()),
    };
    db.update_content_with_versions(
        schema,
        record,
        &[version],
        ContentPublicationChange::EnsureVersion(0),
    )
    .await?;
    Ok(())
}

/// 公開の記録に持つ並べ替え・絞り込み用の列の値（カテゴリ, 並べ替えの時刻）。
/// カテゴリが無い行は空文字（`NULL` は「まだ入れていない」の意味で、起動時に埋める）。
/// 並べ替えの時刻は公開日時、無ければ作成日時
pub fn publication_columns(record: &ContentRecord) -> (String, String) {
    let category = record
        .custom_fields
        .get("category_id")
        .and_then(|v| v.as_str())
        .unwrap_or_default()
        .to_string();
    let sort_at = record
        .published_at
        .clone()
        .unwrap_or_else(|| record.created_at.clone());
    (category, sort_at)
}

/// 公開版の並べ替えの基準
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PublishedOrder {
    /// 作成日時
    CreatedAt,
    /// 公開日時（無ければ作成日時）
    PublishedAt,
}

impl PublishedOrder {
    /// レコードの並べ替えのキー
    pub fn key_of<'a>(&self, record: &'a ContentRecord) -> &'a str {
        match self {
            Self::CreatedAt => &record.created_at,
            Self::PublishedAt => record.published_at.as_deref().unwrap_or(&record.created_at),
        }
    }

    fn column(&self) -> &'static str {
        match self {
            Self::CreatedAt => "p.created_at",
            Self::PublishedAt => "p.sort_at",
        }
    }
}

/// 公開版の前後の問い合わせ（`list_published_neighbors`）
#[derive(Debug, Clone)]
pub struct PublishedNeighborQuery {
    pub order: PublishedOrder,
    /// `false`: 基準より前（古い側）を近い順、`true`: 基準より後（新しい側）を近い順
    pub newer: bool,
    /// 基準の並べ替えのキーと ID。基準自身は含まない
    pub key: String,
    pub id: String,
    /// この `category_id` の記事だけ
    pub category_id: Option<String>,
    pub limit: u64,
}

/// 公開版の前後を全件の一覧から探す（公開の記録に載らない行があるときと、既定の実装）
pub async fn published_neighbors_from_list<D: DatabaseAdapter + ?Sized>(
    db: &D,
    schema: &SchemaDefinition,
    query: &PublishedNeighborQuery,
) -> Result<Vec<ContentRecord>> {
    let all = db
        .list_published_contents_page(
            schema,
            &ContentListQuery {
                status: Some("published".into()),
                ..Default::default()
            },
        )
        .await?;
    let mut found: Vec<ContentRecord> = all
        .records
        .into_iter()
        .filter(|r| {
            let rank = (query.order.key_of(r), r.id.as_str());
            let base = (query.key.as_str(), query.id.as_str());
            let on_side = if query.newer {
                rank > base
            } else {
                rank < base
            };
            on_side
                && query.category_id.as_deref().is_none_or(|category| {
                    r.custom_fields
                        .get("category_id")
                        .and_then(|v| v.as_str())
                        .unwrap_or_default()
                        == category
                })
        })
        .collect();
    found.sort_by(|a, b| {
        let (ka, kb) = (
            (query.order.key_of(a), &a.id),
            (query.order.key_of(b), &b.id),
        );
        if query.newer {
            ka.cmp(&kb)
        } else {
            kb.cmp(&ka)
        }
    });
    found.truncate(usize::try_from(query.limit).unwrap_or(usize::MAX));
    Ok(found)
}

const PUBLISHED_SELECT: &str =
    "SELECT v.snapshot_json, p.status, p.published_at, p.unpublish_at, p.updated_at \
     FROM _aegis_content_publications p \
     JOIN _aegis_content_versions v ON v.content_type = p.content_type \
     AND v.content_id = p.content_id AND v.revision = p.revision";

/// 公開版の SQL（バインドする値は文字列だけ。`placeholder(n)` は n 番目の値の記号）。
/// MySQL の `?` は出てくる順に値を渡すので、値は SQL に出てくる順に並べる
pub struct PublishedSql {
    pub sql: String,
    pub binds: Vec<String>,
}

fn published_where(
    content_type: &str,
    status: Option<&str>,
    category: Option<&str>,
    tag_id: Option<&str>,
    placeholder: &impl Fn(usize) -> String,
    binds: &mut Vec<String>,
) -> String {
    let mut bind = |value: &str| {
        binds.push(value.to_string());
        placeholder(binds.len())
    };
    let mut clause = format!("WHERE p.content_type = {}", bind(content_type));
    if let Some(status) = status {
        clause.push_str(&format!(" AND p.status = {}", bind(status)));
    }
    if let Some(category) = category {
        clause.push_str(&format!(" AND p.category_id = {}", bind(category)));
    }
    if let Some(tag_id) = tag_id {
        clause.push_str(&format!(
            " AND p.content_id IN (SELECT t.content_id FROM {} t              WHERE t.content_type = p.content_type AND t.tag_id = {})",
            tags::CONTENT_TAG_TABLE,
            bind(tag_id)
        ));
    }
    clause
}

/// 絞り込みの `category_id`（値が無いものは空文字 = カテゴリ無し）
fn category_filter(query: &ContentListQuery) -> Option<&str> {
    query
        .filters
        .iter()
        .find(|(field, _)| field == "category_id")
        .map(|(_, value)| value.as_deref().unwrap_or_default())
}

/// 公開版の一覧のページ（`list_published_contents_page`）。公開の記録だけを読み、並べ替え・
/// 絞り込み（状態・`category_id`）・キーセット・`offset`・`limit` を SQL で行う
pub fn build_published_list(
    content_type: &str,
    query: &ContentListQuery,
    placeholder: impl Fn(usize) -> String,
) -> PublishedSql {
    let mut binds = Vec::new();
    let mut sql = format!(
        "{} {}",
        PUBLISHED_SELECT,
        published_where(
            content_type,
            query.status.as_deref(),
            category_filter(query),
            query.tag_id.as_deref(),
            &placeholder,
            &mut binds,
        )
    );
    if let (Some(created_at), Some(id)) = (&query.cursor_created_at, &query.cursor_id) {
        let mut bind = |value: &str| {
            binds.push(value.to_string());
            placeholder(binds.len())
        };
        let (a, b, c) = (bind(created_at), bind(created_at), bind(id));
        sql.push_str(&format!(
            " AND (p.created_at < {a} OR (p.created_at = {b} AND p.content_id < {c}))"
        ));
    }
    sql.push_str(" ORDER BY p.created_at DESC, p.content_id DESC");
    if let Some(limit) = query.limit {
        sql.push_str(&format!(" LIMIT {} OFFSET {}", limit, query.offset));
    }
    PublishedSql { sql, binds }
}

/// 公開版の総数（`build_published_list` と同じ状態・`category_id` の条件。キーセットは含めない）
pub fn build_published_count(
    content_type: &str,
    query: &ContentListQuery,
    placeholder: impl Fn(usize) -> String,
) -> PublishedSql {
    let mut binds = Vec::new();
    let clause = published_where(
        content_type,
        query.status.as_deref(),
        category_filter(query),
        query.tag_id.as_deref(),
        &placeholder,
        &mut binds,
    );
    PublishedSql {
        sql: format!(
            "SELECT COUNT(*) FROM _aegis_content_publications p {}",
            clause
        ),
        binds,
    }
}

/// 公開版の前後（`list_published_neighbors`）。公開中（`status = 'published'`）だけ
pub fn build_published_neighbors(
    content_type: &str,
    query: &PublishedNeighborQuery,
    placeholder: impl Fn(usize) -> String,
) -> PublishedSql {
    let mut binds = Vec::new();
    let mut sql = format!(
        "{} {}",
        PUBLISHED_SELECT,
        published_where(
            content_type,
            Some("published"),
            query.category_id.as_deref(),
            None,
            &placeholder,
            &mut binds,
        )
    );
    let mut bind = |value: &str| {
        binds.push(value.to_string());
        placeholder(binds.len())
    };
    let column = query.order.column();
    let (cmp, direction) = if query.newer {
        (">", "ASC")
    } else {
        ("<", "DESC")
    };
    let (a, b, c) = (bind(&query.key), bind(&query.key), bind(&query.id));
    sql.push_str(&format!(
        " AND ({column} {cmp} {a} OR ({column} = {b} AND p.content_id {cmp} {c})) \
         ORDER BY {column} {direction}, p.content_id {direction} LIMIT {}",
        query.limit
    ));
    PublishedSql { sql, binds }
}

/// 公開版の一覧の索引（並べ替え・状態・カテゴリ）
/// 並べ替えの基準（作成日時 / 公開日時）× カテゴリの有無の 4 通りに 1 つずつ。
/// カテゴリ内の前後は「そのカテゴリで、基準より前に 1 件も無い」ことも多く、
/// 索引が無いと全件を端まで読んで初めて「無い」と分かる
pub const PUBLICATION_INDEXES: [(&str, &str); 4] = [
    (
        "idx_content_publications_list",
        "(content_type, status, created_at, content_id)",
    ),
    (
        "idx_content_publications_category",
        "(content_type, status, category_id, created_at, content_id)",
    ),
    (
        "idx_content_publications_sort",
        "(content_type, status, sort_at, content_id)",
    ),
    (
        "idx_content_publications_category_sort",
        "(content_type, status, category_id, sort_at, content_id)",
    ),
];

/// コンテンツ一覧の条件（`list_contents_page`）
#[derive(Debug, Clone, Default)]
pub struct ContentListQuery {
    /// 指定した公開状態（`published` など）のみ
    pub status: Option<String>,
    /// タイトル・スラッグ（と `search_columns`）への部分一致。大文字小文字を区別しない（ASCII のみ）
    pub search: Option<String>,
    /// `search` の対象に加える列（スキーマの文字列フィールド。タイトルとスラッグは常に対象）
    pub search_columns: Vec<String>,
    /// カスタムフィールドの値での絞り込み（列名, 値）。値が `None` なら未設定（NULL か空文字）のもの。
    /// 列名はスキーマのフィールド名を渡す（安全な識別子でなければ何にも一致しない）
    pub filters: Vec<(String, Option<String>)>,
    /// このタグ（ID）がついているものだけ
    pub tag_id: Option<String>,
    /// 取得する最大件数。`None` なら全件（`offset` は無視）
    pub limit: Option<u64>,
    pub offset: u64,
    /// キーセットページネーション: 指定した作成日時・ID より古いレコードを取得する
    pub cursor_created_at: Option<String>,
    pub cursor_id: Option<String>,
}

/// SQL に埋め込んでよい列名（英数字と `_`）
fn is_safe_identifier(name: &str) -> bool {
    !name.is_empty() && name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
}

/// `LIKE ... ESCAPE '!'` に渡す値の中の `%` `_` と、エスケープ文字 `!` を無効にする
pub(crate) fn escape_like(term: &str) -> String {
    let mut out = String::with_capacity(term.len());
    for c in term.chars() {
        if matches!(c, '!' | '%' | '_') {
            out.push('!');
        }
        out.push(c);
    }
    out
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
    + TagStorage
    + NotificationQueueStorage
    + ForumStorage
    + MemberStorage
    + ReviewStorage
    + AnalyticsStorage
    + ContactStorage
    + NewsletterStorage
    + Send
    + Sync
{
    /// Initialize the database, checking connectivity and setting up migration history tables
    async fn init(&self) -> Result<()>;

    /// Apply schema migrations dynamically across dialects
    async fn run_migrations(&self, schemas: &HashMap<String, SchemaDefinition>) -> Result<()>;

    /// 宣言的クエリが読む公開側の列（型ごとの副表）を用意する。定義が変わった副表は作り直して、
    /// 公開中のスナップショットから埋める。以後の公開・取り下げ・削除は、同じトランザクションで副表を更新する。
    /// 既定は何もしない（副表を持たない実装）
    async fn configure_published_columns(
        &self,
        _plan: published_columns::PublishedColumnsPlan,
    ) -> Result<()> {
        Ok(())
    }

    /// `configure_published_columns` と同じだが、定義が同じでも副表をすべて作り直す。
    /// 副表は公開の記録から導ける派生データなので、`task restore` のあとのリセットに使う
    async fn rebuild_published_columns(
        &self,
        plan: published_columns::PublishedColumnsPlan,
    ) -> Result<()> {
        self.configure_published_columns(plan).await
    }

    /// 検証済みの宣言的クエリを実行する（`max_limit` は行数の上限）。
    /// 集計は行の JSON、`select` 省略は公開版のレコードを返す
    async fn run_query(
        &self,
        _query: &aegis_schema::QueryDefinition,
        _schema: &SchemaDefinition,
        _max_limit: usize,
    ) -> Result<published_exec::QueryRows> {
        Err(AegisError::Database(
            "This database adapter does not support declarative queries".into(),
        ))
    }

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
    ///
    /// 対応する条件は `status`・`limit`/`offset`・キーセット（`cursor_*`）・`tag_id`・`filters` のうち
    /// `category_id` だけ（`search` など、ほかは無視する）。`limit` を指定したときの
    /// `total` は、キーセットを除いた条件に合う公開版の総数
    async fn list_published_contents_page(
        &self,
        schema: &SchemaDefinition,
        query: &ContentListQuery,
    ) -> Result<ContentPage>;

    /// 公開中の記事のうち、基準（並べ替えのキーと ID）の前または後を近い順に `limit` 件。
    /// 記事ページの前後の記事のように、全件を読まずに隣だけが要る場面で使う。
    /// 既定の実装は全件の一覧から探す（対応するバックエンドは索引で引く）
    async fn list_published_neighbors(
        &self,
        schema: &SchemaDefinition,
        query: &PublishedNeighborQuery,
    ) -> Result<Vec<ContentRecord>> {
        published_neighbors_from_list(self, schema, query).await
    }

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

    /// 公開版の記録（`_aegis_content_publications`）から、公開中のものだけを `target_field` の値で引く。
    /// コンテンツ本体のテーブルは編集中の内容（未公開のタイトルや下書き）を持つので使わない。
    /// 公開の配信 API が、公開済みの記事から参照される先を返すために使う。
    /// `target_field` は `id` と `slug` に対応する（ほかは何も返さない）。参照先が公開中でなければ結果に入らない
    async fn resolve_published_relations(
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
    /// List all external identity links
    async fn list_identity_links(&self) -> Result<Vec<UserIdentityRecord>> {
        Ok(Vec::new())
    }
}

/// 外部 IdP とユーザーの紐付けレコード（`_aegis_user_identities` テーブルの 1 行）
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UserIdentityRecord {
    pub provider: String,
    pub subject: String,
    pub username: String,
    pub created_at: String,
}

/// ローカル認証のユーザーアカウント（`_aegis_users` テーブルの 1 行）
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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
    /// TOTP (二段階認証) の Base32 シークレット
    #[serde(default)]
    pub totp_secret: Option<String>,
    /// 二段階認証が有効化されているか
    #[serde(default)]
    pub totp_enabled: bool,
    /// 使い捨てバックアップコードのハッシュ一覧（JSON 配列）
    #[serde(default)]
    pub backup_codes: Option<String>,
}

/// ユーザーテーブル名（コンテンツテーブル `content_*` と衝突しないシステムテーブル）
pub const USER_TABLE: &str = "_aegis_users";

/// INSERT / SELECT で使うユーザーテーブルのカラム（順序固定）
pub const USER_COLUMNS: [&str; 14] = [
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
    "totp_secret",
    "totp_enabled",
    "backup_codes",
];

/// 後から追加したユーザーテーブルのカラム（既存の DB には起動時に `ALTER TABLE` で追加する）
pub const USER_AVATAR_COLUMN: &str = "avatar_key";
pub const USER_PENDING_APPROVAL_COLUMN: &str = "pending_approval";
pub const USER_TOTP_SECRET_COLUMN: &str = "totp_secret";
pub const USER_TOTP_ENABLED_COLUMN: &str = "totp_enabled";
pub const USER_BACKUP_CODES_COLUMN: &str = "backup_codes";

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
/// avatar_key, pending_approval, totp_secret, totp_enabled, backup_codes, updated_at, id
pub fn build_user_update(placeholder: impl Fn(usize) -> String) -> String {
    format!(
        "UPDATE {} SET email = {}, display_name = {}, role = {}, password_hash = {}, \
         disabled = {}, avatar_key = {}, pending_approval = {}, totp_secret = {}, \
         totp_enabled = {}, backup_codes = {}, updated_at = {} WHERE id = {};",
        USER_TABLE,
        placeholder(1),
        placeholder(2),
        placeholder(3),
        placeholder(4),
        placeholder(5),
        placeholder(6),
        placeholder(7),
        placeholder(8),
        placeholder(9),
        placeholder(10),
        placeholder(11),
        placeholder(12)
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

/// 対応表の SELECT 文（全件）
pub fn build_identity_list() -> String {
    format!(
        "SELECT provider, subject, username, created_at FROM {} ORDER BY provider, subject;",
        IDENTITY_TABLE
    )
}

/// Admin Studio から変更したサイト全体の設定（`name` → `value`）。設定ファイルの値より優先する。
/// MySQL の予約語 `key` を避けて `name` / `value` にする
pub const SETTINGS_TABLE: &str = "_aegis_settings";

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
            totp_secret: row.try_get("totp_secret").unwrap_or(None),
            totp_enabled: row.try_get::<i64, _>("totp_enabled").unwrap_or(0) != 0,
            backup_codes: row.try_get("backup_codes").unwrap_or(None),
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

/// 全コンテンツ共通の標準カラム・プロパティか（定義は `aegis-schema` の `STANDARD_COLUMNS` が正）
pub use aegis_schema::is_standard_column;

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

/// 手順（マイグレーション）の適用に失敗したときのエラー。DB が今のバイナリより新しい手順まで
/// 適用済みのとき（新しい版で更新した DB を古い版で開いたとき）は、そのことを伝える
pub(crate) fn migration_error(error: &sqlx::migrate::MigrateError) -> AegisError {
    use sqlx::migrate::MigrateError;
    match error {
        MigrateError::VersionMissing(version) => AegisError::Database(format!(
            "The database has migration {version} applied, but this version of AegisCMS does not know it.              The database was updated by a newer version; start that version, or restore a backup taken before the update."
        )),
        MigrateError::VersionMismatch(version) => AegisError::Database(format!(
            "Migration {version} was changed after it was applied (its checksum differs).              Released migration files must never be edited; add a new migration instead."
        )),
        other => AegisError::Database(format!("Database migration failed: {other}")),
    }
}

/// 以前の版では無かった列。この列が無い（古い版で作った）DB は、読み書きのたびに SQL が失敗するので、
/// 起動時に確かめて、移行の手順を示すエラーで止める。古い DB を起動時に書き換えることはしない
pub const REQUIRED_COLUMNS: &[(&str, &[&str])] = &[
    (
        USER_TABLE,
        &[
            USER_AVATAR_COLUMN,
            USER_PENDING_APPROVAL_COLUMN,
            USER_TOTP_SECRET_COLUMN,
            USER_TOTP_ENABLED_COLUMN,
            USER_BACKUP_CODES_COLUMN,
        ],
    ),
    (
        orders::ORDER_TABLE,
        &[
            "transaction_id",
            "refunded_amount",
            "refunds_json",
            "member_id",
        ],
    ),
    (members::MEMBER_TABLE, &["nickname", "avatar_key"]),
    (forum::FORUM_CATEGORY_TABLE, &["visibility"]),
    (forum::FORUM_THREAD_TABLE, &["status", "author_kind"]),
    (forum::FORUM_POST_TABLE, &["status", "author_kind"]),
    ("_aegis_content_publications", &["category_id", "sort_at"]),
];

/// 列がすべてあるかを確かめる SELECT（行は読まない。どの DB でも同じ文で、列が無ければ失敗する）
pub fn build_column_probe(table: &str, columns: &[&str]) -> String {
    format!("SELECT {} FROM {} LIMIT 0;", columns.join(", "), table)
}

/// 古い版で作った DB に、必要な列が無いときのエラー
pub fn outdated_database_error(table: &str, columns: &[&str], cause: &str) -> AegisError {
    AegisError::Database(format!(
        "The database was created by an older AegisCMS: table `{table}` lacks one of the columns ({}). \
         The system tables are no longer upgraded in place (v0.3.x was the last version that did). \
         Start v0.3.x once so that it upgrades the database, export the data with it \
         (`task dump`), then restore it into a new database with this version (`task restore`). \
         (database error: {cause})",
        columns.join(", ")
    ))
}

/// コンテンツ一覧の生成 SQL とバインド値
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContentListSql {
    pub select: String,
    pub select_binds: Vec<String>,
    pub count: String,
    pub count_binds: Vec<String>,
}

/// コンテンツ一覧の SELECT 文・件数の SELECT 文とそれぞれのバインド値を組み立てる。
/// 値はすべてバインドし、LIMIT / OFFSET のみ数値を埋め込む
pub fn build_content_list(
    table: &str,
    content_type: &str,
    query: &ContentListQuery,
    placeholder: impl Fn(usize) -> String,
) -> ContentListSql {
    let mut binds = Vec::new();
    let mut conditions: Vec<String> = Vec::new();
    if let Some(status) = &query.status {
        binds.push(status.clone());
        conditions.push(format!("status = {}", placeholder(binds.len())));
    }
    if let Some(tag_id) = &query.tag_id {
        binds.push(content_type.to_string());
        let ct_ph = placeholder(binds.len());
        binds.push(tag_id.clone());
        let tag_ph = placeholder(binds.len());
        conditions.push(format!(
            "id IN (SELECT content_id FROM {} WHERE content_type = {} AND tag_id = {})",
            crate::tags::CONTENT_TAG_TABLE,
            ct_ph,
            tag_ph,
        ));
    }
    for (column, value) in &query.filters {
        if !is_safe_identifier(column) {
            conditions.push("1 = 0".to_string());
            continue;
        }
        match value {
            Some(value) => {
                binds.push(value.clone());
                conditions.push(format!("{} = {}", column, placeholder(binds.len())));
            }
            None => conditions.push(format!("({0} IS NULL OR {0} = '')", column)),
        }
    }
    if let Some(term) = query
        .search
        .as_deref()
        .map(str::trim)
        .filter(|term| !term.is_empty())
    {
        let pattern = format!("%{}%", escape_like(&term.to_lowercase()));
        let columns = ["title", "slug"]
            .into_iter()
            .chain(
                query
                    .search_columns
                    .iter()
                    .map(String::as_str)
                    .filter(|column| is_safe_identifier(column)),
            )
            .collect::<Vec<_>>();
        // 値は列の数だけバインドする（`?` の MySQL は位置で対応づけるため）
        let matches: Vec<String> = columns
            .iter()
            .map(|column| {
                binds.push(pattern.clone());
                format!(
                    "LOWER({}) LIKE {} ESCAPE '!'",
                    column,
                    placeholder(binds.len())
                )
            })
            .collect();
        conditions.push(format!("({})", matches.join(" OR ")));
    }
    let count_where_clause = if conditions.is_empty() {
        String::new()
    } else {
        format!(" WHERE {}", conditions.join(" AND "))
    };
    let count_binds = binds.clone();

    let mut select_conditions = conditions;
    let mut select_binds = binds;
    if let (Some(created_at), Some(id)) = (&query.cursor_created_at, &query.cursor_id) {
        let p1 = placeholder(select_binds.len() + 1);
        let p2 = placeholder(select_binds.len() + 2);
        let p3 = placeholder(select_binds.len() + 3);
        select_conditions.push(format!(
            "(created_at < {} OR (created_at = {} AND id < {}))",
            p1, p2, p3
        ));
        select_binds.push(created_at.clone());
        select_binds.push(created_at.clone());
        select_binds.push(id.clone());
    }

    let select_where_clause = if select_conditions.is_empty() {
        String::new()
    } else {
        format!(" WHERE {}", select_conditions.join(" AND "))
    };

    let paging = match query.limit {
        Some(limit)
            if query.cursor_created_at.is_some()
                && query.cursor_id.is_some()
                && query.offset == 0 =>
        {
            format!(" LIMIT {}", limit)
        }
        Some(limit) => format!(" LIMIT {} OFFSET {}", limit, query.offset),
        None => String::new(),
    };

    ContentListSql {
        select: format!(
            "SELECT * FROM {}{} ORDER BY created_at DESC, id DESC{};",
            table, select_where_clause, paging
        ),
        select_binds,
        count: format!("SELECT COUNT(*) FROM {}{};", table, count_where_clause),
        count_binds,
    }
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

/// 公開版の記録から、公開中のものを `id` または `slug` で引く SELECT（`resolve_published_relations`）。
/// 1 つ目のバインドは `content_type`、続けて参照の値。`target_field` が `id` / `slug` 以外なら `None`
pub fn build_published_relation_lookup(
    target_field: &str,
    count: usize,
    placeholder: impl Fn(usize) -> String,
) -> Option<String> {
    let column = match target_field {
        "id" => "content_id",
        "slug" => "slug",
        _ => return None,
    };
    let placeholders: Vec<String> = (2..count + 2).map(&placeholder).collect();
    Some(format!(
        "SELECT content_id AS id, content_type, title, slug, {} AS ref_key FROM _aegis_content_publications WHERE content_type = {} AND status = 'published' AND {} IN ({});",
        column,
        placeholder(1),
        column,
        placeholders.join(", ")
    ))
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

    /// 標準カラムの一覧は `aegis-schema` の 1 つだけ。`blocks` / `rendered_html` という名前の
    /// フィールドは、テーブルの列にも INSERT / UPDATE の列にもならない
    #[test]
    fn test_custom_columns_skip_every_standard_name() {
        let schema: SchemaDefinition = serde_json::from_value(json!({
            "content_type": "article",
            "label": "A",
            "fields": [
                { "name": "blocks", "type": "text" },
                { "name": "rendered_html", "type": "text" },
                { "name": "title", "type": "string" },
                { "name": "excerpt", "type": "string" }
            ]
        }))
        .unwrap();
        assert_eq!(custom_columns(&schema), vec!["excerpt".to_string()]);
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
            ..Default::default()
        };
        let plan = build_content_list("content_article", "article", &query, |i| format!("${}", i));
        assert_eq!(
            plan.select,
            "SELECT * FROM content_article WHERE status = $1 ORDER BY created_at DESC, id DESC LIMIT 10 OFFSET 20;"
        );
        assert_eq!(
            plan.count,
            "SELECT COUNT(*) FROM content_article WHERE status = $1;"
        );
        assert_eq!(plan.select_binds, vec!["published"]);
        assert_eq!(plan.count_binds, vec!["published"]);

        let plan = build_content_list(
            "content_article",
            "article",
            &ContentListQuery::default(),
            |_| "?".into(),
        );
        assert_eq!(
            plan.select,
            "SELECT * FROM content_article ORDER BY created_at DESC, id DESC;"
        );
        assert_eq!(plan.count, "SELECT COUNT(*) FROM content_article;");
        assert!(plan.select_binds.is_empty());
        assert!(plan.count_binds.is_empty());

        // キーセットページネーション
        let query = ContentListQuery {
            status: Some("published".to_string()),
            cursor_created_at: Some("2026-10-22T00:00:03Z".to_string()),
            cursor_id: Some("id-3".to_string()),
            limit: Some(10),
            ..Default::default()
        };
        let plan = build_content_list("content_article", "article", &query, |i| format!("${}", i));
        assert_eq!(
            plan.select,
            "SELECT * FROM content_article WHERE status = $1 AND (created_at < $2 OR (created_at = $3 AND id < $4)) ORDER BY created_at DESC, id DESC LIMIT 10;"
        );
        assert_eq!(
            plan.count,
            "SELECT COUNT(*) FROM content_article WHERE status = $1;"
        );
        assert_eq!(
            plan.select_binds,
            vec![
                "published",
                "2026-10-22T00:00:03Z",
                "2026-10-22T00:00:03Z",
                "id-3"
            ]
        );
        assert_eq!(plan.count_binds, vec!["published"]);

        // 検索と絞り込み: 値はすべてバインドし、`?` でも `$n` でも同じ順に並ぶ
        let query = ContentListQuery {
            status: Some("draft".to_string()),
            search: Some("  50%_Off!  ".to_string()),
            search_columns: vec!["excerpt".to_string(), "bad column".to_string()],
            filters: vec![
                ("category_id".to_string(), Some("cat-1".to_string())),
                ("brand_id".to_string(), None),
            ],
            tag_id: None,
            limit: Some(20),
            offset: 40,
            ..Default::default()
        };
        let plan = build_content_list("content_article", "article", &query, |i| format!("${}", i));
        let where_clause =
            "WHERE status = $1 AND category_id = $2 AND (brand_id IS NULL OR brand_id = '') \
             AND (LOWER(title) LIKE $3 ESCAPE '!' OR LOWER(slug) LIKE $4 ESCAPE '!' \
             OR LOWER(excerpt) LIKE $5 ESCAPE '!')";
        assert_eq!(
            plan.select,
            format!(
                "SELECT * FROM content_article {} ORDER BY created_at DESC, id DESC LIMIT 20 OFFSET 40;",
                where_clause
            )
        );
        assert_eq!(
            plan.count,
            format!("SELECT COUNT(*) FROM content_article {};", where_clause)
        );
        // 検索語の `%` `_` `!` は `!` でエスケープし、小文字にする（列名に使えない文字を含むものは対象外）
        let pattern = "%50!%!_off!!%".to_string();
        assert_eq!(
            plan.select_binds,
            vec![
                "draft",
                "cat-1",
                pattern.as_str(),
                pattern.as_str(),
                pattern.as_str()
            ]
        );
        assert_eq!(plan.count_binds, plan.select_binds);

        // タグでの絞り込み
        let query = ContentListQuery {
            tag_id: Some("tag-123".to_string()),
            ..Default::default()
        };
        let plan = build_content_list("content_article", "article", &query, |i| format!("${}", i));
        assert_eq!(
            plan.select,
            "SELECT * FROM content_article WHERE id IN (SELECT content_id FROM _aegis_content_tags WHERE content_type = $1 AND tag_id = $2) ORDER BY created_at DESC, id DESC;"
        );
        assert_eq!(
            plan.count,
            "SELECT COUNT(*) FROM content_article WHERE id IN (SELECT content_id FROM _aegis_content_tags WHERE content_type = $1 AND tag_id = $2);"
        );
        assert_eq!(plan.select_binds, vec!["article", "tag-123"]);
        assert_eq!(plan.count_binds, vec!["article", "tag-123"]);
        // 列名が安全でない絞り込みは何にも一致しない
        let query = ContentListQuery {
            filters: vec![("x; DROP TABLE y".to_string(), Some("1".to_string()))],
            ..Default::default()
        };
        let plan = build_content_list("content_article", "article", &query, |_| "?".into());
        assert!(plan.select.contains("WHERE 1 = 0"), "{}", plan.select);
        assert!(plan.select_binds.is_empty());
        // 空の検索語は無視する
        let query = ContentListQuery {
            search: Some("   ".to_string()),
            ..Default::default()
        };
        let plan = build_content_list("content_article", "article", &query, |_| "?".into());
        assert!(!plan.select.contains("WHERE"), "{}", plan.select);

        assert_eq!(
            build_relation_lookup("content_category", "id", 3, |_| "?".into()),
            "SELECT id, content_type, title, slug, id AS ref_key FROM content_category WHERE id IN (?, ?, ?);"
        );
        assert_eq!(
            build_published_relation_lookup("slug", 2, |i| format!("${}", i)).unwrap(),
            "SELECT content_id AS id, content_type, title, slug, slug AS ref_key FROM _aegis_content_publications WHERE content_type = $1 AND status = 'published' AND slug IN ($2, $3);"
        );
        // id / slug 以外は引かない（列名を SQL に埋め込まない）
        assert!(
            build_published_relation_lookup("title; DROP TABLE x", 1, |_| "?".into()).is_none()
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

    fn list_query() -> ContentListQuery {
        ContentListQuery {
            status: Some("published".into()),
            filters: vec![("category_id".into(), Some("cat-1".into()))],
            limit: Some(10),
            offset: 20,
            cursor_created_at: Some("2026-10-04T00:00:00Z".into()),
            cursor_id: Some("p4".into()),
            ..Default::default()
        }
    }

    #[test]
    fn published_list_sql_binds_in_the_order_the_placeholders_appear() {
        let dollar = build_published_list("article", &list_query(), |i| format!("${}", i));
        assert!(dollar.sql.contains(
            "WHERE p.content_type = $1 AND p.status = $2 AND p.category_id = $3 AND (p.created_at < $4 OR (p.created_at = $5 AND p.content_id < $6)) ORDER BY p.created_at DESC, p.content_id DESC LIMIT 10 OFFSET 20"
        ));
        let expected = [
            "article",
            "published",
            "cat-1",
            "2026-10-04T00:00:00Z",
            "2026-10-04T00:00:00Z",
            "p4",
        ];
        assert_eq!(dollar.binds, expected);
        // MySQL の `?` は出てくる順に値を渡す。値の並びは同じでよい
        let question = build_published_list("article", &list_query(), |_| "?".to_string());
        assert_eq!(question.binds, expected);
        assert_eq!(question.sql.matches('?').count(), expected.len());

        // 総数はキーセットを含めず、結合もしない
        let count = build_published_count("article", &list_query(), |i| format!("${}", i));
        assert_eq!(
            count.sql,
            "SELECT COUNT(*) FROM _aegis_content_publications p WHERE p.content_type = $1 AND p.status = $2 AND p.category_id = $3"
        );
        assert_eq!(count.binds, ["article", "published", "cat-1"]);

        // `limit` なしは LIMIT / OFFSET を付けない。カテゴリ無し（値 None）は空文字で絞る
        let all = build_published_list(
            "article",
            &ContentListQuery {
                filters: vec![("category_id".into(), None)],
                offset: 5,
                ..Default::default()
            },
            |i| format!("?{}", i),
        );
        assert!(!all.sql.contains("LIMIT"));
        assert_eq!(all.binds, ["article", ""]);
    }

    #[test]
    fn published_neighbors_sql_compares_the_sort_key_then_the_id() {
        let query = |order, newer| PublishedNeighborQuery {
            order,
            newer,
            key: "K".into(),
            id: "I".into(),
            category_id: Some("cat".into()),
            limit: 3,
        };
        let older =
            build_published_neighbors("article", &query(PublishedOrder::PublishedAt, false), |i| {
                format!("${}", i)
            });
        assert!(older.sql.contains(
            "WHERE p.content_type = $1 AND p.status = $2 AND p.category_id = $3 AND (p.sort_at < $4 OR (p.sort_at = $5 AND p.content_id < $6)) ORDER BY p.sort_at DESC, p.content_id DESC LIMIT 3"
        ));
        assert_eq!(older.binds, ["article", "published", "cat", "K", "K", "I"]);
        let newer =
            build_published_neighbors("article", &query(PublishedOrder::CreatedAt, true), |_| {
                "?".to_string()
            });
        assert!(newer.sql.contains(
            "AND (p.created_at > ? OR (p.created_at = ? AND p.content_id > ?)) ORDER BY p.created_at ASC, p.content_id ASC LIMIT 3"
        ));
    }
}

#[cfg(test)]
mod migration_files_tests {
    /// 手順は 3 つの DB で同じ番号・同じ名前にそろえる（1 つの DB にだけ足し忘れると、
    /// その DB を使うサイトだけ、新しい手順が適用されずに動かなくなる）
    #[test]
    fn every_dialect_has_the_same_migrations() {
        let list = |migrator: &sqlx::migrate::Migrator| -> Vec<(i64, String)> {
            migrator
                .iter()
                .map(|m| (m.version, m.description.to_string()))
                .collect()
        };
        let sqlite = list(&crate::sqlite::MIGRATOR);
        assert!(!sqlite.is_empty());
        assert_eq!(sqlite, list(&crate::postgres::MIGRATOR), "postgres");
        assert_eq!(sqlite, list(&crate::mysql::MIGRATOR), "mysql");
        // 改行は LF だけ（sqlx は改行を直さずにチェックサムを取るので、Windows で CRLF で取り出した
        // ファイルをビルドすると、Linux でビルドしたバイナリが適用した DB と、チェックサムが合わなくなる。
        // `.gitattributes` で LF に固定している）
        for migrator in [
            &crate::sqlite::MIGRATOR,
            &crate::postgres::MIGRATOR,
            &crate::mysql::MIGRATOR,
        ] {
            for migration in migrator.iter() {
                assert!(
                    !migration.sql.as_str().contains('\r'),
                    "migration {} has CR characters",
                    migration.version
                );
            }
        }
        // 番号は 1 から連番（欠番や重複があると、適用の順序が分からなくなる）
        for (i, (version, _)) in sqlite.iter().enumerate() {
            assert_eq!(*version, i as i64 + 1, "{sqlite:?}");
        }
    }
}
