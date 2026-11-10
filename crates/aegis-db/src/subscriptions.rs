//! 会員のサブスク（`_aegis_subscriptions`。docs/members.md 16.3）。
//! 契約の状態は Stripe の通知（Webhook）が決める。ここは、その写しを持つ。

use aegis_core::Result;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

/// サブスクのテーブル名
pub const SUBSCRIPTION_TABLE: &str = "_aegis_subscriptions";

/// 契約中
pub const SUBSCRIPTION_ACTIVE: &str = "active";
/// 支払いに失敗している（期間の終わりまでは契約中として扱う）
pub const SUBSCRIPTION_PAST_DUE: &str = "past_due";
/// 解約された（権利はない）
pub const SUBSCRIPTION_CANCELLED: &str = "cancelled";

/// INSERT / SELECT で使うカラム（順序固定）
pub const SUBSCRIPTION_COLUMNS: [&str; 11] = [
    "id",
    "member_id",
    "plan_id",
    "level",
    "status",
    "stripe_customer_id",
    "stripe_subscription_id",
    "current_period_end",
    "cancel_at_period_end",
    "created_at",
    "updated_at",
];

/// サブスク（`_aegis_subscriptions` の 1 行）
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct SubscriptionRecord {
    pub id: String,
    /// 契約した会員。退会すると `None`（記録として残る。権利は外れる）
    pub member_id: Option<String>,
    pub plan_id: String,
    /// 契約した時点のプランのレベル（プランを設定から消しても、契約中は変わらない）
    pub level: String,
    /// `active` / `past_due` / `cancelled`
    pub status: String,
    pub stripe_customer_id: String,
    pub stripe_subscription_id: String,
    /// いまの期間の終わり（RFC 3339）
    pub current_period_end: String,
    /// 期間の終わりで解約する予約がある
    pub cancel_at_period_end: bool,
    pub created_at: String,
    pub updated_at: String,
}

/// サブスクの INSERT 文
pub fn build_subscription_insert(placeholder: impl Fn(usize) -> String) -> String {
    let placeholders: Vec<String> = (1..=SUBSCRIPTION_COLUMNS.len()).map(placeholder).collect();
    format!(
        "INSERT INTO {} ({}) VALUES ({});",
        SUBSCRIPTION_TABLE,
        SUBSCRIPTION_COLUMNS.join(", "),
        placeholders.join(", ")
    )
}

/// sqlx の行を `SubscriptionRecord` に変換する（`cancel_at_period_end` は 0 / 1 の整数で持つ）
macro_rules! subscription_row_from_sqlx {
    ($row:expr) => {{
        let row = $row;
        let get = |e: sqlx::Error| aegis_core::AegisError::Database(e.to_string());
        let cancel_at_period_end: i64 = row.try_get("cancel_at_period_end").map_err(get)?;
        $crate::subscriptions::SubscriptionRecord {
            id: row.try_get("id").map_err(get)?,
            member_id: row.try_get("member_id").map_err(get)?,
            plan_id: row.try_get("plan_id").map_err(get)?,
            level: row.try_get("level").map_err(get)?,
            status: row.try_get("status").map_err(get)?,
            stripe_customer_id: row.try_get("stripe_customer_id").map_err(get)?,
            stripe_subscription_id: row.try_get("stripe_subscription_id").map_err(get)?,
            current_period_end: row.try_get("current_period_end").map_err(get)?,
            cancel_at_period_end: cancel_at_period_end != 0,
            created_at: row.try_get("created_at").map_err(get)?,
            updated_at: row.try_get("updated_at").map_err(get)?,
        }
    }};
}
pub(crate) use subscription_row_from_sqlx;

/// サブスクの保存（`DatabaseAdapter` のスーパートレイト）
#[async_trait]
pub trait SubscriptionStorage: Send + Sync {
    async fn insert_subscription(&self, subscription: &SubscriptionRecord) -> Result<()>;
    async fn get_subscription(&self, id: &str) -> Result<Option<SubscriptionRecord>>;
    async fn get_subscription_by_stripe_id(
        &self,
        stripe_subscription_id: &str,
    ) -> Result<Option<SubscriptionRecord>>;
    /// 状態・期間・解約の予約・レベルを更新する（Stripe の通知の写し）
    async fn update_subscription(&self, subscription: &SubscriptionRecord) -> Result<()>;
    /// 会員のサブスク（作成日の新しい順。解約済みを含む）
    async fn list_member_subscriptions(&self, member_id: &str) -> Result<Vec<SubscriptionRecord>>;
    /// 全サブスク（バックアップ用。作成日の古い順）
    async fn list_subscriptions(
        &self,
        limit: u64,
        offset: u64,
    ) -> Result<(Vec<SubscriptionRecord>, u64)>;
    /// 退会する会員のサブスクから、会員との紐づきを外す。外した件数
    async fn detach_member_subscriptions(&self, member_id: &str) -> Result<u64>;
    /// 行を消す（バックアップの復元で上書きするときだけ）
    async fn delete_subscription(&self, id: &str) -> Result<bool>;
}

