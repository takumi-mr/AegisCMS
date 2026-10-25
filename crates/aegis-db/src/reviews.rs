//! 商品レビューテーブル（`_aegis_reviews`）とストレージ。
//! 購入者による評価（★1〜5）とレビュー本文、運営者の返信、ステマ規制対応（非承認理由）、
//! 審査（モデレーション）およびレーティング集計（AggregateRating）を管理する。

use aegis_core::Result;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

/// レビューテーブル名
pub const REVIEW_TABLE: &str = "_aegis_reviews";

/// INSERT / SELECT で使うレビューテーブルのカラム（順序固定）
pub const REVIEW_COLUMNS: [&str; 17] = [
    "id",
    "product_id",
    "order_id",
    "customer_id",
    "author_name",
    "author_email",
    "rating",
    "title",
    "body",
    "status",
    "verified_purchase",
    "reply",
    "replied_at",
    "rejection_reason",
    "ip",
    "user_agent",
    "created_at",
    // Note: updated_at is at index 17
];

pub const ALL_REVIEW_COLUMNS: [&str; 18] = [
    "id",
    "product_id",
    "order_id",
    "customer_id",
    "author_name",
    "author_email",
    "rating",
    "title",
    "body",
    "status",
    "verified_purchase",
    "reply",
    "replied_at",
    "rejection_reason",
    "ip",
    "user_agent",
    "created_at",
    "updated_at",
];

/// 商品レビュー（`_aegis_reviews` の 1 行）
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct ReviewRecord {
    pub id: String,
    pub product_id: String,
    pub order_id: String,
    pub customer_id: Option<String>,
    pub author_name: String,
    /// 投稿者のメールアドレス（非公開）
    pub author_email: String,
    /// 評価（1..=5）
    pub rating: i64,
    pub title: String,
    /// 本文（プレーンテキスト）
    pub body: String,
    /// `pending` / `approved` / `spam` / `trash`
    pub status: String,
    /// 購入者認証（1: 確認済み, 0: 未確認）
    pub verified_purchase: i64,
    /// 運営者からの公式返信（1段のみ）
    pub reply: Option<String>,
    pub replied_at: Option<String>,
    /// ステマ規制対応: 非承認理由の記録
    pub rejection_reason: Option<String>,
    pub ip: Option<String>,
    pub user_agent: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

/// レビュー一覧の条件
#[derive(Debug, Clone, Default)]
pub struct ReviewListQuery {
    pub product_id: Option<String>,
    pub status: Option<String>,
    pub customer_id: Option<String>,
    pub oldest_first: bool,
    pub limit: Option<u64>,
    pub offset: u64,
}

/// `list_reviews` の結果
#[derive(Debug, Clone, Default)]
pub struct ReviewPage {
    pub reviews: Vec<ReviewRecord>,
    pub total: u64,
}

/// 商品のレビュー集計（JSON-LD AggregateRating や商品ページ向け）
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct ReviewAggregateStats {
    pub average_rating: f64,
    pub review_count: u64,
    pub rating_1: u64,
    pub rating_2: u64,
    pub rating_3: u64,
    pub rating_4: u64,
    pub rating_5: u64,
}

/// レビューテーブルの CREATE 文とインデックス定義
pub fn build_review_table(
    text: &str,
    key: &str,
    bigint: &str,
    inline_indexes: bool,
) -> (String, Vec<String>) {
    let indexes = [
        (
            "idx_aegis_reviews_product",
            "product_id, status, created_at",
        ),
        ("idx_aegis_reviews_order_product", "order_id, product_id"),
        ("idx_aegis_reviews_status", "status, created_at"),
        ("idx_aegis_reviews_customer", "customer_id"),
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
         product_id {key} NOT NULL, \
         order_id {key} NOT NULL, \
         customer_id {key}, \
         author_name {text} NOT NULL, \
         author_email {key} NOT NULL, \
         rating {bigint} NOT NULL, \
         title {text} NOT NULL, \
         body {text} NOT NULL, \
         status {key} NOT NULL, \
         verified_purchase {bigint} NOT NULL DEFAULT 1, \
         reply {text}, \
         replied_at {key}, \
         rejection_reason {text}, \
         ip {key}, \
         user_agent {text}, \
         created_at {key} NOT NULL, \
         updated_at {key} NOT NULL{inline});",
        table = REVIEW_TABLE,
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
                    name, REVIEW_TABLE, columns
                )
            })
            .collect()
    };
    (table, separate)
}

