//! ニュースレターのテーブル（購読者 `_aegis_newsletter_subscribers`、配信の履歴 `_aegis_newsletter_issues`）と
//! ストレージ。購読者は確認メール（ダブルオプトイン）で有効になり、解除リンクで止められる。
//! 3 つのダイアレクトで SQL を共有し、プレースホルダだけを呼び出し側（`?N` / `$N` / `?`）が決める

use aegis_core::Result;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

/// 購読者のテーブル名
pub const NEWSLETTER_SUBSCRIBER_TABLE: &str = "_aegis_newsletter_subscribers";
/// 配信の履歴のテーブル名
pub const NEWSLETTER_ISSUE_TABLE: &str = "_aegis_newsletter_issues";

/// `_aegis_newsletter_subscribers` のカラム（順序固定）
pub const NEWSLETTER_SUBSCRIBER_COLUMNS: [&str; 9] = [
    "id",
    "email",
    "status",
    "token",
    "ip",
    "created_at",
    "confirmed_at",
    "unsubscribed_at",
    "updated_at",
];

/// `_aegis_newsletter_issues` のカラム（順序固定）
pub const NEWSLETTER_ISSUE_COLUMNS: [&str; 8] = [
    "id",
    "content_type",
    "content_id",
    "title",
    "url",
    "recipients",
    "created_at",
    "created_by",
];

/// 購読者（`_aegis_newsletter_subscribers` の 1 行）
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct NewsletterSubscriberRecord {
    pub id: String,
    /// メールアドレス（小文字）。全購読者で重複しない
    pub email: String,
    /// `pending`（確認待ち）/ `active`（配信する）/ `unsubscribed`（解除した）
    pub status: String,
    /// 確認・解除のリンクに入れる推測できない文字列。全購読者で重複しない
    pub token: String,
    pub ip: Option<String>,
    pub created_at: String,
    /// 確認リンクを開いた日時
    pub confirmed_at: Option<String>,
    pub unsubscribed_at: Option<String>,
    /// 最後に変えた日時。`pending` のときは確認メールを送った日時（有効期限の起点）
    pub updated_at: String,
}

/// 配信の履歴（`_aegis_newsletter_issues` の 1 行）。コンテンツを購読者に送ったことの記録
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct NewsletterIssueRecord {
    pub id: String,
    pub content_type: String,
    pub content_id: String,
    /// 送ったときのタイトル
    pub title: String,
    /// 送ったときのページの URL
    pub url: String,
    /// 送った宛先の数
    pub recipients: i64,
    pub created_at: String,
    pub created_by: Option<String>,
}

/// 購読者一覧の条件
#[derive(Debug, Clone, Default)]
pub struct NewsletterListQuery {
    pub status: Option<String>,
    /// メールアドレスへの部分一致（大文字小文字を区別しない、ASCII のみ）
    pub search: Option<String>,
    /// 取得する最大件数。`None` なら全件（`offset` は無視）
    pub limit: Option<u64>,
    pub offset: u64,
}

/// `list_newsletter_subscribers` の結果: 登録の新しい順の行と、ページングしない場合の総件数
#[derive(Debug, Clone, Default)]
pub struct NewsletterPage {
    pub subscribers: Vec<NewsletterSubscriberRecord>,
    pub total: u64,
}

/// ニュースレターの保存（`DatabaseAdapter` のスーパートレイト）。実装は `impl_newsletter_storage!` で生成する
#[async_trait]
pub trait NewsletterStorage: Send + Sync {
    /// 追加する。メールアドレスかトークンが既にあるとエラー
    async fn insert_newsletter_subscriber(
        &self,
        subscriber: &NewsletterSubscriberRecord,
    ) -> Result<()>;

    async fn get_newsletter_subscriber(
        &self,
        id: &str,
    ) -> Result<Option<NewsletterSubscriberRecord>>;

    async fn get_newsletter_subscriber_by_email(
        &self,
        email: &str,
    ) -> Result<Option<NewsletterSubscriberRecord>>;

    async fn get_newsletter_subscriber_by_token(
        &self,
        token: &str,
    ) -> Result<Option<NewsletterSubscriberRecord>>;

    /// 状態・トークン・日時・IP を書き換える（`id` が一致する行。メールアドレスと登録日時は変えない）。
    /// 行が無ければ false
    async fn update_newsletter_subscriber(
        &self,
        subscriber: &NewsletterSubscriberRecord,
    ) -> Result<bool>;

