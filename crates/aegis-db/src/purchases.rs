//! 記事・ページの単独購入の記録（`_aegis_purchases`。docs/members.md 16）。
//! 閲覧の権利は `status = 'paid'` の行があるかで決まる。金額や通貨は、決済を始めた時点の値を残し、
//! 入金の通知（Webhook）と照合する。

use aegis_core::Result;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

/// 購入テーブル名
pub const PURCHASE_TABLE: &str = "_aegis_purchases";

/// 購入の状態: 決済ページを作った（まだ入金されていない）
pub const PURCHASE_PENDING: &str = "pending";
/// 購入の状態: 入金された（閲覧の権利がある）
pub const PURCHASE_PAID: &str = "paid";
/// 購入の状態: 全額を返金した（権利は外れる）
pub const PURCHASE_REFUNDED: &str = "refunded";
/// 購入の状態: 決済ページの期限切れ・支払いの失敗
pub const PURCHASE_CANCELLED: &str = "cancelled";

/// INSERT / SELECT で使う購入テーブルのカラム（順序固定）
pub const PURCHASE_COLUMNS: [&str; 12] = [
    "id",
    "member_id",
    "content_type",
    "content_id",
    "amount",
    "currency",
    "status",
    "provider_ref",
    "transaction_id",
    "created_at",
    "paid_at",
    "updated_at",
];

/// 購入（`_aegis_purchases` の 1 行）
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct PurchaseRecord {
    pub id: String,
    /// 買った会員。退会すると `None`（会計の記録として行は残る。権利は外れる）
    pub member_id: Option<String>,
    pub content_type: String,
    pub content_id: String,
    /// 決済を始めた時点の金額（通貨の最小単位）
    pub amount: i64,
    pub currency: String,
    /// `pending` / `paid` / `refunded` / `cancelled`
    pub status: String,
    /// Checkout Session の ID（Webhook で購入を見つけるのに使う。一意）
    pub provider_ref: String,
    /// 確定した取引の ID（Payment Intent。返金の通知との突き合わせに使う）
    pub transaction_id: Option<String>,
    pub created_at: String,
    pub paid_at: Option<String>,
    pub updated_at: String,
}

/// 購入の INSERT 文
pub fn build_purchase_insert(placeholder: impl Fn(usize) -> String) -> String {
    let placeholders: Vec<String> = (1..=PURCHASE_COLUMNS.len()).map(placeholder).collect();
    format!(
        "INSERT INTO {} ({}) VALUES ({});",
        PURCHASE_TABLE,
        PURCHASE_COLUMNS.join(", "),
        placeholders.join(", ")
    )
}

/// sqlx の行を `PurchaseRecord` に変換する
macro_rules! purchase_row_from_sqlx {
    ($row:expr) => {{
        let row = $row;
        let get = |e: sqlx::Error| aegis_core::AegisError::Database(e.to_string());
        $crate::purchases::PurchaseRecord {
            id: row.try_get("id").map_err(get)?,
            member_id: row.try_get("member_id").map_err(get)?,
            content_type: row.try_get("content_type").map_err(get)?,
            content_id: row.try_get("content_id").map_err(get)?,
            amount: row.try_get("amount").map_err(get)?,
            currency: row.try_get("currency").map_err(get)?,
            status: row.try_get("status").map_err(get)?,
            provider_ref: row.try_get("provider_ref").map_err(get)?,
            transaction_id: row.try_get("transaction_id").map_err(get)?,
            created_at: row.try_get("created_at").map_err(get)?,
            paid_at: row.try_get("paid_at").map_err(get)?,
            updated_at: row.try_get("updated_at").map_err(get)?,
        }
    }};
}
pub(crate) use purchase_row_from_sqlx;

