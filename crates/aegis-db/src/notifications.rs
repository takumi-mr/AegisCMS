//! 通知の送信待ち（`_aegis_notification_queue`）とストレージ。テンプレートから作った通知を
//! 宛先・チャネルごとに 1 行ずつ保存し、送れたら消す。再起動しても送信待ちが残り、複数台のサーバーでは
//! 条件付きの UPDATE（`claim_notification`）で 1 台だけが送る。3 つのダイアレクトで SQL を共有し、
//! プレースホルダだけを呼び出し側（`?N` / `$N` / `?`）が決める。日時は固定長の RFC 3339（UTC、
//! マイクロ秒）で保存し、文字列の比較で前後を決める

use aegis_core::Result;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

/// 送信待ちのテーブル名
pub const NOTIFICATION_QUEUE_TABLE: &str = "_aegis_notification_queue";

/// 送信待ち（次の試行の日時を待っている）
pub const QUEUE_PENDING: &str = "pending";
/// 送れなかった（再試行しない失敗・試行回数の上限）。Admin Studio から再送・破棄できる
pub const QUEUE_FAILED: &str = "failed";

/// INSERT / SELECT で使うカラム（順序固定）
pub const NOTIFICATION_QUEUE_COLUMNS: [&str; 11] = [
    "id",
    "kind",
    "channel",
    "message",
    "status",
    "attempts",
    "next_attempt_at",
    "locked_until",
    "last_error",
    "created_at",
    "updated_at",
];

/// 送信待ちの通知（`_aegis_notification_queue` の 1 行）。1 つの宛先・チャネルへの配信
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct QueuedNotification {
    pub id: String,
    /// 通知の種類（`order.placed` など）
    pub kind: String,
    /// 配信チャネルの名前（`email` / `log` / `plugin:<名前>`）
    pub channel: String,
    /// テンプレートから作った通知（`aegis_core::NotificationMessage` の JSON）
    pub message: String,
    /// `pending` / `failed`
    pub status: String,
    /// これまでの試行回数
    pub attempts: u32,
    /// 次に送る日時（`pending` のとき）
    pub next_attempt_at: String,
    /// 送っている最中のサーバーが持つ期限。過ぎていればほかのサーバーが送り直せる
    pub locked_until: Option<String>,
    pub last_error: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

/// 試行の後の状態（`release_notification`）
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NotificationRelease {
    pub id: String,
    /// `pending`（次の試行を待つ）/ `failed`
    pub status: String,
    pub attempts: u32,
    pub next_attempt_at: String,
    pub last_error: Option<String>,
    pub updated_at: String,
}

/// 一覧の条件（新しい順）
#[derive(Debug, Clone, Default)]
pub struct NotificationListQuery {
    pub status: Option<String>,
    pub limit: u64,
    pub offset: u64,
}

/// `list_notifications` の結果: 条件に一致する行と、ページングしない場合の総件数
#[derive(Debug, Clone, Default)]
pub struct NotificationPage {
    pub items: Vec<QueuedNotification>,
    pub total: u64,
}

/// 送信待ちの保存（`DatabaseAdapter` のスーパートレイト）。実装は `impl_notification_queue_storage!` で生成する
#[async_trait]
pub trait NotificationQueueStorage: Send + Sync {
    /// Insert notifications in one transaction
    async fn enqueue_notifications(&self, items: &[QueuedNotification]) -> Result<()>;

    /// Pending notifications due by `now` that no server holds (oldest due first)
    async fn due_notifications(&self, now: &str, limit: u64) -> Result<Vec<QueuedNotification>>;

    /// Take a due notification until `locked_until`. False when it is gone, failed, or another
    /// server holds it (only one server wins)
    async fn claim_notification(&self, id: &str, now: &str, locked_until: &str) -> Result<bool>;

    /// Record the outcome of an attempt that did not deliver (retry later or give up), letting go of it
    async fn release_notification(&self, release: &NotificationRelease) -> Result<()>;

    /// Delete a notification (delivered or discarded). False when no row has that id
    async fn delete_notification(&self, id: &str) -> Result<bool>;

    /// Notifications matching `query` (newest first) with the total number of matches
    async fn list_notifications(&self, query: &NotificationListQuery) -> Result<NotificationPage>;

    async fn get_notification(&self, id: &str) -> Result<Option<QueuedNotification>>;

    /// Number of notifications per status
    async fn count_notifications_by_status(&self) -> Result<Vec<(String, u64)>>;

    /// Queue a failed notification again from the first attempt. False unless it was failed
    async fn retry_notification(&self, id: &str, now: &str) -> Result<bool>;

    /// Delete failed notifications last updated before `cutoff`, returning how many were deleted
    async fn delete_failed_notifications_before(&self, cutoff: &str) -> Result<u64>;
}

/// 送信待ちのテーブルの CREATE 文と索引。`text` は長い文字列（本文の JSON。MySQL は `LONGTEXT`）、
/// `key` は索引を張る文字列（MySQL は `VARCHAR(255)`）、`bigint` は整数の型。
/// `inline_indexes` なら索引を CREATE TABLE に含める（MySQL）
pub fn build_notification_queue_table(
    text: &str,
    key: &str,
    bigint: &str,
    inline_indexes: bool,
) -> (String, Vec<String>) {
    let indexes = [
        (
            "idx_aegis_notification_queue_due",
            "status, next_attempt_at",
        ),
        ("idx_aegis_notification_queue_created", "created_at"),
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
         kind {key} NOT NULL, \
         channel {key} NOT NULL, \
         message {text} NOT NULL, \
         status {key} NOT NULL, \
         attempts {bigint} NOT NULL DEFAULT 0, \
         next_attempt_at {key} NOT NULL, \
         locked_until {key}, \
         last_error {text}, \
         created_at {key} NOT NULL, \
         updated_at {key} NOT NULL{inline});",
        table = NOTIFICATION_QUEUE_TABLE,
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
                    name, NOTIFICATION_QUEUE_TABLE, columns
                )
            })
            .collect()
    };
    (table, separate)
}