    /// 条件に合う購読者（登録の新しい順）と総数
    async fn list_newsletter_subscribers(
        &self,
        query: &NewsletterListQuery,
    ) -> Result<NewsletterPage>;

    /// 状態ごとの人数（いない状態は含めない）
    async fn count_newsletter_subscribers_by_status(&self) -> Result<Vec<(String, u64)>>;

    /// 配信する購読者（`active`）を id の順に `limit` 人。`after` の次から（最初は `None`）
    async fn list_active_newsletter_subscribers(
        &self,
        after: Option<&str>,
        limit: u64,
    ) -> Result<Vec<NewsletterSubscriberRecord>>;

    /// 購読者を消す。消した人数を返す
    async fn delete_newsletter_subscribers(&self, ids: &[String]) -> Result<u64>;

    /// `updated_at` が `before` より前の確認待ち（`pending`）を消す。消した人数を返す
    async fn delete_stale_pending_newsletter_subscribers(&self, before: &str) -> Result<u64>;

    async fn insert_newsletter_issue(&self, issue: &NewsletterIssueRecord) -> Result<()>;

    /// 配信の履歴（新しい順）と総数
    async fn list_newsletter_issues(
        &self,
        limit: Option<u64>,
        offset: u64,
    ) -> Result<(Vec<NewsletterIssueRecord>, u64)>;

    /// このコンテンツの最新の配信（まだ送っていなければ `None`）
    async fn get_latest_newsletter_issue(
        &self,
        content_type: &str,
        content_id: &str,
    ) -> Result<Option<NewsletterIssueRecord>>;
}

/// テーブルの CREATE 文と索引。`text` は長い文字列、`key` は索引を張る文字列（MySQL は `VARCHAR(255)`）、
/// `int` は整数の型。`inline_indexes` なら索引を CREATE TABLE に含める（MySQL）
pub fn build_newsletter_tables(
    text: &str,
    key: &str,
    int: &str,
    inline_indexes: bool,
) -> (Vec<String>, Vec<String>) {
    let indexes = [
        (
            "idx_aegis_newsletter_status",
            NEWSLETTER_SUBSCRIBER_TABLE,
            "status, created_at",
        ),
        (
            "idx_aegis_newsletter_issue_content",
            NEWSLETTER_ISSUE_TABLE,
            "content_type, content_id",
        ),
    ];
    let inline = |table: &str| {
        if !inline_indexes {
            return String::new();
        }
        indexes
            .iter()
            .filter(|(_, t, _)| *t == table)
            .map(|(name, _, columns)| format!(", INDEX {} ({})", name, columns))
            .collect::<String>()
    };
    let tables = vec![
        format!(
            "CREATE TABLE IF NOT EXISTS {table} (\
             id {key} PRIMARY KEY, \
             email {key} NOT NULL UNIQUE, \
             status {key} NOT NULL, \
             token {key} NOT NULL UNIQUE, \
             ip {key}, \
             created_at {key} NOT NULL, \
             confirmed_at {key}, \
             unsubscribed_at {key}, \
             updated_at {key} NOT NULL{inline});",
            table = NEWSLETTER_SUBSCRIBER_TABLE,
            key = key,
            inline = inline(NEWSLETTER_SUBSCRIBER_TABLE),
        ),
        format!(
            "CREATE TABLE IF NOT EXISTS {table} (\
             id {key} PRIMARY KEY, \
             content_type {key} NOT NULL, \
             content_id {key} NOT NULL, \
             title {key} NOT NULL, \
             url {text} NOT NULL, \
             recipients {int} NOT NULL, \
             created_at {key} NOT NULL, \
             created_by {key}{inline});",
            table = NEWSLETTER_ISSUE_TABLE,
            key = key,
            text = text,
            int = int,
            inline = inline(NEWSLETTER_ISSUE_TABLE),
        ),
    ];
    let separate = if inline_indexes {
        Vec::new()
    } else {
        indexes
            .iter()
            .map(|(name, table, columns)| {
                format!(
                    "CREATE INDEX IF NOT EXISTS {} ON {} ({});",
                    name, table, columns
                )
            })
            .collect()
    };
    (tables, separate)
}

