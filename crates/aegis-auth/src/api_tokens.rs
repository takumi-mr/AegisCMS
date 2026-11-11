//! API トークン（docs/authentication.md 13）。LLM・スクリプト・CI に、ユーザー名とパスワードの
//! 代わりに渡す長命の Bearer トークン。
//!
//! - 平文は発行時に 1 度だけ返し、DB には SHA-256 のハッシュだけを保存する
//! - 権限は「発行時に選んだロールの上限」と「持ち主のいまのロール」の低いほう。`admin` は
//!   上限にできない（ユーザー管理・設定・トークンの発行は人間のセッションだけ）
//! - 持ち主が無効化・削除されたら、トークンも使えなくなる

use std::collections::HashMap;

use aegis_core::{AegisError, Result};
use aegis_db::{ApiTokenRecord, DatabaseAdapter};
use chrono::{DateTime, Duration, SecondsFormat, Utc};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::session::SessionData;
use crate::{role_level, EDITOR};

/// トークンの先頭（秘密情報のスキャナーや目視で API トークンと分かるように）
pub const TOKEN_PREFIX: &str = "aegis_pat_";
/// 一覧に出す、トークンの先頭の文字数（`TOKEN_PREFIX` を含む）
const DISPLAY_PREFIX_LEN: usize = TOKEN_PREFIX.len() + 4;
/// 1 ユーザーが持てるトークンの数
pub const MAX_TOKENS_PER_USER: usize = 20;
/// 名前の最大文字数
pub const MAX_NAME_CHARS: usize = 64;
/// 最終使用日時を更新する最短の間隔（秒）。リクエストごとに DB を書かないため
const TOUCH_INTERVAL_SECS: i64 = 60;
/// `SessionData::metadata["provider"]` の値
pub const API_TOKEN_PROVIDER: &str = "api_token";
/// 検証用のセッション ID の接頭辞（平文のトークンは入れない）
const SESSION_ID_PREFIX: &str = "pat_";

/// 平文のトークンらしい文字列か
pub fn is_api_token(value: &str) -> bool {
    value.starts_with(TOKEN_PREFIX)
}

/// トークン（平文）の SHA-256（16 進）
pub fn hash_token(token: &str) -> String {
    hex::encode(Sha256::digest(token.as_bytes()))
}

/// 新しい平文のトークン（OS の乱数から 244 ビット）
fn generate_token() -> String {
    format!(
        "{}{}{}",
        TOKEN_PREFIX,
        Uuid::new_v4().simple(),
        Uuid::new_v4().simple()
    )
}

fn now() -> DateTime<Utc> {
    Utc::now()
}

fn format_time(t: DateTime<Utc>) -> String {
    t.to_rfc3339_opts(SecondsFormat::Secs, true)
}

/// トークンに許すロール（`admin` は不可）
pub fn validate_token_role(role: &str) -> Result<()> {
    match role {
        "viewer" | "contributor" | "reviewer" | "editor" => Ok(()),
        "admin" => Err(AegisError::Validation(
            "API tokens cannot have the 'admin' role (the highest is 'editor')".to_string(),
        )),
        other => Err(AegisError::Validation(format!(
            "Unknown role '{}' (expected one of: viewer, contributor, reviewer, editor)",
            other
        ))),
    }
}

/// 発行の入力
#[derive(Debug, Clone)]
pub struct NewApiToken {
    pub username: String,
    pub name: String,
    /// トークンに許すロールの上限
    pub role: String,
    /// 有効日数。`None` は無期限
    pub expires_in_days: Option<u32>,
}

/// 発行したトークン（平文はこのとき 1 度しか手に入らない）
#[derive(Debug, Clone)]
pub struct IssuedApiToken {
    pub record: ApiTokenRecord,
    pub token: String,
}