/// 購入の保存（`DatabaseAdapter` のスーパートレイト）
#[async_trait]
pub trait PurchaseStorage: Send + Sync {
    async fn insert_purchase(&self, purchase: &PurchaseRecord) -> Result<()>;
    async fn get_purchase_by_ref(&self, provider_ref: &str) -> Result<Option<PurchaseRecord>>;
    async fn get_purchase_by_transaction(
        &self,
        transaction_id: &str,
    ) -> Result<Option<PurchaseRecord>>;
    /// 状態が `from` のどれかのときだけ `to` に変える（条件付きの更新なので、Webhook の再送や並行する
    /// 通知で二重に動かない）。`paid_at` / `transaction_id` は `Some` のときだけ書く。変えたら `true`
    async fn transition_purchase(
        &self,
        id: &str,
        from: &[&str],
        to: &str,
        paid_at: Option<&str>,
        transaction_id: Option<&str>,
        updated_at: &str,
    ) -> Result<bool>;
    /// この会員が、その記事・ページを買っている（`paid`）か
    async fn has_paid_purchase(
        &self,
        member_id: &str,
        content_type: &str,
        content_id: &str,
    ) -> Result<bool>;
    /// 会員の購入（作成日の新しい順。`pending` / `cancelled` を含む）
    async fn list_member_purchases(&self, member_id: &str) -> Result<Vec<PurchaseRecord>>;
    /// 全購入（バックアップ用。作成日の古い順）
    async fn list_purchases(&self, limit: u64, offset: u64) -> Result<(Vec<PurchaseRecord>, u64)>;
    /// 退会する会員の購入から、会員との紐づきを外す。外した件数
    async fn detach_member_purchases(&self, member_id: &str) -> Result<u64>;
    /// 購入の行を消す（バックアップの復元で上書きするときだけ。会計の記録なので、ほかでは消さない）
    async fn delete_purchase(&self, id: &str) -> Result<bool>;
}

