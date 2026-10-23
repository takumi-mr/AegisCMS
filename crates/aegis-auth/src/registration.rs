//! Admin Studio からのアカウント登録（`[auth.registration]`）。
//!
//! 登録の申し込みはユーザーテーブルではなくキャッシュ（`[cache]`）に確認リンクの有効期間だけ置き、
//! メールアドレスに届いた確認リンク（トークン）が開かれたときにユーザーを作る。確認されない申し込みで
//! ユーザー名がふさがることはない（同じユーザー名の申し込みは先に確認したほうが作られる）。
//! キャッシュのキーにはトークンの SHA-256 を使い、トークン自体は保存しない

use aegis_core::{AegisError, CacheBackend};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::sync::Arc;
use uuid::Uuid;

use super::config::RegistrationConfig;
use super::password::{hash_password, MIN_PASSWORD_LEN};
use super::users::{ensure_username_free, insert_new_user, validate_username, NewUser};
use aegis_db::{DatabaseAdapter, UserRecord};

/// 登録の申し込み（`POST /api/v1/auth/registration` の本文）
#[derive(Debug, Clone, Deserialize)]
pub struct RegistrationRequest {
    pub username: String,
    pub email: String,
    pub password: String,
    #[serde(default)]
    pub display_name: Option<String>,
}

/// 申し込みを受け付けた結果
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Requested {
    /// 確認メールを送る（`token` を確認リンクに入れる）
    Pending {
        token: String,
        username: String,
        email: String,
        display_name: Option<String>,
    },
    /// メールアドレスは既存のユーザーのもの。登録済みかどうかを伏せるため、応答は `Pending` と同じにして
    /// メールは送らない
    EmailInUse { username: String },
}

/// 登録の失敗。`code` はクライアント（Admin Studio）が文言を選ぶのに使う
#[derive(Debug)]
pub enum RegistrationError {
    InvalidUsername,
    UsernameTaken,
    InvalidEmail,
    EmailDomainNotAllowed,
    PasswordTooShort,
    /// 確認の時点でメールアドレスが別のユーザーに使われていた
    EmailTaken,
    /// 確認リンクが無効・期限切れ・使用済み
    InvalidToken,
    Internal(AegisError),
}

impl RegistrationError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::InvalidUsername => "invalid_username",
            Self::UsernameTaken => "username_taken",
            Self::InvalidEmail => "invalid_email",
            Self::EmailDomainNotAllowed => "email_domain_not_allowed",
            Self::PasswordTooShort => "password_too_short",
            Self::EmailTaken => "email_taken",
            Self::InvalidToken => "invalid_token",
            Self::Internal(_) => "server_error",
        }
    }
}

impl std::fmt::Display for RegistrationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidUsername => f.write_str(
                "Username must be 1-64 characters of letters, digits, '.', '_', '-' or '@'",
            ),
            Self::UsernameTaken => f.write_str("This username is already taken"),
            Self::InvalidEmail => f.write_str("Enter a valid email address"),
            Self::EmailDomainNotAllowed => {
                f.write_str("Accounts cannot be registered with this email domain")
            }
            Self::PasswordTooShort => write!(
                f,
                "Password must be at least {} characters",
                MIN_PASSWORD_LEN
            ),
            Self::EmailTaken => f.write_str("This email address is already used by another user"),
            Self::InvalidToken => f.write_str("This confirmation link is invalid or has expired"),
            Self::Internal(e) => write!(f, "{}", e),
        }
    }
}

impl From<AegisError> for RegistrationError {
    fn from(e: AegisError) -> Self {
        Self::Internal(e)
    }
}

type Result<T> = std::result::Result<T, RegistrationError>;

/// キャッシュに置く申し込み
#[derive(Debug, Serialize, Deserialize)]
struct PendingRegistration {
    username: String,
    email: String,
    display_name: Option<String>,
    password_hash: String,
}

#[derive(Clone)]
pub struct RegistrationService {
    db: Arc<dyn DatabaseAdapter>,
    cache: Arc<dyn CacheBackend>,
    config: RegistrationConfig,
}

impl RegistrationService {
    pub fn new(
        db: Arc<dyn DatabaseAdapter>,
        cache: Arc<dyn CacheBackend>,
        config: RegistrationConfig,
    ) -> Self {
        Self { db, cache, config }
    }

    pub fn config(&self) -> &RegistrationConfig {
        &self.config
    }