/// レビューの INSERT 文
pub fn build_review_insert(placeholder: impl Fn(usize) -> String) -> String {
    let placeholders: Vec<String> = (1..=ALL_REVIEW_COLUMNS.len()).map(placeholder).collect();
    format!(
        "INSERT INTO {} ({}) VALUES ({});",
        REVIEW_TABLE,
        ALL_REVIEW_COLUMNS.join(", "),
        placeholders.join(", ")
    )
}

/// sqlx の行を `ReviewRecord` に変換する
macro_rules! review_row_from_sqlx {
    ($row:expr) => {{
        let row = $row;
        let get = |e: sqlx::Error| aegis_core::AegisError::Database(e.to_string());
        $crate::reviews::ReviewRecord {
            id: row.try_get("id").map_err(get)?,
            product_id: row.try_get("product_id").map_err(get)?,
            order_id: row.try_get("order_id").map_err(get)?,
            customer_id: row.try_get("customer_id").map_err(get)?,
            author_name: row.try_get("author_name").map_err(get)?,
            author_email: row.try_get("author_email").map_err(get)?,
            rating: row.try_get("rating").map_err(get)?,
            title: row.try_get("title").map_err(get)?,
            body: row.try_get("body").map_err(get)?,
            status: row.try_get("status").map_err(get)?,
            verified_purchase: row.try_get("verified_purchase").map_err(get).unwrap_or(1),
            reply: row.try_get("reply").map_err(get)?,
            replied_at: row.try_get("replied_at").map_err(get)?,
            rejection_reason: row.try_get("rejection_reason").map_err(get)?,
            ip: row.try_get("ip").map_err(get)?,
            user_agent: row.try_get("user_agent").map_err(get)?,
            created_at: row.try_get("created_at").map_err(get)?,
            updated_at: row.try_get("updated_at").map_err(get)?,
        }
    }};
}
pub(crate) use review_row_from_sqlx;

/// レビュー一覧・集計の保存（`DatabaseAdapter` のスーパートレイト）
#[async_trait]
pub trait ReviewStorage: Send + Sync {
    async fn insert_review(&self, review: &ReviewRecord) -> Result<()>;
    async fn get_review(&self, id: &str) -> Result<Option<ReviewRecord>>;
    async fn get_review_by_order_product(
        &self,
        order_id: &str,
        product_id: &str,
    ) -> Result<Option<ReviewRecord>>;
    async fn list_reviews(&self, query: &ReviewListQuery) -> Result<ReviewPage>;
    async fn count_reviews_by_status(&self) -> Result<Vec<(String, u64)>>;
    async fn set_reviews_status(
        &self,
        ids: &[String],
        status: &str,
        reason: Option<&str>,
        updated_at: &str,
    ) -> Result<u64>;
    async fn set_review_reply(
        &self,
        id: &str,
        reply: Option<&str>,
        replied_at: Option<&str>,
        updated_at: &str,
    ) -> Result<()>;
    async fn delete_reviews(&self, ids: &[String]) -> Result<u64>;
    async fn get_product_review_stats(&self, product_id: &str) -> Result<ReviewAggregateStats>;
}

