//! ユーザー管理 API（`/api/v1/users`）。一覧・作成・削除は `admin`、各ユーザーの参照・変更・
//! アバターは本人か `admin`。本人のパスワード変更には現在のパスワードが要る

use axum::{
    extract::{Multipart, Path, State},
    http::StatusCode,
    Json,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::sync::Arc;
use uuid::Uuid;

use crate::audit::AuditContext;
use crate::auth::authorize::RequireRole;
use crate::auth::{AdminSession, ViewerSession};
use crate::state::AppState;
use aegis_auth::password::verify_password;
use aegis_auth::users::{self, NewUser, UserChanges};
use aegis_auth::SessionData;
use aegis_core::{AegisError, AuditAction, AuditOutcome, NotificationEvent, Recipient};
use aegis_db::UserRecord;

/// アバター画像の最大サイズ
pub const MAX_AVATAR_BYTES: usize = 2 * 1024 * 1024;
/// アバターを保存するストレージキーの接頭辞
pub const AVATAR_PREFIX: &str = "avatars/";

type ApiError = (StatusCode, Json<serde_json::Value>);

fn api_error(status: StatusCode, message: impl Into<String>) -> ApiError {
    (status, Json(json!({ "error": message.into() })))
}

fn from_aegis(e: AegisError) -> ApiError {
    let status = match e {
        AegisError::Validation(_) => StatusCode::BAD_REQUEST,
        AegisError::NotFound(_) => StatusCode::NOT_FOUND,
        _ => StatusCode::INTERNAL_SERVER_ERROR,
    };
    let message = match e {
        AegisError::Validation(m) | AegisError::NotFound(m) => m,
        other => other.to_string(),
    };
    api_error(status, message)
}

/// API が返すユーザー（パスワードハッシュは含めない）
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct UserView {
    pub id: String,
    pub username: String,
    pub email: Option<String>,
    pub display_name: Option<String>,
    pub role: String,
    pub disabled: bool,
    /// アバター画像の公開 URL（未設定なら `null`）
    pub avatar_url: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub pending_approval: bool,
}

fn view(state: &AppState, user: UserRecord) -> UserView {
    UserView {
        avatar_url: user
            .avatar_key
            .as_deref()
            .map(|k| state.storage.public_url(k)),
        id: user.id,
        username: user.username,
        email: user.email,
        display_name: user.display_name,
        role: user.role,
        disabled: user.disabled,
        created_at: user.created_at,
        updated_at: user.updated_at,
        pending_approval: user.pending_approval,
    }
}

fn is_admin(session: &SessionData) -> bool {
    session.role == "admin"
}

/// 本人か admin でなければ 403（`access.denied` を記録）
async fn require_self_or_admin(
    state: &AppState,
    audit: &AuditContext,
    session: &SessionData,
    username: &str,
) -> Result<(), ApiError> {
    if session.user_id == username || is_admin(session) {
        return Ok(());
    }
    state
        .audit
        .record(
            audit
                .event(AuditAction::AccessDenied, AuditOutcome::Denied)
                .with_resource("user", Some(username.to_string()))
                .with_details(json!({
                    "reason": "not_own_account",
                    "required_role": "admin",
                })),
        )
        .await;
    Err(api_error(
        StatusCode::FORBIDDEN,
        "Only admins can manage other users",
    ))
}

async fn record_user_event(
    state: &AppState,
    audit: &AuditContext,
    action: AuditAction,
    username: &str,
    result: &Result<(), String>,
    details: serde_json::Value,
) {
    let mut details = details;
    let outcome = match result {
        Ok(()) => AuditOutcome::Success,
        Err(error) => {
            details["error"] = json!(error);
            AuditOutcome::Failure
        }
    };
    state
        .audit
        .record(
            audit
                .event(action, outcome)
                .with_resource("user", Some(username.to_string()))
                .with_details(details),
        )
        .await;
}

/// `GET /api/v1/users` — ユーザー一覧（username 順）
pub async fn list_users(
    State(state): State<Arc<AppState>>,
    RequireRole(_session): AdminSession,
) -> Result<Json<serde_json::Value>, ApiError> {
    let users = state.db.list_users().await.map_err(from_aegis)?;
    let users: Vec<UserView> = users.into_iter().map(|u| view(&state, u)).collect();
    Ok(Json(json!({ "users": users })))
}

#[derive(Debug, Deserialize)]
pub struct CreateUserRequest {
    pub username: String,
    pub password: String,
    pub role: String,
    #[serde(default)]
    pub email: Option<String>,
    #[serde(default)]
    pub display_name: Option<String>,
}

/// `POST /api/v1/users` — ユーザーを作成する
pub async fn create_user(
    State(state): State<Arc<AppState>>,
    RequireRole(_session): AdminSession,
    audit: AuditContext,
    Json(payload): Json<CreateUserRequest>,
) -> Result<(StatusCode, Json<UserView>), ApiError> {
    let username = payload.username.trim().to_string();
    let role = payload.role.clone();
    let result = users::create_user(
        state.db.as_ref(),
        NewUser {
            username: payload.username,
            password: payload.password,
            role: payload.role,
            email: payload.email,
            display_name: payload.display_name,
        },
    )
    .await;
    // 入力不備（重複・不正なユーザー名・短いパスワード）は記録しない
    if let Err(AegisError::Validation(m)) = result {
        return Err(api_error(StatusCode::BAD_REQUEST, m));
    }
    record_user_event(
        &state,
        &audit,
        AuditAction::UserCreate,
        &username,
        &result.as_ref().map(|_| ()).map_err(|e| e.to_string()),
        json!({ "role": role }),
    )
    .await;
    let user = result.map_err(from_aegis)?;
    Ok((StatusCode::CREATED, Json(view(&state, user))))
}

/// `GET /api/v1/users/{username}` — ユーザー（本人か admin）
pub async fn get_user(
    State(state): State<Arc<AppState>>,
    RequireRole(session): ViewerSession,
    audit: AuditContext,
    Path(username): Path<String>,
) -> Result<Json<UserView>, ApiError> {
    require_self_or_admin(&state, &audit, &session, &username).await?;
    let user = state
        .db
        .get_user_by_username(&username)
        .await
        .map_err(from_aegis)?
        .ok_or_else(|| {
            api_error(
                StatusCode::NOT_FOUND,
                format!("User '{}' does not exist", username),
            )
        })?;
    Ok(Json(view(&state, user)))
}

/// `PATCH /api/v1/users/{username}` の本文。省略した項目は変更しない
#[derive(Debug, Default, Deserialize)]
pub struct UpdateUserRequest {
    #[serde(default)]
    pub email: Option<String>,
    #[serde(default)]
    pub display_name: Option<String>,
    /// admin のみ（自分自身は変更できない）
    #[serde(default)]
    pub role: Option<String>,
    /// admin のみ（自分自身は変更できない）
    #[serde(default)]
    pub disabled: Option<bool>,
    #[serde(default)]
    pub password: Option<String>,
    /// 自分のパスワードを変えるときに必要
    #[serde(default)]
    pub current_password: Option<String>,
}

/// 現在のパスワードを確かめる（Argon2 は別スレッドで検証する）
async fn check_current_password(
    user: &UserRecord,
    password: Option<String>,
) -> Result<(), ApiError> {
    let Some(password) = password else {
        return Err(api_error(
            StatusCode::BAD_REQUEST,
            "current_password is required to change your own password",
        ));
    };
    let hash = user.password_hash.clone();
    let matched = tokio::task::spawn_blocking(move || verify_password(&password, &hash))
        .await
        .map_err(|e| {
            api_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Password verification task failed: {}", e),
            )
        })?
        .map_err(from_aegis)?;
    if matched {
        Ok(())
    } else {
        Err(api_error(
            StatusCode::FORBIDDEN,
            "The current password is incorrect",
        ))
    }
}

