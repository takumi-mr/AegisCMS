//! ユーザー招待機能（`[auth.invitation]`）。
//!
//! 管理者がメールアドレスとロールを指定して招待リンクを発行し、
//! 招待された本人がユーザー名とパスワードを決めてアカウントを作成する。
//! 作成されたアカウントは最初から有効（`require_approval` の承認待ちは不要）。

use aegis_core::{AegisError, CacheBackend};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::sync::Arc;
use uuid::Uuid;

use super::config::InvitationConfig;
use super::password::{hash_password, MIN_PASSWORD_LEN};
use super::users::{
    ensure_username_free, insert_new_user, validate_role, validate_username, NewUser,
};
use aegis_db::{DatabaseAdapter, UserRecord};

/// 招待作成リクエスト
#[derive(Debug, Clone, Deserialize)]
pub struct InviteRequest {
    pub email: String,
    pub role: String,
    #[serde(default)]
    pub display_name: Option<String>,
}

/// 招待作成結果
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RequestedInvitation {
    pub token: String,
    pub email: String,
    pub role: String,
    pub display_name: Option<String>,
}

/// 招待受諾リクエスト
#[derive(Debug, Clone, Deserialize)]
pub struct AcceptInviteRequest {
    pub token: String,
    pub username: String,
    pub password: String,
    #[serde(default)]
    pub display_name: Option<String>,
}

/// 招待の確認情報（受諾画面で表示する内容）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InvitationInfo {
    pub email: String,
    pub role: String,
    pub display_name: Option<String>,
}

/// 招待のエラー
#[derive(Debug)]
pub enum InvitationError {
    InvalidEmail,
    InvalidRole,
    EmailTaken,
    InvalidUsername,
    UsernameTaken,
    PasswordTooShort,
    /// 招待トークンが無効・期限切れ・使用済み
    InvalidToken,
    Internal(AegisError),
}

impl InvitationError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::InvalidEmail => "invalid_email",
            Self::InvalidRole => "invalid_role",
            Self::EmailTaken => "email_taken",
            Self::InvalidUsername => "invalid_username",
            Self::UsernameTaken => "username_taken",
            Self::PasswordTooShort => "password_too_short",
            Self::InvalidToken => "invalid_token",
            Self::Internal(_) => "server_error",
        }
    }
}

impl std::fmt::Display for InvitationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidEmail => f.write_str("Enter a valid email address"),
            Self::InvalidRole => f.write_str("Invalid user role"),
            Self::EmailTaken => {
                f.write_str("This email address is already used by an existing user")
            }
            Self::InvalidUsername => f.write_str(
                "Username must be 1-64 characters of letters, digits, '.', '_', '-' or '@'",
            ),
            Self::UsernameTaken => f.write_str("This username is already taken"),
            Self::PasswordTooShort => write!(
                f,
                "Password must be at least {} characters",
                MIN_PASSWORD_LEN
            ),
            Self::InvalidToken => f.write_str("This invitation link is invalid or has expired"),
            Self::Internal(e) => write!(f, "{}", e),
        }
    }
}

impl From<AegisError> for InvitationError {
    fn from(e: AegisError) -> Self {
        Self::Internal(e)
    }
}

type Result<T> = std::result::Result<T, InvitationError>;

/// キャッシュに置く招待データ
#[derive(Debug, Serialize, Deserialize)]
struct PendingInvitation {
    email: String,
    role: String,
    display_name: Option<String>,
    invited_by: String,
}

#[derive(Clone)]
pub struct InvitationService {
    db: Arc<dyn DatabaseAdapter>,
    cache: Arc<dyn CacheBackend>,
    config: InvitationConfig,
}

impl InvitationService {
    pub fn new(
        db: Arc<dyn DatabaseAdapter>,
        cache: Arc<dyn CacheBackend>,
        config: InvitationConfig,
    ) -> Self {
        Self { db, cache, config }
    }

    pub fn config(&self) -> &InvitationConfig {
        &self.config
    }

    /// 管理者がユーザーを招待する。招待トークンを生成してキャッシュに保存する
    pub async fn create_invitation(
        &self,
        invited_by: &str,
        request: InviteRequest,
    ) -> Result<RequestedInvitation> {
        let email = request.email.trim().to_string();
        if !is_valid_email(&email) {
            return Err(InvitationError::InvalidEmail);
        }
        validate_role(&request.role).map_err(|_| InvitationError::InvalidRole)?;

        // 既存ユーザーのメールアドレス重複チェック
        let users = self.db.list_users().await?;
        if users.iter().any(|u| {
            u.email
                .as_deref()
                .is_some_and(|e| e.eq_ignore_ascii_case(&email))
        }) {
            return Err(InvitationError::EmailTaken);
        }

        let display_name = request
            .display_name
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty());

        let token = random_token();
        let pending = PendingInvitation {
            email: email.clone(),
            role: request.role.clone(),
            display_name: display_name.clone(),
            invited_by: invited_by.to_string(),
        };
        let value = serde_json::to_string(&pending).map_err(AegisError::from)?;
        self.cache
            .set(&cache_key(&token), &value, Some(self.config.token_ttl_secs))
            .await?;

