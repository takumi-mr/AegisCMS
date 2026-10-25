//! 顧客テーブル（`_aegis_customers`）とストレージ。
//! AegisCMS の管理ユーザー（`_aegis_users`）とは完全に別管理の、EC 顧客アカウント。

use aegis_core::Result;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

/// 顧客テーブル名
pub const CUSTOMER_TABLE: &str = "_aegis_customers";

/// INSERT / SELECT で使う顧客テーブルのカラム（順序固定）
pub const CUSTOMER_COLUMNS: [&str; 10] = [
    "id",
    "email",
    "password_hash",
    "name",
    "phone",
    "status",
    "shipping_address_json",
    "created_at",
    "updated_at",
    "last_login_at",
];

/// 顧客（`_aegis_customers` の 1 行）
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct CustomerRecord {
    pub id: String,
    /// メールアドレス（小文字正規化済み、一意）
    pub email: String,
    pub password_hash: String,
    pub name: String,
    pub phone: Option<String>,
    /// `active` / `suspended`
    pub status: String,
    /// 既定の配送先住所 (`ShippingAddress` の JSON)
    pub shipping_address_json: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub last_login_at: Option<String>,
}

/// 顧客テーブルの CREATE 文とインデックス定義
pub fn build_customer_table(text: &str, key: &str, inline_indexes: bool) -> (String, Vec<String>) {
    let indexes = [
        ("idx_aegis_customers_email", "email"),
        ("idx_aegis_customers_created", "created_at"),
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
         phone {key}, \
         status {key} NOT NULL, \
         shipping_address_json {text}, \
         created_at {key} NOT NULL, \
         updated_at {key} NOT NULL, \
         last_login_at {key}{inline});",
        table = CUSTOMER_TABLE,
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
                    name, CUSTOMER_TABLE, columns
                )
            })
            .collect()
    };
    (table, separate)
}

/// 顧客の INSERT 文
pub fn build_customer_insert(placeholder: impl Fn(usize) -> String) -> String {
    let placeholders: Vec<String> = (1..=CUSTOMER_COLUMNS.len()).map(placeholder).collect();
    format!(
        "INSERT INTO {} ({}) VALUES ({});",
        CUSTOMER_TABLE,
        CUSTOMER_COLUMNS.join(", "),
        placeholders.join(", ")
    )
}

/// sqlx の行を `CustomerRecord` に変換する
macro_rules! customer_row_from_sqlx {
    ($row:expr) => {{
        let row = $row;
        let get = |e: sqlx::Error| aegis_core::AegisError::Database(e.to_string());
        $crate::customers::CustomerRecord {
            id: row.try_get("id").map_err(get)?,
            email: row.try_get("email").map_err(get)?,
            password_hash: row.try_get("password_hash").map_err(get)?,
            name: row.try_get("name").map_err(get)?,
            phone: row.try_get("phone").map_err(get)?,
            status: row.try_get("status").map_err(get)?,
            shipping_address_json: row.try_get("shipping_address_json").map_err(get)?,
            created_at: row.try_get("created_at").map_err(get)?,
            updated_at: row.try_get("updated_at").map_err(get)?,
            last_login_at: row.try_get("last_login_at").map_err(get)?,
        }
    }};
}
pub(crate) use customer_row_from_sqlx;

/// 顧客の保存（`DatabaseAdapter` のスーパートレイト）
#[async_trait]
pub trait CustomerStorage: Send + Sync {
    async fn insert_customer(&self, customer: &CustomerRecord) -> Result<()>;
    async fn get_customer_by_id(&self, id: &str) -> Result<Option<CustomerRecord>>;
    async fn get_customer_by_email(&self, email: &str) -> Result<Option<CustomerRecord>>;
    async fn update_customer(&self, customer: &CustomerRecord) -> Result<()>;
    async fn update_customer_last_login(&self, id: &str, last_login_at: &str) -> Result<()>;
    async fn list_customers(&self, limit: u64, offset: u64) -> Result<(Vec<CustomerRecord>, u64)>;
    async fn set_customer_status(&self, id: &str, status: &str, updated_at: &str) -> Result<()>;
    async fn delete_customer(&self, id: &str) -> Result<bool>;
}