/// 購読者の INSERT 文。バインド順は `NEWSLETTER_SUBSCRIBER_COLUMNS` の順
pub fn build_subscriber_insert(placeholder: impl Fn(usize) -> String) -> String {
    let placeholders: Vec<String> = (1..=NEWSLETTER_SUBSCRIBER_COLUMNS.len())
        .map(placeholder)
        .collect();
    format!(
        "INSERT INTO {} ({}) VALUES ({});",
        NEWSLETTER_SUBSCRIBER_TABLE,
        NEWSLETTER_SUBSCRIBER_COLUMNS.join(", "),
        placeholders.join(", ")
    )
}

/// 購読者の UPDATE 文。バインド順は status, token, ip, confirmed_at, unsubscribed_at, updated_at, id
pub fn build_subscriber_update(placeholder: impl Fn(usize) -> String) -> String {
    format!(
        "UPDATE {} SET status = {}, token = {}, ip = {}, confirmed_at = {}, unsubscribed_at = {}, \
         updated_at = {} WHERE id = {};",
        NEWSLETTER_SUBSCRIBER_TABLE,
        placeholder(1),
        placeholder(2),
        placeholder(3),
        placeholder(4),
        placeholder(5),
        placeholder(6),
        placeholder(7),
    )
}

/// 購読者の一覧の SELECT と COUNT、両方に共通のバインド値（status、検索語のパターン）。LIMIT / OFFSET のみ
/// 数値を埋め込む
pub fn build_subscriber_list(
    query: &NewsletterListQuery,
    placeholder: impl Fn(usize) -> String,
) -> (String, String, Vec<String>) {
    let mut conditions = Vec::new();
    let mut binds = Vec::new();
    if let Some(status) = &query.status {
        binds.push(status.clone());
        conditions.push(format!("status = {}", placeholder(binds.len())));
    }
    if let Some(term) = query
        .search
        .as_deref()
        .map(str::trim)
        .filter(|term| !term.is_empty())
    {
        binds.push(format!("%{}%", crate::escape_like(&term.to_lowercase())));
        conditions.push(format!(
            "LOWER(email) LIKE {} ESCAPE '!'",
            placeholder(binds.len())
        ));
    }
    let where_clause = if conditions.is_empty() {
        String::new()
    } else {
        format!(" WHERE {}", conditions.join(" AND "))
    };
    let paging = query
        .limit
        .map(|limit| format!(" LIMIT {} OFFSET {}", limit, query.offset))
        .unwrap_or_default();
    (
        format!(
            "SELECT {} FROM {}{} ORDER BY created_at DESC, id DESC{};",
            NEWSLETTER_SUBSCRIBER_COLUMNS.join(", "),
            NEWSLETTER_SUBSCRIBER_TABLE,
            where_clause,
            paging
        ),
        format!(
            "SELECT COUNT(*) FROM {}{};",
            NEWSLETTER_SUBSCRIBER_TABLE, where_clause
        ),
        binds,
    )
}

/// 配信する購読者を id の順に取る SELECT。`after` があれば最初のバインド値が id
pub fn build_active_subscribers(
    after: bool,
    limit: u64,
    placeholder: impl Fn(usize) -> String,
) -> String {
    let after = if after {
        format!(" AND id > {}", placeholder(1))
    } else {
        String::new()
    };
    format!(
        "SELECT {} FROM {} WHERE status = 'active'{} ORDER BY id ASC LIMIT {};",
        NEWSLETTER_SUBSCRIBER_COLUMNS.join(", "),
        NEWSLETTER_SUBSCRIBER_TABLE,
        after,
        limit
    )
}

/// 購読者の削除。バインド順は id...
pub fn build_subscriber_delete(count: usize, placeholder: impl Fn(usize) -> String) -> String {
    let ids: Vec<String> = (1..=count).map(placeholder).collect();
    format!(
        "DELETE FROM {} WHERE id IN ({});",
        NEWSLETTER_SUBSCRIBER_TABLE,
        ids.join(", ")
    )
}

/// 配信の履歴の INSERT 文。バインド順は `NEWSLETTER_ISSUE_COLUMNS` の順
pub fn build_issue_insert(placeholder: impl Fn(usize) -> String) -> String {
    let placeholders: Vec<String> = (1..=NEWSLETTER_ISSUE_COLUMNS.len())
        .map(placeholder)
        .collect();
    format!(
        "INSERT INTO {} ({}) VALUES ({});",
        NEWSLETTER_ISSUE_TABLE,
        NEWSLETTER_ISSUE_COLUMNS.join(", "),
        placeholders.join(", ")
    )
}