/// レビュー一覧の SQL を組み立てる
pub fn build_review_list(
    query: &ReviewListQuery,
    placeholder: impl Fn(usize) -> String,
) -> (String, String, Vec<String>, Option<(i64, i64)>) {
    let mut where_clauses = Vec::new();
    let mut binds = Vec::new();
    let mut idx = 1;

    if let Some(ref pid) = query.product_id {
        where_clauses.push(format!("product_id = {}", placeholder(idx)));
        binds.push(pid.clone());
        idx += 1;
    }
    if let Some(ref st) = query.status {
        where_clauses.push(format!("status = {}", placeholder(idx)));
        binds.push(st.clone());
        idx += 1;
    }
    if let Some(ref cid) = query.customer_id {
        where_clauses.push(format!("customer_id = {}", placeholder(idx)));
        binds.push(cid.clone());
        idx += 1;
    }

    let where_sql = if where_clauses.is_empty() {
        String::new()
    } else {
        format!("WHERE {}", where_clauses.join(" AND "))
    };

    let count_sql = format!("SELECT COUNT(*) FROM {} {};", REVIEW_TABLE, where_sql);

    let order_by = if query.oldest_first {
        "ORDER BY created_at ASC, id ASC"
    } else {
        "ORDER BY created_at DESC, id DESC"
    };

    let (limit_sql, pagination) = if let Some(limit) = query.limit {
        let l_ph = placeholder(idx);
        let o_ph = placeholder(idx + 1);
        (
            format!("LIMIT {} OFFSET {}", l_ph, o_ph),
            Some((limit as i64, query.offset as i64)),
        )
    } else {
        (String::new(), None)
    };

    let select_sql = format!(
        "SELECT {} FROM {} {} {} {};",
        ALL_REVIEW_COLUMNS.join(", "),
        REVIEW_TABLE,
        where_sql,
        order_by,
        limit_sql
    );

    (select_sql, count_sql, binds, pagination)
}

