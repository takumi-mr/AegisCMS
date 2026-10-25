//! コメントテーブル（`_aegis_comments`）とストレージ。コンテンツ（記事・固定ページ・商品など）に
//! 付く読者のコメントと、Admin Studio からの返信を保存する。3 つのダイアレクトで SQL を共有し、
//! プレースホルダだけを呼び出し側（`?N` / `$N` / `?`）が決める

use aegis_core::Result;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

/// コメントのテーブル名
pub const COMMENT_TABLE: &str = "_aegis_comments";

/// INSERT / SELECT で使うコメントテーブルのカラム（順序固定）
pub const COMMENT_COLUMNS: [&str; 15] = [
    "id",
    "content_type",
    "content_id",
    "parent_id",
    "author_name",
    "author_email",
    "author_user_id",
    "body",
    "status",
    "ip",
    "user_agent",
    "notify_replies",
    "notify_token",
    "created_at",
    "updated_at",
];

/// コメント（`_aegis_comments` の 1 行）
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct CommentRecord {
    pub id: String,
    pub content_type: String,
    /// コメントが付いたコンテンツの ID
    pub content_id: String,
    /// 返信先のコメント（トップレベルなら `None`）
    pub parent_id: Option<String>,
    pub author_name: String,
    /// 投稿者のメールアドレス（公開しない。小文字に揃えて保存する）
    pub author_email: String,
    /// Admin Studio のユーザーが書いた返信ならそのユーザー名
    pub author_user_id: Option<String>,
    /// 本文（プレーンテキスト。表示するときにエスケープする）
    pub body: String,
    /// `pending` / `approved` / `spam` / `trash`
    pub status: String,
    pub ip: Option<String>,
    pub user_agent: Option<String>,
    /// 返信が付いたらメールで知らせるか（投稿者が選ぶ）
    pub notify_replies: bool,
    /// 返信の通知を止めるリンクのトークン（`notify_replies` のときだけ）
    pub notify_token: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

/// コメント一覧の条件
#[derive(Debug, Clone, Default)]
pub struct CommentListQuery {
    pub content_type: Option<String>,
    pub content_id: Option<String>,
    pub status: Option<String>,
    /// 古い順（公開ページのスレッド表示）。既定は新しい順（Admin Studio）
    pub oldest_first: bool,
    /// 取得する最大件数。`None` なら全件（`offset` は無視）
    pub limit: Option<u64>,
    pub offset: u64,
}

/// `list_comments` の結果: 条件に一致する行と、ページングしない場合の総件数
#[derive(Debug, Clone, Default)]
pub struct CommentPage {
    pub comments: Vec<CommentRecord>,
    pub total: u64,
}

/// コメントの保存（`DatabaseAdapter` のスーパートレイト）。実装は `impl_comment_storage!` で生成する
#[async_trait]
pub trait CommentStorage: Send + Sync {
    async fn insert_comment(&self, comment: &CommentRecord) -> Result<()>;

    async fn get_comment(&self, id: &str) -> Result<Option<CommentRecord>>;

    /// Comments matching `query` with the total number of matches
    async fn list_comments(&self, query: &CommentListQuery) -> Result<CommentPage>;

    /// Number of comments per status (`pending` → 3, ...). Statuses without comments are left out
    async fn count_comments_by_status(&self) -> Result<Vec<(String, u64)>>;

    /// Change the status of the given comments, returning how many rows changed
    async fn set_comments_status(
        &self,
        ids: &[String],
        status: &str,
        updated_at: &str,
    ) -> Result<u64>;

    /// Permanently delete the given comments, returning how many rows were deleted
    async fn delete_comments(&self, ids: &[String]) -> Result<u64>;

    /// Delete every comment on a content record (when the record is deleted)
    async fn delete_content_comments(&self, content_type: &str, content_id: &str) -> Result<u64>;

    /// Whether the same author (email and name) already has an approved comment
    async fn has_approved_comment(&self, author_email: &str, author_name: &str) -> Result<bool>;

    /// Replace the text of a comment. Returns false when no comment has that id
    async fn update_comment_body(&self, id: &str, body: &str, updated_at: &str) -> Result<bool>;

    /// Whether a comment with exactly this text is already on the content from the same author
    /// (email) or the same address (`ip`). Trashed comments are ignored
    async fn has_duplicate_comment(
        &self,
        content_type: &str,
        content_id: &str,
        body: &str,
        author_email: &str,
        ip: Option<&str>,
    ) -> Result<bool>;

    /// Stop (or resume) reply notifications for a comment. Returns false when no comment has that id
    async fn set_comment_notify_replies(&self, id: &str, notify: bool) -> Result<bool>;
}

/// コメントテーブルの CREATE 文と索引。`text` は長い文字列、`key` は索引を張る文字列（MySQL は
/// `VARCHAR(255)`）、`bigint` は 0 / 1 の値の型。`inline_indexes` なら索引を CREATE TABLE に含める（MySQL）
pub fn build_comment_table(
    text: &str,
    key: &str,
    bigint: &str,
    inline_indexes: bool,
) -> (String, Vec<String>) {
    // MySQL（utf8mb4）の索引は 3072 バイトまで: VARCHAR(255) の列は 3 つまで
    let indexes = [
        (
            "idx_aegis_comments_content",
            "content_id, status, created_at",
        ),
        ("idx_aegis_comments_status", "status, created_at"),
        ("idx_aegis_comments_author", "author_email, status"),
        ("idx_aegis_comments_parent", "parent_id"),
    ];
    let inline = if inline_indexes {
        indexes
            .iter()
            .map(|(name, columns)| format!(", INDEX {} ({})", name, columns))
            .collect::<String>()
    } else {
        String::new()
    };
    let table = format!(
        "CREATE TABLE IF NOT EXISTS {table} (\
         id {key} PRIMARY KEY, \
         content_type {key} NOT NULL, \
         content_id {key} NOT NULL, \
         parent_id {key}, \
         author_name {text} NOT NULL, \
         author_email {key} NOT NULL, \
         author_user_id {key}, \
         body {text} NOT NULL, \
         status {key} NOT NULL, \
         ip {key}, \
         user_agent {text}, \
         notify_replies {bigint} NOT NULL DEFAULT 0, \
         notify_token {key}, \
         created_at {key} NOT NULL, \
         updated_at {key} NOT NULL{inline});",
        table = COMMENT_TABLE,
        key = key,
        text = text,
        bigint = bigint,
        inline = inline,
    );
    let separate = if inline_indexes {
        Vec::new()
    } else {
        indexes
            .iter()
            .map(|(name, columns)| {
                format!(
                    "CREATE INDEX IF NOT EXISTS {} ON {} ({});",
                    name, COMMENT_TABLE, columns
                )
            })
            .collect()
    };
    (table, separate)
}

/// コメントの INSERT 文。バインド順は `COMMENT_COLUMNS` の順
pub fn build_comment_insert(placeholder: impl Fn(usize) -> String) -> String {
    let placeholders: Vec<String> = (1..=COMMENT_COLUMNS.len()).map(placeholder).collect();
    format!(
        "INSERT INTO {} ({}) VALUES ({});",
        COMMENT_TABLE,
        COMMENT_COLUMNS.join(", "),
        placeholders.join(", ")
    )
}

/// 一覧の SELECT と COUNT、両方に共通のバインド値（content_type, content_id, status のうち指定したもの）。
/// LIMIT / OFFSET のみ数値を埋め込む
pub fn build_comment_list(
    query: &CommentListQuery,
    placeholder: impl Fn(usize) -> String,
) -> (String, String, Vec<String>) {
    let mut conditions = Vec::new();
    let mut binds = Vec::new();
    for (column, value) in [
        ("content_type", &query.content_type),
        ("content_id", &query.content_id),
        ("status", &query.status),
    ] {
        if let Some(value) = value {
            binds.push(value.clone());
            conditions.push(format!("{} = {}", column, placeholder(binds.len())));
        }
    }
    let where_clause = if conditions.is_empty() {
        String::new()
    } else {
        format!(" WHERE {}", conditions.join(" AND "))
    };
    let order = if query.oldest_first {
        "created_at ASC, id ASC"
    } else {
        "created_at DESC, id DESC"
    };
    let paging = query
        .limit
        .map(|limit| format!(" LIMIT {} OFFSET {}", limit, query.offset))
        .unwrap_or_default();
    (
        format!(
            "SELECT {} FROM {}{} ORDER BY {}{};",
            COMMENT_COLUMNS.join(", "),
            COMMENT_TABLE,
            where_clause,
            order,
            paging
        ),
        format!("SELECT COUNT(*) FROM {}{};", COMMENT_TABLE, where_clause),
        binds,
    )
}

/// 状態の変更（`count` 件の id を IN でまとめて）。バインド順は status, updated_at, id...
pub fn build_comment_status_update(count: usize, placeholder: impl Fn(usize) -> String) -> String {
    let ids: Vec<String> = (3..3 + count).map(&placeholder).collect();
    format!(
        "UPDATE {} SET status = {}, updated_at = {} WHERE id IN ({});",
        COMMENT_TABLE,
        placeholder(1),
        placeholder(2),
        ids.join(", ")
    )
}

/// 削除（`count` 件の id を IN でまとめて）
pub fn build_comment_delete(count: usize, placeholder: impl Fn(usize) -> String) -> String {
    let ids: Vec<String> = (1..=count).map(placeholder).collect();
    format!(
        "DELETE FROM {} WHERE id IN ({});",
        COMMENT_TABLE,
        ids.join(", ")
    )
}

/// 同じ本文の重複を探す SELECT。バインド順は content_type, content_id, body, author_email（, ip）
pub fn build_comment_duplicate_check(
    with_ip: bool,
    placeholder: impl Fn(usize) -> String,
) -> String {
    let who = if with_ip {
        format!(
            "(author_email = {} OR ip = {})",
            placeholder(4),
            placeholder(5)
        )
    } else {
        format!("author_email = {}", placeholder(4))
    };
    format!(
        "SELECT COUNT(*) FROM {} WHERE content_type = {} AND content_id = {} AND body = {} AND status <> 'trash' AND {};",
        COMMENT_TABLE,
        placeholder(1),
        placeholder(2),
        placeholder(3),
        who
    )
}

/// sqlx の行 (`SqliteRow` / `PgRow` / `MySqlRow`) を `CommentRecord` に変換する。
/// `notify_replies` は全ダイアレクトで 64bit 整数 (0 / 1) として保存する
macro_rules! comment_row_from_sqlx {
    ($row:expr) => {{
        let row = $row;
        let get = |e: sqlx::Error| aegis_core::AegisError::Database(e.to_string());
        $crate::comments::CommentRecord {
            id: row.try_get("id").map_err(get)?,
            content_type: row.try_get("content_type").map_err(get)?,
            content_id: row.try_get("content_id").map_err(get)?,
            parent_id: row.try_get("parent_id").map_err(get)?,
            author_name: row.try_get("author_name").map_err(get)?,
            author_email: row.try_get("author_email").map_err(get)?,
            author_user_id: row.try_get("author_user_id").map_err(get)?,
            body: row.try_get("body").map_err(get)?,
            status: row.try_get("status").map_err(get)?,
            ip: row.try_get("ip").map_err(get)?,
            user_agent: row.try_get("user_agent").map_err(get)?,
            notify_replies: row.try_get::<i64, _>("notify_replies").map_err(get)? != 0,
            notify_token: row.try_get("notify_token").map_err(get)?,
            created_at: row.try_get("created_at").map_err(get)?,
            updated_at: row.try_get("updated_at").map_err(get)?,
        }
    }};
}
pub(crate) use comment_row_from_sqlx;

/// 各 DB アダプタに `CommentStorage` を実装するマクロ
macro_rules! impl_comment_storage {
    ($adapter:ty, $ph:expr, $label:literal) => {
        #[async_trait::async_trait]
        impl $crate::comments::CommentStorage for $adapter {
            async fn insert_comment(
                &self,
                c: &$crate::comments::CommentRecord,
            ) -> aegis_core::Result<()> {
                let db_err =
                    |e: sqlx::Error| aegis_core::AegisError::Database(format!("{}: {}", $label, e));
                sqlx::query(&$crate::comments::build_comment_insert($ph))
                    .bind(&c.id)
                    .bind(&c.content_type)
                    .bind(&c.content_id)
                    .bind(&c.parent_id)
                    .bind(&c.author_name)
                    .bind(&c.author_email)
                    .bind(&c.author_user_id)
                    .bind(&c.body)
                    .bind(&c.status)
                    .bind(&c.ip)
                    .bind(&c.user_agent)
                    .bind(i64::from(c.notify_replies))
                    .bind(&c.notify_token)
                    .bind(&c.created_at)
                    .bind(&c.updated_at)
                    .execute(&self.pool)
                    .await
                    .map_err(db_err)?;
                Ok(())
            }

            async fn get_comment(
                &self,
                id: &str,
            ) -> aegis_core::Result<Option<$crate::comments::CommentRecord>> {
                let ph = $ph;
                let sql = format!(
                    "SELECT {} FROM {} WHERE id = {};",
                    $crate::comments::COMMENT_COLUMNS.join(", "),
                    $crate::comments::COMMENT_TABLE,
                    ph(1)
                );
                let db_err =
                    |e: sqlx::Error| aegis_core::AegisError::Database(format!("{}: {}", $label, e));
                let row = sqlx::query(&sql)
                    .bind(id)
                    .fetch_optional(&self.pool)
                    .await
                    .map_err(db_err)?;
                match row {
                    Some(r) => Ok(Some($crate::comments::comment_row_from_sqlx!(&r))),
                    None => Ok(None),
                }
            }

            async fn list_comments(
                &self,
                query: &$crate::comments::CommentListQuery,
            ) -> aegis_core::Result<$crate::comments::CommentPage> {
                let (select, count, binds) = $crate::comments::build_comment_list(query, $ph);
                let db_err =
                    |e: sqlx::Error| aegis_core::AegisError::Database(format!("{}: {}", $label, e));
                let mut q = sqlx::query(&select);
                for value in &binds {
                    q = q.bind(value);
                }
                let rows = q.fetch_all(&self.pool).await.map_err(db_err)?;
                let mut comments = Vec::with_capacity(rows.len());
                for r in &rows {
                    comments.push($crate::comments::comment_row_from_sqlx!(r));
                }
                let mut q = sqlx::query_scalar::<_, i64>(&count);
                for value in &binds {
                    q = q.bind(value);
                }
                let total = q.fetch_one(&self.pool).await.map_err(db_err)?;
                Ok($crate::comments::CommentPage {
                    comments,
                    total: total.max(0) as u64,
                })
            }

            async fn count_comments_by_status(&self) -> aegis_core::Result<Vec<(String, u64)>> {
                let sql = format!(
                    "SELECT status, COUNT(*) AS n FROM {} GROUP BY status;",
                    $crate::comments::COMMENT_TABLE
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

            async fn set_comments_status(
                &self,
                ids: &[String],
                status: &str,
                updated_at: &str,
            ) -> aegis_core::Result<u64> {
                if ids.is_empty() {
                    return Ok(0);
                }
                let sql = $crate::comments::build_comment_status_update(ids.len(), $ph);
                let db_err =
                    |e: sqlx::Error| aegis_core::AegisError::Database(format!("{}: {}", $label, e));
                let mut q = sqlx::query(&sql).bind(status).bind(updated_at);
                for id in ids {
                    q = q.bind(id);
                }
                let res = q.execute(&self.pool).await.map_err(db_err)?;
                Ok(res.rows_affected())
            }

            async fn delete_comments(&self, ids: &[String]) -> aegis_core::Result<u64> {
                if ids.is_empty() {
                    return Ok(0);
                }
                let sql = $crate::comments::build_comment_delete(ids.len(), $ph);
                let db_err =
                    |e: sqlx::Error| aegis_core::AegisError::Database(format!("{}: {}", $label, e));
                let mut q = sqlx::query(&sql);
                for id in ids {
                    q = q.bind(id);
                }
                let res = q.execute(&self.pool).await.map_err(db_err)?;
                Ok(res.rows_affected())
            }

            async fn delete_content_comments(
                &self,
                content_type: &str,
                content_id: &str,
            ) -> aegis_core::Result<u64> {
                let ph = $ph;
                let sql = format!(
                    "DELETE FROM {} WHERE content_type = {} AND content_id = {};",
                    $crate::comments::COMMENT_TABLE,
                    ph(1),
                    ph(2)
                );
                let db_err =
                    |e: sqlx::Error| aegis_core::AegisError::Database(format!("{}: {}", $label, e));
                let res = sqlx::query(&sql)
                    .bind(content_type)
                    .bind(content_id)
                    .execute(&self.pool)
                    .await
                    .map_err(db_err)?;
                Ok(res.rows_affected())
            }

            async fn has_approved_comment(
                &self,
                author_email: &str,
                author_name: &str,
            ) -> aegis_core::Result<bool> {
                let ph = $ph;
                let sql = format!(
                    "SELECT COUNT(*) FROM {} WHERE author_email = {} AND author_name = {} \
                     AND status = 'approved';",
                    $crate::comments::COMMENT_TABLE,
                    ph(1),
                    ph(2)
                );
                let db_err =
                    |e: sqlx::Error| aegis_core::AegisError::Database(format!("{}: {}", $label, e));
                let n: i64 = sqlx::query_scalar(&sql)
                    .bind(author_email)
                    .bind(author_name)
                    .fetch_one(&self.pool)
                    .await
                    .map_err(db_err)?;
                Ok(n > 0)
            }

            async fn update_comment_body(
                &self,
                id: &str,
                body: &str,
                updated_at: &str,
            ) -> aegis_core::Result<bool> {
                let ph = $ph;
                let sql = format!(
                    "UPDATE {} SET body = {}, updated_at = {} WHERE id = {};",
                    $crate::comments::COMMENT_TABLE,
                    ph(1),
                    ph(2),
                    ph(3)
                );
                let db_err =
                    |e: sqlx::Error| aegis_core::AegisError::Database(format!("{}: {}", $label, e));
                let res = sqlx::query(&sql)
                    .bind(body)
                    .bind(updated_at)
                    .bind(id)
                    .execute(&self.pool)
                    .await
                    .map_err(db_err)?;
                Ok(res.rows_affected() > 0)
            }

            async fn has_duplicate_comment(
                &self,
                content_type: &str,
                content_id: &str,
                body: &str,
                author_email: &str,
                ip: Option<&str>,
            ) -> aegis_core::Result<bool> {
                let sql = $crate::comments::build_comment_duplicate_check(ip.is_some(), $ph);
                let db_err =
                    |e: sqlx::Error| aegis_core::AegisError::Database(format!("{}: {}", $label, e));
                let mut q = sqlx::query_scalar::<_, i64>(&sql)
                    .bind(content_type)
                    .bind(content_id)
                    .bind(body)
                    .bind(author_email);
                if let Some(ip) = ip {
                    q = q.bind(ip);
                }
                let n = q.fetch_one(&self.pool).await.map_err(db_err)?;
                Ok(n > 0)
            }

            async fn set_comment_notify_replies(
                &self,
                id: &str,
                notify: bool,
            ) -> aegis_core::Result<bool> {
                let ph = $ph;
                let sql = format!(
                    "UPDATE {} SET notify_replies = {} WHERE id = {};",
                    $crate::comments::COMMENT_TABLE,
                    ph(1),
                    ph(2)
                );
                let db_err =
                    |e: sqlx::Error| aegis_core::AegisError::Database(format!("{}: {}", $label, e));
                let res = sqlx::query(&sql)
                    .bind(i64::from(notify))
                    .bind(id)
                    .execute(&self.pool)
                    .await
                    .map_err(db_err)?;
                Ok(res.rows_affected() > 0)
            }
        }
    };
}
pub(crate) use impl_comment_storage;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_comment_list_sql() {
        let query = CommentListQuery {
            content_type: Some("article".to_string()),
            content_id: Some("a1".to_string()),
            status: Some("approved".to_string()),
            oldest_first: true,
            limit: None,
            offset: 0,
        };
        let (select, count, binds) = build_comment_list(&query, |i| format!("${}", i));
        assert_eq!(
            select,
            format!(
                "SELECT {} FROM _aegis_comments WHERE content_type = $1 AND content_id = $2 \
                 AND status = $3 ORDER BY created_at ASC, id ASC;",
                COMMENT_COLUMNS.join(", ")
            )
        );
        assert_eq!(
            count,
            "SELECT COUNT(*) FROM _aegis_comments WHERE content_type = $1 AND content_id = $2 \
             AND status = $3;"
        );
        assert_eq!(binds, vec!["article", "a1", "approved"]);

        let query = CommentListQuery {
            status: Some("pending".to_string()),
            limit: Some(20),
            offset: 40,
            ..Default::default()
        };
        let (select, _, binds) = build_comment_list(&query, |_| "?".to_string());
        assert!(select
            .ends_with(" WHERE status = ? ORDER BY created_at DESC, id DESC LIMIT 20 OFFSET 40;"));
        assert_eq!(binds, vec!["pending"]);
    }

    #[test]
    fn test_comment_bulk_sql() {
        assert_eq!(
            build_comment_status_update(2, |i| format!("${}", i)),
            "UPDATE _aegis_comments SET status = $1, updated_at = $2 WHERE id IN ($3, $4);"
        );
        assert_eq!(
            build_comment_delete(3, |i| format!("?{}", i)),
            "DELETE FROM _aegis_comments WHERE id IN (?1, ?2, ?3);"
        );
    }

    #[test]
    fn test_comment_duplicate_sql() {
        assert_eq!(
            build_comment_duplicate_check(true, |i| format!("${}", i)),
            "SELECT COUNT(*) FROM _aegis_comments WHERE content_type = $1 AND content_id = $2 AND body = $3 AND status <> 'trash' AND (author_email = $4 OR ip = $5);"
        );
        assert!(build_comment_duplicate_check(false, |_| "?".to_string())
            .ends_with("AND status <> 'trash' AND author_email = ?;"));
    }

    #[test]
    fn test_comment_table_indexes() {
        let (table, indexes) = build_comment_table("TEXT", "TEXT", "INTEGER", false);
        assert!(table.starts_with("CREATE TABLE IF NOT EXISTS _aegis_comments ("));
        assert_eq!(indexes.len(), 4);
        let (table, indexes) = build_comment_table("TEXT", "VARCHAR(255)", "BIGINT", true);
        assert!(table.contains(", INDEX idx_aegis_comments_status (status, created_at)"));
        assert!(indexes.is_empty());
    }
}