/// `CustomerStorage` の実装マクロ
macro_rules! impl_customer_storage {
    ($adapter:ty, $ph:expr, $label:literal) => {
        #[async_trait::async_trait]
        impl $crate::customers::CustomerStorage for $adapter {
            async fn insert_customer(
                &self,
                c: &$crate::customers::CustomerRecord,
            ) -> aegis_core::Result<()> {
                let db_err =
                    |e: sqlx::Error| aegis_core::AegisError::Database(format!("{}: {}", $label, e));
                sqlx::query(&$crate::customers::build_customer_insert($ph))
                    .bind(&c.id)
                    .bind(&c.email)
                    .bind(&c.password_hash)
                    .bind(&c.name)
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

            async fn get_customer_by_id(
                &self,
                id: &str,
            ) -> aegis_core::Result<Option<$crate::customers::CustomerRecord>> {
                let ph = $ph;
                let sql = format!(
                    "SELECT {} FROM {} WHERE id = {};",
                    $crate::customers::CUSTOMER_COLUMNS.join(", "),
                    $crate::customers::CUSTOMER_TABLE,
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
                    Some(r) => Ok(Some($crate::customers::customer_row_from_sqlx!(&r))),
                    None => Ok(None),
                }
            }

            async fn get_customer_by_email(
                &self,
                email: &str,
            ) -> aegis_core::Result<Option<$crate::customers::CustomerRecord>> {
                let ph = $ph;
                let sql = format!(
                    "SELECT {} FROM {} WHERE email = {};",
                    $crate::customers::CUSTOMER_COLUMNS.join(", "),
                    $crate::customers::CUSTOMER_TABLE,
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
                    Some(r) => Ok(Some($crate::customers::customer_row_from_sqlx!(&r))),
                    None => Ok(None),
                }
            }

            async fn update_customer(
                &self,
                c: &$crate::customers::CustomerRecord,
            ) -> aegis_core::Result<()> {
                let ph = $ph;
                let sql = format!(
                    "UPDATE {} SET email = {}, password_hash = {}, name = {}, phone = {}, status = {}, \
                     shipping_address_json = {}, updated_at = {}, last_login_at = {} WHERE id = {};",
                    $crate::customers::CUSTOMER_TABLE,
                    ph(1), ph(2), ph(3), ph(4), ph(5), ph(6), ph(7), ph(8), ph(9)
                );
                let db_err =
                    |e: sqlx::Error| aegis_core::AegisError::Database(format!("{}: {}", $label, e));
                sqlx::query(&sql)
                    .bind(&c.email)
                    .bind(&c.password_hash)
                    .bind(&c.name)
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

            async fn update_customer_last_login(
                &self,
                id: &str,
                last_login_at: &str,
            ) -> aegis_core::Result<()> {
                let ph = $ph;
                let sql = format!(
                    "UPDATE {} SET last_login_at = {} WHERE id = {};",
                    $crate::customers::CUSTOMER_TABLE,
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

            async fn list_customers(
                &self,
                limit: u64,
                offset: u64,
            ) -> aegis_core::Result<(Vec<$crate::customers::CustomerRecord>, u64)> {
                let ph = $ph;
                let count_sql = format!("SELECT COUNT(*) FROM {};", $crate::customers::CUSTOMER_TABLE);
                let select_sql = format!(
                    "SELECT {} FROM {} ORDER BY created_at DESC LIMIT {} OFFSET {};",
                    $crate::customers::CUSTOMER_COLUMNS.join(", "),
                    $crate::customers::CUSTOMER_TABLE,
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
                    records.push($crate::customers::customer_row_from_sqlx!(r));
                }

                Ok((records, total))
            }

            async fn set_customer_status(
                &self,
                id: &str,
                status: &str,
                updated_at: &str,
            ) -> aegis_core::Result<()> {
                let ph = $ph;
                let sql = format!(
                    "UPDATE {} SET status = {}, updated_at = {} WHERE id = {};",
                    $crate::customers::CUSTOMER_TABLE,
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

            async fn delete_customer(&self, id: &str) -> aegis_core::Result<bool> {
                let ph = $ph;
                let sql = format!(
                    "DELETE FROM {} WHERE id = {};",
                    $crate::customers::CUSTOMER_TABLE,
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
pub(crate) use impl_customer_storage;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_customer_sql() {
        let ph = |i: usize| format!("${}", i);
        let insert = build_customer_insert(ph);
        assert!(insert.starts_with("INSERT INTO _aegis_customers ("));
        assert!(insert.contains("$10)"));

        let (mysql, indexes) = build_customer_table("TEXT", "VARCHAR(255)", true);
        assert!(mysql.contains(", INDEX idx_aegis_customers_email (email)"));
        assert!(indexes.is_empty());

        let (_, indexes) = build_customer_table("TEXT", "TEXT", false);
        assert_eq!(indexes.len(), 2);
    }
}
