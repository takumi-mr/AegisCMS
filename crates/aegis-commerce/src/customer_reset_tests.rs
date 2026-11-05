//! パスワードの再設定と、セッションの失効のテスト

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use aegis_audit::AuditService;
    use aegis_core::AegisError;
    use aegis_db::{create_database_adapter, DatabaseAdapter, DatabaseConfig};

    use crate::customer::{
        CustomerLoginRequest, CustomerRegisterRequest, CustomerService,
        CustomerUpdateProfileRequest,
    };

    async fn setup_service() -> (CustomerService, Arc<dyn DatabaseAdapter>) {
        let config = DatabaseConfig {
            url: "sqlite::memory:".to_string(),
            ..DatabaseConfig::default()
        };
        let db = create_database_adapter(&config).await.unwrap();
        db.init().await.unwrap();
        let service = CustomerService::new(
            db.clone(),
            Arc::new(aegis_cache::MemoryCacheBackend::new()),
            Arc::new(AuditService::with_sinks(vec![])),
        );
        (service, db)
    }

    fn register_request(email: &str) -> CustomerRegisterRequest {
        CustomerRegisterRequest {
            email: email.to_string(),
            password: "password123".to_string(),
            name: "Hanako".to_string(),
            phone: None,
            shipping_address: None,
        }
    }

    async fn log_in(service: &CustomerService, email: &str, password: &str) -> String {
        let (session, _) = service
            .login(
                CustomerLoginRequest {
                    email: email.to_string(),
                    password: password.to_string(),
                },
                None,
                None,
            )
            .await
            .unwrap();
        session.token
    }

    /// 再設定: メールのトークンで新しいパスワードを設定でき、古いパスワードでは入れなくなる。トークンは 1 回だけ
    #[tokio::test]
    async fn test_password_reset_flow() {
        let (service, _db) = setup_service().await;
        service
            .register(register_request("reset@example.com"), None, None)
            .await
            .unwrap();

        let requested = service
            .request_password_reset("Reset@Example.com", None)
            .await
            .unwrap()
            .expect("a customer with that email");
        assert_eq!(requested.email, "reset@example.com");
        assert_eq!(requested.token.len(), 64);

        // 短いパスワードは拒否する
        let short = service
            .complete_password_reset(&requested.token, "short", None, None)
            .await;
        assert!(matches!(short, Err(AegisError::Validation(_))));

        service
            .complete_password_reset(&requested.token, "brand-new-password", None, None)
            .await
            .unwrap();

        // 新しいパスワードで入れて、古いパスワードでは入れない
        log_in(&service, "reset@example.com", "brand-new-password").await;
        let old = service
            .login(
                CustomerLoginRequest {
                    email: "reset@example.com".to_string(),
                    password: "password123".to_string(),
                },
                None,
                None,
            )
            .await;
        assert!(old.is_err());

        // 同じトークンは 2 回使えない。作り物のトークンも通らない
        let again = service
            .complete_password_reset(&requested.token, "another-password-1", None, None)
            .await;
        assert!(matches!(again, Err(AegisError::Validation(_))));
        let forged = service
            .complete_password_reset(&"0".repeat(64), "another-password-1", None, None)
            .await;
        assert!(forged.is_err());
    }

    /// 登録の有無を教えない: 無い・停止中は `None`、メールの形が誤りなら検証エラー。IP と宛先で数を制限する
    #[tokio::test]
    async fn test_password_reset_request_does_not_reveal_accounts_and_is_limited() {
        let (service, db) = setup_service().await;
        service
            .register(register_request("known@example.com"), None, None)
            .await
            .unwrap();

        assert!(service
            .request_password_reset("nobody@example.com", None)
            .await
            .unwrap()
            .is_none());
        assert!(matches!(
            service.request_password_reset("not-an-email", None).await,
            Err(AegisError::Validation(_))
        ));

        // 停止中の顧客には送らない
        let id = db
            .get_customer_by_email("known@example.com")
            .await
            .unwrap()
            .unwrap()
            .id;
        service.admin_set_status(&id, "suspended").await.unwrap();
        assert!(service
            .request_password_reset("known@example.com", None)
            .await
            .unwrap()
            .is_none());
        service.admin_set_status(&id, "active").await.unwrap();

        // 同じ宛先へのメールは 1 時間に 3 通まで（4 通目からは送らない。応答は同じ）
        for _ in 0..3 {
            assert!(service
                .request_password_reset("known@example.com", None)
                .await
                .unwrap()
                .is_some());
        }
        assert!(service
            .request_password_reset("known@example.com", None)
            .await
            .unwrap()
            .is_none());

        // 同じ IP からの申し込みは、1 時間に 5 回まで（登録の有無によらず数える）
        let (limited, _) = setup_service().await;
        for n in 0..5 {
            limited
                .request_password_reset(&format!("user{n}@example.com"), Some("203.0.113.9"))
                .await
                .unwrap();
        }
        let sixth = limited
            .request_password_reset("user6@example.com", Some("203.0.113.9"))
            .await;
        assert!(matches!(sixth, Err(AegisError::ResourceLimitExceeded(_))));
        // 別の IP は影響を受けない
        assert!(limited
            .request_password_reset("user7@example.com", Some("203.0.113.10"))
            .await
            .is_ok());
    }

    /// 停止・パスワードの変更・再設定で、ログイン中のセッションが終わる。あとで入ったセッションは使える
    #[tokio::test]
    async fn test_sessions_end_on_suspension_password_change_and_reset() {
        let (service, _db) = setup_service().await;
        let registered = service
            .register(register_request("sess@example.com"), None, None)
            .await
            .unwrap();

        // 停止: ログイン中のセッションが、その場で使えなくなる。再開しても戻らない
        let token = log_in(&service, "sess@example.com", "password123").await;
        assert!(service.verify_session(&token).await.unwrap().is_some());
        service
            .admin_set_status(&registered.id, "suspended")
            .await
            .unwrap();
        assert!(service.verify_session(&token).await.unwrap().is_none());
        service
            .admin_set_status(&registered.id, "active")
            .await
            .unwrap();
        assert!(
            service.verify_session(&token).await.unwrap().is_none(),
            "a session from before the suspension stays ended"
        );
        let fresh = log_in(&service, "sess@example.com", "password123").await;
        assert!(service.verify_session(&fresh).await.unwrap().is_some());

        // パスワードの変更: ほかのセッションは終わり、作り直した本人の分は使える
        let other = log_in(&service, "sess@example.com", "password123").await;
        service
            .update_profile(
                &registered.id,
                CustomerUpdateProfileRequest {
                    name: None,
                    phone: None,
                    shipping_address: None,
                    current_password: Some("password123".to_string()),
                    new_password: Some("changed-password-1".to_string()),
                },
            )
            .await
            .unwrap();
        assert!(service.verify_session(&other).await.unwrap().is_none());
        assert!(service.verify_session(&fresh).await.unwrap().is_none());
        let reissued = service.reissue_session(&registered.id).await.unwrap();
        assert!(service
            .verify_session(&reissued.token)
            .await
            .unwrap()
            .is_some());

        // 再設定の完了: すべてのセッションが終わる
        let requested = service
            .request_password_reset("sess@example.com", None)
            .await
            .unwrap()
            .unwrap();
        service
            .complete_password_reset(&requested.token, "reset-password-1", None, None)
            .await
            .unwrap();
        assert!(service
            .verify_session(&reissued.token)
            .await
            .unwrap()
            .is_none());
        let after = log_in(&service, "sess@example.com", "reset-password-1").await;
        assert!(service.verify_session(&after).await.unwrap().is_some());

        // 停止中の顧客に、セッションを作り直さない
        service
            .admin_set_status(&registered.id, "suspended")
            .await
            .unwrap();
        assert!(service.reissue_session(&registered.id).await.is_err());
    }
}
