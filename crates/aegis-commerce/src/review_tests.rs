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
        member_id: Option<&str>,
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
            customer_name: "Member Name".to_string(),
            payment_provider: "stripe".to_string(),
            access_token_hash: hash_token(access_token),
            created_at: "2026-10-25T10:00:00Z".to_string(),
            updated_at: "2026-10-25T10:00:00Z".to_string(),
            paid_at: Some("2026-10-25T10:05:00Z".to_string()),
            member_id: member_id.map(str::to_string),
            ..Default::default()
        };

        db.create_order(&order, &[]).await.unwrap().unwrap();
    }

    #[tokio::test]
    async fn test_verified_purchase_and_single_review_per_order() {
        let (service, db) = setup_review_service().await;

        let member_id = "cust_123";
        let product_id = "prod_keyboard";
        let order_id = "ord_001";
        let token = "tok_secret_123";

        // 支払い済み注文を作成
        create_dummy_order(
            &db,
            order_id,
            Some(member_id),
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
        let author = ReviewAuthor::Member {
            member_id: member_id.to_string(),
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

        let member_id = "cust_hacker";
        let product_id = "prod_laptop";
        let order_id = "ord_unpaid";
        let token = "tok_abc";

        // 未払いの注文
        create_dummy_order(
            &db,
            order_id,
            Some(member_id),
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
        let author = ReviewAuthor::Member {
            member_id: member_id.to_string(),
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
        let author = ReviewAuthor::Member {
            member_id: "cust_spk".to_string(),
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
            let author = ReviewAuthor::Member {
                member_id: format!("cust_{}", i),
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

    // --- 入力・権限・通知・管理操作 ---

    use aegis_comments::PolicyOverrides;
    use aegis_core::{AegisError, NotificationEvent, Notifier};
    use std::sync::Mutex;

    #[derive(Default)]
    struct Recording(Mutex<Vec<NotificationEvent>>);

    impl Notifier for Recording {
        fn notify(&self, event: NotificationEvent) {
            self.0.lock().unwrap().push(event);
        }
    }

    fn submission(order_id: &str, rating: i64, title: &str, body: &str) -> ReviewSubmission {
        ReviewSubmission {
            order_id: order_id.to_string(),
            rating,
            title: title.to_string(),
            body: body.to_string(),
            author_name: None,
            access_token: None,
        }
    }

    fn member(member_id: &str, email: &str) -> ReviewAuthor {
        ReviewAuthor::Member {
            member_id: member_id.to_string(),
            email: email.to_string(),
            name: "Alice".to_string(),
        }
    }

    /// 支払い済みの注文を作る（持ち主は会員 `m1`、メールは user@example.com、トークンは `tok`）
    async fn paid_order(db: &Arc<dyn DatabaseAdapter>, order_id: &str, product_id: &str) {
        create_dummy_order(
            db,
            order_id,
            Some("m1"),
            "user@example.com",
            product_id,
            OrderStatus::Paid,
            "tok",
        )
        .await;
    }

    #[tokio::test]
    async fn test_review_input_is_validated_before_anything_is_looked_up() {
        let (service, _db) = setup_review_service().await;
        let long_title = "t".repeat(201);
        let long_body = "b".repeat(5001);
        let cases = [
            (submission("o", 0, "Title", "Body"), "Rating"),
            (submission("o", 6, "Title", "Body"), "Rating"),
            (submission("o", 3, "   ", "Body"), "Title is required"),
            (submission("o", 3, &long_title, "Body"), "at most 200"),
            (submission("o", 3, "Title", "  "), "body is required"),
            (submission("o", 3, "Title", &long_body), "at most 5,000"),
        ];
        for (sub, expected) in cases {
            let err = service
                .submit_review("p1", sub, member("m1", "user@example.com"), None, None)
                .await
                .unwrap_err();
            assert!(
                matches!(&err, AegisError::Validation(m) if m.contains(expected)),
                "{expected}: {err:?}"
            );
        }
        // 長さは文字数で数える（200 文字ちょうどの日本語は通る）
        let (service, db) = setup_review_service().await;
        paid_order(&db, "o1", "p1").await;
        let title = "あ".repeat(200);
        service
            .submit_review(
                "p1",
                submission("o1", 5, &title, "Body"),
                member("m1", "user@example.com"),
                None,
                None,
            )
            .await
            .expect("200 characters is allowed");
    }

    #[tokio::test]
    async fn test_review_is_rejected_for_the_wrong_order_product_or_person() {
        let (service, db) = setup_review_service().await;
        paid_order(&db, "o1", "p1").await;
        create_dummy_order(
            &db,
            "o2",
            Some("m1"),
            "user@example.com",
            "p1",
            OrderStatus::PendingPayment,
            "tok",
        )
        .await;

        // 注文がない
        let err = service
            .submit_review(
                "p1",
                submission("nope", 5, "T", "B"),
                member("m1", "user@example.com"),
                None,
                None,
            )
            .await
            .unwrap_err();
        assert!(matches!(err, AegisError::NotFound(_)), "{err:?}");

        // 未払いの注文
        let err = service
            .submit_review(
                "p1",
                submission("o2", 5, "T", "B"),
                member("m1", "user@example.com"),
                None,
                None,
            )
            .await
            .unwrap_err();
        assert!(matches!(err, AegisError::PermissionDenied(_)), "{err:?}");

        // 注文に入っていない商品（商品 ID でも slug でも一致しない）
        let err = service
            .submit_review(
                "other-product",
                submission("o1", 5, "T", "B"),
                member("m1", "user@example.com"),
                None,
                None,
            )
            .await
            .unwrap_err();
        assert!(
            matches!(&err, AegisError::PermissionDenied(m) if m.contains("does not contain")),
            "{err:?}"
        );

        // 別の人（会員 ID もメールも違う）
        let err = service
            .submit_review(
                "p1",
                submission("o1", 5, "T", "B"),
                member("someone-else", "other@example.com"),
                None,
                None,
            )
            .await
            .unwrap_err();
        assert!(
            matches!(&err, AegisError::PermissionDenied(m) if m.contains("your own orders")),
            "{err:?}"
        );

        // 会員 ID が違っても、注文のメールと同じなら（大文字小文字を問わず）本人とみなす。
        // 商品は slug でも指せる
        service
            .submit_review(
                "test-product",
                submission("o1", 5, "T", "B"),
                member("another-id", "USER@example.com"),
                None,
                None,
            )
            .await
            .expect("the order's email identifies the buyer; the slug also names the product");
    }

    #[tokio::test]
    async fn test_guest_review_needs_the_right_token_and_falls_back_to_the_order_name() {
        let (service, db) = setup_review_service().await;
        paid_order(&db, "o1", "p1").await;
        let guest = |token: &str, name: &str| ReviewAuthor::GuestWithToken {
            order_id: "o1".to_string(),
            access_token: token.to_string(),
            author_name: name.to_string(),
        };

        let err = service
            .submit_review(
                "p1",
                submission("o1", 4, "T", "B"),
                guest("wrong", "Bob"),
                None,
                None,
            )
            .await
            .unwrap_err();
        assert!(matches!(err, AegisError::Authentication(_)), "{err:?}");

        // 名前が空なら、注文の顧客名を使う
        let review = service
            .submit_review(
                "p1",
                submission("o1", 4, "T", "B"),
                guest("tok", "  "),
                None,
                None,
            )
            .await
            .unwrap();
        assert_eq!(review.author_name, "Member Name");
        assert_eq!(review.member_id, None);
        assert_eq!(review.author_email, "user@example.com");
    }

    #[tokio::test]
    async fn test_staff_are_notified_with_the_review_status() {
        for (require_approval, event, status) in [
            (false, "review.posted", "approved"),
            (true, "review.pending", "pending"),
        ] {
            let config = DatabaseConfig {
                url: "sqlite::memory:".to_string(),
                ..DatabaseConfig::default()
            };
            let db = create_database_adapter(&config).await.unwrap();
            db.init().await.unwrap();
            let notifier = Arc::new(Recording::default());
            let service = ReviewService::new(
                db.clone(),
                Arc::new(AuditService::with_sinks(vec![])),
                Some(notifier.clone()),
                aegis_comments::ModerationConfig {
                    require_approval,
                    ..Default::default()
                },
            );
            paid_order(&db, "o1", "p1").await;
            let review = service
                .submit_review(
                    "p1",
                    submission("o1", 2, "Not for me", "Too small."),
                    member("m1", "user@example.com"),
                    Some("203.0.113.9".to_string()),
                    Some("test-agent".to_string()),
                )
                .await
                .unwrap();
            // 低評価でも、そのことで保留にはしない（承認制の設定だけで決まる）
            assert_eq!(review.status, status);
            assert_eq!(review.ip.as_deref(), Some("203.0.113.9"));

            let events = notifier.0.lock().unwrap();
            assert_eq!(events.len(), 1, "{event}");
            assert_eq!(events[0].kind, event);
            assert_eq!(events[0].data["status"], status);
            assert_eq!(events[0].data["rating"], 2);
            assert_eq!(events[0].data["review_id"], review.id);
        }
    }

    #[tokio::test]
    async fn test_admin_status_reply_and_bulk_operations() {
        let (service, db) = setup_review_service().await;
        let mut ids = Vec::new();
        for n in 1..=2 {
            let order = format!("o{n}");
            paid_order(&db, &order, "p1").await;
            let review = service
                .submit_review(
                    "p1",
                    submission(&order, 5, "Great", "Works."),
                    member("m1", "user@example.com"),
                    None,
                    None,
                )
                .await
                .unwrap();
            ids.push(review.id);
        }

        // 状態の名前と、存在するレビューか
        let err = service
            .admin_set_status(&ids[0], "published", None)
            .await
            .unwrap_err();
        assert!(matches!(&err, AegisError::Validation(m) if m.contains("Invalid status")));
        let err = service
            .admin_set_status("missing", "spam", None)
            .await
            .unwrap_err();
        assert!(matches!(err, AegisError::NotFound(_)), "{err:?}");
        let err = service.admin_get_review("missing").await.unwrap_err();
        assert!(matches!(err, AegisError::NotFound(_)), "{err:?}");

        // 返信は、付けても外せる
        service
            .admin_set_reply(&ids[0], Some("Thank you!"))
            .await
            .unwrap();
        let replied = service.admin_get_review(&ids[0]).await.unwrap();
        assert_eq!(replied.reply.as_deref(), Some("Thank you!"));
        assert!(replied.replied_at.is_some());
        service.admin_set_reply(&ids[0], None).await.unwrap();
        let cleared = service.admin_get_review(&ids[0]).await.unwrap();
        assert_eq!(cleared.reply, None);
        assert_eq!(cleared.replied_at, None);

        // 一括で状態を変えると、公開の一覧から消える
        let (public, _) = service.list_public_reviews("p1", None, 0).await.unwrap();
        assert_eq!(public.len(), 2);
        let changed = service
            .admin_bulk_status(&ids, "trash", Some("Off topic"))
            .await
            .unwrap();
        assert_eq!(changed, 2);
        let (public, stats) = service.list_public_reviews("p1", None, 0).await.unwrap();
        assert!(public.is_empty());
        assert_eq!(stats.review_count, 0);
        let trashed = service.admin_get_review(&ids[1]).await.unwrap();
        assert_eq!(trashed.rejection_reason.as_deref(), Some("Off topic"));

        // 管理者の一覧は、状態を問わず見える。一括削除で消える
        let page = service
            .admin_list_reviews(aegis_db::ReviewListQuery::default())
            .await
            .unwrap();
        assert_eq!(page.reviews.len(), 2);
        assert_eq!(service.admin_bulk_delete(&ids).await.unwrap(), 2);
        let err = service.admin_get_review(&ids[0]).await.unwrap_err();
        assert!(matches!(err, AegisError::NotFound(_)));
    }

    #[tokio::test]
    async fn test_review_settings_override_the_default_policy_and_can_be_reset() {
        let (service, db) = setup_review_service().await;
        for order in ["o1", "o2", "o3"] {
            paid_order(&db, order, "p1").await;
        }
        let post = |order: &'static str| {
            let service = &service;
            async move {
                service
                    .submit_review(
                        "p1",
                        submission(order, 5, "T", "B"),
                        member("m1", "user@example.com"),
                        None,
                        None,
                    )
                    .await
                    .unwrap()
            }
        };

        // 初期値: 設定なし。テストの設定は承認制でないので、承認される
        assert_eq!(
            service.admin_get_settings().await.unwrap(),
            PolicyOverrides::default()
        );
        assert_eq!(post("o1").await.status, "approved");

        // 設定で承認制にすると、保留になる
        let saved = service
            .admin_update_settings(PolicyOverrides {
                require_approval: Some(true),
                auto_approve_known: None,
            })
            .await
            .unwrap();
        assert_eq!(saved.require_approval, Some(true));
        assert_eq!(post("o2").await.status, "pending");

        // 設定を外すと、元の設定（承認制でない）に戻る
        let reset = service
            .admin_update_settings(PolicyOverrides::default())
            .await
            .unwrap();
        assert_eq!(reset, PolicyOverrides::default());
        assert_eq!(post("o3").await.status, "approved");
    }

    #[tokio::test]
    async fn test_member_and_public_review_lists() {
        let (service, db) = setup_review_service().await;
        for n in 1..=3 {
            let order = format!("o{n}");
            paid_order(&db, &order, "p1").await;
            service
                .submit_review(
                    "p1",
                    submission(&order, n, &format!("Review {n}"), "Body"),
                    member("m1", "user@example.com"),
                    None,
                    None,
                )
                .await
                .unwrap();
        }

        // 会員の一覧は、自分の分だけ。ほかの会員は空
        assert_eq!(service.list_member_reviews("m1").await.unwrap().len(), 3);
        assert!(service.list_member_reviews("m2").await.unwrap().is_empty());

        // 公開の一覧は、件数と位置で区切れる。集計は全件
        let (first, stats) = service.list_public_reviews("p1", Some(2), 0).await.unwrap();
        assert_eq!(first.len(), 2);
        assert_eq!(stats.review_count, 3);
        let (rest, _) = service.list_public_reviews("p1", Some(2), 2).await.unwrap();
        assert_eq!(rest.len(), 1);
        let (none, stats) = service.list_public_reviews("other", None, 0).await.unwrap();
        assert!(none.is_empty());
        assert_eq!(stats.review_count, 0);
    }
}
