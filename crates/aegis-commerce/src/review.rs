//! 商品レビュー（Product Review）機能。
//! 購入者（Verified Purchase）による★1〜5の評価、1注文1件の制約、
//! `aegis-comments` のモデレーション機構の共有、ステマ規制対応（低評価を理由に非承認としない方針と非承認理由の記録）、
//! 運営者の1段返信、および JSON-LD AggregateRating 向けの集計を提供する。

use std::sync::Arc;

use aegis_audit::AuditService;
use aegis_comments::moderation::{ModerationConfig, PolicyOverrides};
use aegis_core::{
    AegisError, AuditAction, AuditActor, AuditEvent, AuditOutcome, ModerationStatus,
    NotificationEvent, Notifier, OrderLine, Recipient, Result,
};
use aegis_db::{DatabaseAdapter, ReviewAggregateStats, ReviewListQuery, ReviewPage, ReviewRecord};
use chrono::{SecondsFormat, Utc};
use serde::{Deserialize, Serialize};
use serde_json::json;
use uuid::Uuid;

/// `_aegis_settings` のレビュー接頭辞
pub const REVIEW_SETTINGS_PREFIX: &str = "reviews";

/// レビュー投稿者
#[derive(Debug, Clone)]
pub enum ReviewAuthor {
    /// ログイン中の顧客
    Customer {
        customer_id: String,
        email: String,
        name: String,
    },
    /// ゲスト購入者（注文アクセストークンによる検証）
    GuestWithToken {
        order_id: String,
        access_token: String,
        author_name: String,
    },
}

/// レビュー投稿リクエスト
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ReviewSubmission {
    pub order_id: String,
    pub rating: i64,
    pub title: String,
    pub body: String,
    /// ゲストの場合の投稿者名（顧客ログイン時は顧客の名前を使用）
    #[serde(default)]
    pub author_name: Option<String>,
    /// ゲストの場合の注文確認アクセストークン
    #[serde(default)]
    pub access_token: Option<String>,
}

/// 公開用のレビュー表示構造体
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct PublicReview {
    pub id: String,
    pub product_id: String,
    pub author_name: String,
    pub rating: i64,
    pub title: String,
    pub body: String,
    pub verified_purchase: bool,
    pub reply: Option<String>,
    pub replied_at: Option<String>,
    pub created_at: String,
}

impl From<ReviewRecord> for PublicReview {
    fn from(r: ReviewRecord) -> Self {
        Self {
            id: r.id,
            product_id: r.product_id,
            author_name: r.author_name,
            rating: r.rating,
            title: r.title,
            body: r.body,
            verified_purchase: r.verified_purchase != 0,
            reply: r.reply,
            replied_at: r.replied_at,
            created_at: r.created_at,
        }
    }
}

/// JSON-LD の AggregateRating 構造
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct JsonLdAggregateRating {
    #[serde(rename = "@type")]
    pub schema_type: String,
    pub rating_value: f64,
    pub review_count: u64,
    pub best_rating: u32,
    pub worst_rating: u32,
}

impl JsonLdAggregateRating {
    pub fn from_stats(stats: &ReviewAggregateStats) -> Option<Self> {
        if stats.review_count == 0 {
            None
        } else {
            Some(Self {
                schema_type: "AggregateRating".to_string(),
                rating_value: stats.average_rating,
                review_count: stats.review_count,
                best_rating: 5,
                worst_rating: 1,
            })
        }
    }
}

fn now() -> String {
    Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true)
}

fn hash_token(token: &str) -> String {
    use sha2::{Digest, Sha256};
    hex::encode(Sha256::digest(token.as_bytes()))
}

pub struct ReviewService {
    db: Arc<dyn DatabaseAdapter>,
    audit: Arc<AuditService>,
    notifier: Option<Arc<dyn Notifier>>,
    config: ModerationConfig,
}

impl ReviewService {
    pub fn new(
        db: Arc<dyn DatabaseAdapter>,
        audit: Arc<AuditService>,
        notifier: Option<Arc<dyn Notifier>>,
        config: ModerationConfig,
    ) -> Self {
        Self {
            db,
            audit,
            notifier,
            config,
        }
    }

