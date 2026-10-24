use aegis_core::{AegisError, Result};
use chrono::{SecondsFormat, Utc};
use uuid::Uuid;

use super::password::hash_password;
use super::ROLES;
use aegis_db::{DatabaseAdapter, UserRecord};

/// ローカルユーザー作成時の入力
#[derive(Debug, Clone)]
pub struct NewUser {
    pub username: String,
    pub password: String,
    pub role: String,
    pub email: Option<String>,
    pub display_name: Option<String>,
}

pub(crate) fn now() -> String {
    Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true)
}

pub(crate) fn validate_role(role: &str) -> Result<()> {
    if ROLES.contains(&role) {
        Ok(())
    } else {
        Err(AegisError::Validation(format!(
            "Unknown role '{}' (expected one of: {})",
            role,
            ROLES.join(", ")
        )))
    }
}

pub(crate) fn validate_username(username: &str) -> Result<()> {
    let valid = !username.is_empty()
        && username.len() <= 64
        && username
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-' | '@'));
    if valid {
        Ok(())
    } else {
        Err(AegisError::Validation(
            "Username must be 1-64 characters of letters, digits, '.', '_', '-' or '@'".to_string(),
        ))
    }
}

async fn find_user(db: &dyn DatabaseAdapter, username: &str) -> Result<UserRecord> {
    db.get_user_by_username(username)
        .await?
        .ok_or_else(|| AegisError::NotFound(format!("User '{}' does not exist", username)))
}

/// ローカルユーザーを作成する（パスワードは Argon2id でハッシュして保存）
pub async fn create_user(db: &dyn DatabaseAdapter, new_user: NewUser) -> Result<UserRecord> {
    let username = new_user.username.trim().to_string();
    validate_username(&username)?;
    validate_role(&new_user.role)?;
    ensure_username_free(db, &username).await?;
    let password_hash = hash_password(&new_user.password)?;
    insert_new_user(
        db,
        NewUser {
            username,
            ..new_user
        },
        password_hash,
        false,
        false,
    )
    .await
}

/// ユーザー名がまだ使われていないことを確かめる
pub(crate) async fn ensure_username_free(db: &dyn DatabaseAdapter, username: &str) -> Result<()> {
    if db.get_user_by_username(username).await?.is_some() {
        return Err(AegisError::Validation(format!(
            "User '{}' already exists",
            username
        )));
    }
    Ok(())
}

/// 検証済みの入力（`new_user.password` は使わず、ハッシュ済みの `password_hash`）でユーザーを保存する
pub async fn insert_new_user(
    db: &dyn DatabaseAdapter,
    new_user: NewUser,
    password_hash: String,
    disabled: bool,
    pending_approval: bool,
) -> Result<UserRecord> {
    let now = now();
    let user = UserRecord {
        id: Uuid::new_v4().to_string(),
        username: new_user.username,
        email: new_user.email.filter(|s| !s.trim().is_empty()),
        display_name: new_user.display_name.filter(|s| !s.trim().is_empty()),
        role: new_user.role,
        password_hash,
        disabled,
        avatar_key: None,
        created_at: now.clone(),
        updated_at: now,
        pending_approval,
    };
    db.insert_user(&user).await?;
    Ok(user)
}

/// パスワードを変更する
pub async fn set_password(db: &dyn DatabaseAdapter, username: &str, password: &str) -> Result<()> {
    let mut user = find_user(db, username).await?;
    user.password_hash = hash_password(password)?;
    user.updated_at = now();
    db.update_user(&user).await
}

/// ロールを変更する（既存のセッションには反映されず、次回ログインから有効）
pub async fn set_role(db: &dyn DatabaseAdapter, username: &str, role: &str) -> Result<()> {
    validate_role(role)?;
    let mut user = find_user(db, username).await?;
    user.role = role.to_string();
    user.updated_at = now();
    db.update_user(&user).await
}

/// アカウントを無効化 / 有効化する（既存のセッションは失効しない）
pub async fn set_disabled(db: &dyn DatabaseAdapter, username: &str, disabled: bool) -> Result<()> {
    let mut user = find_user(db, username).await?;
    user.disabled = disabled;
    user.updated_at = now();
    db.update_user(&user).await
}