    /// 申し込みを検証してキャッシュに置く。ユーザーはまだ作らない
    pub async fn request(&self, request: RegistrationRequest) -> Result<Requested> {
        let username = request.username.trim().to_string();
        validate_username(&username).map_err(|_| RegistrationError::InvalidUsername)?;
        let email = request.email.trim().to_string();
        if !is_valid_email(&email) {
            return Err(RegistrationError::InvalidEmail);
        }
        if !self.config.allows_email(&email) {
            return Err(RegistrationError::EmailDomainNotAllowed);
        }
        if request.password.chars().count() < MIN_PASSWORD_LEN {
            return Err(RegistrationError::PasswordTooShort);
        }
        if self.db.get_user_by_username(&username).await?.is_some() {
            return Err(RegistrationError::UsernameTaken);
        }

        // Argon2 は CPU を使うため非同期ランタイムのワーカーを塞がないよう別スレッドで計算する。
        // 登録済みのメールアドレスでも計算してから答え、応答時間の差で登録の有無を推測させない
        let password = request.password;
        let password_hash = tokio::task::spawn_blocking(move || hash_password(&password))
            .await
            .map_err(|e| AegisError::Internal(format!("Password hashing task failed: {}", e)))??;
        if self.email_in_use(&email).await? {
            return Ok(Requested::EmailInUse { username });
        }

        let display_name = request
            .display_name
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty());
        let pending = PendingRegistration {
            username: username.clone(),
            email: email.clone(),
            display_name: display_name.clone(),
            password_hash,
        };
        let token = random_token();
        let value = serde_json::to_string(&pending).map_err(AegisError::from)?;
        self.cache
            .set(&cache_key(&token), &value, Some(self.config.token_ttl_secs))
            .await?;
        Ok(Requested::Pending {
            token,
            username,
            email,
            display_name,
        })
    }

    /// 確認リンクのトークンでユーザーを作る。トークンは 1 回しか使えない。
    /// `require_approval` なら無効の状態で作る（admin が有効にするまでログインできない）
    pub async fn complete(&self, token: &str) -> Result<UserRecord> {
        let token = token.trim();
        if token.is_empty() {
            return Err(RegistrationError::InvalidToken);
        }
        let key = cache_key(token);
        let Some(value) = self.cache.get(&key).await? else {
            return Err(RegistrationError::InvalidToken);
        };
        self.cache.delete(&key).await?;
        let pending: PendingRegistration =
            serde_json::from_str(&value).map_err(|_| RegistrationError::InvalidToken)?;

        // 申し込みの後に同じユーザー名・メールアドレスのユーザーができていることがある
        ensure_username_free(&*self.db, &pending.username)
            .await
            .map_err(|e| match e {
                AegisError::Validation(_) => RegistrationError::UsernameTaken,
                other => RegistrationError::Internal(other),
            })?;
        if self.email_in_use(&pending.email).await? {
            return Err(RegistrationError::EmailTaken);
        }

        let user = insert_new_user(
            &*self.db,
            NewUser {
                username: pending.username,
                password: String::new(),
                role: self.config.default_role.clone(),
                email: Some(pending.email),
                display_name: pending.display_name,
            },
            pending.password_hash,
            self.config.require_approval,
        )
        .await?;
        Ok(user)
    }

    async fn email_in_use(&self, email: &str) -> Result<bool> {
        Ok(self.db.list_users().await?.iter().any(|user| {
            user.email
                .as_deref()
                .is_some_and(|existing| existing.eq_ignore_ascii_case(email))
        }))
    }
}

/// 簡単な形式の検査（`local@domain.tld`、空白なし）。届くかどうかは確認メールで確かめる
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

/// 256 bits from the OS RNG (two v4 UUIDs), hex encoded so it is safe in a URL fragment
fn random_token() -> String {
    let mut bytes = Vec::with_capacity(32);
    bytes.extend_from_slice(Uuid::new_v4().as_bytes());
    bytes.extend_from_slice(Uuid::new_v4().as_bytes());
    hex::encode(bytes)
}