/// INSERT 文。バインド順は `NOTIFICATION_QUEUE_COLUMNS` の順
pub fn build_notification_insert(placeholder: impl Fn(usize) -> String) -> String {
    let placeholders: Vec<String> = (1..=NOTIFICATION_QUEUE_COLUMNS.len())
        .map(placeholder)
        .collect();
    format!(
        "INSERT INTO {} ({}) VALUES ({});",
        NOTIFICATION_QUEUE_TABLE,
        NOTIFICATION_QUEUE_COLUMNS.join(", "),
        placeholders.join(", ")
    )
}

/// 送る時刻を迎えた送信待ち。バインド順は now, now
pub fn build_due_notifications(limit: u64, placeholder: impl Fn(usize) -> String) -> String {
    format!(
        "SELECT {} FROM {} WHERE status = '{}' AND next_attempt_at <= {} \
         AND (locked_until IS NULL OR locked_until < {}) \
         ORDER BY next_attempt_at ASC, id ASC LIMIT {};",
        NOTIFICATION_QUEUE_COLUMNS.join(", "),
        NOTIFICATION_QUEUE_TABLE,
        QUEUE_PENDING,
        placeholder(1),
        placeholder(2),
        limit
    )
}

/// 送信の権利を取る。バインド順は locked_until, id, now。条件に合わなければ 0 行
pub fn build_notification_claim(placeholder: impl Fn(usize) -> String) -> String {
    format!(
        "UPDATE {} SET locked_until = {} WHERE id = {} AND status = '{}' \
         AND (locked_until IS NULL OR locked_until < {});",
        NOTIFICATION_QUEUE_TABLE,
        placeholder(1),
        placeholder(2),
        QUEUE_PENDING,
        placeholder(3)
    )
}

/// 試行の結果を残して手放す。バインド順は status, attempts, next_attempt_at, last_error, updated_at, id
pub fn build_notification_release(placeholder: impl Fn(usize) -> String) -> String {
    format!(
        "UPDATE {} SET status = {}, attempts = {}, next_attempt_at = {}, last_error = {}, \
         updated_at = {}, locked_until = NULL WHERE id = {};",
        NOTIFICATION_QUEUE_TABLE,
        placeholder(1),
        placeholder(2),
        placeholder(3),
        placeholder(4),
        placeholder(5),
        placeholder(6)
    )
}

