//! 会員（`aegis-members`）とショップの結びつき: 会員の注文履歴と、退会のときの後始末。
//! 会員のクレートは注文もレビューも知らないので、結びつけはこちらが受け持つ。

use aegis_core::{AegisError, Result};
use aegis_db::{DatabaseAdapter, OrderRecord, ReviewListQuery};

/// 会員の注文履歴（作成日の新しい順）
pub async fn list_member_orders(
    db: &dyn DatabaseAdapter,
    member_id: &str,
) -> Result<Vec<OrderRecord>> {
    db.list_member_orders(member_id).await
}

/// 会員の注文の詳細。自分の注文であること（注文の会員 ID が一致する、またはゲストで買った注文の
/// メールアドレスが会員のものと一致する）を確かめる
pub async fn get_member_order(
    db: &dyn DatabaseAdapter,
    member_id: &str,
    order_id: &str,
) -> Result<OrderRecord> {
    let order = db
        .get_order(order_id)
        .await?
        .ok_or_else(|| AegisError::NotFound("Order not found".to_string()))?;

    if order.member_id.as_deref() != Some(member_id) {
        let member = db
            .get_member_by_id(member_id)
            .await?
            .ok_or_else(|| AegisError::NotFound("Member not found".to_string()))?;
        if order.customer_email.to_lowercase() != member.email.to_lowercase() {
            return Err(AegisError::PermissionDenied(
                "You do not have access to this order".to_string(),
            ));
        }
    }
    Ok(order)
}

/// 退会する会員のショップのデータを後始末する: 注文は会員との紐づきだけを外して残し（会計の記録）、
/// レビューは匿名にして残す。戻り値は、レビューを匿名にした商品の ID（公開ページのキャッシュを更新するため）。
/// 会員の行を消す前に呼ぶ。途中で失敗しても、同じ手順をやり直せる
pub async fn release_member(db: &dyn DatabaseAdapter, member_id: &str) -> Result<Vec<String>> {
    let mut product_ids: Vec<String> = db
        .list_reviews(&ReviewListQuery {
            product_id: None,
            status: None,
            member_id: Some(member_id.to_string()),
            oldest_first: false,
            limit: None,
            offset: 0,
        })
        .await?
        .reviews
        .into_iter()
        .map(|review| review.product_id)
        .collect();
    product_ids.sort();
    product_ids.dedup();

    db.detach_member_orders(member_id).await?;
    db.anonymize_member_reviews(member_id).await?;
    Ok(product_ids)
}