        Ok(RequestedInvitation {
            token,
            email,
            role: request.role,
            display_name,
        })
    }

    /// 招待リンクを開いたときにトークンを検証し、招待情報を返す（トークンは消費しない）
    pub async fn get_invitation_info(&self, token: &str) -> Result<InvitationInfo> {
        let token = token.trim();
        if token.is_empty() {
            return Err(InvitationError::InvalidToken);
        }
        let key = cache_key(token);
        let Some(value) = self.cache.get(&key).await? else {
            return Err(InvitationError::InvalidToken);
        };
        let pending: PendingInvitation =
            serde_json::from_str(&value).map_err(|_| InvitationError::InvalidToken)?;
        Ok(InvitationInfo {
            email: pending.email,
            role: pending.role,
            display_name: pending.display_name,
        })
    }

    /// 招待を受諾してユーザーを作成する。トークンは 1 回限りで消費される
    pub async fn accept_invitation(&self, request: AcceptInviteRequest) -> Result<UserRecord> {
        let token = request.token.trim();
        if token.is_empty() {
            return Err(InvitationError::InvalidToken);
        }
        let key = cache_key(token);
        let Some(value) = self.cache.get(&key).await? else {
            return Err(InvitationError::InvalidToken);
        };
        self.cache.delete(&key).await?;

        let pending: PendingInvitation =
            serde_json::from_str(&value).map_err(|_| InvitationError::InvalidToken)?;

        let username = request.username.trim().to_string();
        validate_username(&username).map_err(|_| InvitationError::InvalidUsername)?;
        ensure_username_free(&*self.db, &username)
            .await
            .map_err(|e| match e {
                AegisError::Validation(_) => InvitationError::UsernameTaken,
                other => InvitationError::Internal(other),
            })?;

        if request.password.chars().count() < MIN_PASSWORD_LEN {
            return Err(InvitationError::PasswordTooShort);
        }

        // 既存メールアドレスの再確認
        let users = self.db.list_users().await?;
        if users.iter().any(|u| {
            u.email
                .as_deref()
                .is_some_and(|e| e.eq_ignore_ascii_case(&pending.email))
        }) {
            return Err(InvitationError::EmailTaken);
        }

        let password = request.password;
        let password_hash = tokio::task::spawn_blocking(move || hash_password(&password))
            .await
            .map_err(|e| AegisError::Internal(format!("Password hashing task failed: {}", e)))??;

        let display_name = request
            .display_name
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .or(pending.display_name);

        // 管理者招待によるアカウントは即座に有効化（disabled: false, pending_approval: false）
        let user = insert_new_user(
            &*self.db,
            NewUser {
                username,
                password: String::new(),
                role: pending.role,
                email: Some(pending.email),
                display_name,
            },
            password_hash,
            false,
            false,
        )
        .await?;

        Ok(user)
    }
}

fn cache_key(token: &str) -> String {
    format!(
        "invitation:{}",
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
    use aegis_cache::MemoryCacheBackend;
    use aegis_db::SqliteAdapter;

    async fn service() -> InvitationService {
        let db = SqliteAdapter::connect("sqlite::memory:", 1).await.unwrap();
        db.init().await.unwrap();
        let cache = Arc::new(MemoryCacheBackend::new());
        InvitationService::new(
            Arc::new(db),
            cache,
            InvitationConfig {
                enabled: true,
                token_ttl_secs: 604_800,
            },
        )
    }

    #[tokio::test]
    async fn test_invitation_lifecycle() {
        let service = service().await;

        // 招待作成
        let invite = service
            .create_invitation(
                "admin",
                InviteRequest {
                    email: "charlie@example.com".to_string(),
                    role: "editor".to_string(),
                    display_name: Some("Charlie".to_string()),
                },
            )
            .await
            .unwrap();

        // 招待情報確認（リンクを開いた画面）
        let info = service.get_invitation_info(&invite.token).await.unwrap();
        assert_eq!(info.email, "charlie@example.com");
        assert_eq!(info.role, "editor");
        assert_eq!(info.display_name.as_deref(), Some("Charlie"));

        // 招待受諾
        let user = service
            .accept_invitation(AcceptInviteRequest {
                token: invite.token.clone(),
                username: "charlie".to_string(),
                password: "secure-pass-789".to_string(),
                display_name: None,
            })
            .await
            .unwrap();

        assert_eq!(user.username, "charlie");
        assert_eq!(user.role, "editor");
        assert_eq!(user.email.as_deref(), Some("charlie@example.com"));
        assert!(!user.disabled);
        assert!(!user.pending_approval);
        assert!(verify_password("secure-pass-789", &user.password_hash).unwrap());

        // トークンは再利用できない
        assert!(matches!(
            service.get_invitation_info(&invite.token).await,
            Err(InvitationError::InvalidToken)
        ));
    }
}