/// 失敗した通知を最初の試行から送り直す。バインド順は now, now, id
pub fn build_notification_retry(placeholder: impl Fn(usize) -> String) -> String {
    format!(
        "UPDATE {} SET status = '{}', attempts = 0, next_attempt_at = {}, updated_at = {}, \
         locked_until = NULL WHERE id = {} AND status = '{}';",
        NOTIFICATION_QUEUE_TABLE,
        QUEUE_PENDING,
        placeholder(1),
        placeholder(2),
        placeholder(3),
        QUEUE_FAILED
    )
}

/// 一覧の SELECT と COUNT と、両方に共通のバインド値（status を指定したときのみ）
pub fn build_notification_list(
    query: &NotificationListQuery,
    placeholder: impl Fn(usize) -> String,
) -> (String, String, Vec<String>) {
    let (where_clause, binds) = match &query.status {
        Some(status) => (
            format!(" WHERE status = {}", placeholder(1)),
            vec![status.clone()],
        ),
        None => (String::new(), Vec::new()),
    };
    (
        format!(
            "SELECT {} FROM {}{} ORDER BY created_at DESC, id DESC LIMIT {} OFFSET {};",
            NOTIFICATION_QUEUE_COLUMNS.join(", "),
            NOTIFICATION_QUEUE_TABLE,
            where_clause,
            query.limit,
            query.offset
        ),
        format!(
            "SELECT COUNT(*) FROM {}{};",
            NOTIFICATION_QUEUE_TABLE, where_clause
        ),
        binds,
    )
}

/// sqlx の行 (`SqliteRow` / `PgRow` / `MySqlRow`) を `QueuedNotification` に変換する。
/// `attempts` は全ダイアレクトで 64bit 整数として保存する
macro_rules! notification_row_from_sqlx {
    ($row:expr) => {{
        let row = $row;
        let get = |e: sqlx::Error| aegis_core::AegisError::Database(e.to_string());
        $crate::notifications::QueuedNotification {
            id: row.try_get("id").map_err(get)?,
            kind: row.try_get("kind").map_err(get)?,
            channel: row.try_get("channel").map_err(get)?,
            message: row.try_get("message").map_err(get)?,
            status: row.try_get("status").map_err(get)?,
            attempts: row
                .try_get::<i64, _>("attempts")
                .map_err(get)?
                .clamp(0, i64::from(u32::MAX)) as u32,
            next_attempt_at: row.try_get("next_attempt_at").map_err(get)?,
            locked_until: row.try_get("locked_until").map_err(get)?,
            last_error: row.try_get("last_error").map_err(get)?,
            created_at: row.try_get("created_at").map_err(get)?,
            updated_at: row.try_get("updated_at").map_err(get)?,
        }
    }};
}
pub(crate) use notification_row_from_sqlx;

