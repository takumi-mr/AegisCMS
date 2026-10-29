//! アクセス解析の集計テーブル（`_aegis_analytics`）とストレージ。
//! 1 行は「日 × 集計の切り口（サイト全体・ページ・参照元・端末） × 値」のページビュー数と訪問者数。
//! 個々のアクセス（IP アドレス・User-Agent・訪問者の識別子）は保存しない

use aegis_core::Result;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

/// 集計テーブル名
pub const ANALYTICS_TABLE: &str = "_aegis_analytics";

/// 値の長さの上限（文字数）。これより長いページのパスなどは切り詰めてから保存する
pub const ANALYTICS_VALUE_MAX_CHARS: usize = 200;

/// 集計の 1 行（加算する量、または集計済みの値）
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct AnalyticsCount {
    /// 日（UTC、`YYYY-MM-DD`）
    pub day: String,
    /// 集計の切り口（`site` / `path` / `referrer` / `device`）
    pub dimension: String,
    /// 切り口の値（`site` は空文字列）
    pub value: String,
    pub pageviews: u64,
    pub visitors: u64,
}

/// 期間の中の値ごとの合計
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct AnalyticsTotal {
    pub value: String,
    pub pageviews: u64,
    pub visitors: u64,
}

#[async_trait]
pub trait AnalyticsStorage: Send + Sync {
    /// 集計に加算する（同じ日・切り口・値の行があれば足し、無ければ作る）
    async fn add_analytics_counts(&self, counts: &[AnalyticsCount]) -> Result<()>;

    /// `from`〜`to`（両端を含む `YYYY-MM-DD`）の、切り口 `dimension` の日ごとの行（日の古い順）
    async fn list_analytics_daily(
        &self,
        dimension: &str,
        from: &str,
        to: &str,
    ) -> Result<Vec<AnalyticsCount>>;

    /// `from`〜`to` の、切り口 `dimension` の値ごとの合計（ページビューの多い順に `limit` 件）
    async fn top_analytics_values(
        &self,
        dimension: &str,
        from: &str,
        to: &str,
        limit: u64,
    ) -> Result<Vec<AnalyticsTotal>>;

    /// `day` より前の行を消す（保存期間）。消した行数を返す
    async fn delete_analytics_before(&self, day: &str) -> Result<u64>;
}

/// 集計テーブルの CREATE 文と索引。`key` は索引を張る文字列（MySQL は `VARCHAR(255)`）。
/// `inline_indexes` なら索引を CREATE TABLE に含める（MySQL）
pub fn build_analytics_table(key: &str, inline_indexes: bool) -> (String, Vec<String>) {
    let index = ("idx_aegis_analytics_dimension", "dimension, day");
    let inline = if inline_indexes {
        format!(", INDEX {} ({})", index.0, index.1)
    } else {
        String::new()
    };
    let table = format!(
        "CREATE TABLE IF NOT EXISTS {table} (\
         day VARCHAR(10) NOT NULL, \
         dimension VARCHAR(16) NOT NULL, \
         value {key} NOT NULL, \
         pageviews BIGINT NOT NULL DEFAULT 0, \
         visitors BIGINT NOT NULL DEFAULT 0, \
         PRIMARY KEY (day, dimension, value){inline});",
        table = ANALYTICS_TABLE,
        key = key,
        inline = inline,
    );
    let separate = if inline_indexes {
        Vec::new()
    } else {
        vec![format!(
            "CREATE INDEX IF NOT EXISTS {} ON {} ({});",
            index.0, ANALYTICS_TABLE, index.1
        )]
    };
    (table, separate)
}

/// 加算の INSERT 文。`upsert` は重なったときの句（方言ごと。[`UPSERT_ON_CONFLICT`] / [`UPSERT_MYSQL`]）
pub fn build_analytics_upsert(placeholder: impl Fn(usize) -> String, upsert: &str) -> String {
    format!(
        "INSERT INTO {} (day, dimension, value, pageviews, visitors) VALUES ({}, {}, {}, {}, {}) {};",
        ANALYTICS_TABLE,
        placeholder(1),
        placeholder(2),
        placeholder(3),
        placeholder(4),
        placeholder(5),
        upsert
    )
}

/// SQLite・PostgreSQL の加算
pub const UPSERT_ON_CONFLICT: &str = "ON CONFLICT (day, dimension, value) DO UPDATE SET \
     pageviews = _aegis_analytics.pageviews + excluded.pageviews, \
     visitors = _aegis_analytics.visitors + excluded.visitors";

/// MySQL の加算
pub const UPSERT_MYSQL: &str = "ON DUPLICATE KEY UPDATE \
     pageviews = pageviews + VALUES(pageviews), visitors = visitors + VALUES(visitors)";

