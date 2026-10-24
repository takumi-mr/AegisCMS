//! パスワード再設定（`[auth.password_reset]`）。
//!
//! メールアドレスを受け取り、該当する有効なユーザーがいれば確認リンク（トークン）を通知する。
//! トークンはキャッシュに SHA-256 で保存し、1 回のみ使用可能。
//! 登録の有無を推測させないため、該当ユーザーがいなくても成功と同じレスポンスを返す。

use aegis_core::{AegisError, CacheBackend};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::sync::Arc;
use uuid::Uuid;

use super::config::PasswordResetConfig;
use super::password::{hash_password, MIN_PASSWORD_LEN};
use aegis_db::{DatabaseAdapter, UserRecord};

/// 再設定の申し込み結果
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RequestedReset {
    pub token: String,
    pub username: String,
    pub email: String,
    pub display_name: Option<String>,
}

/// パスワード再設定のエラー
#[derive(Debug)]
pub enum PasswordResetError {
    InvalidEmail,
    PasswordTooShort,
    /// トークンが無効・期限切れ・使用済み
    InvalidToken,
    UserNotFound,
    Internal(AegisError),
}

impl PasswordResetError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::InvalidEmail => "invalid_email",
            Self::PasswordTooShort => "password_too_short",
            Self::InvalidToken => "invalid_token",
            Self::UserNotFound => "user_not_found",
            Self::Internal(_) => "server_error",
        }
    }
}

impl std::fmt::Display for PasswordResetError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidEmail => f.write_str("Enter a valid email address"),
            Self::PasswordTooShort => write!(
                f,
                "Password must be at least {} characters",
                MIN_PASSWORD_LEN
            ),
            Self::InvalidToken => f.write_str("This reset link is invalid or has expired"),
            Self::UserNotFound => f.write_str("User not found"),
            Self::Internal(e) => write!(f, "{}", e),
        }
    }
}

impl From<AegisError> for PasswordResetError {
    fn from(e: AegisError) -> Self {
        Self::Internal(e)
    }
}

type Result<T> = std::result::Result<T, PasswordResetError>;

/// キャッシュに置く再設定データ
#[derive(Debug, Serialize, Deserialize)]
struct PendingReset {
    user_id: String,
    username: String,
    email: String,
}

#[derive(Clone)]
pub struct PasswordResetService {
    db: Arc<dyn DatabaseAdapter>,
    cache: Arc<dyn CacheBackend>,
    config: PasswordResetConfig,
}

impl PasswordResetService {
    pub fn new(
        db: Arc<dyn DatabaseAdapter>,
        cache: Arc<dyn CacheBackend>,
        config: PasswordResetConfig,
    ) -> Self {
        Self { db, cache, config }
    }

    pub fn config(&self) -> &PasswordResetConfig {
        &self.config
    }

    /// 再設定を要求する。該当する有効なユーザーがいれば `RequestedReset` を返し、
    /// いない場合でもメールアドレス列挙防止のために `Ok(None)` を返す
    pub async fn request(&self, email: &str) -> Result<Option<RequestedReset>> {
        let email = email.trim().to_string();
        if !is_valid_email(&email) {
            return Err(PasswordResetError::InvalidEmail);
        }

        let users = self.db.list_users().await?;
        let matching_user = users.into_iter().find(|u| {
            u.email
                .as_deref()
                .is_some_and(|e| e.eq_ignore_ascii_case(&email))
        });

        let Some(user) = matching_user else {
            return Ok(None);
        };

        if user.disabled {
            // アカウントが無効化されている場合は再設定メールを送らない（存在は伏せるため Ok(None)）
            return Ok(None);
        }

        let token = random_token();
        let pending = PendingReset {
            user_id: user.id.clone(),
            username: user.username.clone(),
            email: email.clone(),
        };
        let value = serde_json::to_string(&pending).map_err(AegisError::from)?;
        self.cache
            .set(&cache_key(&token), &value, Some(self.config.token_ttl_secs))
            .await?;

        Ok(Some(RequestedReset {
            token,
            username: user.username,
            email,
            display_name: user.display_name,
        }))
    }