/// 監査ログの `details`: 実際に変わった項目（パスワードは値を残さない）と、ロール・無効化の変更内容
fn changes_between(before: &UserRecord, after: &UserRecord) -> serde_json::Value {
    let changed: Vec<&str> = [
        ("email", before.email != after.email),
        ("display_name", before.display_name != after.display_name),
        ("role", before.role != after.role),
        ("disabled", before.disabled != after.disabled),
        ("password", before.password_hash != after.password_hash),
        (
            "pending_approval",
            before.pending_approval != after.pending_approval,
        ),
    ]
    .into_iter()
    .filter_map(|(name, differs)| differs.then_some(name))
    .collect();
    let mut details = json!({ "changed": changed });
    if before.role != after.role {
        details["role"] = json!({ "from": before.role, "to": after.role });
    }
    if before.disabled != after.disabled {
        details["disabled"] = json!(after.disabled);
    }
    if before.pending_approval && !after.pending_approval {
        details["approved"] = json!(true);
    }
    details
}

/// `PATCH /api/v1/users/{username}` — メール・表示名・パスワード（本人か admin）、
/// ロール・無効化（admin が他のユーザーに対してのみ）を変更する
pub async fn update_user(
    State(state): State<Arc<AppState>>,
    RequireRole(session): ViewerSession,
    audit: AuditContext,
    Path(username): Path<String>,
    Json(payload): Json<UpdateUserRequest>,
) -> Result<Json<UserView>, ApiError> {
    require_self_or_admin(&state, &audit, &session, &username).await?;
    let is_self = session.user_id == username;
    if (payload.role.is_some() || payload.disabled.is_some()) && !is_admin(&session) {
        return Err(api_error(
            StatusCode::FORBIDDEN,
            "Only admins can change roles or disable accounts",
        ));
    }
    if is_self && (payload.role.is_some() || payload.disabled.is_some()) {
        return Err(api_error(
            StatusCode::BAD_REQUEST,
            "You cannot change your own role or disable your own account",
        ));
    }
    let current = state
        .db
        .get_user_by_username(&username)
        .await
        .map_err(from_aegis)?
        .ok_or_else(|| {
            api_error(
                StatusCode::NOT_FOUND,
                format!("User '{}' does not exist", username),
            )
        })?;
    if is_self && payload.password.is_some() {
        check_current_password(&current, payload.current_password.clone()).await?;
    }

    // 失敗時に記録する、変更しようとした項目
    let requested: Vec<&str> = [
        ("email", payload.email.is_some()),
        ("display_name", payload.display_name.is_some()),
        ("role", payload.role.is_some()),
        ("disabled", payload.disabled.is_some()),
        ("password", payload.password.is_some()),
    ]
    .into_iter()
    .filter_map(|(name, set)| set.then_some(name))
    .collect();

    let result = users::update_user(
        state.db.as_ref(),
        &username,
        UserChanges {
            email: payload.email,
            display_name: payload.display_name,
            role: payload.role,
            disabled: payload.disabled,
            password: payload.password,
            pending_approval: None,
        },
    )
    .await;
    if let Err(AegisError::Validation(m)) = result {
        return Err(api_error(StatusCode::BAD_REQUEST, m));
    }
    if let Ok(updated) = &result {
        // 承認待ちだったアカウントが有効化された場合、本人に承認完了メールを通知する
        if current.pending_approval && !updated.pending_approval && !updated.disabled {
            if let (Some(notifier), Some(email)) = (&state.notifier, &updated.email) {
                let admin_url = state.admin_url.as_deref().unwrap_or("");
                notifier.notify(NotificationEvent::new(
                    "account.approved",
                    vec![Recipient::person(
                        email,
                        updated
                            .display_name
                            .clone()
                            .unwrap_or_else(|| updated.username.clone()),
                    )],
                    json!({
                        "username": updated.username,
                        "display_name": updated.display_name,
                        "login_url": admin_url,
                    }),
                ));
            }
        }
        if current.role != updated.role
            || current.disabled != updated.disabled
            || current.password_hash != updated.password_hash
        {
            if let Err(e) = state.sessions.revoke_user_sessions(&username).await {
                tracing::warn!("Failed to revoke sessions for '{}': {}", username, e);
            }
        }
    }
    let details = match &result {
        Ok(updated) => changes_between(&current, updated),
        Err(_) => json!({ "changed": requested }),
    };
    record_user_event(
        &state,
        &audit,
        AuditAction::UserUpdate,
        &username,
        &result.as_ref().map(|_| ()).map_err(|e| e.to_string()),
        details,
    )
    .await;
    Ok(Json(view(&state, result.map_err(from_aegis)?)))
}

