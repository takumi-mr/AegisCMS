//! 注文テーブル（`_aegis_orders`）と在庫の確保・戻しの SQL。3 つのダイアレクトで共有し、
//! プレースホルダだけを呼び出し側（`?N` / `$N` / `?`）が決める。バインドは常にプレースホルダの番号順

use aegis_core::Result;
use async_trait::async_trait;

/// 注文の保存（`DatabaseAdapter` のスーパートレイト）。実装は `impl_order_storage!` で生成する
#[async_trait]
pub trait OrderStorage: Send + Sync {
    /// Reserve stock for every line and insert the order in one transaction. When a product
    /// lacks stock nothing is stored and the inner result names that product
    async fn create_order(
        &self,
        order: &OrderRecord,
        reservations: &[StockReservation],
    ) -> Result<std::result::Result<(), OutOfStock>>;

    async fn get_order(&self, id: &str) -> Result<Option<OrderRecord>>;

    /// Orders newest first, with the number of orders matching the filter
    async fn list_orders(&self, query: &OrderListQuery) -> Result<OrderPage>;

    /// Store the payment provider's id and the reservation deadline after starting a payment
    async fn set_order_payment(
        &self,
        id: &str,
        payment_ref: &str,
        expires_at: Option<&str>,
    ) -> Result<()>;

    /// Change the status only if it is still `from_status`, returning stock in the same
    /// transaction. Returns false (and changes nothing) when the status was different
    async fn transition_order(&self, change: &OrderTransition) -> Result<bool>;

    /// Pending orders whose reservation deadline is before `now`
    async fn list_expired_orders(&self, now: &str, limit: u64) -> Result<Vec<OrderRecord>>;
}

/// 注文テーブル名（コンテンツテーブル `content_*` と衝突しないシステムテーブル）
pub const ORDER_TABLE: &str = "_aegis_orders";

/// 注文（`_aegis_orders` の 1 行）。金額は通貨の最小単位の整数
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct OrderRecord {
    pub id: String,
    /// 顧客に見せる注文番号（`261022-7KQ2XM`）
    pub number: String,
    pub status: String,
    pub currency: String,
    pub subtotal: i64,
    pub total: i64,
    /// `Vec<OrderLine>` の JSON
    pub lines_json: String,
    /// `Vec<OrderAdjustment>` の JSON
    pub adjustments_json: String,
    pub customer_email: String,
    pub customer_name: String,
    /// `ShippingAddress` の JSON
    pub shipping_json: Option<String>,
    pub coupon_code: Option<String>,
    /// プラグインが付けた値（`OrderContext::metadata`）の JSON
    pub metadata_json: String,
    /// `manual` / `stripe`
    pub payment_provider: String,
    /// 決済サービス側の ID（Stripe Checkout Session ID）
    pub payment_ref: Option<String>,
    /// 注文確認ページの URL に付けるトークンの SHA-256（hex）
    pub access_token_hash: String,
    /// 支払い待ちの在庫確保の期限（RFC 3339 UTC、秒まで。文字列の大小で比較する）
    pub expires_at: Option<String>,
    pub paid_at: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    /// 管理者のメモ（最後の状態変更のもの）
    pub note: Option<String>,
}

/// INSERT / SELECT で使う注文テーブルのカラム（順序固定）
pub const ORDER_COLUMNS: [&str; 21] = [
    "id",
    "number",
    "status",
    "currency",
    "subtotal",
    "total",
    "lines_json",
    "adjustments_json",
    "customer_email",
    "customer_name",
    "shipping_json",
    "coupon_code",
    "metadata_json",
    "payment_provider",
    "payment_ref",
    "access_token_hash",
    "expires_at",
    "paid_at",
    "created_at",
    "updated_at",
    "note",
];

/// 注文の作成時に確保する在庫（商品テーブル `table` の `id` の `stock` を `quantity` 減らす）
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StockReservation {
    pub table: String,
    pub product_id: String,
    pub quantity: i64,
}

/// `create_order` が在庫不足で注文を作らなかった
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutOfStock {
    pub product_id: String,
}

