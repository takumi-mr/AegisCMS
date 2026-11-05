//! 会員テーブル（`_aegis_members`）とストレージ。
//! AegisCMS の管理ユーザー（`_aegis_users`）とは完全に別管理の、EC 会員アカウント。

use aegis_core::Result;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

/// 会員テーブル名
pub const MEMBER_TABLE: &str = "_aegis_members";

/// INSERT / SELECT で使う会員テーブルのカラム（順序固定）
pub const MEMBER_COLUMNS: [&str; 11] = [
    "id",
    "email",
    "password_hash",
    "name",
    "nickname",
    "phone",
    "status",
    "shipping_address_json",
    "created_at",
    "updated_at",
    "last_login_at",
];

/// 会員（`_aegis_members` の 1 行）
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct MemberRecord {
    pub id: String,
    /// メールアドレス（小文字正規化済み、一意）
    pub email: String,
    pub password_hash: String,
    pub name: String,
    /// 公開名（掲示板・レビューに出す名前）。`name` は配送先にも使う本名なので、別に持つ。
    /// 未設定なら `None`（古いバックアップにも無い）
    #[serde(default)]
    pub nickname: Option<String>,
    pub phone: Option<String>,
    /// `active` / `suspended`
    pub status: String,
    /// 既定の配送先住所 (`ShippingAddress` の JSON)
    pub shipping_address_json: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub last_login_at: Option<String>,
}

/// 会員テーブルの CREATE 文とインデックス定義
pub fn build_member_table(text: &str, key: &str, inline_indexes: bool) -> (String, Vec<String>) {
    let indexes = [
        ("idx_aegis_members_email", "email"),
        ("idx_aegis_members_created", "created_at"),
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
         email {key} NOT NULL UNIQUE, \
         password_hash {text} NOT NULL, \
         name {text} NOT NULL, \
         nickname {key}, \
         phone {key}, \
         status {key} NOT NULL, \
         shipping_address_json {text}, \
         created_at {key} NOT NULL, \
         updated_at {key} NOT NULL, \
         last_login_at {key}{inline});",
        table = MEMBER_TABLE,
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
                    name, MEMBER_TABLE, columns
                )
            })
            .collect()
    };
    (table, separate)
}

/// 会員の INSERT 文
pub fn build_member_insert(placeholder: impl Fn(usize) -> String) -> String {
    let placeholders: Vec<String> = (1..=MEMBER_COLUMNS.len()).map(placeholder).collect();
    format!(
        "INSERT INTO {} ({}) VALUES ({});",
        MEMBER_TABLE,
        MEMBER_COLUMNS.join(", "),
        placeholders.join(", ")
    )
}

/// sqlx の行を `MemberRecord` に変換する
macro_rules! member_row_from_sqlx {
    ($row:expr) => {{
        let row = $row;
        let get = |e: sqlx::Error| aegis_core::AegisError::Database(e.to_string());
        $crate::members::MemberRecord {
            id: row.try_get("id").map_err(get)?,
            email: row.try_get("email").map_err(get)?,
            password_hash: row.try_get("password_hash").map_err(get)?,
            name: row.try_get("name").map_err(get)?,
            nickname: row.try_get("nickname").map_err(get)?,
            phone: row.try_get("phone").map_err(get)?,
            status: row.try_get("status").map_err(get)?,
            shipping_address_json: row.try_get("shipping_address_json").map_err(get)?,
            created_at: row.try_get("created_at").map_err(get)?,
            updated_at: row.try_get("updated_at").map_err(get)?,
            last_login_at: row.try_get("last_login_at").map_err(get)?,
        }
    }};
}
pub(crate) use member_row_from_sqlx;