/// sqlx の行 (`SqliteRow` / `PgRow` / `MySqlRow`) を `NewsletterSubscriberRecord` に変換する
macro_rules! newsletter_subscriber_from_sqlx {
    ($row:expr) => {{
        let row = $row;
        let get = |e: sqlx::Error| aegis_core::AegisError::Database(e.to_string());
        $crate::newsletter::NewsletterSubscriberRecord {
            id: row.try_get("id").map_err(get)?,
            email: row.try_get("email").map_err(get)?,
            status: row.try_get("status").map_err(get)?,
            token: row.try_get("token").map_err(get)?,
            ip: row.try_get("ip").map_err(get)?,
            created_at: row.try_get("created_at").map_err(get)?,
            confirmed_at: row.try_get("confirmed_at").map_err(get)?,
            unsubscribed_at: row.try_get("unsubscribed_at").map_err(get)?,
            updated_at: row.try_get("updated_at").map_err(get)?,
        }
    }};
}
pub(crate) use newsletter_subscriber_from_sqlx;

/// sqlx の行を `NewsletterIssueRecord` に変換する
macro_rules! newsletter_issue_from_sqlx {
    ($row:expr) => {{
        let row = $row;
        let get = |e: sqlx::Error| aegis_core::AegisError::Database(e.to_string());
        $crate::newsletter::NewsletterIssueRecord {
            id: row.try_get("id").map_err(get)?,
            content_type: row.try_get("content_type").map_err(get)?,
            content_id: row.try_get("content_id").map_err(get)?,
            title: row.try_get("title").map_err(get)?,
            url: row.try_get("url").map_err(get)?,
            recipients: row.try_get("recipients").map_err(get)?,
            created_at: row.try_get("created_at").map_err(get)?,
            created_by: row.try_get("created_by").map_err(get)?,
        }
    }};
}
pub(crate) use newsletter_issue_from_sqlx;

