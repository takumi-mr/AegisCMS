//! 退会のときのショップのデータの後始末（`member_data`）のテスト。会員の行を消すのは `aegis-members`

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use aegis_audit::AuditService;
    use aegis_core::AegisError;
    use aegis_db::{
        create_database_adapter, DatabaseAdapter, DatabaseConfig, OrderRecord, ReviewListQuery,
        ReviewRecord,
    };

    use aegis_members::{MemberLoginRequest, MemberRegisterRequest, MemberService};

    use crate::member_data::{get_member_order, release_member};

    async fn setup_service() -> (MemberService, Arc<dyn DatabaseAdapter>) {
        let config = DatabaseConfig {
            url: "sqlite::memory:".to_string(),
            ..DatabaseConfig::default()
        };
        let db = create_database_adapter(&config).await.unwrap();
        db.init().await.unwrap();
        let service = MemberService::new(
            db.clone(),
            Arc::new(aegis_cache::MemoryCacheBackend::new()),
            Arc::new(AuditService::with_sinks(vec![])),
        );
        (service, db)
    }

    async fn sign_up(service: &MemberService, email: &str, password: &str) -> String {
        service
            .register(
                MemberRegisterRequest {
                    email: email.to_string(),
                    password: password.to_string(),
                    name: "Hanako".to_string(),
                    phone: None,
                    shipping_address: None,
                },
                None,
                None,
            )
            .await
            .unwrap()
            .id
    }

    async fn add_order(db: &Arc<dyn DatabaseAdapter>, id: &str, member_id: &str, email: &str) {
        let order = OrderRecord {
            id: id.to_string(),
            number: format!("ORD-{id}"),
            status: "paid".to_string(),
            currency: "JPY".to_string(),
            subtotal: 1000,
            total: 1000,
            lines_json: "[]".to_string(),
            adjustments_json: "[]".to_string(),
            customer_email: email.to_string(),
            customer_name: "Hanako Yamada".to_string(),
            payment_provider: "manual".to_string(),
            access_token_hash: "hash".to_string(),
            created_at: "2026-10-25T10:00:00Z".to_string(),
            updated_at: "2026-10-25T10:00:00Z".to_string(),
            member_id: Some(member_id.to_string()),
            ..Default::default()
        };
        db.create_order(&order, &[]).await.unwrap().unwrap();
    }

    async fn add_review(
        db: &Arc<dyn DatabaseAdapter>,
        id: &str,
        product_id: &str,
        member_id: &str,
        name: &str,
    ) {
        db.insert_review(&ReviewRecord {
            id: id.to_string(),
            product_id: product_id.to_string(),
            order_id: format!("order-{id}"),
            member_id: Some(member_id.to_string()),
            author_name: name.to_string(),
            author_email: format!("{name}@example.com"),
            rating: 5,
            title: "Great".to_string(),
            body: "Really good beans".to_string(),
            status: "approved".to_string(),
            verified_purchase: 1,
            reply: Some("Thank you".to_string()),
            replied_at: Some("2026-10-26T00:00:00Z".to_string()),
            rejection_reason: None,
            ip: Some("203.0.113.1".to_string()),
            user_agent: Some("Mozilla/5.0".to_string()),
            created_at: "2026-10-25T12:00:00Z".to_string(),
            updated_at: "2026-10-25T12:00:00Z".to_string(),
        })
        .await
        .unwrap();
    }

    async fn review(db: &Arc<dyn DatabaseAdapter>, id: &str) -> ReviewRecord {
        db.get_review(id).await.unwrap().expect("the review stays")
    }

    /// 退会: 本人であることをパスワードで確かめ、会員とセッションは消し、注文とレビューは残して紐づきを外す
    #[tokio::test]
    async fn test_delete_account_keeps_orders_and_reviews_without_the_person() {
        let (service, db) = setup_service().await;
        let me = sign_up(&service, "me@example.com", "password123").await;
        let other = sign_up(&service, "other@example.com", "password123").await;
        add_order(&db, "o-me", &me, "me@example.com").await;
        add_order(&db, "o-other", &other, "other@example.com").await;
        add_review(&db, "r-me-1", "beans", &me, "Hanako").await;
        add_review(&db, "r-me-2", "mug", &me, "Hanako").await;
        add_review(&db, "r-me-3", "mug", &me, "Hanako").await;
        add_review(&db, "r-other", "beans", &other, "Taro").await;

        let (session, _) = service
            .login(
                MemberLoginRequest {
                    email: "me@example.com".to_string(),
                    password: "password123".to_string(),
                },
                None,
                None,
            )
            .await
            .unwrap();

        // パスワードが違うと、何も変わらない（本人の確認が先。後始末も会員の削除もしない）
        let wrong = service.confirm_password(&me, "wrong-password-1").await;
        assert!(matches!(wrong, Err(AegisError::Authentication(_))));
        assert!(db.get_member_by_id(&me).await.unwrap().is_some());
        assert!(service
            .verify_session(&session.token)
            .await
            .unwrap()
            .is_some());
        assert_eq!(db.list_member_orders(&me).await.unwrap().len(), 1);
        assert_eq!(review(&db, "r-me-1").await.author_name, "Hanako");

        // 退会: レビューを匿名にした商品の ID（重複なし）を返す
        service.confirm_password(&me, "password123").await.unwrap();
        let products = release_member(&*db, &me).await.unwrap();
        assert_eq!(products, vec!["beans".to_string(), "mug".to_string()]);
        service.delete_account(&me, None, None).await.unwrap();

        // 会員・ログイン・セッションは無い
        assert!(db.get_member_by_id(&me).await.unwrap().is_none());
        assert!(db
            .get_member_by_email("me@example.com")
            .await
            .unwrap()
            .is_none());
        assert!(service
            .verify_session(&session.token)
            .await
            .unwrap()
            .is_none());
        assert!(service
            .login(
                MemberLoginRequest {
                    email: "me@example.com".to_string(),
                    password: "password123".to_string(),
                },
                None,
                None,
            )
            .await
            .is_err());

        // 注文は会計の記録として残る。会員との紐づきだけが外れ、注文時のメール・氏名は残る
        assert!(db.list_member_orders(&me).await.unwrap().is_empty());
        let kept = db
            .get_order("o-me")
            .await
            .unwrap()
            .expect("the order stays");
        assert_eq!(kept.member_id, None);
        assert_eq!(kept.customer_email, "me@example.com");
        assert_eq!(kept.customer_name, "Hanako Yamada");
        assert_eq!(kept.total, 1000);

        // レビューは残り、人に結びつく情報だけが消える
        for id in ["r-me-1", "r-me-2", "r-me-3"] {
            let r = review(&db, id).await;
            assert_eq!(r.member_id, None, "{id}");
            assert_eq!(r.author_name, "", "{id}");
            assert_eq!(r.author_email, "", "{id}");
            assert_eq!(r.ip, None, "{id}");
            assert_eq!(r.user_agent, None, "{id}");
            assert_eq!(r.body, "Really good beans", "{id}");
            assert_eq!(r.rating, 5, "{id}");
            assert_eq!(r.reply.as_deref(), Some("Thank you"), "{id}");
            assert_eq!(r.status, "approved", "{id}");
        }

        // ほかの会員のものは、そのまま
        assert_eq!(db.list_member_orders(&other).await.unwrap().len(), 1);
        let untouched = review(&db, "r-other").await;
        assert_eq!(untouched.member_id.as_deref(), Some(other.as_str()));
        assert_eq!(untouched.author_name, "Taro");
        assert_eq!(untouched.ip.as_deref(), Some("203.0.113.1"));
        // 公開の一覧には、匿名にしたレビューもそのまま出る（星の集計も変わらない）
        let page = db
            .list_reviews(&ReviewListQuery {
                product_id: Some("beans".to_string()),
                status: Some("approved".to_string()),
                member_id: None,
                oldest_first: false,
                limit: None,
                offset: 0,
            })
            .await
            .unwrap();
        assert_eq!(page.reviews.len(), 2);
        assert_eq!(
            db.get_product_review_stats("beans")
                .await
                .unwrap()
                .review_count,
            2
        );

        // 他人の注文は見えない（会員の ID もメールアドレスも違う）
        assert!(matches!(
            get_member_order(&*db, &other, "o-me").await,
            Err(AegisError::PermissionDenied(_))
        ));
        assert!(get_member_order(&*db, &other, "o-other").await.is_ok());

        // 同じメールアドレスで、あらためて登録できる（新しい会員。過去の注文は結びつかない）
        let again = sign_up(&service, "me@example.com", "password456").await;
        assert_ne!(again, me);
        assert!(db.list_member_orders(&again).await.unwrap().is_empty());
    }
}