/// API（Admin Studio のユーザー管理・プロフィール）からの変更。`None` の項目は変更しない。
/// `email` / `display_name` は空文字（空白のみ）で削除する
#[derive(Debug, Clone, Default)]
pub struct UserChanges {
    pub email: Option<String>,
    pub display_name: Option<String>,
    pub role: Option<String>,
    pub disabled: Option<bool>,
    pub password: Option<String>,
    pub pending_approval: Option<bool>,
}

fn optional_text(value: String) -> Option<String> {
    let trimmed = value.trim();
    (!trimmed.is_empty()).then(|| trimmed.to_string())
}

fn is_active_admin(user: &UserRecord) -> bool {
    user.role == "admin" && !user.disabled
}

/// `user` が有効な admin でなくなる（降格・無効化・削除）とき、ほかに有効な admin が残るかを確かめる。
/// API から全員を締め出せないようにする（CLI の `aegis user role` などは復旧用に確かめない）
async fn ensure_other_admin(db: &dyn DatabaseAdapter, user: &UserRecord) -> Result<()> {
    if !is_active_admin(user) {
        return Ok(());
    }
    let others = db
        .list_users()
        .await?
        .iter()
        .filter(|u| u.id != user.id && is_active_admin(u))
        .count();
    if others == 0 {
        return Err(AegisError::Validation(
            "At least one active admin must remain".to_string(),
        ));
    }
    Ok(())
}

/// ユーザーの属性をまとめて変更する（ロール・無効化は既存のセッションには反映されない）
pub async fn update_user(
    db: &dyn DatabaseAdapter,
    username: &str,
    changes: UserChanges,
) -> Result<UserRecord> {
    let current = find_user(db, username).await?;
    let mut user = current.clone();
    if let Some(email) = changes.email {
        user.email = optional_text(email);
    }
    if let Some(display_name) = changes.display_name {
        user.display_name = optional_text(display_name);
    }
    if let Some(role) = changes.role {
        validate_role(&role)?;
        user.role = role;
    }
    if let Some(disabled) = changes.disabled {
        user.disabled = disabled;
    }
    if let Some(pending_approval) = changes.pending_approval {
        user.pending_approval = pending_approval;
    } else if changes.disabled == Some(false) && current.pending_approval {
        // 有効化された場合、未承認状態なら自動的に承認済みに遷移
        user.pending_approval = false;
    }
    if is_active_admin(&current) && !is_active_admin(&user) {
        ensure_other_admin(db, &current).await?;
    }
    if let Some(password) = changes.password {
        user.password_hash = hash_password(&password)?;
    }
    if user != current {
        user.updated_at = now();
        db.update_user(&user).await?;
    }
    Ok(user)
}

/// アバター画像のストレージキーを設定（`None` で解除）し、更新後のユーザーと以前のキーを返す
pub async fn set_avatar(
    db: &dyn DatabaseAdapter,
    username: &str,
    avatar_key: Option<String>,
) -> Result<(UserRecord, Option<String>)> {
    let mut user = find_user(db, username).await?;
    let previous = std::mem::replace(&mut user.avatar_key, avatar_key);
    user.updated_at = now();
    db.update_user(&user).await?;
    Ok((user, previous))
}

/// ユーザーと ID 連携を削除し、削除したユーザーを返す（最後の有効な admin は削除できない）
pub async fn delete_user(db: &dyn DatabaseAdapter, username: &str) -> Result<UserRecord> {
    let user = find_user(db, username).await?;
    ensure_other_admin(db, &user).await?;
    db.delete_user(&user).await?;
    Ok(user)
}