/// 会員の保存（`DatabaseAdapter` のスーパートレイト）
#[async_trait]
pub trait MemberStorage: Send + Sync {
    async fn insert_member(&self, member: &MemberRecord) -> Result<()>;
    async fn get_member_by_id(&self, id: &str) -> Result<Option<MemberRecord>>;
    async fn get_member_by_email(&self, email: &str) -> Result<Option<MemberRecord>>;
    async fn update_member(&self, member: &MemberRecord) -> Result<()>;
    async fn update_member_last_login(&self, id: &str, last_login_at: &str) -> Result<()>;
    async fn list_members(&self, limit: u64, offset: u64) -> Result<(Vec<MemberRecord>, u64)>;
    async fn set_member_status(&self, id: &str, status: &str, updated_at: &str) -> Result<()>;
    async fn delete_member(&self, id: &str) -> Result<bool>;
}

/// `MemberStorage` の実装マクロ
macro_rules! impl_member_storage {
    ($adapter:ty, $ph:expr, $label:literal) => {
        #[async_trait::async_trait]
        impl $crate::members::MemberStorage for $adapter {
            async fn insert_member(
                &self,
                c: &$crate::members::MemberRecord,
            ) -> aegis_core::Result<()> {
                let db_err =
                    |e: sqlx::Error| aegis_core::AegisError::Database(format!("{}: {}", $label, e));
                sqlx::query(&$crate::members::build_member_insert($ph))
                    .bind(&c.id)
                    .bind(&c.email)
                    .bind(&c.password_hash)
                    .bind(&c.name)
                    .bind(&c.nickname)
                    .bind(&c.phone)
                    .bind(&c.status)
                    .bind(&c.shipping_address_json)
                    .bind(&c.created_at)
                    .bind(&c.updated_at)
                    .bind(&c.last_login_at)
                    .execute(&self.pool)
                    .await
                    .map_err(db_err)?;
                Ok(())
            }

            async fn get_member_by_id(
                &self,
                id: &str,
            ) -> aegis_core::Result<Option<$crate::members::MemberRecord>> {
                let ph = $ph;
                let sql = format!(
                    "SELECT {} FROM {} WHERE id = {};",
                    $crate::members::MEMBER_COLUMNS.join(", "),
                    $crate::members::MEMBER_TABLE,
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
                    Some(r) => Ok(Some($crate::members::member_row_from_sqlx!(&r))),
                    None => Ok(None),
                }
            }

            async fn get_member_by_email(
                &self,
                email: &str,
            ) -> aegis_core::Result<Option<$crate::members::MemberRecord>> {
                let ph = $ph;
                let sql = format!(
                    "SELECT {} FROM {} WHERE email = {};",
                    $crate::members::MEMBER_COLUMNS.join(", "),
                    $crate::members::MEMBER_TABLE,
                    ph(1)
                );
                let db_err =
                    |e: sqlx::Error| aegis_core::AegisError::Database(format!("{}: {}", $label, e));
                let row = sqlx::query(&sql)
                    .bind(email)
                    .fetch_optional(&self.pool)
                    .await
                    .map_err(db_err)?;
                match row {
                    Some(r) => Ok(Some($crate::members::member_row_from_sqlx!(&r))),
                    None => Ok(None),
                }
            }

            async fn update_member(
                &self,
                c: &$crate::members::MemberRecord,
            ) -> aegis_core::Result<()> {
                let ph = $ph;
                let sql = format!(
                    "UPDATE {} SET email = {}, password_hash = {}, name = {}, nickname = {}, phone = {}, \
                     status = {}, shipping_address_json = {}, updated_at = {}, last_login_at = {} \
                     WHERE id = {};",
                    $crate::members::MEMBER_TABLE,
                    ph(1), ph(2), ph(3), ph(4), ph(5), ph(6), ph(7), ph(8), ph(9), ph(10)
                );
                let db_err =
                    |e: sqlx::Error| aegis_core::AegisError::Database(format!("{}: {}", $label, e));
                sqlx::query(&sql)
                    .bind(&c.email)
                    .bind(&c.password_hash)
                    .bind(&c.name)
                    .bind(&c.nickname)
                    .bind(&c.phone)
                    .bind(&c.status)
                    .bind(&c.shipping_address_json)
                    .bind(&c.updated_at)
                    .bind(&c.last_login_at)
                    .bind(&c.id)
                    .execute(&self.pool)
                    .await
                    .map_err(db_err)?;
                Ok(())
            }

            async fn update_member_last_login(
                &self,
                id: &str,
                last_login_at: &str,
            ) -> aegis_core::Result<()> {
                let ph = $ph;
                let sql = format!(
                    "UPDATE {} SET last_login_at = {} WHERE id = {};",
                    $crate::members::MEMBER_TABLE,
                    ph(1), ph(2)
                );
                let db_err =
                    |e: sqlx::Error| aegis_core::AegisError::Database(format!("{}: {}", $label, e));
                sqlx::query(&sql)
                    .bind(last_login_at)
                    .bind(id)
                    .execute(&self.pool)
                    .await
                    .map_err(db_err)?;
                Ok(())
            }

            async fn list_members(
                &self,
                limit: u64,
                offset: u64,
            ) -> aegis_core::Result<(Vec<$crate::members::MemberRecord>, u64)> {
                let ph = $ph;
                let count_sql = format!("SELECT COUNT(*) FROM {};", $crate::members::MEMBER_TABLE);
                let select_sql = format!(
                    "SELECT {} FROM {} ORDER BY created_at DESC LIMIT {} OFFSET {};",
                    $crate::members::MEMBER_COLUMNS.join(", "),
                    $crate::members::MEMBER_TABLE,
                    ph(1), ph(2)
                );
                let db_err =
                    |e: sqlx::Error| aegis_core::AegisError::Database(format!("{}: {}", $label, e));

                let total_i64: i64 = sqlx::query_scalar(&count_sql)
                    .fetch_one(&self.pool)
                    .await
                    .map_err(db_err)?;
                let total = total_i64.max(0) as u64;

                let rows = sqlx::query(&select_sql)
                    .bind(limit as i64)
                    .bind(offset as i64)
                    .fetch_all(&self.pool)
                    .await
                    .map_err(db_err)?;

                let mut records = Vec::with_capacity(rows.len());
                for r in &rows {
                    records.push($crate::members::member_row_from_sqlx!(r));
                }

                Ok((records, total))
            }

            async fn set_member_status(
                &self,
                id: &str,
                status: &str,
                updated_at: &str,
            ) -> aegis_core::Result<()> {
                let ph = $ph;
                let sql = format!(
                    "UPDATE {} SET status = {}, updated_at = {} WHERE id = {};",
                    $crate::members::MEMBER_TABLE,
                    ph(1), ph(2), ph(3)
                );
                let db_err =
                    |e: sqlx::Error| aegis_core::AegisError::Database(format!("{}: {}", $label, e));
                sqlx::query(&sql)
                    .bind(status)
                    .bind(updated_at)
                    .bind(id)
                    .execute(&self.pool)
                    .await
                    .map_err(db_err)?;
                Ok(())
            }

            async fn delete_member(&self, id: &str) -> aegis_core::Result<bool> {
                let ph = $ph;
                let sql = format!(
                    "DELETE FROM {} WHERE id = {};",
                    $crate::members::MEMBER_TABLE,
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
        }
    };
}
pub(crate) use impl_member_storage;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_member_sql() {
        let ph = |i: usize| format!("${}", i);
        let insert = build_member_insert(ph);
        assert!(insert.starts_with("INSERT INTO _aegis_members ("));
        assert!(insert.contains("$11)"));

        let (mysql, indexes) = build_member_table("TEXT", "VARCHAR(255)", true);
        assert!(mysql.contains(", INDEX idx_aegis_members_email (email)"));
        assert!(indexes.is_empty());

        let (_, indexes) = build_member_table("TEXT", "TEXT", false);
        assert_eq!(indexes.len(), 2);
    }
}