/// 注文の状態の変更（`from_status` のときだけ変える。決済の Webhook の重複・同時の変更に備える）
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OrderTransition {
    pub id: String,
    pub from_status: String,
    pub to_status: String,
    pub updated_at: String,
    /// 支払い済みにするときの日時（それ以外は `None` で今の値のまま）
    pub paid_at: Option<String>,
    /// `None` なら今のメモのまま
    pub note: Option<String>,
    /// 同じトランザクションで戻す在庫（取り消し時）
    pub restock: Vec<StockReservation>,
}

/// 注文一覧の条件
#[derive(Debug, Clone, Default)]
pub struct OrderListQuery {
    pub status: Option<String>,
    pub limit: u64,
    pub offset: u64,
}

#[derive(Debug, Clone)]
pub struct OrderPage {
    pub orders: Vec<OrderRecord>,
    pub total: u64,
}

/// 注文テーブルの CREATE 文と索引。`text` は長い文字列、`key` は索引を張る文字列（MySQL は
/// `VARCHAR(255)`）、`bigint` は金額の型。`inline_indexes` なら索引を CREATE TABLE に含める（MySQL）
pub fn build_order_table(
    text: &str,
    key: &str,
    bigint: &str,
    inline_indexes: bool,
) -> (String, Vec<String>) {
    let indexes = [
        ("idx_aegis_orders_status", "status, created_at"),
        ("idx_aegis_orders_created", "created_at"),
        ("idx_aegis_orders_payment_ref", "payment_ref"),
        ("idx_aegis_orders_expires", "status, expires_at"),
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
         number {key} NOT NULL UNIQUE, \
         status {key} NOT NULL, \
         currency {key} NOT NULL, \
         subtotal {bigint} NOT NULL, \
         total {bigint} NOT NULL, \
         lines_json {text} NOT NULL, \
         adjustments_json {text} NOT NULL, \
         customer_email {key} NOT NULL, \
         customer_name {text} NOT NULL, \
         shipping_json {text}, \
         coupon_code {key}, \
         metadata_json {text} NOT NULL, \
         payment_provider {key} NOT NULL, \
         payment_ref {key}, \
         access_token_hash {key} NOT NULL, \
         expires_at {key}, \
         paid_at {key}, \
         created_at {key} NOT NULL, \
         updated_at {key} NOT NULL, \
         note {text}{inline});",
        table = ORDER_TABLE,
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
                    name, ORDER_TABLE, columns
                )
            })
            .collect()
    };
    (table, separate)
}

/// 注文の INSERT 文。バインド順は `ORDER_COLUMNS` の順
pub fn build_order_insert(placeholder: impl Fn(usize) -> String) -> String {
    let placeholders: Vec<String> = (1..=ORDER_COLUMNS.len()).map(placeholder).collect();
    format!(
        "INSERT INTO {} ({}) VALUES ({});",
        ORDER_TABLE,
        ORDER_COLUMNS.join(", "),
        placeholders.join(", ")
    )
}

/// 注文の SELECT 文（新しい順）。`where_clause` を省略すると全件
pub fn build_order_select(where_clause: Option<&str>, paging: Option<(u64, u64)>) -> String {
    format!(
        "SELECT {} FROM {}{} ORDER BY created_at DESC, id DESC{};",
        ORDER_COLUMNS.join(", "),
        ORDER_TABLE,
        where_clause
            .map(|w| format!(" WHERE {}", w))
            .unwrap_or_default(),
        paging
            .map(|(limit, offset)| format!(" LIMIT {} OFFSET {}", limit, offset))
            .unwrap_or_default()
    )
}

/// 一覧の SELECT と COUNT。`status` があれば 1 つ目のプレースホルダにバインドする
pub fn build_order_list(
    query: &OrderListQuery,
    placeholder: impl Fn(usize) -> String,
) -> (String, String) {
    let where_clause = query
        .status
        .as_ref()
        .map(|_| format!("status = {}", placeholder(1)));
    (
        build_order_select(where_clause.as_deref(), Some((query.limit, query.offset))),
        format!(
            "SELECT COUNT(*) FROM {}{};",
            ORDER_TABLE,
            where_clause
                .map(|w| format!(" WHERE {}", w))
                .unwrap_or_default()
        ),
    )
}