/// 既存のユーザーを外部 IdP のアカウント（Issuer + `sub`）に紐付ける。
/// `[auth.oidc].auto_create_users = false` の運用や、IdP と同名のローカルユーザーを使い続けるために使う
pub async fn link_identity(
    db: &dyn DatabaseAdapter,
    username: &str,
    provider: &str,
    subject: &str,
) -> Result<()> {
    find_user(db, username).await?;
    if let Some(linked) = db.get_identity_link(provider, subject).await? {
        return Err(AegisError::Validation(format!(
            "This {} account is already linked to user '{}'",
            provider, linked
        )));
    }
    db.insert_identity_link(provider, subject, username).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::password::verify_password;
    use aegis_db::SqliteAdapter;

    async fn memory_db() -> SqliteAdapter {
        let adapter = SqliteAdapter::connect("sqlite::memory:", 1).await.unwrap();
        adapter.init().await.unwrap();
        adapter
    }

    fn new_user(username: &str, role: &str) -> NewUser {
        NewUser {
            username: username.to_string(),
            password: "initial-pass".to_string(),
            role: role.to_string(),
            email: Some("".to_string()),
            display_name: Some("Editor One".to_string()),
        }
    }

    #[tokio::test]
    async fn test_user_lifecycle() {
        let db = memory_db().await;
        let created = create_user(&db, new_user(" editor1 ", "editor"))
            .await
            .unwrap();
        assert_eq!(created.username, "editor1");
        assert_eq!(created.email, None);
        assert_eq!(created.display_name.as_deref(), Some("Editor One"));
        assert!(!created.disabled);

        let duplicate = create_user(&db, new_user("editor1", "editor")).await;
        assert!(matches!(duplicate, Err(AegisError::Validation(_))));

        set_password(&db, "editor1", "changed-pass").await.unwrap();
        set_role(&db, "editor1", "admin").await.unwrap();
        set_disabled(&db, "editor1", true).await.unwrap();

        let stored = db.get_user_by_username("editor1").await.unwrap().unwrap();
        assert!(verify_password("changed-pass", &stored.password_hash).unwrap());
        assert_eq!(stored.role, "admin");
        assert!(stored.disabled);
        assert_eq!(stored.created_at, created.created_at);

        create_user(&db, new_user("admin", "admin")).await.unwrap();
        let usernames: Vec<String> = db
            .list_users()
            .await
            .unwrap()
            .into_iter()
            .map(|u| u.username)
            .collect();
        assert_eq!(usernames, ["admin", "editor1"]);
    }

    #[tokio::test]
    async fn test_update_and_delete_keep_an_admin() {
        let db = memory_db().await;
        create_user(&db, new_user("root", "admin")).await.unwrap();
        create_user(&db, new_user("editor1", "editor"))
            .await
            .unwrap();

        // The only admin can be neither demoted, disabled nor deleted
        let demote = UserChanges {
            role: Some("editor".to_string()),
            ..Default::default()
        };
        let disable = UserChanges {
            disabled: Some(true),
            ..Default::default()
        };
        assert!(update_user(&db, "root", demote.clone()).await.is_err());
        assert!(update_user(&db, "root", disable.clone()).await.is_err());
        assert!(delete_user(&db, "root").await.is_err());

        let updated = update_user(
            &db,
            "editor1",
            UserChanges {
                role: Some("admin".to_string()),
                email: Some(" editor@example.com ".to_string()),
                display_name: Some("  ".to_string()),
                password: Some("brand-new-pass".to_string()),
                ..Default::default()
            },
        )
        .await
        .unwrap();
        assert_eq!(updated.email.as_deref(), Some("editor@example.com"));
        assert_eq!(updated.display_name, None);
        assert!(verify_password("brand-new-pass", &updated.password_hash).unwrap());

        // With a second admin, the first one can step down
        update_user(&db, "root", demote).await.unwrap();
        assert!(update_user(&db, "editor1", disable).await.is_err());

        let (with_avatar, previous) = set_avatar(&db, "editor1", Some("avatars/a.png".to_string()))
            .await
            .unwrap();
        assert_eq!(with_avatar.avatar_key.as_deref(), Some("avatars/a.png"));
        assert_eq!(previous, None);
        let (_, previous) = set_avatar(&db, "editor1", None).await.unwrap();
        assert_eq!(previous.as_deref(), Some("avatars/a.png"));

        link_identity(&db, "root", "oidc", "https://idp#root")
            .await
            .unwrap();
        delete_user(&db, "root").await.unwrap();
        assert!(db.get_user_by_username("root").await.unwrap().is_none());
        assert_eq!(
            db.get_identity_link("oidc", "https://idp#root")
                .await
                .unwrap(),
            None
        );
    }

    #[tokio::test]
    async fn test_user_validation() {
        let db = memory_db().await;
        assert!(create_user(&db, new_user("root", "superuser"))
            .await
            .is_err());
        assert!(create_user(&db, new_user("has space", "editor"))
            .await
            .is_err());
        let mut short = new_user("shorty", "editor");
        short.password = "short".to_string();
        assert!(create_user(&db, short).await.is_err());
        assert!(matches!(
            set_role(&db, "nobody", "editor").await,
            Err(AegisError::NotFound(_))
        ));
        assert!(db.list_users().await.unwrap().is_empty());
    }
}
