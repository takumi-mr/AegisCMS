//! メールアドレスの確認のテスト

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use aegis_audit::AuditService;
    use aegis_core::AegisError;
    use aegis_db::{create_database_adapter, DatabaseAdapter, DatabaseConfig};

    use crate::member::{
        MemberLoginRequest, MemberRegisterRequest, MemberService, EMAIL_NOT_VERIFIED_MESSAGE,
    };

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
        )
        .with_email_verification(true);
        (service, db)
    }

    fn register_request(email: &str, password: &str) -> MemberRegisterRequest {
        MemberRegisterRequest {
            email: email.to_string(),
            password: password.to_string(),
            name: "Hanako".to_string(),
            phone: None,
            shipping_address: None,
        }
    }

    fn login_request(email: &str, password: &str) -> MemberLoginRequest {
        MemberLoginRequest {
            email: email.to_string(),
            password: password.to_string(),
        }
    }

    /// 確認が要るとき、登録した会員は確認が済むまでログインできない。確認すると入れる。リンクは 1 回だけ
    #[tokio::test]
    async fn test_a_registration_waits_for_the_confirmation() {
        let (service, db) = setup_service().await;
        // 確認を飛ばして有効な会員を作る道は無い
        assert!(service
            .register(register_request("a@example.com", "password123"), None, None)
            .await
            .is_err());

        let verification = service
            .register_pending(register_request("A@Example.com", "password123"), None, None)
            .await
            .unwrap()
            .expect("a new address gets a confirmation email");
        assert_eq!(verification.email, "a@example.com");
        assert_eq!(verification.token.len(), 64);
        let stored = db
            .get_member_by_email("a@example.com")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(stored.status, "unverified");

        // パスワードが合っていても、確認が済むまで入れない。合っていないときは、確認待ちかどうかを教えない
        let waiting = service
            .login(login_request("a@example.com", "password123"), None, None)
            .await;
        assert!(
            matches!(&waiting, Err(AegisError::PermissionDenied(m)) if m == EMAIL_NOT_VERIFIED_MESSAGE)
        );
        let wrong = service
            .login(
                login_request("a@example.com", "wrong-password-1"),
                None,
                None,
            )
            .await;
        assert!(matches!(wrong, Err(AegisError::Authentication(_))));

        let confirmed = service
            .verify_email(&verification.token, None, None)
            .await
            .unwrap();
        assert_eq!(confirmed.status, "active");
        assert!(service
            .login(login_request("a@example.com", "password123"), None, None)
            .await
            .is_ok());

        // リンクは 1 回だけ。作り物も通らない
        assert!(matches!(
            service.verify_email(&verification.token, None, None).await,
            Err(AegisError::Validation(_))
        ));
        assert!(service
            .verify_email(&"0".repeat(64), None, None)
            .await
            .is_err());
    }

    /// 登録済みのアドレスは `None`（応答で区別させない）。確認待ちのアドレスでの登録し直しは、
    /// 名前とパスワードを今の登録のもので置き換える（他人のアドレスで先に登録されても、本人のものになる）
    #[tokio::test]
    async fn test_registering_again_takes_over_an_unconfirmed_address_only() {
        let (service, db) = setup_service().await;

        // 他人が先に、自分のパスワードで登録した
        service
            .register_pending(
                register_request("victim@example.com", "attacker-password"),
                None,
                None,
            )
            .await
            .unwrap();
        // 本人が登録し直し、自分のパスワードで確認した
        let owners = service
            .register_pending(
                register_request("victim@example.com", "owners-password-1"),
                None,
                None,
            )
            .await
            .unwrap()
            .unwrap();
        service
            .verify_email(&owners.token, None, None)
            .await
            .unwrap();
        assert!(service
            .login(
                login_request("victim@example.com", "owners-password-1"),
                None,
                None
            )
            .await
            .is_ok());
        assert!(service
            .login(
                login_request("victim@example.com", "attacker-password"),
                None,
                None
            )
            .await
            .is_err());

        // 有効になったアドレスでの登録は、何も変えず、メールも送らない
        let taken = service
            .register_pending(
                register_request("victim@example.com", "someone-else-1"),
                None,
                None,
            )
            .await
            .unwrap();
        assert!(taken.is_none());
        assert!(service
            .login(
                login_request("victim@example.com", "owners-password-1"),
                None,
                None
            )
            .await
            .is_ok());

        // 停止中のアドレスも同じ
        let id = db
            .get_member_by_email("victim@example.com")
            .await
            .unwrap()
            .unwrap()
            .id;
        service.admin_set_status(&id, "suspended").await.unwrap();
        assert!(service
            .register_pending(
                register_request("victim@example.com", "someone-else-1"),
                None,
                None
            )
            .await
            .unwrap()
            .is_none());
    }

    /// 再送: 確認待ちのときだけ送る（応答では区別しない）。宛先ごとの数と、IP ごとの数を制限する
    #[tokio::test]
    async fn test_resend_is_for_waiting_addresses_and_is_limited() {
        let (service, _db) = setup_service().await;
        service
            .register_pending(register_request("w@example.com", "password123"), None, None)
            .await
            .unwrap();

        assert!(service
            .resend_verification("nobody@example.com", None)
            .await
            .unwrap()
            .is_none());
        assert!(matches!(
            service.resend_verification("not-an-email", None).await,
            Err(AegisError::Validation(_))
        ));
        assert!(service
            .resend_verification("w@example.com", None)
            .await
            .unwrap()
            .is_some());

        // 同じ宛先へのメールは 1 時間に 3 通まで（登録の 1 通を含む）
        assert!(service
            .resend_verification("w@example.com", None)
            .await
            .unwrap()
            .is_some());
        assert!(service
            .resend_verification("w@example.com", None)
            .await
            .unwrap()
            .is_none());

        // 確認が済んだアドレスには送らない
        let (other, _) = setup_service().await;
        let v = other
            .register_pending(
                register_request("done@example.com", "password123"),
                None,
                None,
            )
            .await
            .unwrap()
            .unwrap();
        other.verify_email(&v.token, None, None).await.unwrap();
        assert!(other
            .resend_verification("done@example.com", None)
            .await
            .unwrap()
            .is_none());

        // 同じ IP からは 1 時間に 5 回まで（登録の有無によらず数える）
        let (limited, _) = setup_service().await;
        for n in 0..5 {
            limited
                .resend_verification(&format!("u{n}@example.com"), Some("203.0.113.7"))
                .await
                .unwrap();
        }
        assert!(matches!(
            limited
                .resend_verification("u9@example.com", Some("203.0.113.7"))
                .await,
            Err(AegisError::ResourceLimitExceeded(_))
        ));
    }

    /// 再設定のリンクをメールで開けたなら、アドレスの確認にもなる
    #[tokio::test]
    async fn test_a_password_reset_also_confirms_the_address() {
        let (service, db) = setup_service().await;
        service
            .register_pending(register_request("r@example.com", "password123"), None, None)
            .await
            .unwrap();
        // 確認待ちの会員も、再設定を申し込める（パスワードを忘れて、確認のメールも見失った場合）
        let reset = service
            .request_password_reset("r@example.com", None)
            .await
            .unwrap()
            .expect("an unconfirmed member may reset the password");
        service
            .complete_password_reset(&reset.token, "a-new-password-1", None, None)
            .await
            .unwrap();
        assert_eq!(
            db.get_member_by_email("r@example.com")
                .await
                .unwrap()
                .unwrap()
                .status,
            "active"
        );
        assert!(service
            .login(
                login_request("r@example.com", "a-new-password-1"),
                None,
                None
            )
            .await
            .is_ok());
    }
}
