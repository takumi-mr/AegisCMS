use aegis_core::{AegisError, Result};
use chrono::{SecondsFormat, Utc};
use uuid::Uuid;

use super::password::hash_password;
use super::ROLES;
use crate::db::{DatabaseAdapter, UserRecord};

/// ローカルユーザー作成時の入力
#[derive(Debug, Clone)]
pub struct NewUser {
    pub username: String,
    pub password: String,
    pub role: String,
    pub email: Option<String>,
    pub display_name: Option<String>,
}

fn now() -> String {
    Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true)
}

fn validate_role(role: &str) -> Result<()> {
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

fn validate_username(username: &str) -> Result<()> {
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
    if db.get_user_by_username(&username).await?.is_some() {
        return Err(AegisError::Validation(format!(
            "User '{}' already exists",
            username
        )));
    }

    let now = now();
    let user = UserRecord {
        id: Uuid::new_v4().to_string(),
        username,
        email: new_user.email.filter(|s| !s.trim().is_empty()),
        display_name: new_user.display_name.filter(|s| !s.trim().is_empty()),
        role: new_user.role,
        password_hash: hash_password(&new_user.password)?,
        disabled: false,
        created_at: now.clone(),
        updated_at: now,
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth::password::verify_password;
    use crate::db::SqliteAdapter;

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