/// 各 DB アダプタに `NotificationQueueStorage` を実装するマクロ
macro_rules! impl_notification_queue_storage {
    ($adapter:ty, $ph:expr, $label:literal) => {
        #[async_trait::async_trait]
        impl $crate::notifications::NotificationQueueStorage for $adapter {
            async fn enqueue_notifications(
                &self,
                items: &[$crate::notifications::QueuedNotification],
            ) -> aegis_core::Result<()> {
                if items.is_empty() {
                    return Ok(());
                }
                let db_err =
                    |e: sqlx::Error| aegis_core::AegisError::Database(format!("{}: {}", $label, e));
                let sql = $crate::notifications::build_notification_insert($ph);
                let mut tx = self.pool.begin().await.map_err(db_err)?;
                for n in items {
                    sqlx::query(&sql)
                        .bind(&n.id)
                        .bind(&n.kind)
                        .bind(&n.channel)
                        .bind(&n.message)
                        .bind(&n.status)
                        .bind(i64::from(n.attempts))
                        .bind(&n.next_attempt_at)
                        .bind(&n.locked_until)
                        .bind(&n.last_error)
                        .bind(&n.created_at)
                        .bind(&n.updated_at)
                        .execute(&mut *tx)
                        .await
                        .map_err(db_err)?;
                }
                tx.commit().await.map_err(db_err)?;
                Ok(())
            }

            async fn due_notifications(
                &self,
                now: &str,
                limit: u64,
            ) -> aegis_core::Result<Vec<$crate::notifications::QueuedNotification>> {
                let db_err =
                    |e: sqlx::Error| aegis_core::AegisError::Database(format!("{}: {}", $label, e));
                let rows = sqlx::query(&$crate::notifications::build_due_notifications(limit, $ph))
                    .bind(now)
                    .bind(now)
                    .fetch_all(&self.pool)
                    .await
                    .map_err(db_err)?;
                let mut items = Vec::with_capacity(rows.len());
                for r in &rows {
                    items.push($crate::notifications::notification_row_from_sqlx!(r));
                }
                Ok(items)
            }

            async fn claim_notification(
                &self,
                id: &str,
                now: &str,
                locked_until: &str,
            ) -> aegis_core::Result<bool> {
                let db_err =
                    |e: sqlx::Error| aegis_core::AegisError::Database(format!("{}: {}", $label, e));
                let res = sqlx::query(&$crate::notifications::build_notification_claim($ph))
                    .bind(locked_until)
                    .bind(id)
                    .bind(now)
                    .execute(&self.pool)
                    .await
                    .map_err(db_err)?;
                Ok(res.rows_affected() > 0)
            }

            async fn release_notification(
                &self,
                r: &$crate::notifications::NotificationRelease,
            ) -> aegis_core::Result<()> {
                let db_err =
                    |e: sqlx::Error| aegis_core::AegisError::Database(format!("{}: {}", $label, e));
                sqlx::query(&$crate::notifications::build_notification_release($ph))
                    .bind(&r.status)
                    .bind(i64::from(r.attempts))
                    .bind(&r.next_attempt_at)
                    .bind(&r.last_error)
                    .bind(&r.updated_at)
                    .bind(&r.id)
                    .execute(&self.pool)
                    .await
                    .map_err(db_err)?;
                Ok(())
            }

            async fn delete_notification(&self, id: &str) -> aegis_core::Result<bool> {
                let ph = $ph;
                let sql = format!(
                    "DELETE FROM {} WHERE id = {};",
                    $crate::notifications::NOTIFICATION_QUEUE_TABLE,
                    ph(1)
                );
                let db_err =
                    |e: sqlx::Error| aegis_core::AegisError::Database(format!("{}: {}", $label, e));
                let res = sqlx::query(&sql)
                    .bind(id)
                    .execute(&self.pool)
                    .await
                    .map_err(db_err)?;
                Ok(res.rows_affected() > 0)
            }

            async fn list_notifications(
                &self,
                query: &$crate::notifications::NotificationListQuery,
            ) -> aegis_core::Result<$crate::notifications::NotificationPage> {
                let (select, count, binds) =
                    $crate::notifications::build_notification_list(query, $ph);
                let db_err =
                    |e: sqlx::Error| aegis_core::AegisError::Database(format!("{}: {}", $label, e));
                let mut q = sqlx::query(&select);
                for value in &binds {
                    q = q.bind(value);
                }
                let rows = q.fetch_all(&self.pool).await.map_err(db_err)?;
                let mut items = Vec::with_capacity(rows.len());
                for r in &rows {
                    items.push($crate::notifications::notification_row_from_sqlx!(r));
                }
                let mut q = sqlx::query_scalar::<_, i64>(&count);
                for value in &binds {
                    q = q.bind(value);
                }
                let total = q.fetch_one(&self.pool).await.map_err(db_err)?;
                Ok($crate::notifications::NotificationPage {
                    items,
                    total: total.max(0) as u64,
                })
            }

            async fn get_notification(
                &self,
                id: &str,
            ) -> aegis_core::Result<Option<$crate::notifications::QueuedNotification>> {
                let ph = $ph;
                let sql = format!(
                    "SELECT {} FROM {} WHERE id = {};",
                    $crate::notifications::NOTIFICATION_QUEUE_COLUMNS.join(", "),
                    $crate::notifications::NOTIFICATION_QUEUE_TABLE,
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
                    Some(r) => Ok(Some($crate::notifications::notification_row_from_sqlx!(&r))),
                    None => Ok(None),
                }
            }

            async fn count_notifications_by_status(
                &self,
            ) -> aegis_core::Result<Vec<(String, u64)>> {
                let sql = format!(
                    "SELECT status, COUNT(*) AS n FROM {} GROUP BY status;",
                    $crate::notifications::NOTIFICATION_QUEUE_TABLE
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

            async fn retry_notification(&self, id: &str, now: &str) -> aegis_core::Result<bool> {
                let db_err =
                    |e: sqlx::Error| aegis_core::AegisError::Database(format!("{}: {}", $label, e));
                let res = sqlx::query(&$crate::notifications::build_notification_retry($ph))
                    .bind(now)
                    .bind(now)
                    .bind(id)
                    .execute(&self.pool)
                    .await
                    .map_err(db_err)?;
                Ok(res.rows_affected() > 0)
            }

            async fn delete_failed_notifications_before(
                &self,
                cutoff: &str,
            ) -> aegis_core::Result<u64> {
                let ph = $ph;
                let sql = format!(
                    "DELETE FROM {} WHERE status = '{}' AND updated_at < {};",
                    $crate::notifications::NOTIFICATION_QUEUE_TABLE,
                    $crate::notifications::QUEUE_FAILED,
                    ph(1)
                );
                let db_err =
                    |e: sqlx::Error| aegis_core::AegisError::Database(format!("{}: {}", $label, e));
                let res = sqlx::query(&sql)
                    .bind(cutoff)
                    .execute(&self.pool)
                    .await
                    .map_err(db_err)?;
                Ok(res.rows_affected())
            }
        }
    };
}
pub(crate) use impl_notification_queue_storage;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_notification_queue_sql() {
        assert_eq!(
            build_due_notifications(20, |i| format!("${}", i)),
            format!(
                "SELECT {} FROM _aegis_notification_queue WHERE status = 'pending' \
                 AND next_attempt_at <= $1 AND (locked_until IS NULL OR locked_until < $2) \
                 ORDER BY next_attempt_at ASC, id ASC LIMIT 20;",
                NOTIFICATION_QUEUE_COLUMNS.join(", ")
            )
        );
        assert_eq!(
            build_notification_claim(|_| "?".to_string()),
            "UPDATE _aegis_notification_queue SET locked_until = ? WHERE id = ? \
             AND status = 'pending' AND (locked_until IS NULL OR locked_until < ?);"
        );
        assert_eq!(
            build_notification_retry(|i| format!("?{}", i)),
            "UPDATE _aegis_notification_queue SET status = 'pending', attempts = 0, \
             next_attempt_at = ?1, updated_at = ?2, locked_until = NULL WHERE id = ?3 \
             AND status = 'failed';"
        );
        let (select, count, binds) = build_notification_list(
            &NotificationListQuery {
                status: Some("failed".to_string()),
                limit: 50,
                offset: 100,
            },
            |i| format!("${}", i),
        );
        assert!(select.ends_with(
            " WHERE status = $1 ORDER BY created_at DESC, id DESC LIMIT 50 OFFSET 100;"
        ));
        assert_eq!(
            count,
            "SELECT COUNT(*) FROM _aegis_notification_queue WHERE status = $1;"
        );
        assert_eq!(binds, vec!["failed"]);
    }

    #[test]
    fn test_notification_queue_table() {
        let (table, indexes) = build_notification_queue_table("TEXT", "TEXT", "INTEGER", false);
        assert!(table.starts_with("CREATE TABLE IF NOT EXISTS _aegis_notification_queue ("));
        assert_eq!(indexes.len(), 2);
        let (table, indexes) =
            build_notification_queue_table("LONGTEXT", "VARCHAR(255)", "BIGINT", true);
        assert!(table.contains("message LONGTEXT NOT NULL"));
        assert!(
            table.contains(", INDEX idx_aegis_notification_queue_due (status, next_attempt_at)")
        );
        assert!(indexes.is_empty());
    }
}
