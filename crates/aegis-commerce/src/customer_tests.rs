#[cfg(test)]
mod tests {
    use crate::customer::{
        CustomerLoginRequest, CustomerRegisterRequest, CustomerService,
        CustomerUpdateProfileRequest,
    };
    use aegis_audit::AuditService;
    use aegis_core::ShippingAddress;
    use aegis_db::{create_database_adapter, DatabaseAdapter, DatabaseConfig};
    use std::sync::Arc;

    async fn setup_service() -> (CustomerService, Arc<dyn DatabaseAdapter>) {
        let config = DatabaseConfig {
            url: "sqlite::memory:".to_string(),
            ..DatabaseConfig::default()
        };
        let db = create_database_adapter(&config).await.unwrap();
        db.init().await.unwrap();

        let cache = Arc::new(aegis_cache::MemoryCacheBackend::new());
        let audit = Arc::new(AuditService::with_sinks(vec![]));

        let service = CustomerService::new(db.clone(), cache, audit);
        (service, db)
    }

    #[tokio::test]
    async fn test_customer_registration_and_login() {
        let (service, _db) = setup_service().await;

        let reg_req = CustomerRegisterRequest {
            email: "alice@example.com".to_string(),
            password: "password123".to_string(),
            name: "Alice Wonderland".to_string(),
            phone: Some("090-1234-5678".to_string()),
            shipping_address: Some(ShippingAddress {
                name: "Alice Wonderland".to_string(),
                postal_code: "100-0001".to_string(),
                region: "Tokyo".to_string(),
                city: "Chiyoda".to_string(),
                line1: "1-1".to_string(),
                line2: String::new(),
                phone: "090-1234-5678".to_string(),
                country: "JP".to_string(),
            }),
        };

        // 1. 登録
        let customer = service
            .register(reg_req, Some("127.0.0.1".into()), None)
            .await
            .expect("Registration should succeed");

        assert_eq!(customer.email, "alice@example.com");
        assert_eq!(customer.name, "Alice Wonderland");
        assert_eq!(customer.status, "active");

        // 重複登録の拒否
        let duplicate_res = service
            .register(
                CustomerRegisterRequest {
                    email: "ALICE@example.com".to_string(), // 大文字小文字同一視
                    password: "anotherpassword".to_string(),
                    name: "Alice Twin".to_string(),
                    phone: None,
                    shipping_address: None,
                },
                None,
                None,
            )
            .await;
        assert!(duplicate_res.is_err(), "Duplicate email should be rejected");

        // 2. ログイン
        let (session, profile) = service
            .login(
                CustomerLoginRequest {
                    email: "alice@example.com".to_string(),
                    password: "password123".to_string(),
                },
                Some("127.0.0.1".into()),
                None,
            )
            .await
            .expect("Login should succeed");

        assert_eq!(profile.id, customer.id);
        assert!(session.token.starts_with("cses_"));

        // 3. セッション検証
        let verified = service
            .verify_session(&session.token)
            .await
            .unwrap()
            .expect("Session should be valid");
        assert_eq!(verified.customer_id, customer.id);

        // 4. 不正パスワードでのログイン失敗
        let bad_login = service
            .login(
                CustomerLoginRequest {
                    email: "alice@example.com".to_string(),
                    password: "wrongpassword".to_string(),
                },
                None,
                None,
            )
            .await;
        assert!(bad_login.is_err(), "Wrong password should fail");

        // 5. ログアウト
        service.logout(&session.token).await.unwrap();
        let expired = service.verify_session(&session.token).await.unwrap();
        assert!(
            expired.is_none(),
            "Session should be invalidated after logout"
        );
    }

    #[tokio::test]
    async fn test_profile_update_and_password_change() {
        let (service, _db) = setup_service().await;

        let customer = service
            .register(
                CustomerRegisterRequest {
                    email: "bob@example.com".to_string(),
                    password: "oldpassword".to_string(),
                    name: "Bob".to_string(),
                    phone: None,
                    shipping_address: None,
                },
                None,
                None,
            )
            .await
            .unwrap();

        // プロフィール更新（名前とパスワード変更）
        let update_req = CustomerUpdateProfileRequest {
            name: Some("Bob Builder".to_string()),
            phone: Some("080-9999-8888".to_string()),
            shipping_address: None,
            current_password: Some("oldpassword".to_string()),
            new_password: Some("newpassword123".to_string()),
        };

        let updated = service
            .update_profile(&customer.id, update_req)
            .await
            .expect("Profile update should succeed");
        assert_eq!(updated.name, "Bob Builder");
        assert_eq!(updated.phone.as_deref(), Some("080-9999-8888"));

        // 旧パスワードでログイン失敗
        let failed_login = service
            .login(
                CustomerLoginRequest {
                    email: "bob@example.com".to_string(),
                    password: "oldpassword".to_string(),
                },
                None,
                None,
            )
            .await;
        assert!(failed_login.is_err(), "Old password should not work");

        // 新パスワードでログイン成功
        let success_login = service
            .login(
                CustomerLoginRequest {
                    email: "bob@example.com".to_string(),
                    password: "newpassword123".to_string(),
                },
                None,
                None,
            )
            .await;
        assert!(success_login.is_ok(), "New password should work");
    }

    #[tokio::test]
    async fn test_admin_customer_management() {
        let (service, _db) = setup_service().await;

        let customer = service
            .register(
                CustomerRegisterRequest {
                    email: "charlie@example.com".to_string(),
                    password: "password".to_string(),
                    name: "Charlie".to_string(),
                    phone: None,
                    shipping_address: None,
                },
                None,
                None,
            )
            .await
            .unwrap();

        // 管理者: 一覧取得
        let (list, total) = service.admin_list_customers(10, 0).await.unwrap();
        assert_eq!(total, 1);
        assert_eq!(list[0].id, customer.id);

        // 管理者: アカウント停止 (suspend)
        service
            .admin_set_status(&customer.id, "suspended")
            .await
            .unwrap();

        let detail = service.admin_get_customer(&customer.id).await.unwrap();
        assert_eq!(detail.status, "suspended");

        // 停止中アカウントはログイン不可
        let login_res = service
            .login(
                CustomerLoginRequest {
                    email: "charlie@example.com".to_string(),
                    password: "password".to_string(),
                },
                None,
                None,
            )
            .await;
        assert!(login_res.is_err(), "Suspended account cannot log in");
    }
}