/// 在庫を確保する UPDATE 文。バインド順は quantity, id, quantity。在庫が足りなければ 0 行。
/// `stock` が NULL（在庫を数えない商品）なら NULL のまま成功する
pub fn build_stock_reserve(table: &str, placeholder: impl Fn(usize) -> String) -> String {
    format!(
        "UPDATE {} SET stock = stock - {} WHERE id = {} AND (stock IS NULL OR stock >= {});",
        table,
        placeholder(1),
        placeholder(2),
        placeholder(3)
    )
}

/// 在庫を戻す UPDATE 文。バインド順は quantity, id
pub fn build_stock_release(table: &str, placeholder: impl Fn(usize) -> String) -> String {
    format!(
        "UPDATE {} SET stock = stock + {} WHERE id = {};",
        table,
        placeholder(1),
        placeholder(2)
    )
}

/// 状態の変更。バインド順は to_status, updated_at, paid_at, note, id, from_status
pub fn build_order_transition(placeholder: impl Fn(usize) -> String) -> String {
    format!(
        "UPDATE {} SET status = {}, updated_at = {}, paid_at = COALESCE({}, paid_at), \
         note = COALESCE({}, note) WHERE id = {} AND status = {};",
        ORDER_TABLE,
        placeholder(1),
        placeholder(2),
        placeholder(3),
        placeholder(4),
        placeholder(5),
        placeholder(6)
    )
}

/// 決済の開始後に決済サービスの ID と期限を保存する。バインド順は payment_ref, expires_at,
/// updated_at, id
pub fn build_order_payment_update(placeholder: impl Fn(usize) -> String) -> String {
    format!(
        "UPDATE {} SET payment_ref = {}, expires_at = {}, updated_at = {} WHERE id = {};",
        ORDER_TABLE,
        placeholder(1),
        placeholder(2),
        placeholder(3),
        placeholder(4)
    )
}

/// 期限切れの支払い待ちの注文。バインドは now
pub fn build_expired_orders(limit: u64, placeholder: impl Fn(usize) -> String) -> String {
    build_order_select(
        Some(&format!(
            "status = 'pending_payment' AND expires_at IS NOT NULL AND expires_at < {}",
            placeholder(1)
        )),
        Some((limit, 0)),
    )
}

/// sqlx の行を `OrderRecord` に変換する（全ダイアレクト共通）
macro_rules! order_row_from_sqlx {
    ($row:expr) => {{
        let row = $row;
        let get = |e: sqlx::Error| aegis_core::AegisError::Database(e.to_string());
        $crate::db::OrderRecord {
            id: row.try_get("id").map_err(get)?,
            number: row.try_get("number").map_err(get)?,
            status: row.try_get("status").map_err(get)?,
            currency: row.try_get("currency").map_err(get)?,
            subtotal: row.try_get("subtotal").map_err(get)?,
            total: row.try_get("total").map_err(get)?,
            lines_json: row.try_get("lines_json").map_err(get)?,
            adjustments_json: row.try_get("adjustments_json").map_err(get)?,
            customer_email: row.try_get("customer_email").map_err(get)?,
            customer_name: row.try_get("customer_name").map_err(get)?,
            shipping_json: row.try_get("shipping_json").map_err(get)?,
            coupon_code: row.try_get("coupon_code").map_err(get)?,
            metadata_json: row.try_get("metadata_json").map_err(get)?,
            payment_provider: row.try_get("payment_provider").map_err(get)?,
            payment_ref: row.try_get("payment_ref").map_err(get)?,
            access_token_hash: row.try_get("access_token_hash").map_err(get)?,
            expires_at: row.try_get("expires_at").map_err(get)?,
            paid_at: row.try_get("paid_at").map_err(get)?,
            created_at: row.try_get("created_at").map_err(get)?,
            updated_at: row.try_get("updated_at").map_err(get)?,
            note: row.try_get("note").map_err(get)?,
        }
    }};
}
pub(crate) use order_row_from_sqlx;