fn cache_key(token: &str) -> String {
    format!(
        "registration:{}",
        hex::encode(Sha256::digest(token.as_bytes()))
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::password::verify_password;
    use crate::users::create_user;
    use aegis_cache::MemoryCacheBackend;
    use aegis_db::SqliteAdapter;

    async fn service(config: RegistrationConfig) -> RegistrationService {
        let db = SqliteAdapter::connect("sqlite::memory:", 1).await.unwrap();
        db.init().await.unwrap();
        RegistrationService::new(
            Arc::new(db),
            Arc::new(MemoryCacheBackend::new()),
            RegistrationConfig {
                enabled: true,
                ..config
            },
        )
    }

    fn request(username: &str, email: &str) -> RegistrationRequest {
        RegistrationRequest {
            username: username.to_string(),
            email: email.to_string(),
            password: "long-enough-pass".to_string(),
            display_name: Some(" Hanako ".to_string()),
        }
    }

    fn token(requested: Requested) -> String {
        match requested {
            Requested::Pending { token, .. } => token,
            other => panic!("expected a pending registration, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn test_registration_creates_the_user_only_after_confirmation() {
        let service = service(RegistrationConfig::default()).await;
        let requested = service
            .request(request(" hanako ", "hanako@example.com"))
            .await
            .unwrap();
        let Requested::Pending {
            token,
            username,
            display_name,
            ..
        } = requested
        else {
            panic!("expected a pending registration");
        };
        assert_eq!(username, "hanako");
        assert_eq!(display_name.as_deref(), Some("Hanako"));
        assert_eq!(token.len(), 64);
        assert!(service
            .db
            .get_user_by_username("hanako")
            .await
            .unwrap()
            .is_none());

        let user = service.complete(&token).await.unwrap();
        assert_eq!(user.username, "hanako");
        assert_eq!(user.email.as_deref(), Some("hanako@example.com"));
        assert_eq!(user.role, "viewer");
        // Approval is required by default
        assert!(user.disabled);
        assert!(verify_password("long-enough-pass", &user.password_hash).unwrap());

        // The link works once
        assert!(matches!(
            service.complete(&token).await,
            Err(RegistrationError::InvalidToken)
        ));
        assert!(matches!(
            service.complete("not-a-token").await,
            Err(RegistrationError::InvalidToken)
        ));
    }

    #[tokio::test]
    async fn test_registration_without_approval_uses_the_configured_role() {
        let service = service(RegistrationConfig {
            require_approval: false,
            default_role: "editor".to_string(),
            ..Default::default()
        })
        .await;
        let token = token(
            service
                .request(request("taro", "taro@example.com"))
                .await
                .unwrap(),
        );
        let user = service.complete(&token).await.unwrap();
        assert!(!user.disabled);
        assert_eq!(user.role, "editor");
    }

    #[tokio::test]
    async fn test_registration_validation() {
        let service = service(RegistrationConfig {
            allowed_email_domains: vec!["example.com".to_string()],
            ..Default::default()
        })
        .await;
        create_user(
            &*service.db,
            NewUser {
                username: "root".to_string(),
                password: "initial-pass".to_string(),
                role: "admin".to_string(),
                email: Some("Root@Example.com".to_string()),
                display_name: None,
            },
        )
        .await
        .unwrap();

        let code = |r: Result<Requested>| r.map(|_| "ok").unwrap_or_else(|e| e.code());
        assert_eq!(
            code(service.request(request("has space", "a@example.com")).await),
            "invalid_username"
        );
        assert_eq!(
            code(service.request(request("root", "a@example.com")).await),
            "username_taken"
        );
        assert_eq!(
            code(service.request(request("new", "not-an-email")).await),
            "invalid_email"
        );
        assert_eq!(
            code(service.request(request("new", "a@evil.com")).await),
            "email_domain_not_allowed"
        );
        let mut short = request("new", "a@example.com");
        short.password = "short".to_string();
        assert_eq!(code(service.request(short).await), "password_too_short");
        // Subdomains of an allowed domain are allowed
        assert_eq!(
            code(service.request(request("new", "a@mail.example.com")).await),
            "ok"
        );

        // An address that already has an account gets the same answer but no link
        assert_eq!(
            service
                .request(request("other", "root@example.com"))
                .await
                .unwrap(),
            Requested::EmailInUse {
                username: "other".to_string()
            }
        );
    }

    #[tokio::test]
    async fn test_first_confirmation_wins() {
        let service = service(RegistrationConfig::default()).await;
        let first = token(
            service
                .request(request("same", "one@example.com"))
                .await
                .unwrap(),
        );
        let second = token(
            service
                .request(request("same", "two@example.com"))
                .await
                .unwrap(),
        );
        let third = token(
            service
                .request(request("third", "one@example.com"))
                .await
                .unwrap(),
        );
        service.complete(&second).await.unwrap();
        assert!(matches!(
            service.complete(&first).await,
            Err(RegistrationError::UsernameTaken)
        ));
        service
            .complete(&token(
                service
                    .request(request("fourth", "four@example.com"))
                    .await
                    .unwrap(),
            ))
            .await
            .unwrap();
        // `one@example.com` is still free: the first link failed before creating a user
        service.complete(&third).await.unwrap();
    }

    #[test]
    fn test_config_validation() {
        let enabled = |role: &str| RegistrationConfig {
            enabled: true,
            default_role: role.to_string(),
            ..Default::default()
        };
        assert!(enabled("viewer").validate().is_ok());
        assert!(enabled("editor").validate().is_ok());
        assert!(enabled("admin").validate().is_err());
        assert!(enabled("owner").validate().is_err());
        let no_ttl = RegistrationConfig {
            token_ttl_secs: 0,
            ..enabled("viewer")
        };
        assert!(no_ttl.validate().is_err());
        // Settings of a disabled feature are not checked
        assert!(RegistrationConfig {
            enabled: false,
            ..enabled("admin")
        }
        .validate()
        .is_ok());
    }

    #[test]
    fn test_email_format_and_domains() {
        assert!(is_valid_email("a.b+c@example.co.jp"));
        for invalid in [
            "",
            "a@b",
            "@example.com",
            "a@@example.com",
            "a b@example.com",
            "a@.com",
            "a@example.",
        ] {
            assert!(!is_valid_email(invalid), "{}", invalid);
        }
        let config = RegistrationConfig {
            allowed_email_domains: vec!["@Example.com".to_string()],
            ..Default::default()
        };
        assert!(config.allows_email("a@example.com"));
        assert!(config.allows_email("a@dev.EXAMPLE.com"));
        assert!(!config.allows_email("a@notexample.com"));
        assert!(!config.allows_email("a@example.com.evil.net"));
        assert!(RegistrationConfig::default().allows_email("a@anything.dev"));
    }
}