/// `DELETE /api/v1/users/{username}` — ユーザーを削除する（自分自身・最後の admin は不可）。
/// コンテンツの `author_id` はそのまま残る
pub async fn delete_user(
    State(state): State<Arc<AppState>>,
    RequireRole(session): AdminSession,
    audit: AuditContext,
    Path(username): Path<String>,
) -> Result<StatusCode, ApiError> {
    if session.user_id == username {
        return Err(api_error(
            StatusCode::BAD_REQUEST,
            "You cannot delete your own account",
        ));
    }
    let result = users::delete_user(state.db.as_ref(), &username).await;
    let user = match result {
        Ok(user) => user,
        Err(e @ (AegisError::Validation(_) | AegisError::NotFound(_))) => {
            return Err(from_aegis(e))
        }
        Err(e) => {
            let error = e.to_string();
            record_user_event(
                &state,
                &audit,
                AuditAction::UserDelete,
                &username,
                &Err(error),
                json!({}),
            )
            .await;
            return Err(from_aegis(e));
        }
    };
    if let Err(e) = state.sessions.revoke_user_sessions(&username).await {
        tracing::warn!(
            "Failed to revoke sessions for deleted user '{}': {}",
            username,
            e
        );
    }
    if let Some(key) = &user.avatar_key {
        remove_avatar_object(&state, key).await;
    }
    record_user_event(
        &state,
        &audit,
        AuditAction::UserDelete,
        &username,
        &Ok(()),
        json!({ "role": user.role }),
    )
    .await;
    Ok(StatusCode::NO_CONTENT)
}