/// トークンを発行する。ロールの上限は持ち主のロール以下でなければならない
pub async fn issue_api_token(db: &dyn DatabaseAdapter, new: NewApiToken) -> Result<IssuedApiToken> {
    let name = new.name.trim().to_string();
    if name.is_empty() || name.chars().count() > MAX_NAME_CHARS {
        return Err(AegisError::Validation(format!(
            "Token name must be 1-{} characters",
            MAX_NAME_CHARS
        )));
    }
    validate_token_role(&new.role)?;
    if let Some(days) = new.expires_in_days {
        if days == 0 || days > 3650 {
            return Err(AegisError::Validation(
                "expires_in_days must be between 1 and 3650 (omit for no expiry)".to_string(),
            ));
        }
    }
    let user = db
        .get_user_by_username(&new.username)
        .await?
        .ok_or_else(|| AegisError::NotFound(format!("User '{}' does not exist", new.username)))?;
    if user.disabled {
        return Err(AegisError::Validation("Account is disabled".to_string()));
    }
    if role_level(&new.role) > role_level(&user.role) {
        return Err(AegisError::Validation(format!(
            "The token role '{}' is higher than the user's role '{}'",
            new.role, user.role
        )));
    }
    if db.list_api_tokens(&new.username).await?.len() >= MAX_TOKENS_PER_USER {
        return Err(AegisError::Validation(format!(
            "At most {} API tokens per user; revoke one first",
            MAX_TOKENS_PER_USER
        )));
    }

    let token = generate_token();
    let created = now();
    let record = ApiTokenRecord {
        id: Uuid::new_v4().to_string(),
        username: new.username,
        name,
        token_hash: hash_token(&token),
        token_prefix: token.chars().take(DISPLAY_PREFIX_LEN).collect(),
        role: new.role,
        created_at: format_time(created),
        expires_at: new
            .expires_in_days
            .map(|d| format_time(created + Duration::days(d as i64))),
        last_used_at: None,
    };
    db.insert_api_token(&record).await?;
    Ok(IssuedApiToken { record, token })
}

/// 平文のトークンからセッションを作る。無効（未登録・期限切れ・持ち主が無効または不在）なら `None`。
/// ロールは毎回、持ち主のいまのロールと突き合わせる（降格が即座に効く）
pub async fn resolve_api_token(
    db: &dyn DatabaseAdapter,
    token: &str,
) -> Result<Option<SessionData>> {
    if !is_api_token(token) {
        return Ok(None);
    }
    let Some(record) = db.get_api_token_by_hash(&hash_token(token)).await? else {
        return Ok(None);
    };
    let current = now();
    if let Some(expires) = &record.expires_at {
        match DateTime::parse_from_rfc3339(expires) {
            Ok(t) if t > current => {}
            _ => return Ok(None),
        }
    }
    let Some(user) = db.get_user_by_username(&record.username).await? else {
        return Ok(None);
    };
    if user.disabled {
        return Ok(None);
    }
    let level = role_level(&record.role)
        .min(role_level(&user.role))
        .min(EDITOR);
    if level == 0 {
        return Ok(None);
    }
    let role = crate::role_name(level).to_string();

    let stale = match &record.last_used_at {
        Some(t) => DateTime::parse_from_rfc3339(t)
            .map(|t| (current - t.with_timezone(&Utc)).num_seconds() >= TOUCH_INTERVAL_SECS)
            .unwrap_or(true),
        None => true,
    };
    if stale {
        // 最終使用日時の記録に失敗しても、認証は通す
        let _ = db.touch_api_token(&record.id, &format_time(current)).await;
    }

    let mut metadata = HashMap::new();
    metadata.insert(
        "provider".to_string(),
        serde_json::json!(API_TOKEN_PROVIDER),
    );
    metadata.insert("token_id".to_string(), serde_json::json!(record.id));
    metadata.insert("token_name".to_string(), serde_json::json!(record.name));
    if let Some(name) = user.display_name {
        metadata.insert("display_name".to_string(), serde_json::json!(name));
    }
    Ok(Some(SessionData {
        id: format!("{}{}", SESSION_ID_PREFIX, record.id),
        user_id: record.username,
        role,
        created_at: format_time(current),
        expires_at: record
            .expires_at
            .unwrap_or_else(|| format_time(current + Duration::days(1))),
        metadata,
    }))
}