/// 注文の INSERT にバインドする（`ORDER_COLUMNS` の順）
macro_rules! bind_order_insert {
    ($query:expr, $order:expr) => {{
        let o = $order;
        $query
            .bind(&o.id)
            .bind(&o.number)
            .bind(&o.status)
            .bind(&o.currency)
            .bind(o.subtotal)
            .bind(o.total)
            .bind(&o.lines_json)
            .bind(&o.adjustments_json)
            .bind(&o.customer_email)
            .bind(&o.customer_name)
            .bind(&o.shipping_json)
            .bind(&o.coupon_code)
            .bind(&o.metadata_json)
            .bind(&o.payment_provider)
            .bind(&o.payment_ref)
            .bind(&o.access_token_hash)
            .bind(&o.expires_at)
            .bind(&o.paid_at)
            .bind(&o.created_at)
            .bind(&o.updated_at)
            .bind(&o.note)
    }};
}
pub(crate) use bind_order_insert;

/// 注文の作成・状態の変更（在庫の確保・戻しを含む）を 1 つのトランザクションで行う
/// `DatabaseAdapter` の実装。`$ph` はプレースホルダ生成関数、`$label` はエラーメッセージ用
macro_rules! impl_order_storage {
    ($adapter:ty, $ph:expr, $label:literal) => {
        #[async_trait::async_trait]
        impl $crate::db::OrderStorage for $adapter {
            async fn create_order(
                &self,
                order: &$crate::db::OrderRecord,
                reservations: &[$crate::db::StockReservation],
            ) -> aegis_core::Result<std::result::Result<(), $crate::db::OutOfStock>> {
                let db_err =
                    |e: sqlx::Error| aegis_core::AegisError::Database(format!("{}: {}", $label, e));
                let mut tx = self.pool.begin().await.map_err(db_err)?;
                for r in reservations {
                    let result =
                        sqlx::query(&$crate::db::orders::build_stock_reserve(&r.table, $ph))
                            .bind(r.quantity)
                            .bind(&r.product_id)
                            .bind(r.quantity)
                            .execute(&mut *tx)
                            .await
                            .map_err(db_err)?;
                    if result.rows_affected() == 0 {
                        tx.rollback().await.map_err(db_err)?;
                        return Ok(Err($crate::db::OutOfStock {
                            product_id: r.product_id.clone(),
                        }));
                    }
                }
                let insert = $crate::db::orders::build_order_insert($ph);
                $crate::db::orders::bind_order_insert!(sqlx::query(&insert), order)
                    .execute(&mut *tx)
                    .await
                    .map_err(db_err)?;
                tx.commit().await.map_err(db_err)?;
                Ok(Ok(()))
            }

            async fn get_order(
                &self,
                id: &str,
            ) -> aegis_core::Result<Option<$crate::db::OrderRecord>> {
                let ph = $ph;
                let sql =
                    $crate::db::orders::build_order_select(Some(&format!("id = {}", ph(1))), None);
                let row = sqlx::query(&sql)
                    .bind(id)
                    .fetch_optional(&self.pool)
                    .await
                    .map_err(|e| aegis_core::AegisError::Database(e.to_string()))?;
                match row {
                    Some(row) => Ok(Some($crate::db::orders::order_row_from_sqlx!(&row))),
                    None => Ok(None),
                }
            }

            async fn list_orders(
                &self,
                query: &$crate::db::OrderListQuery,
            ) -> aegis_core::Result<$crate::db::OrderPage> {
                let (select, count) = $crate::db::orders::build_order_list(query, $ph);
                let mut rows_query = sqlx::query(&select);
                let mut count_query = sqlx::query_scalar::<_, i64>(&count);
                if let Some(status) = &query.status {
                    rows_query = rows_query.bind(status);
                    count_query = count_query.bind(status);
                }
                let db_err = |e: sqlx::Error| aegis_core::AegisError::Database(e.to_string());
                let rows = rows_query.fetch_all(&self.pool).await.map_err(db_err)?;
                let total = count_query.fetch_one(&self.pool).await.map_err(db_err)?;
                let mut orders = Vec::with_capacity(rows.len());
                for row in &rows {
                    orders.push($crate::db::orders::order_row_from_sqlx!(row));
                }
                Ok($crate::db::OrderPage {
                    orders,
                    total: total.max(0) as u64,
                })
            }

            async fn set_order_payment(
                &self,
                id: &str,
                payment_ref: &str,
                expires_at: Option<&str>,
            ) -> aegis_core::Result<()> {
                sqlx::query(&$crate::db::orders::build_order_payment_update($ph))
                    .bind(payment_ref)
                    .bind(expires_at)
                    .bind(chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true))
                    .bind(id)
                    .execute(&self.pool)
                    .await
                    .map_err(|e| aegis_core::AegisError::Database(e.to_string()))?;
                Ok(())
            }

            async fn transition_order(
                &self,
                change: &$crate::db::OrderTransition,
            ) -> aegis_core::Result<bool> {
                let db_err =
                    |e: sqlx::Error| aegis_core::AegisError::Database(format!("{}: {}", $label, e));
                let mut tx = self.pool.begin().await.map_err(db_err)?;
                let result = sqlx::query(&$crate::db::orders::build_order_transition($ph))
                    .bind(&change.to_status)
                    .bind(&change.updated_at)
                    .bind(&change.paid_at)
                    .bind(&change.note)
                    .bind(&change.id)
                    .bind(&change.from_status)
                    .execute(&mut *tx)
                    .await
                    .map_err(db_err)?;
                if result.rows_affected() == 0 {
                    tx.rollback().await.map_err(db_err)?;
                    return Ok(false);
                }
                for r in &change.restock {
                    sqlx::query(&$crate::db::orders::build_stock_release(&r.table, $ph))
                        .bind(r.quantity)
                        .bind(&r.product_id)
                        .execute(&mut *tx)
                        .await
                        .map_err(db_err)?;
                }
                tx.commit().await.map_err(db_err)?;
                Ok(true)
            }

            async fn list_expired_orders(
                &self,
                now: &str,
                limit: u64,
            ) -> aegis_core::Result<Vec<$crate::db::OrderRecord>> {
                let rows = sqlx::query(&$crate::db::orders::build_expired_orders(limit, $ph))
                    .bind(now)
                    .fetch_all(&self.pool)
                    .await
                    .map_err(|e| aegis_core::AegisError::Database(e.to_string()))?;
                let mut orders = Vec::with_capacity(rows.len());
                for row in &rows {
                    orders.push($crate::db::orders::order_row_from_sqlx!(row));
                }
                Ok(orders)
            }
        }
    };
}
pub(crate) use impl_order_storage;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_order_sql() {
        let ph = |i: usize| format!("${}", i);
        assert_eq!(
            build_stock_reserve("content_product", ph),
            "UPDATE content_product SET stock = stock - $1 WHERE id = $2 \
             AND (stock IS NULL OR stock >= $3);"
        );
        assert_eq!(
            build_order_transition(ph),
            "UPDATE _aegis_orders SET status = $1, updated_at = $2, \
             paid_at = COALESCE($3, paid_at), note = COALESCE($4, note) \
             WHERE id = $5 AND status = $6;"
        );
        let (select, count) = build_order_list(
            &OrderListQuery {
                status: Some("paid".to_string()),
                limit: 20,
                offset: 40,
            },
            ph,
        );
        assert!(select.ends_with(
            " FROM _aegis_orders WHERE status = $1 ORDER BY created_at DESC, id DESC \
             LIMIT 20 OFFSET 40;"
        ));
        assert_eq!(
            count,
            "SELECT COUNT(*) FROM _aegis_orders WHERE status = $1;"
        );
        assert!(build_order_insert(ph).contains("$21)"));

        let (mysql, indexes) = build_order_table("TEXT", "VARCHAR(255)", "BIGINT", true);
        assert!(mysql.contains(", INDEX idx_aegis_orders_status (status, created_at)"));
        assert!(indexes.is_empty());
        let (_, indexes) = build_order_table("TEXT", "TEXT", "INTEGER", false);
        assert_eq!(indexes.len(), 4);
    }
}