/// 画像の先頭バイトから形式を判定する（クライアントの Content-Type は信用しない。
/// スクリプトを含められる SVG は受け付けない）
fn detect_image(data: &[u8]) -> Option<(&'static str, &'static str)> {
    if data.starts_with(b"\x89PNG\r\n\x1a\n") {
        Some(("image/png", "png"))
    } else if data.starts_with(&[0xFF, 0xD8, 0xFF]) {
        Some(("image/jpeg", "jpg"))
    } else if data.starts_with(b"GIF87a") || data.starts_with(b"GIF89a") {
        Some(("image/gif", "gif"))
    } else if data.len() >= 12 && &data[..4] == b"RIFF" && &data[8..12] == b"WEBP" {
        Some(("image/webp", "webp"))
    } else {
        None
    }
}

/// 以前のアバター画像を削除する（失敗しても API は成功させ、ログにだけ残す）
async fn remove_avatar_object(state: &AppState, key: &str) {
    if !key.starts_with(AVATAR_PREFIX) {
        return;
    }
    if let Err(e) = state.storage.delete(key).await {
        tracing::warn!("Failed to delete the old avatar '{}': {}", key, e);
    }
}

/// `PUT /api/v1/users/{username}/avatar` — アバター画像（multipart の `file`。PNG / JPEG / GIF /
/// WebP、2 MB まで）を設定する（本人か admin）
pub async fn upload_avatar(
    State(state): State<Arc<AppState>>,
    RequireRole(session): ViewerSession,
    audit: AuditContext,
    Path(username): Path<String>,
    mut multipart: Multipart,
) -> Result<Json<UserView>, ApiError> {
    require_self_or_admin(&state, &audit, &session, &username).await?;
    let user = state
        .db
        .get_user_by_username(&username)
        .await
        .map_err(from_aegis)?
        .ok_or_else(|| {
            api_error(
                StatusCode::NOT_FOUND,
                format!("User '{}' does not exist", username),
            )
        })?;

    let mut data: Option<Vec<u8>> = None;
    while let Ok(Some(field)) = multipart.next_field().await {
        if field.name() != Some("file") {
            continue;
        }
        let bytes = field.bytes().await.map_err(|e| {
            let status = e.status();
            if status == StatusCode::PAYLOAD_TOO_LARGE {
                api_error(status, "Avatar image is too large (limit: 2 MB)")
            } else {
                api_error(status, format!("Failed to read multipart file: {}", e))
            }
        })?;
        data = Some(bytes.to_vec());
    }
    let data = data.ok_or_else(|| {
        api_error(
            StatusCode::BAD_REQUEST,
            "No 'file' field found in multipart request",
        )
    })?;
    if data.len() > MAX_AVATAR_BYTES {
        return Err(api_error(
            StatusCode::PAYLOAD_TOO_LARGE,
            "Avatar image is too large (limit: 2 MB)",
        ));
    }
    let (content_type, extension) = detect_image(&data).ok_or_else(|| {
        api_error(
            StatusCode::UNSUPPORTED_MEDIA_TYPE,
            "Avatar must be a PNG, JPEG, GIF or WebP image",
        )
    })?;

    // 画像を差し替えるたびにキーを変え、ブラウザ・CDN のキャッシュに古い画像が残らないようにする
    let suffix = Uuid::new_v4().simple().to_string();
    let key = format!(
        "{}{}-{}.{}",
        AVATAR_PREFIX,
        user.id,
        &suffix[..8],
        extension
    );
    if let Err(e) = state.storage.put(&key, &data, content_type).await {
        record_user_event(
            &state,
            &audit,
            AuditAction::UserUpdate,
            &username,
            &Err(e.to_string()),
            json!({ "changed": ["avatar"] }),
        )
        .await;
        return Err(api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Storage put error: {}", e),
        ));
    }
    let (user, previous) = users::set_avatar(state.db.as_ref(), &username, Some(key.clone()))
        .await
        .map_err(from_aegis)?;
    if let Some(previous) = previous.filter(|p| *p != key) {
        remove_avatar_object(&state, &previous).await;
    }
    record_user_event(
        &state,
        &audit,
        AuditAction::UserUpdate,
        &username,
        &Ok(()),
        json!({ "changed": ["avatar"], "avatar_key": key, "size": data.len() }),
    )
    .await;
    Ok(Json(view(&state, user)))
}

