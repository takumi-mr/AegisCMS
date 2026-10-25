#[cfg(test)]
mod tests {
    use crate::review::{JsonLdAggregateRating, ReviewAuthor, ReviewService, ReviewSubmission};
    use aegis_audit::AuditService;
    use aegis_comments::ModerationConfig;
    use aegis_core::{OrderLine, OrderStatus};
    use aegis_db::{create_database_adapter, DatabaseAdapter, DatabaseConfig, OrderRecord};
    use sha2::{Digest, Sha256};
    use std::sync::Arc;

    async fn setup_review_service() -> (ReviewService, Arc<dyn DatabaseAdapter>) {
        let config = DatabaseConfig {
            url: "sqlite::memory:".to_string(),
            ..DatabaseConfig::default()
        };
        let db = create_database_adapter(&config).await.unwrap();
        db.init().await.unwrap();

        let audit = Arc::new(AuditService::with_sinks(vec![]));
        let mod_config = ModerationConfig {
            require_approval: false, // テスト簡素化のため既定で自動承認
            ..ModerationConfig::default()
        };

        let service = ReviewService::new(db.clone(), audit, None, mod_config);
        (service, db)
    }

    fn hash_token(token: &str) -> String {
        hex::encode(Sha256::digest(token.as_bytes()))
    }

    async fn create_dummy_order(
        db: &Arc<dyn DatabaseAdapter>,
        order_id: &str,
        customer_id: Option<&str>,
        customer_email: &str,
        product_id: &str,
        status: OrderStatus,
        access_token: &str,
    ) {
        let lines = vec![OrderLine {
            product_id: product_id.to_string(),
            variant_id: None,
            variant_title: None,
            slug: "test-product".to_string(),
            title: "Test Product".to_string(),
            sku: None,
            unit_price: 1000,
            quantity: 1,
            line_total: 1000,
        }];

        let order = OrderRecord {
            id: order_id.to_string(),
            number: format!("ORD-{}", order_id),
            status: status.as_str().to_string(),
            currency: "JPY".to_string(),
            subtotal: 1000,
            total: 1000,
            lines_json: serde_json::to_string(&lines).unwrap(),
            adjustments_json: "[]".to_string(),
            customer_email: customer_email.to_string(),
            customer_name: "Customer Name".to_string(),
            payment_provider: "stripe".to_string(),
            access_token_hash: hash_token(access_token),
            created_at: "2026-10-25T10:00:00Z".to_string(),
            updated_at: "2026-10-25T10:00:00Z".to_string(),
            paid_at: Some("2026-10-25T10:05:00Z".to_string()),
            customer_id: customer_id.map(str::to_string),
            ..Default::default()
        };

        db.create_order(&order, &[]).await.unwrap().unwrap();
    }

    #[tokio::test]
    async fn test_verified_purchase_and_single_review_per_order() {
        let (service, db) = setup_review_service().await;

        let customer_id = "cust_123";
        let product_id = "prod_keyboard";
        let order_id = "ord_001";
        let token = "tok_secret_123";

        // 支払い済み注文を作成
        create_dummy_order(
            &db,
            order_id,
            Some(customer_id),
            "user@example.com",
            product_id,
            OrderStatus::Paid,
            token,
        )
        .await;

        // 1. レビュー投稿（正常系）
        let submission = ReviewSubmission {
            order_id: order_id.to_string(),
            rating: 5,
            title: "Great Keyboard!".to_string(),
            body: "Very tactile and comfortable typing experience.".to_string(),
            author_name: None,
            access_token: None,
        };
        let author = ReviewAuthor::Customer {
            customer_id: customer_id.to_string(),
            email: "user@example.com".to_string(),
            name: "Alice".to_string(),
        };

        let review = service
            .submit_review(product_id, submission.clone(), author.clone(), None, None)
            .await
            .expect("First review submission should succeed");

        assert_eq!(review.rating, 5);
        assert_eq!(review.status, "approved");
        assert_eq!(review.verified_purchase, 1);

        // 2. 同じ注文から同じ商品への2度目の投稿は拒否（1注文1件の制約）
        let duplicate_res = service
            .submit_review(product_id, submission, author, None, None)
            .await;
        assert!(
            duplicate_res.is_err(),
            "Duplicate review for the same order and product must be rejected"
        );
    }

    #[tokio::test]
    async fn test_unverified_purchase_rejection() {
        let (service, db) = setup_review_service().await;

        let customer_id = "cust_hacker";
        let product_id = "prod_laptop";
        let order_id = "ord_unpaid";
        let token = "tok_abc";

        // 未払いの注文
        create_dummy_order(
            &db,
            order_id,
            Some(customer_id),
            "hacker@example.com",
            product_id,
            OrderStatus::PendingPayment,
            token,
        )
        .await;

        let submission = ReviewSubmission {
            order_id: order_id.to_string(),
            rating: 1,
            title: "Haven't got it".to_string(),
            body: "Terrible".to_string(),
            author_name: None,
            access_token: None,
        };
        let author = ReviewAuthor::Customer {
            customer_id: customer_id.to_string(),
            email: "hacker@example.com".to_string(),
            name: "Hacker".to_string(),
        };

        // 未払い注文によるレビューは拒否される
        let res = service
            .submit_review(product_id, submission, author, None, None)
            .await;
        assert!(res.is_err(), "Pending payment order must not allow reviews");
    }