/// `$int` は `SUM(...)` を整数に戻す CAST の型（PostgreSQL の `SUM(BIGINT)` は `NUMERIC` になる。MySQL は `SIGNED`）
macro_rules! impl_analytics_storage {
    ($adapter:ty, $ph:expr, $upsert:expr, $int:literal, $label:literal) => {
        #[async_trait::async_trait]
        impl $crate::analytics::AnalyticsStorage for $adapter {
            async fn add_analytics_counts(
                &self,
                counts: &[$crate::analytics::AnalyticsCount],
            ) -> aegis_core::Result<()> {
                if counts.is_empty() {
                    return Ok(());
                }
                let db_err =
                    |e: sqlx::Error| aegis_core::AegisError::Database(format!("{}: {}", $label, e));
                let sql = $crate::analytics::build_analytics_upsert($ph, $upsert);
                let mut tx = self.pool.begin().await.map_err(db_err)?;
                for c in counts {
                    let value: String = c
                        .value
                        .chars()
                        .take($crate::analytics::ANALYTICS_VALUE_MAX_CHARS)
                        .collect();
                    sqlx::query(&sql)
                        .bind(&c.day)
                        .bind(&c.dimension)
                        .bind(value)
                        .bind(i64::try_from(c.pageviews).unwrap_or(i64::MAX))
                        .bind(i64::try_from(c.visitors).unwrap_or(i64::MAX))
                        .execute(&mut *tx)
                        .await
                        .map_err(db_err)?;
                }
                tx.commit().await.map_err(db_err)?;
                Ok(())
            }

            async fn list_analytics_daily(
                &self,
                dimension: &str,
                from: &str,
                to: &str,
            ) -> aegis_core::Result<Vec<$crate::analytics::AnalyticsCount>> {
                let ph = $ph;
                let sql = format!(
                    "SELECT day, CAST(SUM(pageviews) AS {int}) AS pageviews, CAST(SUM(visitors) AS {int}) AS visitors \
                     FROM {} WHERE dimension = {} AND day >= {} AND day <= {} GROUP BY day ORDER BY day;",
                    $crate::analytics::ANALYTICS_TABLE,
                    ph(1),
                    ph(2),
                    ph(3),
                    int = $int
                );
                let db_err =
                    |e: sqlx::Error| aegis_core::AegisError::Database(format!("{}: {}", $label, e));
                let rows = sqlx::query(&sql)
                    .bind(dimension)
                    .bind(from)
                    .bind(to)
                    .fetch_all(&self.pool)
                    .await
                    .map_err(db_err)?;
                let mut out = Vec::with_capacity(rows.len());
                for r in &rows {
                    out.push($crate::analytics::AnalyticsCount {
                        day: r.try_get("day").map_err(db_err)?,
                        dimension: dimension.to_string(),
                        value: String::new(),
                        pageviews: r.try_get::<i64, _>("pageviews").map_err(db_err)?.max(0) as u64,
                        visitors: r.try_get::<i64, _>("visitors").map_err(db_err)?.max(0) as u64,
                    });
                }
                Ok(out)
            }

            async fn top_analytics_values(
                &self,
                dimension: &str,
                from: &str,
                to: &str,
                limit: u64,
            ) -> aegis_core::Result<Vec<$crate::analytics::AnalyticsTotal>> {
                let ph = $ph;
                let sql = format!(
                    "SELECT value, CAST(SUM(pageviews) AS {int}) AS pageviews, CAST(SUM(visitors) AS {int}) AS visitors \
                     FROM {} WHERE dimension = {} AND day >= {} AND day <= {} GROUP BY value \
                     ORDER BY pageviews DESC, value LIMIT {};",
                    $crate::analytics::ANALYTICS_TABLE,
                    ph(1),
                    ph(2),
                    ph(3),
                    limit.min(1000),
                    int = $int
                );
                let db_err =
                    |e: sqlx::Error| aegis_core::AegisError::Database(format!("{}: {}", $label, e));
                let rows = sqlx::query(&sql)
                    .bind(dimension)
                    .bind(from)
                    .bind(to)
                    .fetch_all(&self.pool)
                    .await
                    .map_err(db_err)?;
                let mut out = Vec::with_capacity(rows.len());
                for r in &rows {
                    out.push($crate::analytics::AnalyticsTotal {
                        value: r.try_get("value").map_err(db_err)?,
                        pageviews: r.try_get::<i64, _>("pageviews").map_err(db_err)?.max(0) as u64,
                        visitors: r.try_get::<i64, _>("visitors").map_err(db_err)?.max(0) as u64,
                    });
                }
                Ok(out)
            }

            async fn delete_analytics_before(&self, day: &str) -> aegis_core::Result<u64> {
                let ph = $ph;
                let sql = format!(
                    "DELETE FROM {} WHERE day < {};",
                    $crate::analytics::ANALYTICS_TABLE,
                    ph(1)
                );
                let db_err =
                    |e: sqlx::Error| aegis_core::AegisError::Database(format!("{}: {}", $label, e));
                let result = sqlx::query(&sql)
                    .bind(day)
                    .execute(&self.pool)
                    .await
                    .map_err(db_err)?;
                Ok(result.rows_affected())
            }
        }
    };
}
pub(crate) use impl_analytics_storage;
