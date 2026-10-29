//! 問い合わせテーブル（`_aegis_contact_messages`）とストレージ。公開ページの問い合わせフォームから
//! 届いたメッセージを保存し、Admin Studio の受信箱で読む。3 つのダイアレクトで SQL を共有し、
//! プレースホルダだけを呼び出し側（`?N` / `$N` / `?`）が決める

use aegis_core::Result;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

/// 問い合わせのテーブル名
pub const CONTACT_TABLE: &str = "_aegis_contact_messages";

/// INSERT / SELECT で使うカラム（順序固定）
pub const CONTACT_COLUMNS: [&str; 11] = [
    "id",
    "form",
    "name",
    "email",
    "subject",
    "fields_json",
    "status",
    "ip",
    "user_agent",
    "created_at",
    "updated_at",
];

/// 問い合わせ（`_aegis_contact_messages` の 1 行）
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct ContactMessageRecord {
    pub id: String,
    /// フォームの名前（`[[contact.forms]].name`）
    pub form: String,
    /// 送信者の名前（フォームに `name` の項目が無ければ空）
    pub name: String,
    /// 送信者のメールアドレス（小文字。フォームにメールアドレスの項目が無ければ空）
    pub email: String,
    /// 件名（フォームに `subject` の項目が無ければ空）
    pub subject: String,
    /// 入力されたすべての項目（`[{"name", "label", "value"}]` の JSON。フォームの項目の順）
    pub fields_json: String,
    /// `new` / `read` / `done` / `spam`
    pub status: String,
    pub ip: Option<String>,
    pub user_agent: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

/// 一覧の条件
#[derive(Debug, Clone, Default)]
pub struct ContactListQuery {
    pub status: Option<String>,
    pub form: Option<String>,
    /// 取得する最大件数。`None` なら全件（`offset` は無視）
    pub limit: Option<u64>,
    pub offset: u64,
}

/// `list_contact_messages` の結果: 条件に一致する行（新しい順）と、ページングしない場合の総件数
#[derive(Debug, Clone, Default)]
pub struct ContactPage {
    pub messages: Vec<ContactMessageRecord>,
    pub total: u64,
}

/// 問い合わせの保存（`DatabaseAdapter` のスーパートレイト）。実装は `impl_contact_storage!` で生成する
#[async_trait]
pub trait ContactStorage: Send + Sync {
    async fn insert_contact_message(&self, message: &ContactMessageRecord) -> Result<()>;

    async fn get_contact_message(&self, id: &str) -> Result<Option<ContactMessageRecord>>;

    /// Messages matching `query`, newest first, with the total number of matches
    async fn list_contact_messages(&self, query: &ContactListQuery) -> Result<ContactPage>;

    /// Number of messages per status. Statuses without messages are left out
    async fn count_contact_messages_by_status(&self) -> Result<Vec<(String, u64)>>;

    /// Change the status of the given messages, returning how many rows changed
    async fn set_contact_messages_status(
        &self,
        ids: &[String],
        status: &str,
        updated_at: &str,
    ) -> Result<u64>;

    /// Permanently delete the given messages, returning how many rows were deleted
    async fn delete_contact_messages(&self, ids: &[String]) -> Result<u64>;

    /// Delete messages received before `created_at` (the retention period), returning how many
    async fn delete_contact_messages_before(&self, created_at: &str) -> Result<u64>;
}

/// テーブルの CREATE 文と索引。`text` は長い文字列、`key` は索引を張る文字列（MySQL は `VARCHAR(255)`）。
/// `inline_indexes` なら索引を CREATE TABLE に含める（MySQL）
pub fn build_contact_table(text: &str, key: &str, inline_indexes: bool) -> (String, Vec<String>) {
    let indexes = [
        ("idx_aegis_contact_status", "status, created_at"),
        ("idx_aegis_contact_created", "created_at"),
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
         form {key} NOT NULL, \
         name {text} NOT NULL, \
         email {key} NOT NULL, \
         subject {text} NOT NULL, \
         fields_json {text} NOT NULL, \
         status {key} NOT NULL, \
         ip {key}, \
         user_agent {text}, \
         created_at {key} NOT NULL, \
         updated_at {key} NOT NULL{inline});",
        table = CONTACT_TABLE,
        key = key,
        text = text,
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
                    name, CONTACT_TABLE, columns
                )
            })
            .collect()
    };
    (table, separate)
}