    /// 商品レビューの投稿
    pub async fn submit_review(
        &self,
        product_id: &str,
        submission: ReviewSubmission,
        author: ReviewAuthor,
        ip: Option<String>,
        user_agent: Option<String>,
    ) -> Result<ReviewRecord> {
        // 1. 基本入力バリデーション
        if submission.rating < 1 || submission.rating > 5 {
            return Err(AegisError::Validation(
                "Rating must be between 1 and 5".to_string(),
            ));
        }
        let title = submission.title.trim();
        if title.is_empty() {
            return Err(AegisError::Validation("Title is required".to_string()));
        }
        if title.chars().count() > 200 {
            return Err(AegisError::Validation(
                "Title must be at most 200 characters".to_string(),
            ));
        }
        let body = submission.body.trim();
        if body.is_empty() {
            return Err(AegisError::Validation(
                "Review body is required".to_string(),
            ));
        }
        if body.chars().count() > 5000 {
            return Err(AegisError::Validation(
                "Review body must be at most 5,000 characters".to_string(),
            ));
        }

        // 2. 注文と購入履歴の検証（Verified Purchase）
        let order = self
            .db
            .get_order(&submission.order_id)
            .await?
            .ok_or_else(|| AegisError::NotFound("Order not found".to_string()))?;

        // ステータス検証: 支払い済み/配送中/配達完了/完了のみ
        let valid_order_status = matches!(
            order.status.as_str(),
            "paid" | "processing" | "shipped" | "delivered" | "completed"
        );
        if !valid_order_status {
            return Err(AegisError::PermissionDenied(
                "Reviews can only be submitted for completed or paid orders".to_string(),
            ));
        }

        // 注文内容に当該商品が含まれているか検証
        let lines: Vec<OrderLine> = serde_json::from_str(&order.lines_json)
            .map_err(|e| AegisError::Internal(format!("Failed to parse order lines: {}", e)))?;
        let product_in_order = lines
            .iter()
            .any(|l| l.product_id == product_id || l.slug == product_id);
        if !product_in_order {
            return Err(AegisError::PermissionDenied(
                "This order does not contain the specified product".to_string(),
            ));
        }

        // 3. 投稿者権限の検証
        let (customer_id, author_name, author_email) = match author {
            ReviewAuthor::Customer {
                customer_id,
                email,
                name,
            } => {
                // 注文の顧客IDまたはメールアドレスと一致するか確認
                let matches_id = order.customer_id.as_deref() == Some(&customer_id);
                let matches_email = order.customer_email.eq_ignore_ascii_case(&email);
                if !matches_id && !matches_email {
                    return Err(AegisError::PermissionDenied(
                        "You can only review products from your own orders".to_string(),
                    ));
                }
                (Some(customer_id), name, email)
            }
            ReviewAuthor::GuestWithToken {
                order_id: _,
                access_token,
                author_name,
            } => {
                let token_hash = hash_token(&access_token);
                if token_hash != order.access_token_hash {
                    return Err(AegisError::Authentication(
                        "Invalid order access token".to_string(),
                    ));
                }
                let name = if author_name.trim().is_empty() {
                    order.customer_name.clone()
                } else {
                    author_name.trim().to_string()
                };
                (None, name, order.customer_email.clone())
            }
        };

        // 4. 1注文1商品1件の制約検証
        if let Some(_existing) = self
            .db
            .get_review_by_order_product(&order.id, product_id)
            .await?
        {
            return Err(AegisError::Validation(
                "A review for this product has already been submitted for this order".to_string(),
            ));
        }

        // 5. モデレーション（審査）
        // 設定を _aegis_settings から取得
        let settings = self.get_settings_map().await.unwrap_or_default();
        let overrides = PolicyOverrides::from_settings(REVIEW_SETTINGS_PREFIX, &settings);
        let policy = self.config.policy(&overrides);

        // 語句・リンク数検査
        let screened = self
            .config
            .screen(&[title, body, &author_name, &author_email]);

        let initial_status = match screened {
            Some((status, _reason)) => status,
            None => {
                // ステマ規制・公正運用方針:
                // 低評価（★1や★2）であることを理由に保留・スパムにはしない。
                let is_known = false;
                policy.status_for(is_known)
            }
        };

        let final_status = match initial_status {
            ModerationStatus::Trash => "trash",
            ModerationStatus::Spam => "spam",
            ModerationStatus::Pending => "pending",
            ModerationStatus::Approved => "approved",
        };

        let review_id = Uuid::new_v4().to_string();
        let timestamp = now();

        let record = ReviewRecord {
            id: review_id.clone(),
            product_id: product_id.to_string(),
            order_id: order.id.clone(),
            customer_id: customer_id.clone(),
            author_name: author_name.clone(),
            author_email: author_email.clone(),
            rating: submission.rating,
            title: title.to_string(),
            body: body.to_string(),
            status: final_status.to_string(),
            verified_purchase: 1,
            reply: None,
            replied_at: None,
            rejection_reason: None,
            ip: ip.clone(),
            user_agent: user_agent.clone(),
            created_at: timestamp.clone(),
            updated_at: timestamp,
        };

        self.db.insert_review(&record).await?;

        // 6. 監査ログ
        let event = AuditEvent::new(AuditAction::ReviewCreate, AuditOutcome::Success)
            .with_actor(Some(AuditActor {
                user_id: customer_id.unwrap_or_else(|| "guest".to_string()),
                role: "customer".to_string(),
            }))
            .with_resource("review", Some(review_id.clone()))
            .with_client(ip, user_agent)
            .with_details(json!({
                "product_id": product_id,
                "rating": record.rating,
                "status": final_status,
            }));
        self.audit.record(event).await;

        // 7. 通知（運営者への通知）
        if let Some(notifier) = &self.notifier {
            let event_name = if final_status == "approved" {
                "review.posted"
            } else {
                "review.pending"
            };
            let payload = json!({
                "review_id": review_id,
                "product_id": product_id,
                "rating": record.rating,
                "title": record.title,
                "author_name": record.author_name,
                "status": final_status,
            });

            let notif = NotificationEvent::new(event_name, vec![Recipient::Staff], payload);
            notifier.notify(notif);
        }

        Ok(record)
    }