/// 各 DB アダプタに `NewsletterStorage` を実装するマクロ
macro_rules! impl_newsletter_storage {
    ($adapter:ty, $ph:expr, $label:literal) => {
        #[async_trait::async_trait]
        impl $crate::newsletter::NewsletterStorage for $adapter {
            async fn insert_newsletter_subscriber(
                &self,
                s: &$crate::newsletter::NewsletterSubscriberRecord,
            ) -> aegis_core::Result<()> {
                let db_err =
                    |e: sqlx::Error| aegis_core::AegisError::Database(format!("{}: {}", $label, e));
                sqlx::query(&$crate::newsletter::build_subscriber_insert($ph))
                    .bind(&s.id)
                    .bind(&s.email)
                    .bind(&s.status)
                    .bind(&s.token)
                    .bind(&s.ip)
                    .bind(&s.created_at)
                    .bind(&s.confirmed_at)
                    .bind(&s.unsubscribed_at)
                    .bind(&s.updated_at)
                    .execute(&self.pool)
                    .await
                    .map_err(db_err)?;
                Ok(())
            }

            async fn get_newsletter_subscriber(
                &self,
                id: &str,
            ) -> aegis_core::Result<Option<$crate::newsletter::NewsletterSubscriberRecord>> {
                self.fetch_newsletter_subscriber("id", id).await
            }

            async fn get_newsletter_subscriber_by_email(
                &self,
                email: &str,
            ) -> aegis_core::Result<Option<$crate::newsletter::NewsletterSubscriberRecord>> {
                self.fetch_newsletter_subscriber("email", email).await
            }

            async fn get_newsletter_subscriber_by_token(
                &self,
                token: &str,
            ) -> aegis_core::Result<Option<$crate::newsletter::NewsletterSubscriberRecord>> {
                self.fetch_newsletter_subscriber("token", token).await
            }

            async fn update_newsletter_subscriber(
                &self,
                s: &$crate::newsletter::NewsletterSubscriberRecord,
            ) -> aegis_core::Result<bool> {
                let db_err =
                    |e: sqlx::Error| aegis_core::AegisError::Database(format!("{}: {}", $label, e));
                let res = sqlx::query(&$crate::newsletter::build_subscriber_update($ph))
                    .bind(&s.status)
                    .bind(&s.token)
                    .bind(&s.ip)
                    .bind(&s.confirmed_at)
                    .bind(&s.unsubscribed_at)
                    .bind(&s.updated_at)
                    .bind(&s.id)
                    .execute(&self.pool)
                    .await
                    .map_err(db_err)?;
                Ok(res.rows_affected() > 0)
            }

            async fn list_newsletter_subscribers(
                &self,
                query: &$crate::newsletter::NewsletterListQuery,
            ) -> aegis_core::Result<$crate::newsletter::NewsletterPage> {
                let (select, count, binds) = $crate::newsletter::build_subscriber_list(query, $ph);
                let db_err =
                    |e: sqlx::Error| aegis_core::AegisError::Database(format!("{}: {}", $label, e));
                let mut q = sqlx::query(&select);
                for value in &binds {
                    q = q.bind(value);
                }
                let rows = q.fetch_all(&self.pool).await.map_err(db_err)?;
                let mut subscribers = Vec::with_capacity(rows.len());
                for r in &rows {
                    subscribers.push($crate::newsletter::newsletter_subscriber_from_sqlx!(r));
                }
                let mut q = sqlx::query_scalar::<_, i64>(&count);
                for value in &binds {
                    q = q.bind(value);
                }
                let total = q.fetch_one(&self.pool).await.map_err(db_err)?;
                Ok($crate::newsletter::NewsletterPage {
                    subscribers,
                    total: total.max(0) as u64,
                })
            }

            async fn count_newsletter_subscribers_by_status(
                &self,
            ) -> aegis_core::Result<Vec<(String, u64)>> {
                let sql = format!(
                    "SELECT status, COUNT(*) AS n FROM {} GROUP BY status;",
                    $crate::newsletter::NEWSLETTER_SUBSCRIBER_TABLE
                );
                let db_err =
                    |e: sqlx::Error| aegis_core::AegisError::Database(format!("{}: {}", $label, e));
                let rows = sqlx::query(&sql)
                    .fetch_all(&self.pool)
                    .await
                    .map_err(db_err)?;
                let mut counts = Vec::with_capacity(rows.len());
                for r in &rows {
                    let status: String = r.try_get("status").map_err(db_err)?;
                    let n: i64 = r.try_get("n").map_err(db_err)?;
                    counts.push((status, n.max(0) as u64));
                }
                Ok(counts)
            }

            async fn list_active_newsletter_subscribers(
                &self,
                after: Option<&str>,
                limit: u64,
            ) -> aegis_core::Result<Vec<$crate::newsletter::NewsletterSubscriberRecord>> {
                let sql = $crate::newsletter::build_active_subscribers(after.is_some(), limit, $ph);
                let db_err =
                    |e: sqlx::Error| aegis_core::AegisError::Database(format!("{}: {}", $label, e));
                let mut q = sqlx::query(&sql);
                if let Some(after) = after {
                    q = q.bind(after);
                }
                let rows = q.fetch_all(&self.pool).await.map_err(db_err)?;
                let mut subscribers = Vec::with_capacity(rows.len());
                for r in &rows {
                    subscribers.push($crate::newsletter::newsletter_subscriber_from_sqlx!(r));
                }
                Ok(subscribers)
            }

            async fn delete_newsletter_subscribers(
                &self,
                ids: &[String],
            ) -> aegis_core::Result<u64> {
                if ids.is_empty() {
                    return Ok(0);
                }
                let sql = $crate::newsletter::build_subscriber_delete(ids.len(), $ph);
                let db_err =
                    |e: sqlx::Error| aegis_core::AegisError::Database(format!("{}: {}", $label, e));
                let mut q = sqlx::query(&sql);
                for id in ids {
                    q = q.bind(id);
                }
                let res = q.execute(&self.pool).await.map_err(db_err)?;
                Ok(res.rows_affected())
            }

            async fn delete_stale_pending_newsletter_subscribers(
                &self,
                before: &str,
            ) -> aegis_core::Result<u64> {
                let ph = $ph;
                let sql = format!(
                    "DELETE FROM {} WHERE status = 'pending' AND updated_at < {};",
                    $crate::newsletter::NEWSLETTER_SUBSCRIBER_TABLE,
                    ph(1)
                );
                let db_err =
                    |e: sqlx::Error| aegis_core::AegisError::Database(format!("{}: {}", $label, e));
                let res = sqlx::query(&sql)
                    .bind(before)
                    .execute(&self.pool)
                    .await
                    .map_err(db_err)?;
                Ok(res.rows_affected())
            }

            async fn insert_newsletter_issue(
                &self,
                i: &$crate::newsletter::NewsletterIssueRecord,
            ) -> aegis_core::Result<()> {
                let db_err =
                    |e: sqlx::Error| aegis_core::AegisError::Database(format!("{}: {}", $label, e));
                sqlx::query(&$crate::newsletter::build_issue_insert($ph))
                    .bind(&i.id)
                    .bind(&i.content_type)
                    .bind(&i.content_id)
                    .bind(&i.title)
                    .bind(&i.url)
                    .bind(i.recipients)
                    .bind(&i.created_at)
                    .bind(&i.created_by)
                    .execute(&self.pool)
                    .await
                    .map_err(db_err)?;
                Ok(())
            }

            async fn list_newsletter_issues(
                &self,
                limit: Option<u64>,
                offset: u64,
            ) -> aegis_core::Result<(Vec<$crate::newsletter::NewsletterIssueRecord>, u64)> {
                let paging = limit
                    .map(|limit| format!(" LIMIT {} OFFSET {}", limit, offset))
                    .unwrap_or_default();
                let sql = format!(
                    "SELECT {} FROM {} ORDER BY created_at DESC, id DESC{};",
                    $crate::newsletter::NEWSLETTER_ISSUE_COLUMNS.join(", "),
                    $crate::newsletter::NEWSLETTER_ISSUE_TABLE,
                    paging
                );
                let count = format!(
                    "SELECT COUNT(*) FROM {};",
                    $crate::newsletter::NEWSLETTER_ISSUE_TABLE
                );
                let db_err =
                    |e: sqlx::Error| aegis_core::AegisError::Database(format!("{}: {}", $label, e));
                let rows = sqlx::query(&sql)
                    .fetch_all(&self.pool)
                    .await
                    .map_err(db_err)?;
                let mut issues = Vec::with_capacity(rows.len());
                for r in &rows {
                    issues.push($crate::newsletter::newsletter_issue_from_sqlx!(r));
                }
                let total = sqlx::query_scalar::<_, i64>(&count)
                    .fetch_one(&self.pool)
                    .await
                    .map_err(db_err)?;
                Ok((issues, total.max(0) as u64))
            }

            async fn get_latest_newsletter_issue(
                &self,
                content_type: &str,
                content_id: &str,
            ) -> aegis_core::Result<Option<$crate::newsletter::NewsletterIssueRecord>> {
                let ph = $ph;
                let sql = format!(
                    "SELECT {} FROM {} WHERE content_type = {} AND content_id = {} \
                     ORDER BY created_at DESC, id DESC LIMIT 1;",
                    $crate::newsletter::NEWSLETTER_ISSUE_COLUMNS.join(", "),
                    $crate::newsletter::NEWSLETTER_ISSUE_TABLE,
                    ph(1),
                    ph(2)
                );
                let db_err =
                    |e: sqlx::Error| aegis_core::AegisError::Database(format!("{}: {}", $label, e));
                let row = sqlx::query(&sql)
                    .bind(content_type)
                    .bind(content_id)
                    .fetch_optional(&self.pool)
                    .await
                    .map_err(db_err)?;
                match row {
                    Some(r) => Ok(Some($crate::newsletter::newsletter_issue_from_sqlx!(&r))),
                    None => Ok(None),
                }
            }
        }

        impl $adapter {
            /// 列の値（`id` / `email` / `token`）で購読者を 1 人取る
            async fn fetch_newsletter_subscriber(
                &self,
                column: &str,
                value: &str,
            ) -> aegis_core::Result<Option<$crate::newsletter::NewsletterSubscriberRecord>> {
                let ph = $ph;
                let sql = format!(
                    "SELECT {} FROM {} WHERE {} = {};",
                    $crate::newsletter::NEWSLETTER_SUBSCRIBER_COLUMNS.join(", "),
                    $crate::newsletter::NEWSLETTER_SUBSCRIBER_TABLE,
                    column,
                    ph(1)
                );
                let db_err =
                    |e: sqlx::Error| aegis_core::AegisError::Database(format!("{}: {}", $label, e));
                let row = sqlx::query(&sql)
                    .bind(value)
                    .fetch_optional(&self.pool)
                    .await
                    .map_err(db_err)?;
                match row {
                    Some(r) => Ok(Some($crate::newsletter::newsletter_subscriber_from_sqlx!(
                        &r
                    ))),
                    None => Ok(None),
                }
            }
        }
    };
}
pub(crate) use impl_newsletter_storage;