/// INSERT 文。バインド順は `CONTACT_COLUMNS` の順
pub fn build_contact_insert(placeholder: impl Fn(usize) -> String) -> String {
    let placeholders: Vec<String> = (1..=CONTACT_COLUMNS.len()).map(placeholder).collect();
    format!(
        "INSERT INTO {} ({}) VALUES ({});",
        CONTACT_TABLE,
        CONTACT_COLUMNS.join(", "),
        placeholders.join(", ")
    )
}

/// 一覧の SELECT と COUNT、両方に共通のバインド値（status, form のうち指定したもの）。
/// LIMIT / OFFSET のみ数値を埋め込む
pub fn build_contact_list(
    query: &ContactListQuery,
    placeholder: impl Fn(usize) -> String,
) -> (String, String, Vec<String>) {
    let mut conditions = Vec::new();
    let mut binds = Vec::new();
    for (column, value) in [("status", &query.status), ("form", &query.form)] {
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
    let paging = query
        .limit
        .map(|limit| format!(" LIMIT {} OFFSET {}", limit, query.offset))
        .unwrap_or_default();
    (
        format!(
            "SELECT {} FROM {}{} ORDER BY created_at DESC, id DESC{};",
            CONTACT_COLUMNS.join(", "),
            CONTACT_TABLE,
            where_clause,
            paging
        ),
        format!("SELECT COUNT(*) FROM {}{};", CONTACT_TABLE, where_clause),
        binds,
    )
}

/// `count` 件の id の IN 句（`first` 番目のプレースホルダから）
fn id_list(count: usize, first: usize, placeholder: impl Fn(usize) -> String) -> String {
    (first..first + count)
        .map(placeholder)
        .collect::<Vec<_>>()
        .join(", ")
}

/// 状態の変更。バインド順は status, updated_at, id...
pub fn build_contact_status_update(count: usize, placeholder: impl Fn(usize) -> String) -> String {
    format!(
        "UPDATE {} SET status = {}, updated_at = {} WHERE id IN ({});",
        CONTACT_TABLE,
        placeholder(1),
        placeholder(2),
        id_list(count, 3, &placeholder)
    )
}

/// 削除。バインド順は id...
pub fn build_contact_delete(count: usize, placeholder: impl Fn(usize) -> String) -> String {
    format!(
        "DELETE FROM {} WHERE id IN ({});",
        CONTACT_TABLE,
        id_list(count, 1, placeholder)
    )
}

/// sqlx の行 (`SqliteRow` / `PgRow` / `MySqlRow`) を `ContactMessageRecord` に変換する
macro_rules! contact_row_from_sqlx {
    ($row:expr) => {{
        let row = $row;
        let get = |e: sqlx::Error| aegis_core::AegisError::Database(e.to_string());
        $crate::contact::ContactMessageRecord {
            id: row.try_get("id").map_err(get)?,
            form: row.try_get("form").map_err(get)?,
            name: row.try_get("name").map_err(get)?,
            email: row.try_get("email").map_err(get)?,
            subject: row.try_get("subject").map_err(get)?,
            fields_json: row.try_get("fields_json").map_err(get)?,
            status: row.try_get("status").map_err(get)?,
            ip: row.try_get("ip").map_err(get)?,
            user_agent: row.try_get("user_agent").map_err(get)?,
            created_at: row.try_get("created_at").map_err(get)?,
            updated_at: row.try_get("updated_at").map_err(get)?,
        }
    }};
}
pub(crate) use contact_row_from_sqlx;

/// 各 DB アダプタに `ContactStorage` を実装するマクロ
macro_rules! impl_contact_storage {
    ($adapter:ty, $ph:expr, $label:literal) => {
        #[async_trait::async_trait]
        impl $crate::contact::ContactStorage for $adapter {
            async fn insert_contact_message(
                &self,
                m: &$crate::contact::ContactMessageRecord,
            ) -> aegis_core::Result<()> {
                let db_err =
                    |e: sqlx::Error| aegis_core::AegisError::Database(format!("{}: {}", $label, e));
                sqlx::query(&$crate::contact::build_contact_insert($ph))
                    .bind(&m.id)
                    .bind(&m.form)
                    .bind(&m.name)
                    .bind(&m.email)
                    .bind(&m.subject)
                    .bind(&m.fields_json)
                    .bind(&m.status)
                    .bind(&m.ip)
                    .bind(&m.user_agent)
                    .bind(&m.created_at)
                    .bind(&m.updated_at)
                    .execute(&self.pool)
                    .await
                    .map_err(db_err)?;
                Ok(())
            }

            async fn get_contact_message(
                &self,
                id: &str,
            ) -> aegis_core::Result<Option<$crate::contact::ContactMessageRecord>> {
                let ph = $ph;
                let sql = format!(
                    "SELECT {} FROM {} WHERE id = {};",
                    $crate::contact::CONTACT_COLUMNS.join(", "),
                    $crate::contact::CONTACT_TABLE,
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
                    Some(r) => Ok(Some($crate::contact::contact_row_from_sqlx!(&r))),
                    None => Ok(None),
                }
            }

            async fn list_contact_messages(
                &self,
                query: &$crate::contact::ContactListQuery,
            ) -> aegis_core::Result<$crate::contact::ContactPage> {
                let (select, count, binds) = $crate::contact::build_contact_list(query, $ph);
                let db_err =
                    |e: sqlx::Error| aegis_core::AegisError::Database(format!("{}: {}", $label, e));
                let mut q = sqlx::query(&select);
                for value in &binds {
                    q = q.bind(value);
                }
                let rows = q.fetch_all(&self.pool).await.map_err(db_err)?;
                let mut messages = Vec::with_capacity(rows.len());
                for r in &rows {
                    messages.push($crate::contact::contact_row_from_sqlx!(r));
                }
                let mut q = sqlx::query_scalar::<_, i64>(&count);
                for value in &binds {
                    q = q.bind(value);
                }
                let total = q.fetch_one(&self.pool).await.map_err(db_err)?;
                Ok($crate::contact::ContactPage {
                    messages,
                    total: total.max(0) as u64,
                })
            }

            async fn count_contact_messages_by_status(
                &self,
            ) -> aegis_core::Result<Vec<(String, u64)>> {
                let sql = format!(
                    "SELECT status, COUNT(*) AS n FROM {} GROUP BY status;",
                    $crate::contact::CONTACT_TABLE
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

            async fn set_contact_messages_status(
                &self,
                ids: &[String],
                status: &str,
                updated_at: &str,
            ) -> aegis_core::Result<u64> {
                if ids.is_empty() {
                    return Ok(0);
                }
                let sql = $crate::contact::build_contact_status_update(ids.len(), $ph);
                let db_err =
                    |e: sqlx::Error| aegis_core::AegisError::Database(format!("{}: {}", $label, e));
                let mut q = sqlx::query(&sql).bind(status).bind(updated_at);
                for id in ids {
                    q = q.bind(id);
                }
                let res = q.execute(&self.pool).await.map_err(db_err)?;
                Ok(res.rows_affected())
            }

            async fn delete_contact_messages(&self, ids: &[String]) -> aegis_core::Result<u64> {
                if ids.is_empty() {
                    return Ok(0);
                }
                let sql = $crate::contact::build_contact_delete(ids.len(), $ph);
                let db_err =
                    |e: sqlx::Error| aegis_core::AegisError::Database(format!("{}: {}", $label, e));
                let mut q = sqlx::query(&sql);
                for id in ids {
                    q = q.bind(id);
                }
                let res = q.execute(&self.pool).await.map_err(db_err)?;
                Ok(res.rows_affected())
            }

            async fn delete_contact_messages_before(
                &self,
                created_at: &str,
            ) -> aegis_core::Result<u64> {
                let ph = $ph;
                let sql = format!(
                    "DELETE FROM {} WHERE created_at < {};",
                    $crate::contact::CONTACT_TABLE,
                    ph(1)
                );
                let db_err =
                    |e: sqlx::Error| aegis_core::AegisError::Database(format!("{}: {}", $label, e));
                let res = sqlx::query(&sql)
                    .bind(created_at)
                    .execute(&self.pool)
                    .await
                    .map_err(db_err)?;
                Ok(res.rows_affected())
            }
        }
    };
}
pub(crate) use impl_contact_storage;