    /// トークンと新パスワードでパスワードを再設定する。トークンは 1 回のみ有効
    pub async fn complete(&self, token: &str, new_password: &str) -> Result<UserRecord> {
        let token = token.trim();
        if token.is_empty() {
            return Err(PasswordResetError::InvalidToken);
        }
        if new_password.chars().count() < MIN_PASSWORD_LEN {
            return Err(PasswordResetError::PasswordTooShort);
        }

        let key = cache_key(token);
        let Some(value) = self.cache.get(&key).await? else {
            return Err(PasswordResetError::InvalidToken);
        };
        self.cache.delete(&key).await?;

        let pending: PendingReset =
            serde_json::from_str(&value).map_err(|_| PasswordResetError::InvalidToken)?;

        let mut user = self
            .db
            .get_user_by_username(&pending.username)
            .await?
            .ok_or(PasswordResetError::UserNotFound)?;

        if user.id != pending.user_id {
            return Err(PasswordResetError::InvalidToken);
        }

        let password = new_password.to_string();
        let password_hash = tokio::task::spawn_blocking(move || hash_password(&password))
            .await
            .map_err(|e| AegisError::Internal(format!("Password hashing task failed: {}", e)))??;

        user.password_hash = password_hash;
        user.updated_at = super::users::now();
        self.db.update_user(&user).await?;

        Ok(user)
    }
}

fn cache_key(token: &str) -> String {
    format!(
        "password_reset:{}",
        hex::encode(Sha256::digest(token.as_bytes()))
    )
}

fn is_valid_email(email: &str) -> bool {
    if email.len() > 254 || email.chars().any(|c| c.is_whitespace() || c.is_control()) {
        return false;
    }
    match email.split_once('@') {
        Some((local, domain)) => {
            !local.is_empty()
                && !domain.contains('@')
                && domain.contains('.')
                && !domain.starts_with('.')
                && !domain.ends_with('.')
        }
        None => false,
    }
}

fn random_token() -> String {
    let mut bytes = Vec::with_capacity(32);
    bytes.extend_from_slice(Uuid::new_v4().as_bytes());
    bytes.extend_from_slice(Uuid::new_v4().as_bytes());
    hex::encode(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::password::verify_password;
    use crate::users::{create_user, NewUser};
    use aegis_cache::MemoryCacheBackend;
    use aegis_db::SqliteAdapter;

    async fn service() -> (PasswordResetService, Arc<SqliteAdapter>) {
        let db = SqliteAdapter::connect("sqlite::memory:", 1).await.unwrap();
        db.init().await.unwrap();
        let db = Arc::new(db);
        let cache = Arc::new(MemoryCacheBackend::new());
        let service = PasswordResetService::new(
            db.clone(),
            cache,
            PasswordResetConfig {
                enabled: true,
                token_ttl_secs: 3600,
                max_per_ip_per_hour: 5,
            },
        );
        (service, db)
    }

    #[tokio::test]
    async fn test_password_reset_flow() {
        let (service, db) = service().await;
        create_user(
            db.as_ref(),
            NewUser {
                username: "bob".to_string(),
                password: "old-password-123".to_string(),
                role: "editor".to_string(),
                email: Some("bob@example.com".to_string()),
                display_name: Some("Bob".to_string()),
            },
        )
        .await
        .unwrap();

        // 申し込み
        let requested = service.request("bob@example.com").await.unwrap().unwrap();
        assert_eq!(requested.username, "bob");
        assert_eq!(requested.email, "bob@example.com");

        // 存在しないメールアドレス
        assert!(service
            .request("nonexistent@example.com")
            .await
            .unwrap()
            .is_none());

        // 新しいパスワードで完了
        let updated = service
            .complete(&requested.token, "new-password-456")
            .await
            .unwrap();
        assert_eq!(updated.username, "bob");
        assert!(verify_password("new-password-456", &updated.password_hash).unwrap());

        // 使用済みトークンは使えない
        assert!(matches!(
            service.complete(&requested.token, "another-pass").await,
            Err(PasswordResetError::InvalidToken)
        ));
    }
}