    #[tokio::test]
    async fn test_guest_review_with_token() {
        let (service, db) = setup_review_service().await;

        let product_id = "prod_mouse";
        let order_id = "ord_guest_01";
        let valid_token = "tok_guest_secret";

        create_dummy_order(
            &db,
            order_id,
            None,
            "guest@example.com",
            product_id,
            OrderStatus::Completed,
            valid_token,
        )
        .await;

        // 1. 不正なトークンでのゲスト投稿は拒否
        let bad_sub = ReviewSubmission {
            order_id: order_id.to_string(),
            rating: 4,
            title: "Nice mouse".to_string(),
            body: "Smooth movement".to_string(),
            author_name: Some("Guesty".to_string()),
            access_token: Some("invalid_token".to_string()),
        };
        let bad_author = ReviewAuthor::GuestWithToken {
            order_id: order_id.to_string(),
            access_token: "invalid_token".to_string(),
            author_name: "Guesty".to_string(),
        };
        let bad_res = service
            .submit_review(product_id, bad_sub, bad_author, None, None)
            .await;
        assert!(bad_res.is_err(), "Invalid access token must be rejected");

        // 2. 正しいトークンでのゲスト投稿は成功
        let good_sub = ReviewSubmission {
            order_id: order_id.to_string(),
            rating: 4,
            title: "Nice mouse".to_string(),
            body: "Smooth movement".to_string(),
            author_name: Some("Guesty".to_string()),
            access_token: Some(valid_token.to_string()),
        };
        let good_author = ReviewAuthor::GuestWithToken {
            order_id: order_id.to_string(),
            access_token: valid_token.to_string(),
            author_name: "Guesty".to_string(),
        };
        let review = service
            .submit_review(product_id, good_sub, good_author, None, None)
            .await
            .expect("Guest review with valid token should succeed");

        assert_eq!(review.rating, 4);
        assert_eq!(review.verified_purchase, 1);
    }

    #[tokio::test]
    async fn test_stealth_marketing_and_rejection_reason() {
        let (service, db) = setup_review_service().await;

        let product_id = "prod_speaker";
        let order_id = "ord_speaker_01";
        let token = "tok_spk";

        create_dummy_order(
            &db,
            order_id,
            Some("cust_spk"),
            "spk@example.com",
            product_id,
            OrderStatus::Paid,
            token,
        )
        .await;

        // ★1 の低評価レビューを投稿
        let sub = ReviewSubmission {
            order_id: order_id.to_string(),
            rating: 1,
            title: "Broke quickly".to_string(),
            body: "Stopped working after two days.".to_string(),
            author_name: None,
            access_token: None,
        };
        let author = ReviewAuthor::Customer {
            customer_id: "cust_spk".to_string(),
            email: "spk@example.com".to_string(),
            name: "Reviewer".to_string(),
        };

        // 自動承認モードなので★1でも機械的に非承認にされず承認される（ステマ規制への配慮）
        let review = service
            .submit_review(product_id, sub, author, None, None)
            .await
            .unwrap();
        assert_eq!(review.rating, 1);
        assert_eq!(review.status, "approved");

        // 管理者がモデレーションする場合、非承認時には理由 (rejection_reason) を残せる
        service
            .admin_set_status(
                &review.id,
                "spam",
                Some("Contains external promotional link"),
            )
            .await
            .unwrap();

        let updated = service.admin_get_review(&review.id).await.unwrap();
        assert_eq!(updated.status, "spam");
        assert_eq!(
            updated.rejection_reason.as_deref(),
            Some("Contains external promotional link")
        );

        // 運営者の返信
        service
            .admin_set_reply(
                &review.id,
                Some("We apologize for the inconvenience. Our support team will contact you."),
            )
            .await
            .unwrap();

        let with_reply = service.admin_get_review(&review.id).await.unwrap();
        assert!(with_reply.reply.is_some());
    }

    #[tokio::test]
    async fn test_aggregate_ratings_and_json_ld() {
        let (service, db) = setup_review_service().await;

        let product_id = "prod_camera";

        for (i, rating) in [5, 4, 3, 5, 5].iter().enumerate() {
            let order_id = format!("ord_cam_{}", i);
            let token = format!("tok_{}", i);
            create_dummy_order(
                &db,
                &order_id,
                Some(&format!("cust_{}", i)),
                &format!("user{}@example.com", i),
                product_id,
                OrderStatus::Paid,
                &token,
            )
            .await;

            let sub = ReviewSubmission {
                order_id,
                rating: *rating,
                title: format!("Review {}", i),
                body: "Content test".to_string(),
                author_name: None,
                access_token: None,
            };
            let author = ReviewAuthor::Customer {
                customer_id: format!("cust_{}", i),
                email: format!("user{}@example.com", i),
                name: format!("User {}", i),
            };
            service
                .submit_review(product_id, sub, author, None, None)
                .await
                .unwrap();
        }

        let (items, stats) = service
            .list_public_reviews(product_id, None, 0)
            .await
            .unwrap();
        assert_eq!(items.len(), 5);
        assert_eq!(stats.review_count, 5);
        // (5 + 4 + 3 + 5 + 5) / 5 = 22 / 5 = 4.4
        assert!((stats.average_rating - 4.4).abs() < 0.01);
        assert_eq!(stats.rating_5, 3);
        assert_eq!(stats.rating_4, 1);
        assert_eq!(stats.rating_3, 1);

        // JSON-LD AggregateRating
        let json_ld = JsonLdAggregateRating::from_stats(&stats).expect("Should have JSON-LD");
        assert_eq!(json_ld.schema_type, "AggregateRating");
        assert_eq!(json_ld.review_count, 5);
        assert!((json_ld.rating_value - 4.4).abs() < 0.01);
    }
}