/// `ReviewStorage` の実装マクロ
macro_rules! impl_review_storage {
    ($adapter:ty, $ph:expr, $label:literal) => {
        #[async_trait::async_trait]
        impl $crate::reviews::ReviewStorage for $adapter {
            async fn insert_review(
                &self,
                r: &$crate::reviews::ReviewRecord,
            ) -> aegis_core::Result<()> {
                let db_err =
                    |e: sqlx::Error| aegis_core::AegisError::Database(format!("{}: {}", $label, e));
                sqlx::query(&$crate::reviews::build_review_insert($ph))
                    .bind(&r.id)
                    .bind(&r.product_id)
                    .bind(&r.order_id)
                    .bind(&r.customer_id)
                    .bind(&r.author_name)
                    .bind(&r.author_email)
                    .bind(r.rating)
                    .bind(&r.title)
                    .bind(&r.body)
                    .bind(&r.status)
                    .bind(r.verified_purchase)
                    .bind(&r.reply)
                    .bind(&r.replied_at)
                    .bind(&r.rejection_reason)
                    .bind(&r.ip)
                    .bind(&r.user_agent)
                    .bind(&r.created_at)
                    .bind(&r.updated_at)
                    .execute(&self.pool)
                    .await
                    .map_err(db_err)?;
                Ok(())
            }

            async fn get_review(
                &self,
                id: &str,
            ) -> aegis_core::Result<Option<$crate::reviews::ReviewRecord>> {
                let ph = $ph;
                let sql = format!(
                    "SELECT {} FROM {} WHERE id = {};",
                    $crate::reviews::ALL_REVIEW_COLUMNS.join(", "),
                    $crate::reviews::REVIEW_TABLE,
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
                    Some(r) => Ok(Some($crate::reviews::review_row_from_sqlx!(&r))),
                    None => Ok(None),
                }
            }

            async fn get_review_by_order_product(
                &self,
                order_id: &str,
                product_id: &str,
            ) -> aegis_core::Result<Option<$crate::reviews::ReviewRecord>> {
                let ph = $ph;
                let sql = format!(
                    "SELECT {} FROM {} WHERE order_id = {} AND product_id = {};",
                    $crate::reviews::ALL_REVIEW_COLUMNS.join(", "),
                    $crate::reviews::REVIEW_TABLE,
                    ph(1), ph(2)
                );
                let db_err =
                    |e: sqlx::Error| aegis_core::AegisError::Database(format!("{}: {}", $label, e));
                let row = sqlx::query(&sql)
                    .bind(order_id)
                    .bind(product_id)
                    .fetch_optional(&self.pool)
                    .await
                    .map_err(db_err)?;
                match row {
                    Some(r) => Ok(Some($crate::reviews::review_row_from_sqlx!(&r))),
                    None => Ok(None),
                }
            }

            async fn list_reviews(
                &self,
                query: &$crate::reviews::ReviewListQuery,
            ) -> aegis_core::Result<$crate::reviews::ReviewPage> {
                let (select_sql, count_sql, binds, pagination) =
                    $crate::reviews::build_review_list(query, $ph);
                let db_err =
                    |e: sqlx::Error| aegis_core::AegisError::Database(format!("{}: {}", $label, e));

                let mut count_q = sqlx::query_scalar::<_, i64>(&count_sql);
                for val in &binds {
                    count_q = count_q.bind(val);
                }
                let total_i64: i64 = count_q.fetch_one(&self.pool).await.map_err(db_err)?;
                let total = total_i64.max(0) as u64;

                let mut select_q = sqlx::query(&select_sql);
                for val in &binds {
                    select_q = select_q.bind(val);
                }
                if let Some((limit, offset)) = pagination {
                    select_q = select_q.bind(limit).bind(offset);
                }

                let rows = select_q.fetch_all(&self.pool).await.map_err(db_err)?;
                let mut reviews = Vec::with_capacity(rows.len());
                for r in &rows {
                    reviews.push($crate::reviews::review_row_from_sqlx!(r));
                }

                Ok($crate::reviews::ReviewPage { reviews, total })
            }

            async fn count_reviews_by_status(&self) -> aegis_core::Result<Vec<(String, u64)>> {
                let sql = format!(
                    "SELECT status, COUNT(*) AS count FROM {} GROUP BY status;",
                    $crate::reviews::REVIEW_TABLE
                );
                let db_err =
                    |e: sqlx::Error| aegis_core::AegisError::Database(format!("{}: {}", $label, e));
                let rows = sqlx::query(&sql).fetch_all(&self.pool).await.map_err(db_err)?;
                let mut counts = Vec::new();
                for r in rows {
                    let st: String = r.try_get("status").map_err(db_err)?;
                    let cnt: i64 = r.try_get("count").map_err(db_err)?;
                    counts.push((st, cnt.max(0) as u64));
                }
                Ok(counts)
            }

            async fn set_reviews_status(
                &self,
                ids: &[String],
                status: &str,
                reason: Option<&str>,
                updated_at: &str,
            ) -> aegis_core::Result<u64> {
                if ids.is_empty() {
                    return Ok(0);
                }
                let ph = $ph;
                let id_phs: Vec<String> = (4..=3 + ids.len()).map(ph).collect();
                let sql = format!(
                    "UPDATE {} SET status = {}, rejection_reason = {}, updated_at = {} WHERE id IN ({});",
                    $crate::reviews::REVIEW_TABLE,
                    ph(1), ph(2), ph(3), id_phs.join(", ")
                );
                let db_err =
                    |e: sqlx::Error| aegis_core::AegisError::Database(format!("{}: {}", $label, e));
                let mut q = sqlx::query(&sql)
                    .bind(status)
                    .bind(reason)
                    .bind(updated_at);
                for id in ids {
                    q = q.bind(id);
                }
                let res = q.execute(&self.pool).await.map_err(db_err)?;
                Ok(res.rows_affected())
            }

            async fn set_review_reply(
                &self,
                id: &str,
                reply: Option<&str>,
                replied_at: Option<&str>,
                updated_at: &str,
            ) -> aegis_core::Result<()> {
                let ph = $ph;
                let sql = format!(
                    "UPDATE {} SET reply = {}, replied_at = {}, updated_at = {} WHERE id = {};",
                    $crate::reviews::REVIEW_TABLE,
                    ph(1), ph(2), ph(3), ph(4)
                );
                let db_err =
                    |e: sqlx::Error| aegis_core::AegisError::Database(format!("{}: {}", $label, e));
                sqlx::query(&sql)
                    .bind(reply)
                    .bind(replied_at)
                    .bind(updated_at)
                    .bind(id)
                    .execute(&self.pool)
                    .await
                    .map_err(db_err)?;
                Ok(())
            }

            async fn delete_reviews(&self, ids: &[String]) -> aegis_core::Result<u64> {
                if ids.is_empty() {
                    return Ok(0);
                }
                let ph = $ph;
                let id_phs: Vec<String> = (1..=ids.len()).map(ph).collect();
                let sql = format!(
                    "DELETE FROM {} WHERE id IN ({});",
                    $crate::reviews::REVIEW_TABLE,
                    id_phs.join(", ")
                );
                let db_err =
                    |e: sqlx::Error| aegis_core::AegisError::Database(format!("{}: {}", $label, e));
                let mut q = sqlx::query(&sql);
                for id in ids {
                    q = q.bind(id);
                }
                let res = q.execute(&self.pool).await.map_err(db_err)?;
                Ok(res.rows_affected())
            }

            async fn get_product_review_stats(
                &self,
                product_id: &str,
            ) -> aegis_core::Result<$crate::reviews::ReviewAggregateStats> {
                let ph = $ph;
                let sql = format!(
                    "SELECT rating, COUNT(*) AS count FROM {} WHERE product_id = {} AND status = 'approved' GROUP BY rating;",
                    $crate::reviews::REVIEW_TABLE,
                    ph(1)
                );
                let db_err =
                    |e: sqlx::Error| aegis_core::AegisError::Database(format!("{}: {}", $label, e));
                let rows = sqlx::query(&sql)
                    .bind(product_id)
                    .fetch_all(&self.pool)
                    .await
                    .map_err(db_err)?;

                let mut total_count: u64 = 0;
                let mut sum_rating: f64 = 0.0;
                let mut r1 = 0;
                let mut r2 = 0;
                let mut r3 = 0;
                let mut r4 = 0;
                let mut r5 = 0;

                for r in rows {
                    let rating: i64 = r.try_get("rating").map_err(db_err)?;
                    let count_i64: i64 = r.try_get("count").map_err(db_err)?;
                    let count = count_i64.max(0) as u64;
                    total_count += count;
                    sum_rating += (rating as f64) * (count as f64);
                    match rating {
                        1 => r1 += count,
                        2 => r2 += count,
                        3 => r3 += count,
                        4 => r4 += count,
                        5 => r5 += count,
                        _ => {}
                    }
                }

                let average_rating = if total_count > 0 {
                    (sum_rating / total_count as f64 * 10.0).round() / 10.0
                } else {
                    0.0
                };

                Ok($crate::reviews::ReviewAggregateStats {
                    average_rating,
                    review_count: total_count,
                    rating_1: r1,
                    rating_2: r2,
                    rating_3: r3,
                    rating_4: r4,
                    rating_5: r5,
                })
            }
        }
    };
}
pub(crate) use impl_review_storage;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_review_sql() {
        let ph = |i: usize| format!("${}", i);
        let insert = build_review_insert(ph);
        assert!(insert.starts_with("INSERT INTO _aegis_reviews ("));
        assert!(insert.contains("$18)"));

        let (mysql, indexes) = build_review_table("TEXT", "VARCHAR(255)", "BIGINT", true);
        assert!(
            mysql.contains(", INDEX idx_aegis_reviews_product (product_id, status, created_at)")
        );
        assert!(indexes.is_empty());

        let (_, indexes) = build_review_table("TEXT", "TEXT", "INTEGER", false);
        assert_eq!(indexes.len(), 4);

        let (select, count, binds, pagination) = build_review_list(
            &ReviewListQuery {
                product_id: Some("p1".to_string()),
                status: Some("approved".to_string()),
                customer_id: None,
                oldest_first: false,
                limit: Some(10),
                offset: 20,
            },
            ph,
        );
        assert_eq!(binds, vec!["p1", "approved"]);
        assert!(select.contains("product_id = $1 AND status = $2"));
        assert!(select.contains("LIMIT $3 OFFSET $4"));
        assert!(count
            .contains("SELECT COUNT(*) FROM _aegis_reviews WHERE product_id = $1 AND status = $2"));
        assert_eq!(pagination, Some((10, 20)));
    }
}