/// `PurchaseStorage` の実装マクロ
macro_rules! impl_purchase_storage {
    ($adapter:ty, $ph:expr, $label:literal) => {
        #[async_trait::async_trait]
        impl $crate::purchases::PurchaseStorage for $adapter {
            async fn insert_purchase(
                &self,
                p: &$crate::purchases::PurchaseRecord,
            ) -> aegis_core::Result<()> {
                let db_err =
                    |e: sqlx::Error| aegis_core::AegisError::Database(format!("{}: {}", $label, e));
                sqlx::query(&$crate::purchases::build_purchase_insert($ph))
                    .bind(&p.id)
                    .bind(&p.member_id)
                    .bind(&p.content_type)
                    .bind(&p.content_id)
                    .bind(p.amount)
                    .bind(&p.currency)
                    .bind(&p.status)
                    .bind(&p.provider_ref)
                    .bind(&p.transaction_id)
                    .bind(&p.created_at)
                    .bind(&p.paid_at)
                    .bind(&p.updated_at)
                    .execute(&self.pool)
                    .await
                    .map_err(db_err)?;
                Ok(())
            }

            async fn get_purchase_by_ref(
                &self,
                provider_ref: &str,
            ) -> aegis_core::Result<Option<$crate::purchases::PurchaseRecord>> {
                use sqlx::Row;
                let ph = $ph;
                let sql = format!(
                    "SELECT {} FROM {} WHERE provider_ref = {};",
                    $crate::purchases::PURCHASE_COLUMNS.join(", "),
                    $crate::purchases::PURCHASE_TABLE,
                    ph(1)
                );
                let db_err =
                    |e: sqlx::Error| aegis_core::AegisError::Database(format!("{}: {}", $label, e));
                let row = sqlx::query(&sql)
                    .bind(provider_ref)
                    .fetch_optional(&self.pool)
                    .await
                    .map_err(db_err)?;
                match row {
                    Some(r) => Ok(Some($crate::purchases::purchase_row_from_sqlx!(&r))),
                    None => Ok(None),
                }
            }

            async fn get_purchase_by_transaction(
                &self,
                transaction_id: &str,
            ) -> aegis_core::Result<Option<$crate::purchases::PurchaseRecord>> {
                use sqlx::Row;
                let ph = $ph;
                let sql = format!(
                    "SELECT {} FROM {} WHERE transaction_id = {} ORDER BY created_at DESC LIMIT 1;",
                    $crate::purchases::PURCHASE_COLUMNS.join(", "),
                    $crate::purchases::PURCHASE_TABLE,
                    ph(1)
                );
                let db_err =
                    |e: sqlx::Error| aegis_core::AegisError::Database(format!("{}: {}", $label, e));
                let row = sqlx::query(&sql)
                    .bind(transaction_id)
                    .fetch_optional(&self.pool)
                    .await
                    .map_err(db_err)?;
                match row {
                    Some(r) => Ok(Some($crate::purchases::purchase_row_from_sqlx!(&r))),
                    None => Ok(None),
                }
            }

            async fn transition_purchase(
                &self,
                id: &str,
                from: &[&str],
                to: &str,
                paid_at: Option<&str>,
                transaction_id: Option<&str>,
                updated_at: &str,
            ) -> aegis_core::Result<bool> {
                let ph = $ph;
                // 位置: 1=to, 2=updated_at, 3=paid_at, 4=transaction_id, 5=id, 6.. = from
                let from_placeholders: Vec<String> = (0..from.len()).map(|i| ph(6 + i)).collect();
                let sql = format!(
                    "UPDATE {} SET status = {}, updated_at = {}, paid_at = COALESCE({}, paid_at), \
                     transaction_id = COALESCE({}, transaction_id) WHERE id = {} AND status IN ({});",
                    $crate::purchases::PURCHASE_TABLE,
                    ph(1),
                    ph(2),
                    ph(3),
                    ph(4),
                    ph(5),
                    from_placeholders.join(", ")
                );
                let db_err =
                    |e: sqlx::Error| aegis_core::AegisError::Database(format!("{}: {}", $label, e));
                let mut query = sqlx::query(&sql)
                    .bind(to)
                    .bind(updated_at)
                    .bind(paid_at)
                    .bind(transaction_id)
                    .bind(id);
                for status in from {
                    query = query.bind(*status);
                }
                let res = query.execute(&self.pool).await.map_err(db_err)?;
                Ok(res.rows_affected() > 0)
            }

            async fn has_paid_purchase(
                &self,
                member_id: &str,
                content_type: &str,
                content_id: &str,
            ) -> aegis_core::Result<bool> {
                let ph = $ph;
                let sql = format!(
                    "SELECT COUNT(*) FROM {} WHERE member_id = {} AND content_type = {} \
                     AND content_id = {} AND status = {};",
                    $crate::purchases::PURCHASE_TABLE,
                    ph(1),
                    ph(2),
                    ph(3),
                    ph(4)
                );
                let db_err =
                    |e: sqlx::Error| aegis_core::AegisError::Database(format!("{}: {}", $label, e));
                let count: i64 = sqlx::query_scalar(&sql)
                    .bind(member_id)
                    .bind(content_type)
                    .bind(content_id)
                    .bind($crate::purchases::PURCHASE_PAID)
                    .fetch_one(&self.pool)
                    .await
                    .map_err(db_err)?;
                Ok(count > 0)
            }

            async fn list_member_purchases(
                &self,
                member_id: &str,
            ) -> aegis_core::Result<Vec<$crate::purchases::PurchaseRecord>> {
                use sqlx::Row;
                let ph = $ph;
                let sql = format!(
                    "SELECT {} FROM {} WHERE member_id = {} ORDER BY created_at DESC;",
                    $crate::purchases::PURCHASE_COLUMNS.join(", "),
                    $crate::purchases::PURCHASE_TABLE,
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
                    out.push($crate::purchases::purchase_row_from_sqlx!(r));
                }
                Ok(out)
            }

            async fn list_purchases(
                &self,
                limit: u64,
                offset: u64,
            ) -> aegis_core::Result<(Vec<$crate::purchases::PurchaseRecord>, u64)> {
                use sqlx::Row;
                let ph = $ph;
                let count_sql =
                    format!("SELECT COUNT(*) FROM {};", $crate::purchases::PURCHASE_TABLE);
                let select_sql = format!(
                    "SELECT {} FROM {} ORDER BY created_at ASC, id ASC LIMIT {} OFFSET {};",
                    $crate::purchases::PURCHASE_COLUMNS.join(", "),
                    $crate::purchases::PURCHASE_TABLE,
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
                    out.push($crate::purchases::purchase_row_from_sqlx!(r));
                }
                Ok((out, total.max(0) as u64))
            }

            async fn delete_purchase(&self, id: &str) -> aegis_core::Result<bool> {
                let ph = $ph;
                let sql = format!(
                    "DELETE FROM {} WHERE id = {};",
                    $crate::purchases::PURCHASE_TABLE,
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

            async fn detach_member_purchases(&self, member_id: &str) -> aegis_core::Result<u64> {
                let ph = $ph;
                let sql = format!(
                    "UPDATE {} SET member_id = NULL WHERE member_id = {};",
                    $crate::purchases::PURCHASE_TABLE,
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
        }
    };
}
pub(crate) use impl_purchase_storage;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{DatabaseAdapter, SqliteAdapter};

    fn purchase(id: &str, member: Option<&str>, status: &str) -> PurchaseRecord {
        PurchaseRecord {
            id: id.to_string(),
            member_id: member.map(str::to_string),
            content_type: "article".to_string(),
            content_id: "a1".to_string(),
            amount: 500,
            currency: "jpy".to_string(),
            status: status.to_string(),
            provider_ref: format!("cs_{id}"),
            transaction_id: None,
            created_at: format!("2026-11-10T00:00:0{}Z", id.len()),
            paid_at: None,
            updated_at: "2026-11-10T00:00:00Z".to_string(),
        }
    }

    async fn adapter() -> SqliteAdapter {
        let adapter = SqliteAdapter::connect("sqlite::memory:", 1).await.unwrap();
        adapter.init().await.unwrap();
        adapter
    }

    /// 状態の変更は条件付き。再送や並行する通知で、二重に動かず、終わった状態を巻き戻さない
    #[tokio::test]
    async fn test_transitions_are_conditional() {
        let db = adapter().await;
        db.insert_purchase(&purchase("p1", Some("m1"), PURCHASE_PENDING))
            .await
            .unwrap();
        assert!(!db.has_paid_purchase("m1", "article", "a1").await.unwrap());

        let now = "2026-11-10T01:00:00Z";
        assert!(db
            .transition_purchase(
                "p1",
                &[PURCHASE_PENDING],
                PURCHASE_PAID,
                Some(now),
                Some("pi_1"),
                now
            )
            .await
            .unwrap());
        // 2 回目は何も変えない
        assert!(!db
            .transition_purchase(
                "p1",
                &[PURCHASE_PENDING],
                PURCHASE_PAID,
                Some(now),
                Some("pi_2"),
                now
            )
            .await
            .unwrap());
        // 期限切れの通知が遅れて来ても、支払い済みを取り消さない
        assert!(!db
            .transition_purchase(
                "p1",
                &[PURCHASE_PENDING],
                PURCHASE_CANCELLED,
                None,
                None,
                now
            )
            .await
            .unwrap());

        let got = db.get_purchase_by_ref("cs_p1").await.unwrap().unwrap();
        assert_eq!(got.status, PURCHASE_PAID);
        assert_eq!(got.transaction_id.as_deref(), Some("pi_1"));
        assert_eq!(got.paid_at.as_deref(), Some(now));
        assert!(db.has_paid_purchase("m1", "article", "a1").await.unwrap());
        assert!(!db.has_paid_purchase("m2", "article", "a1").await.unwrap());
        assert!(!db.has_paid_purchase("m1", "article", "a2").await.unwrap());

        // 返金は支払い済みからだけ。取引 ID で見つかる
        let by_tx = db
            .get_purchase_by_transaction("pi_1")
            .await
            .unwrap()
            .unwrap();
        assert!(db
            .transition_purchase(
                &by_tx.id,
                &[PURCHASE_PAID],
                PURCHASE_REFUNDED,
                None,
                None,
                now
            )
            .await
            .unwrap());
        assert!(!db.has_paid_purchase("m1", "article", "a1").await.unwrap());
        // 値を渡さなかった列（paid_at・取引 ID）は消えない
        let got = db.get_purchase_by_ref("cs_p1").await.unwrap().unwrap();
        assert_eq!(got.paid_at.as_deref(), Some(now));
        assert_eq!(got.transaction_id.as_deref(), Some("pi_1"));
    }

    #[tokio::test]
    async fn test_listing_detaching_and_deleting() {
        let db = adapter().await;
        for (id, member) in [
            ("p1", Some("m1")),
            ("p22", Some("m1")),
            ("p333", Some("m2")),
        ] {
            db.insert_purchase(&purchase(id, member, PURCHASE_PAID))
                .await
                .unwrap();
        }
        // 同じ Checkout Session は 2 回記録できない
        let mut twin = purchase("p4444", Some("m1"), PURCHASE_PENDING);
        twin.provider_ref = "cs_p1".to_string();
        assert!(db.insert_purchase(&twin).await.is_err());

        let mine = db.list_member_purchases("m1").await.unwrap();
        assert_eq!(
            mine.iter().map(|p| p.id.as_str()).collect::<Vec<_>>(),
            vec!["p22", "p1"],
            "newest first"
        );
        let (all, total) = db.list_purchases(2, 0).await.unwrap();
        assert_eq!((all.len(), total), (2, 3));
        let (rest, _) = db.list_purchases(2, 2).await.unwrap();
        assert_eq!(rest.len(), 1);

        assert_eq!(db.detach_member_purchases("m1").await.unwrap(), 2);
        assert!(db.list_member_purchases("m1").await.unwrap().is_empty());
        assert!(!db.has_paid_purchase("m1", "article", "a1").await.unwrap());
        // 行は残る
        let kept = db.get_purchase_by_ref("cs_p1").await.unwrap().unwrap();
        assert_eq!(kept.member_id, None);

        assert!(db.delete_purchase("p1").await.unwrap());
        assert!(!db.delete_purchase("p1").await.unwrap());
        assert_eq!(db.list_purchases(10, 0).await.unwrap().1, 2);
    }
}