/// `SubscriptionStorage` の実装マクロ
macro_rules! impl_subscription_storage {
    ($adapter:ty, $ph:expr, $label:literal) => {
        #[async_trait::async_trait]
        impl $crate::subscriptions::SubscriptionStorage for $adapter {
            async fn insert_subscription(
                &self,
                s: &$crate::subscriptions::SubscriptionRecord,
            ) -> aegis_core::Result<()> {
                let db_err =
                    |e: sqlx::Error| aegis_core::AegisError::Database(format!("{}: {}", $label, e));
                sqlx::query(&$crate::subscriptions::build_subscription_insert($ph))
                    .bind(&s.id)
                    .bind(&s.member_id)
                    .bind(&s.plan_id)
                    .bind(&s.level)
                    .bind(&s.status)
                    .bind(&s.stripe_customer_id)
                    .bind(&s.stripe_subscription_id)
                    .bind(&s.current_period_end)
                    .bind(s.cancel_at_period_end as i64)
                    .bind(&s.created_at)
                    .bind(&s.updated_at)
                    .execute(&self.pool)
                    .await
                    .map_err(db_err)?;
                Ok(())
            }

            async fn get_subscription(
                &self,
                id: &str,
            ) -> aegis_core::Result<Option<$crate::subscriptions::SubscriptionRecord>> {
                use sqlx::Row;
                let ph = $ph;
                let sql = format!(
                    "SELECT {} FROM {} WHERE id = {};",
                    $crate::subscriptions::SUBSCRIPTION_COLUMNS.join(", "),
                    $crate::subscriptions::SUBSCRIPTION_TABLE,
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
                    Some(r) => Ok(Some($crate::subscriptions::subscription_row_from_sqlx!(
                        &r
                    ))),
                    None => Ok(None),
                }
            }

            async fn get_subscription_by_stripe_id(
                &self,
                stripe_subscription_id: &str,
            ) -> aegis_core::Result<Option<$crate::subscriptions::SubscriptionRecord>> {
                use sqlx::Row;
                let ph = $ph;
                let sql = format!(
                    "SELECT {} FROM {} WHERE stripe_subscription_id = {};",
                    $crate::subscriptions::SUBSCRIPTION_COLUMNS.join(", "),
                    $crate::subscriptions::SUBSCRIPTION_TABLE,
                    ph(1)
                );
                let db_err =
                    |e: sqlx::Error| aegis_core::AegisError::Database(format!("{}: {}", $label, e));
                let row = sqlx::query(&sql)
                    .bind(stripe_subscription_id)
                    .fetch_optional(&self.pool)
                    .await
                    .map_err(db_err)?;
                match row {
                    Some(r) => Ok(Some($crate::subscriptions::subscription_row_from_sqlx!(
                        &r
                    ))),
                    None => Ok(None),
                }
            }

            async fn update_subscription(
                &self,
                s: &$crate::subscriptions::SubscriptionRecord,
            ) -> aegis_core::Result<()> {
                let ph = $ph;
                let sql = format!(
                    "UPDATE {} SET level = {}, status = {}, current_period_end = {}, \
                     cancel_at_period_end = {}, stripe_customer_id = {}, updated_at = {} WHERE id = {};",
                    $crate::subscriptions::SUBSCRIPTION_TABLE,
                    ph(1),
                    ph(2),
                    ph(3),
                    ph(4),
                    ph(5),
                    ph(6),
                    ph(7)
                );
                let db_err =
                    |e: sqlx::Error| aegis_core::AegisError::Database(format!("{}: {}", $label, e));
                sqlx::query(&sql)
                    .bind(&s.level)
                    .bind(&s.status)
                    .bind(&s.current_period_end)
                    .bind(s.cancel_at_period_end as i64)
                    .bind(&s.stripe_customer_id)
                    .bind(&s.updated_at)
                    .bind(&s.id)
                    .execute(&self.pool)
                    .await
                    .map_err(db_err)?;
                Ok(())
            }

            async fn list_member_subscriptions(
                &self,
                member_id: &str,
            ) -> aegis_core::Result<Vec<$crate::subscriptions::SubscriptionRecord>> {
                use sqlx::Row;
                let ph = $ph;
                let sql = format!(
                    "SELECT {} FROM {} WHERE member_id = {} ORDER BY created_at DESC;",
                    $crate::subscriptions::SUBSCRIPTION_COLUMNS.join(", "),
                    $crate::subscriptions::SUBSCRIPTION_TABLE,
                    ph(1)
                );
                let db_err =
                    |e: sqlx::Error| aegis_core::AegisError::Database(format!("{}: {}", $label, e));
                let rows = sqlx::query(&sql)
                    .bind(member_id)
                    .fetch_all(&self.pool)
                    .await
                    .map_err(db_err)?;
                let mut out = Vec::with_capacity(rows.len());
                for r in &rows {
                    out.push($crate::subscriptions::subscription_row_from_sqlx!(r));
                }
                Ok(out)
            }

            async fn list_subscriptions(
                &self,
                limit: u64,
                offset: u64,
            ) -> aegis_core::Result<(Vec<$crate::subscriptions::SubscriptionRecord>, u64)> {
                use sqlx::Row;
                let ph = $ph;
                let count_sql = format!(
                    "SELECT COUNT(*) FROM {};",
                    $crate::subscriptions::SUBSCRIPTION_TABLE
                );
                let select_sql = format!(
                    "SELECT {} FROM {} ORDER BY created_at ASC, id ASC LIMIT {} OFFSET {};",
                    $crate::subscriptions::SUBSCRIPTION_COLUMNS.join(", "),
                    $crate::subscriptions::SUBSCRIPTION_TABLE,
                    ph(1),
                    ph(2)
                );
                let db_err =
                    |e: sqlx::Error| aegis_core::AegisError::Database(format!("{}: {}", $label, e));
                let total: i64 = sqlx::query_scalar(&count_sql)
                    .fetch_one(&self.pool)
                    .await
                    .map_err(db_err)?;
                let rows = sqlx::query(&select_sql)
                    .bind(limit as i64)
                    .bind(offset as i64)
                    .fetch_all(&self.pool)
                    .await
                    .map_err(db_err)?;
                let mut out = Vec::with_capacity(rows.len());
                for r in &rows {
                    out.push($crate::subscriptions::subscription_row_from_sqlx!(r));
                }
                Ok((out, total.max(0) as u64))
            }

            async fn detach_member_subscriptions(
                &self,
                member_id: &str,
            ) -> aegis_core::Result<u64> {
                let ph = $ph;
                let sql = format!(
                    "UPDATE {} SET member_id = NULL WHERE member_id = {};",
                    $crate::subscriptions::SUBSCRIPTION_TABLE,
                    ph(1)
                );
                let db_err =
                    |e: sqlx::Error| aegis_core::AegisError::Database(format!("{}: {}", $label, e));
                let res = sqlx::query(&sql)
                    .bind(member_id)
                    .execute(&self.pool)
                    .await
                    .map_err(db_err)?;
                Ok(res.rows_affected())
            }

            async fn delete_subscription(&self, id: &str) -> aegis_core::Result<bool> {
                let ph = $ph;
                let sql = format!(
                    "DELETE FROM {} WHERE id = {};",
                    $crate::subscriptions::SUBSCRIPTION_TABLE,
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
pub(crate) use impl_subscription_storage;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{DatabaseAdapter, SqliteAdapter};

    fn subscription(id: &str, member: Option<&str>) -> SubscriptionRecord {
        SubscriptionRecord {
            id: id.to_string(),
            member_id: member.map(str::to_string),
            plan_id: "monthly".to_string(),
            level: "paid".to_string(),
            status: SUBSCRIPTION_ACTIVE.to_string(),
            stripe_customer_id: "cus_1".to_string(),
            stripe_subscription_id: format!("sub_{id}"),
            current_period_end: "2026-12-10T00:00:00Z".to_string(),
            cancel_at_period_end: false,
            created_at: format!("2026-11-10T00:00:0{}Z", id.len()),
            updated_at: "2026-11-10T00:00:00Z".to_string(),
        }
    }

    #[tokio::test]
    async fn test_store_update_list_detach_and_delete() {
        let db = SqliteAdapter::connect("sqlite::memory:", 1).await.unwrap();
        db.init().await.unwrap();
        db.insert_subscription(&subscription("s1", Some("m1")))
            .await
            .unwrap();
        db.insert_subscription(&subscription("s22", Some("m1")))
            .await
            .unwrap();
        db.insert_subscription(&subscription("s333", Some("m2")))
            .await
            .unwrap();
        // 同じ Stripe の契約は 2 回記録できない
        let mut twin = subscription("s4444", Some("m1"));
        twin.stripe_subscription_id = "sub_s1".to_string();
        assert!(db.insert_subscription(&twin).await.is_err());

        let mut got = db
            .get_subscription_by_stripe_id("sub_s1")
            .await
            .unwrap()
            .unwrap();
        assert!(!got.cancel_at_period_end);
        got.status = SUBSCRIPTION_PAST_DUE.to_string();
        got.cancel_at_period_end = true;
        got.current_period_end = "2027-01-10T00:00:00Z".to_string();
        got.updated_at = "2026-11-11T00:00:00Z".to_string();
        db.update_subscription(&got).await.unwrap();
        assert_eq!(db.get_subscription("s1").await.unwrap().unwrap(), got);

        let mine = db.list_member_subscriptions("m1").await.unwrap();
        assert_eq!(
            mine.iter().map(|s| s.id.as_str()).collect::<Vec<_>>(),
            vec!["s22", "s1"]
        );
        let (page, total) = db.list_subscriptions(2, 0).await.unwrap();
        assert_eq!((page.len(), total), (2, 3));

        assert_eq!(db.detach_member_subscriptions("m1").await.unwrap(), 2);
        assert!(db.list_member_subscriptions("m1").await.unwrap().is_empty());
        assert_eq!(
            db.get_subscription("s1").await.unwrap().unwrap().member_id,
            None
        );
        assert!(db.delete_subscription("s1").await.unwrap());
        assert!(!db.delete_subscription("s1").await.unwrap());
    }
}