    /// 公開用の承認済みレビュー一覧と集計統計を取得
    pub async fn list_public_reviews(
        &self,
        product_id: &str,
        limit: Option<u64>,
        offset: u64,
    ) -> Result<(Vec<PublicReview>, ReviewAggregateStats)> {
        let query = ReviewListQuery {
            product_id: Some(product_id.to_string()),
            status: Some("approved".to_string()),
            customer_id: None,
            oldest_first: false,
            limit,
            offset,
        };
        let page = self.db.list_reviews(&query).await?;
        let stats = self.db.get_product_review_stats(product_id).await?;
        let reviews = page.reviews.into_iter().map(PublicReview::from).collect();
        Ok((reviews, stats))
    }

    /// 顧客自身の投稿レビュー一覧
    pub async fn list_customer_reviews(&self, customer_id: &str) -> Result<Vec<ReviewRecord>> {
        let query = ReviewListQuery {
            product_id: None,
            status: None,
            customer_id: Some(customer_id.to_string()),
            oldest_first: false,
            limit: None,
            offset: 0,
        };
        let page = self.db.list_reviews(&query).await?;
        Ok(page.reviews)
    }

    // --- 管理者向け操作 ---

    /// 管理者: レビュー一覧
    pub async fn admin_list_reviews(&self, query: ReviewListQuery) -> Result<ReviewPage> {
        self.db.list_reviews(&query).await
    }

    /// 管理者: レビュー詳細取得
    pub async fn admin_get_review(&self, id: &str) -> Result<ReviewRecord> {
        self.db
            .get_review(id)
            .await?
            .ok_or_else(|| AegisError::NotFound("Review not found".to_string()))
    }

    /// 管理者: レビューステータス変更（ステマ規制対応: 非承認時は理由を記録）
    pub async fn admin_set_status(
        &self,
        id: &str,
        status: &str,
        rejection_reason: Option<&str>,
    ) -> Result<()> {
        let valid_statuses = ["pending", "approved", "spam", "trash"];
        if !valid_statuses.contains(&status) {
            return Err(AegisError::Validation(format!(
                "Invalid status: '{}'. Must be one of: {:?}",
                status, valid_statuses
            )));
        }
        let affected = self
            .db
            .set_reviews_status(&[id.to_string()], status, rejection_reason, &now())
            .await?;
        if affected == 0 {
            return Err(AegisError::NotFound("Review not found".to_string()));
        }

        self.audit
            .record(
                AuditEvent::new(AuditAction::ReviewModerate, AuditOutcome::Success)
                    .with_resource("review", Some(id.to_string()))
                    .with_details(json!({
                        "status": status,
                        "rejection_reason": rejection_reason.unwrap_or(""),
                    })),
            )
            .await;

        Ok(())
    }

    /// 管理者: 運営者返信の設定（1段のみ）
    pub async fn admin_set_reply(&self, id: &str, reply: Option<&str>) -> Result<()> {
        let replied_at = if reply.is_some() { Some(now()) } else { None };
        self.db
            .set_review_reply(id, reply, replied_at.as_deref(), &now())
            .await?;

        self.audit
            .record(
                AuditEvent::new(AuditAction::ReviewReply, AuditOutcome::Success)
                    .with_resource("review", Some(id.to_string())),
            )
            .await;

        Ok(())
    }

    /// 管理者: 一括ステータス変更
    pub async fn admin_bulk_status(
        &self,
        ids: &[String],
        status: &str,
        rejection_reason: Option<&str>,
    ) -> Result<u64> {
        self.db
            .set_reviews_status(ids, status, rejection_reason, &now())
            .await
    }

    /// 管理者: 一括削除
    pub async fn admin_bulk_delete(&self, ids: &[String]) -> Result<u64> {
        let count = self.db.delete_reviews(ids).await?;
        for id in ids {
            self.audit
                .record(
                    AuditEvent::new(AuditAction::ReviewDelete, AuditOutcome::Success)
                        .with_resource("review", Some(id.clone())),
                )
                .await;
        }
        Ok(count)
    }

    /// 管理者: 設定取得
    pub async fn admin_get_settings(&self) -> Result<PolicyOverrides> {
        let settings = self.get_settings_map().await?;
        Ok(PolicyOverrides::from_settings(
            REVIEW_SETTINGS_PREFIX,
            &settings,
        ))
    }

    /// 管理者: 設定更新
    pub async fn admin_update_settings(&self, next: PolicyOverrides) -> Result<PolicyOverrides> {
        let (approval, known) = aegis_comments::moderation::setting_names(REVIEW_SETTINGS_PREFIX);
        for (name, value) in [
            (&approval, next.require_approval),
            (&known, next.auto_approve_known),
        ] {
            match value {
                Some(val) => self.db.set_setting(name, &val.to_string()).await?,
                None => self.db.delete_setting(name).await?,
            }
        }
        self.admin_get_settings().await
    }

    async fn get_settings_map(&self) -> Result<std::collections::HashMap<String, String>> {
        self.db.list_settings().await
    }
}