/// セッションが API トークンで認証されたものか。トークンの管理・パスワード変更・2FA など、
/// 人間のセッションに限りたい操作で使う
pub fn is_api_token_session(session: &SessionData) -> bool {
    session
        .metadata
        .get("provider")
        .and_then(|v| v.as_str())
        .is_some_and(|p| p == API_TOKEN_PROVIDER)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::users::{create_user, NewUser};
    use aegis_db::{ApiTokenStorage, SqliteAdapter};

    async fn db_with_user(role: &str) -> SqliteAdapter {
        let db = SqliteAdapter::connect("sqlite::memory:", 1).await.unwrap();
        db.init().await.unwrap();
        create_user(
            &db,
            NewUser {
                username: "alice".into(),
                password: "password-123".into(),
                role: role.into(),
                email: None,
                display_name: Some("Alice".into()),
            },
        )
        .await
        .unwrap();
        db
    }

    fn input(role: &str, days: Option<u32>) -> NewApiToken {
        NewApiToken {
            username: "alice".into(),
            name: "claude".into(),
            role: role.into(),
            expires_in_days: days,
        }
    }

    #[tokio::test]
    async fn test_issue_and_resolve_stores_only_the_hash() {
        let db = db_with_user("admin").await;
        let issued = issue_api_token(&db, input("editor", Some(90)))
            .await
            .unwrap();
        assert!(issued.token.starts_with(TOKEN_PREFIX));
        assert!(issued.token.len() > 60);
        assert_ne!(issued.record.token_hash, issued.token);
        assert!(issued.token.starts_with(&issued.record.token_prefix));
        let listed = db.list_api_tokens("alice").await.unwrap();
        assert_eq!(listed.len(), 1);
        assert!(!format!("{listed:?}").contains(&issued.token));

        let session = resolve_api_token(&db, &issued.token)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(session.user_id, "alice");
        assert_eq!(session.role, "editor");
        assert!(is_api_token_session(&session));
        assert!(!session.id.contains(&issued.token));
        // 初回の使用で最終使用日時が入る
        let got = db
            .get_api_token_by_hash(&issued.record.token_hash)
            .await
            .unwrap()
            .unwrap();
        assert!(got.last_used_at.is_some());

        assert!(resolve_api_token(&db, "aegis_pat_nope")
            .await
            .unwrap()
            .is_none());
        assert!(resolve_api_token(&db, "ses_abc").await.unwrap().is_none());
    }

    #[tokio::test]
    async fn test_role_rules() {
        let db = db_with_user("editor").await;
        // reviewer は上限にできる（校正・承認だけをさせたい LLM 向け）
        let issued = issue_api_token(&db, input("reviewer", None)).await.unwrap();
        let session = resolve_api_token(&db, &issued.token)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(session.role, "reviewer");
        // admin は上限にできない
        assert!(issue_api_token(&db, input("admin", None)).await.is_err());
        assert!(issue_api_token(&db, input("root", None)).await.is_err());
        // 持ち主より高いロールは不可
        let db = db_with_user("contributor").await;
        assert!(issue_api_token(&db, input("editor", None)).await.is_err());

        // 発行後に持ち主が降格したら、低いほうになる
        let db = db_with_user("editor").await;
        let issued = issue_api_token(&db, input("editor", None)).await.unwrap();
        crate::users::set_role(&db, "alice", "viewer")
            .await
            .unwrap();
        let session = resolve_api_token(&db, &issued.token)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(session.role, "viewer");
    }

    #[tokio::test]
    async fn test_admin_owner_is_capped_below_admin() {
        let db = db_with_user("admin").await;
        let issued = issue_api_token(&db, input("viewer", None)).await.unwrap();
        let session = resolve_api_token(&db, &issued.token)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(session.role, "viewer");
    }

    #[tokio::test]
    async fn test_expired_disabled_and_deleted_owner_are_rejected() {
        let db = db_with_user("editor").await;
        let issued = issue_api_token(&db, input("editor", Some(1)))
            .await
            .unwrap();
        // 期限切れ
        let mut record = issued.record.clone();
        record.id = "expired".into();
        record.token_hash = hash_token("aegis_pat_expired");
        record.expires_at = Some("2020-01-01T00:00:00Z".into());
        db.insert_api_token(&record).await.unwrap();
        assert!(resolve_api_token(&db, "aegis_pat_expired")
            .await
            .unwrap()
            .is_none());

        // 無効化
        assert!(resolve_api_token(&db, &issued.token)
            .await
            .unwrap()
            .is_some());
        crate::users::set_disabled(&db, "alice", true)
            .await
            .unwrap();
        assert!(resolve_api_token(&db, &issued.token)
            .await
            .unwrap()
            .is_none());
        assert!(issue_api_token(&db, input("viewer", None)).await.is_err());
    }

    #[tokio::test]
    async fn test_limits_and_validation() {
        let db = db_with_user("editor").await;
        let mut bad = input("editor", None);
        bad.name = "  ".into();
        assert!(issue_api_token(&db, bad).await.is_err());
        assert!(issue_api_token(&db, input("editor", Some(0)))
            .await
            .is_err());
        for _ in 0..MAX_TOKENS_PER_USER {
            issue_api_token(&db, input("viewer", None)).await.unwrap();
        }
        assert!(issue_api_token(&db, input("viewer", None)).await.is_err());
    }
}