/// `DELETE /api/v1/users/{username}/avatar` — アバター画像を外す（本人か admin）
pub async fn delete_avatar(
    State(state): State<Arc<AppState>>,
    RequireRole(session): ViewerSession,
    audit: AuditContext,
    Path(username): Path<String>,
) -> Result<Json<UserView>, ApiError> {
    require_self_or_admin(&state, &audit, &session, &username).await?;
    let (user, previous) = users::set_avatar(state.db.as_ref(), &username, None)
        .await
        .map_err(from_aegis)?;
    if let Some(previous) = &previous {
        remove_avatar_object(&state, previous).await;
        record_user_event(
            &state,
            &audit,
            AuditAction::UserUpdate,
            &username,
            &Ok(()),
            json!({ "changed": ["avatar"], "avatar_key": null }),
        )
        .await;
    }
    Ok(Json(view(&state, user)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use aegis_auth::users::create_user as create_account;

    fn session(user_id: &str, role: &str) -> SessionData {
        SessionData {
            id: format!("ses_{}", user_id),
            user_id: user_id.to_string(),
            role: role.to_string(),
            created_at: String::new(),
            expires_at: String::new(),
            metadata: Default::default(),
        }
    }

    async fn state_with_users() -> Arc<AppState> {
        let state = crate::test_support::test_state(aegis_runtime::PluginSupervisor::new()).await;
        for (username, role) in [("root", "admin"), ("alice", "editor"), ("bob", "viewer")] {
            create_account(
                state.db.as_ref(),
                NewUser {
                    username: username.to_string(),
                    password: "initial-pass".to_string(),
                    role: role.to_string(),
                    email: None,
                    display_name: None,
                },
            )
            .await
            .unwrap();
        }
        state
    }

    async fn patch(
        state: &Arc<AppState>,
        actor: SessionData,
        username: &str,
        payload: serde_json::Value,
    ) -> Result<UserView, StatusCode> {
        update_user(
            State(state.clone()),
            RequireRole(actor),
            AuditContext::default(),
            Path(username.to_string()),
            Json(serde_json::from_value(payload).unwrap()),
        )
        .await
        .map(|Json(user)| user)
        .map_err(|(status, _)| status)
    }

    async fn actions(state: &Arc<AppState>) -> Vec<(AuditAction, AuditOutcome)> {
        let mut events = state
            .db
            .query_audit_events(&aegis_core::AuditQuery {
                limit: 50,
                ..Default::default()
            })
            .await
            .unwrap();
        events.reverse();
        events.into_iter().map(|e| (e.action, e.outcome)).collect()
    }

    #[tokio::test]
    async fn test_admin_manages_users() {
        let state = state_with_users().await;
        let admin = || RequireRole(session("root", "admin"));

        let (status, Json(created)) = create_user(
            State(state.clone()),
            admin(),
            AuditContext::default(),
            Json(CreateUserRequest {
                username: "carol".to_string(),
                password: "carol-password".to_string(),
                role: "editor".to_string(),
                email: Some("carol@example.com".to_string()),
                display_name: Some("Carol".to_string()),
            }),
        )
        .await
        .unwrap();
        assert_eq!(status, StatusCode::CREATED);
        assert_eq!(created.username, "carol");
        assert_eq!(created.avatar_url, None);

        let Json(list) = list_users(State(state.clone()), admin()).await.unwrap();
        let names: Vec<&str> = list["users"]
            .as_array()
            .unwrap()
            .iter()
            .map(|u| u["username"].as_str().unwrap())
            .collect();
        assert_eq!(names, ["alice", "bob", "carol", "root"]);
        assert!(list["users"][0].get("password_hash").is_none());

        let updated = patch(
            &state,
            session("root", "admin"),
            "carol",
            json!({ "role": "viewer", "disabled": true, "password": "reset-by-admin" }),
        )
        .await
        .unwrap();
        assert_eq!(updated.role, "viewer");
        assert!(updated.disabled);

        // Admins cannot lock themselves out
        assert_eq!(
            patch(
                &state,
                session("root", "admin"),
                "root",
                json!({ "role": "editor" })
            )
            .await,
            Err(StatusCode::BAD_REQUEST)
        );
        assert_eq!(
            delete_user(
                State(state.clone()),
                admin(),
                AuditContext::default(),
                Path("root".to_string())
            )
            .await
            .unwrap_err()
            .0,
            StatusCode::BAD_REQUEST
        );

        assert_eq!(
            delete_user(
                State(state.clone()),
                admin(),
                AuditContext::default(),
                Path("carol".to_string())
            )
            .await
            .unwrap(),
            StatusCode::NO_CONTENT
        );
        assert!(state
            .db
            .get_user_by_username("carol")
            .await
            .unwrap()
            .is_none());

        assert_eq!(
            actions(&state).await,
            vec![
                (AuditAction::UserCreate, AuditOutcome::Success),
                (AuditAction::UserUpdate, AuditOutcome::Success),
                (AuditAction::UserDelete, AuditOutcome::Success),
            ]
        );
        let events = state
            .db
            .query_audit_events(&aegis_core::AuditQuery {
                action: Some(AuditAction::UserUpdate),
                limit: 10,
                ..Default::default()
            })
            .await
            .unwrap();
        assert_eq!(
            events[0].details,
            json!({
                "changed": ["role", "disabled", "password"],
                "role": { "from": "editor", "to": "viewer" },
                "disabled": true,
            })
        );
    }

    #[tokio::test]
    async fn test_users_edit_only_their_own_profile() {
        let state = state_with_users().await;

        let me = patch(
            &state,
            session("bob", "viewer"),
            "bob",
            json!({ "display_name": "Bob B.", "email": "bob@example.com" }),
        )
        .await
        .unwrap();
        assert_eq!(me.display_name.as_deref(), Some("Bob B."));

        // Other users and privileged fields are off limits
        assert_eq!(
            patch(
                &state,
                session("bob", "viewer"),
                "alice",
                json!({ "display_name": "x" })
            )
            .await,
            Err(StatusCode::FORBIDDEN)
        );
        assert_eq!(
            patch(
                &state,
                session("bob", "viewer"),
                "bob",
                json!({ "role": "admin" })
            )
            .await,
            Err(StatusCode::FORBIDDEN)
        );

        // Changing one's own password needs the current one
        let new_password = json!({ "password": "bob-new-password" });
        assert_eq!(
            patch(&state, session("bob", "viewer"), "bob", new_password).await,
            Err(StatusCode::BAD_REQUEST)
        );
        assert_eq!(
            patch(
                &state,
                session("bob", "viewer"),
                "bob",
                json!({ "password": "bob-new-password", "current_password": "wrong-pass" })
            )
            .await,
            Err(StatusCode::FORBIDDEN)
        );
        patch(
            &state,
            session("bob", "viewer"),
            "bob",
            json!({ "password": "bob-new-password", "current_password": "initial-pass" }),
        )
        .await
        .unwrap();
        let stored = state.db.get_user_by_username("bob").await.unwrap().unwrap();
        assert!(verify_password("bob-new-password", &stored.password_hash).unwrap());

        let denied = get_user(
            State(state.clone()),
            RequireRole(session("bob", "viewer")),
            AuditContext::default(),
            Path("root".to_string()),
        )
        .await;
        assert_eq!(denied.unwrap_err().0, StatusCode::FORBIDDEN);
        assert!(actions(&state)
            .await
            .contains(&(AuditAction::AccessDenied, AuditOutcome::Denied)));
    }

    #[test]
    fn test_detect_image() {
        assert_eq!(
            detect_image(b"\x89PNG\r\n\x1a\nrest"),
            Some(("image/png", "png"))
        );
        assert_eq!(
            detect_image(&[0xFF, 0xD8, 0xFF, 0xE0]),
            Some(("image/jpeg", "jpg"))
        );
        assert_eq!(detect_image(b"GIF89a..."), Some(("image/gif", "gif")));
        assert_eq!(
            detect_image(b"RIFF\x00\x00\x00\x00WEBPVP8 "),
            Some(("image/webp", "webp"))
        );
        assert_eq!(detect_image(b"<svg onload=alert(1)>"), None);
        assert_eq!(detect_image(b""), None);
    }

    #[tokio::test]
    async fn test_avatar_is_replaced_and_removed() {
        let state = state_with_users().await;
        let alice = state
            .db
            .get_user_by_username("alice")
            .await
            .unwrap()
            .unwrap();

        let (key, _) = users::set_avatar(
            state.db.as_ref(),
            "alice",
            Some(format!("{}{}-first.png", AVATAR_PREFIX, alice.id)),
        )
        .await
        .unwrap();
        let first = key.avatar_key.unwrap();
        state
            .storage
            .put(&first, b"png", "image/png")
            .await
            .unwrap();

        let Json(cleared) = delete_avatar(
            State(state.clone()),
            RequireRole(session("alice", "editor")),
            AuditContext::default(),
            Path("alice".to_string()),
        )
        .await
        .unwrap();
        assert_eq!(cleared.avatar_url, None);
        assert!(!state.storage.exists(&first).await.unwrap());
    }

    async fn upload(
        state: &Arc<AppState>,
        token: &str,
        username: &str,
        file: &[u8],
    ) -> (StatusCode, serde_json::Value) {
        use tower::ServiceExt;
        let boundary = "aegis-test-boundary";
        let mut body = format!(
            "--{b}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"me.png\"\r\n\
             Content-Type: image/png\r\n\r\n",
            b = boundary
        )
        .into_bytes();
        body.extend_from_slice(file);
        body.extend_from_slice(format!("\r\n--{}--\r\n", boundary).as_bytes());
        let request = axum::http::Request::put(format!("/api/v1/users/{}/avatar", username))
            .header("authorization", format!("Bearer {}", token))
            .header(
                "content-type",
                format!("multipart/form-data; boundary={}", boundary),
            )
            .body(axum::body::Body::from(body))
            .unwrap();
        let response = crate::app::router(&state.config, state.clone())
            .oneshot(request)
            .await
            .unwrap();
        let status = response.status();
        let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        (status, serde_json::from_slice(&bytes).unwrap_or_default())
    }

    #[tokio::test]
    async fn test_avatar_upload_through_the_router() {
        let state = state_with_users().await;
        let bob = state
            .sessions
            .create_session("bob", "viewer", Default::default(), None)
            .await
            .unwrap();
        let png = b"\x89PNG\r\n\x1a\n-not-really-pixels";

        let (status, first) = upload(&state, &bob.id, "bob", png).await;
        assert_eq!(status, StatusCode::OK, "{}", first);
        let first_url = first["avatar_url"].as_str().unwrap().to_string();
        assert!(first_url.starts_with("/uploads/avatars/"), "{}", first_url);
        assert!(first_url.ends_with(".png"));
        let first_key = first_url.trim_start_matches("/uploads/").to_string();
        assert!(state.storage.exists(&first_key).await.unwrap());

        // Replacing the avatar removes the old file
        let (_, second) = upload(&state, &bob.id, "bob", png).await;
        assert_ne!(second["avatar_url"], first["avatar_url"]);
        assert!(!state.storage.exists(&first_key).await.unwrap());

        let (status, _) = upload(&state, &bob.id, "bob", b"<svg/>").await;
        assert_eq!(status, StatusCode::UNSUPPORTED_MEDIA_TYPE);
        let (status, _) = upload(&state, &bob.id, "alice", png).await;
        assert_eq!(status, StatusCode::FORBIDDEN);
        let too_large = [png.as_slice(), &vec![0u8; MAX_AVATAR_BYTES]].concat();
        let (status, _) = upload(&state, &bob.id, "bob", &too_large).await;
        assert_eq!(status, StatusCode::PAYLOAD_TOO_LARGE);

        let stored = state.db.get_user_by_username("bob").await.unwrap().unwrap();
        state
            .storage
            .delete(stored.avatar_key.as_deref().unwrap())
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn test_session_revocation_on_role_change_and_disable_and_delete() {
        let state = state_with_users().await;
        let admin_actor = session("root", "admin");

        // 1. alice (editor) logs in and gets a session
        let alice_sess = state
            .sessions
            .create_session("alice", "editor", Default::default(), None)
            .await
            .unwrap();
        assert!(state
            .sessions
            .get_session(&alice_sess.id)
            .await
            .unwrap()
            .is_some());

        tokio::time::sleep(std::time::Duration::from_millis(5)).await;

        // 2. Admin changes alice's role to viewer
        patch(
            &state,
            admin_actor.clone(),
            "alice",
            json!({ "role": "viewer" }),
        )
        .await
        .unwrap();

        // 3. alice's old session is now revoked (get_session returns None)
        assert_eq!(
            state.sessions.get_session(&alice_sess.id).await.unwrap(),
            None
        );

        tokio::time::sleep(std::time::Duration::from_millis(5)).await;

        // 4. alice logs in again with the new role
        let alice_sess2 = state
            .sessions
            .create_session("alice", "viewer", Default::default(), None)
            .await
            .unwrap();
        assert!(state
            .sessions
            .get_session(&alice_sess2.id)
            .await
            .unwrap()
            .is_some());

        tokio::time::sleep(std::time::Duration::from_millis(5)).await;

        // 5. Admin disables alice
        patch(
            &state,
            admin_actor.clone(),
            "alice",
            json!({ "disabled": true }),
        )
        .await
        .unwrap();

        // alice's session is revoked again
        assert_eq!(
            state.sessions.get_session(&alice_sess2.id).await.unwrap(),
            None
        );

        tokio::time::sleep(std::time::Duration::from_millis(5)).await;

        // 6. bob creates a session, then admin deletes bob
        let bob_sess = state
            .sessions
            .create_session("bob", "viewer", Default::default(), None)
            .await
            .unwrap();
        assert!(state
            .sessions
            .get_session(&bob_sess.id)
            .await
            .unwrap()
            .is_some());

        tokio::time::sleep(std::time::Duration::from_millis(5)).await;

        delete_user(
            State(state.clone()),
            RequireRole(admin_actor),
            AuditContext::default(),
            Path("bob".to_string()),
        )
        .await
        .unwrap();

        // bob's session is now revoked
        assert_eq!(
            state.sessions.get_session(&bob_sess.id).await.unwrap(),
            None
        );
    }

    #[tokio::test]
    async fn test_approval_notification_when_admin_enables_user() {
        use crate::test_support::RecordingNotifier;

        let state = crate::test_support::test_state_with_plugins(
            aegis_runtime::PluginSupervisor::new(),
            Vec::new(),
        )
        .await;
        let mails = Arc::new(RecordingNotifier::default());
        let mut state_builder = (*state).clone();
        state_builder.notifier = Some(mails.clone());
        state_builder.admin_url = Some("https://cms.example.com/admin/".to_string());
        let state = Arc::new(state_builder);

        // 承認待ちユーザーを作成（disabled: true, pending_approval: true）
        let user = aegis_auth::users::insert_new_user(
            state.db.as_ref(),
            NewUser {
                username: "pending_user".to_string(),
                password: String::new(),
                role: "viewer".to_string(),
                email: Some("pending@example.com".to_string()),
                display_name: Some("Pending User".to_string()),
            },
            "hash".to_string(),
            true,
            true,
        )
        .await
        .unwrap();
        assert!(user.disabled);
        assert!(user.pending_approval);

        let admin_actor = session("admin", "admin");

        // admin がユーザーを有効化する
        let resp = patch(
            &state,
            admin_actor,
            "pending_user",
            json!({ "disabled": false }),
        )
        .await
        .unwrap();

        assert!(!resp.disabled);
        assert!(!resp.pending_approval);

        // 承認メールが送られたことを確認
        let events = mails.take();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].kind, "account.approved");
        assert_eq!(events[0].data["username"], "pending_user");
        assert_eq!(
            events[0].data["login_url"],
            "https://cms.example.com/admin/"
        );
    }
}
