use aegis_core::{AegisError, AuthCredentials, AuthIdentity, AuthProvider, Result};
use async_trait::async_trait;
use std::sync::Arc;

use super::password::{verify_dummy_password, verify_password};
use crate::db::DatabaseAdapter;

/// `AuthCredentials::kind` for username / password logins
pub const PASSWORD_CREDENTIALS: &str = "password";

/// `AegisError::Authentication` の理由コード。クライアントには区別せず 401 を返し、監査ログにだけ残す
pub const REASON_UNKNOWN_USER: &str = "unknown_user";
pub const REASON_INVALID_PASSWORD: &str = "invalid_password";
pub const REASON_ACCOUNT_DISABLED: &str = "account_disabled";
pub const REASON_UNSUPPORTED_CREDENTIALS: &str = "unsupported_credentials";

/// `_aegis_users` テーブルのユーザー名 / パスワードで認証するプロバイダ
pub struct LocalAuthProvider {
    db: Arc<dyn DatabaseAdapter>,
}

impl LocalAuthProvider {
    pub fn new(db: Arc<dyn DatabaseAdapter>) -> Self {
        Self { db }
    }
}

#[async_trait]
impl AuthProvider for LocalAuthProvider {
    fn provider_id(&self) -> &str {
        "local"
    }

    async fn authenticate(&self, credentials: &AuthCredentials) -> Result<AuthIdentity> {
        let (Some(username), Some(password)) = (&credentials.identifier, &credentials.secret)
        else {
            return Err(AegisError::Authentication(
                REASON_UNSUPPORTED_CREDENTIALS.to_string(),
            ));
        };
        if credentials.kind != PASSWORD_CREDENTIALS {
            return Err(AegisError::Authentication(
                REASON_UNSUPPORTED_CREDENTIALS.to_string(),
            ));
        }

        let user = self.db.get_user_by_username(username.trim()).await?;
        let password = password.clone();

        // Argon2 は CPU を使うため非同期ランタイムのワーカーを塞がないよう別スレッドで検証する
        let (user, matched) = tokio::task::spawn_blocking(move || match user {
            Some(user) => {
                let matched = verify_password(&password, &user.password_hash)?;
                Ok::<_, AegisError>((Some(user), matched))
            }
            None => {
                verify_dummy_password(&password);
                Ok((None, false))
            }
        })
        .await
        .map_err(|e| AegisError::Internal(format!("Password verification task failed: {}", e)))??;

        let user = match (user, matched) {
            (None, _) => return Err(AegisError::Authentication(REASON_UNKNOWN_USER.to_string())),
            (Some(_), false) => {
                return Err(AegisError::Authentication(
                    REASON_INVALID_PASSWORD.to_string(),
                ))
            }
            (Some(user), true) => user,
        };
        // 無効化の判定はパスワード検証の後に行い、パスワードを知らない相手には無効化の有無も伏せる
        if user.disabled {
            return Err(AegisError::Authentication(
                REASON_ACCOUNT_DISABLED.to_string(),
            ));
        }

        let mut identity = AuthIdentity::new(user.username, user.email.unwrap_or_default());
        identity.display_name = user.display_name;
        identity.role = user.role;
        Ok(identity)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth::password::hash_password;
    use crate::db::{SqliteAdapter, UserRecord};

    async fn provider_with_user(disabled: bool) -> LocalAuthProvider {
        let adapter = SqliteAdapter::connect("sqlite::memory:", 1).await.unwrap();
        adapter.init().await.unwrap();
        adapter
            .insert_user(&UserRecord {
                id: "u1".to_string(),
                username: "alice".to_string(),
                email: Some("alice@example.com".to_string()),
                display_name: Some("Alice".to_string()),
                role: "admin".to_string(),
                password_hash: hash_password("s3cret-pass").unwrap(),
                disabled,
                created_at: "2026-10-22T00:00:00Z".to_string(),
                updated_at: "2026-10-22T00:00:00Z".to_string(),
            })
            .await
            .unwrap();
        LocalAuthProvider::new(Arc::new(adapter))
    }

    fn password(username: &str, secret: &str) -> AuthCredentials {
        AuthCredentials {
            kind: PASSWORD_CREDENTIALS.to_string(),
            identifier: Some(username.to_string()),
            secret: Some(secret.to_string()),
            ..Default::default()
        }
    }

    fn reason(result: Result<AuthIdentity>) -> String {
        match result {
            Err(AegisError::Authentication(reason)) => reason,
            other => panic!("expected an authentication error, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn test_local_provider_authenticates_with_stored_role() {
        let provider = provider_with_user(false).await;
        let identity = provider
            .authenticate(&password("alice", "s3cret-pass"))
            .await
            .unwrap();
        assert_eq!(identity.external_id, "alice");
        assert_eq!(identity.email, "alice@example.com");
        assert_eq!(identity.display_name.as_deref(), Some("Alice"));
        assert_eq!(identity.role, "admin");
    }

    #[tokio::test]
    async fn test_local_provider_rejects_bad_credentials() {
        let provider = provider_with_user(false).await;
        assert_eq!(
            reason(
                provider
                    .authenticate(&password("alice", "wrong-pass"))
                    .await
            ),
            REASON_INVALID_PASSWORD
        );
        assert_eq!(
            reason(provider.authenticate(&password("bob", "s3cret-pass")).await),
            REASON_UNKNOWN_USER
        );
        let mut token = password("alice", "s3cret-pass");
        token.kind = "token".to_string();
        assert_eq!(
            reason(provider.authenticate(&token).await),
            REASON_UNSUPPORTED_CREDENTIALS
        );
    }

    #[tokio::test]
    async fn test_local_provider_rejects_disabled_account() {
        let provider = provider_with_user(true).await;
        assert_eq!(
            reason(
                provider
                    .authenticate(&password("alice", "s3cret-pass"))
                    .await
            ),
            REASON_ACCOUNT_DISABLED
        );
        // パスワードが違う場合は無効化されていることを明かさない
        assert_eq!(
            reason(
                provider
                    .authenticate(&password("alice", "wrong-pass"))
                    .await
            ),
            REASON_INVALID_PASSWORD
        );
    }
}
