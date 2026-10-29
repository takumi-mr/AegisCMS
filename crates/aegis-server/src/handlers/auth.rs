use axum::{
    extract::State,
    http::{header, HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use serde::Deserialize;
use serde_json::json;
use std::collections::HashMap;
use std::sync::Arc;

use crate::audit::{AuditContext, ClientIp};
use crate::auth::extract_session_id;
use crate::auth::hook::{run_auth_hook, RESERVED_METADATA_KEYS};
use crate::state::AppState;
use aegis_auth::local::PASSWORD_CREDENTIALS;
use aegis_auth::{session_fingerprint, SessionData};
use aegis_core::{
    AegisError, AuditAction, AuditActor, AuditOutcome, AuthCredentials, AuthIdentity,
};

#[derive(Debug, Deserialize)]
pub struct CreateSessionRequest {
    pub username: String,
    pub password: String,
    /// 有効期限（秒）。サーバーの既定値（`[cache].default_ttl_secs`）より長くはできない
    #[serde(default)]
    pub ttl_secs: Option<u64>,
}

/// `POST /api/v1/auth/session` — ユーザー名 / パスワードで認証し、セッションを発行する。
/// ロールはリクエストではなくユーザーアカウントから決まる。
/// 失敗が続いたユーザー名・クライアント IP は `[auth.login_rate_limit]` に従い一時的に `429` で拒否する
pub async fn create_session(
    State(state): State<Arc<AppState>>,
    audit: AuditContext,
    ClientIp(client_ip): ClientIp,
    Json(payload): Json<CreateSessionRequest>,
) -> Response {
    let username = payload.username.trim().to_string();
    let ip = client_ip.as_deref();
    let provider = state.auth_provider.provider_id();

    // ロック中は資格情報を検証しない（Argon2 の負荷もかけさせない）
    let lockout = match state.login_limiter.check(&username, ip).await {
        Ok(lockout) => lockout,
        Err(e) => {
            state
                .audit
                .record(
                    audit
                        .event(AuditAction::SessionCreate, AuditOutcome::Failure)
                        .with_resource("session", None)
                        .with_details(json!({
                            "username": username,
                            "provider": provider,
                            "error": format!("rate_limit_check_failed: {}", e),
                        })),
                )
                .await;
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": format!("Failed to check login rate limit: {}", e) })),
            )
                .into_response();
        }
    };
    if let Some(lockout) = lockout {
        state
            .audit
            .record(
                audit
                    .event(AuditAction::SessionCreate, AuditOutcome::Denied)
                    .with_resource("session", None)
                    .with_details(json!({
                        "username": username,
                        "provider": provider,
                        "error": "rate_limited",
                        "lock_scope": lockout.scope.as_str(),
                        "retry_after_secs": lockout.retry_after_secs,
                    })),
            )
            .await;
        return too_many_attempts(lockout.retry_after_secs);
    }

    let credentials = AuthCredentials {
        kind: PASSWORD_CREDENTIALS.to_string(),
        identifier: Some(username.clone()),
        secret: Some(payload.password),
        ..Default::default()
    };

    let identity = match state.auth_provider.authenticate(&credentials).await {
        Ok(identity) => identity,
        Err(e) => {
            // 失敗理由（ユーザー不在・パスワード不一致・無効化）は監査ログにだけ残し、応答では区別しない
            let (status, message, reason) = match &e {
                AegisError::Authentication(reason) => (
                    StatusCode::UNAUTHORIZED,
                    "Invalid username or password".to_string(),
                    reason.clone(),
                ),
                other => (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    format!("Failed to authenticate: {}", other),
                    other.to_string(),
                ),
            };
            let mut details = json!({
                "username": username,
                "provider": provider,
                "error": reason,
            });
            // サーバー側のエラーは利用者の失敗として数えない
            if status == StatusCode::UNAUTHORIZED {
                match state
                    .login_limiter
                    .record_failure_with_max_ip(&username, ip, state.system().max_login_attempts)
                    .await
                {
                    Ok(Some(locked)) => {
                        details["locked"] = json!(locked.scope.as_str());
                    }
                    Ok(None) => {}
                    Err(e) => tracing::warn!("Failed to record login failure: {}", e),
                }
            }
            state
                .audit
                .record(
                    audit
                        .event(AuditAction::SessionCreate, AuditOutcome::Failure)
                        .with_resource("session", None)
                        .with_details(details),
                )
                .await;
            return (status, Json(json!({ "error": message }))).into_response();
        }
    };

    // 2FA (TOTP) が有効か確認
    let user_record = state
        .db
        .get_user_by_username(&username)
        .await
        .ok()
        .flatten();
    if let Some(user) = &user_record {
        if user.totp_enabled {
            let challenge_token = format!("mfa_{}", uuid::Uuid::new_v4().simple());
            let challenge_data = json!({
                "username": username,
                "provider": provider,
                "identity": identity,
                "ttl_secs": payload.ttl_secs,
            });
            let cache_key = format!("mfa_challenge:{}", challenge_token);
            if let Err(e) = state
                .cache
                .set(&cache_key, &challenge_data.to_string(), Some(300))
                .await
            {
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(json!({ "error": format!("Failed to initiate 2FA challenge: {}", e) })),
                )
                    .into_response();
            }

            return (
                StatusCode::OK,
                Json(json!({
                    "mfa_required": true,
                    "challenge_token": challenge_token,
                    "user_id": username,
                })),
            )
                .into_response();
        }
    }

    if let Err(e) = state.login_limiter.record_success(&username).await {
        tracing::warn!("Failed to reset login failures: {}", e);
    }

    let max_ttl = state.config.cache.default_ttl_secs;
    let ttl_secs = payload.ttl_secs.map(|ttl| ttl.clamp(1, max_ttl));
    match complete_login(&state, &audit, provider, identity, ttl_secs).await {
        Ok(session) => (StatusCode::CREATED, Json(session)).into_response(),
        Err(LoginRefusal::Rejected(message)) => {
            (StatusCode::FORBIDDEN, Json(json!({ "error": message }))).into_response()
        }
        Err(LoginRefusal::Failed(message)) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": message })),
        )
            .into_response(),
    }
}

#[derive(Debug, Deserialize)]
pub struct Verify2faRequest {
    pub challenge_token: String,
    pub code: String,
}

/// `POST /api/v1/auth/session/verify-2fa` - 2FA チャレンジに対するコード（TOTP またはバックアップコード）を検証し、セッションを発行する
pub async fn verify_2fa(
    State(state): State<Arc<AppState>>,
    audit: AuditContext,
    ClientIp(client_ip): ClientIp,
    Json(payload): Json<Verify2faRequest>,
) -> Response {
    let ip = client_ip.as_deref();
    let cache_key = format!("mfa_challenge:{}", payload.challenge_token.trim());
    let challenge_json = match state.cache.get(&cache_key).await {
        Ok(Some(json_str)) => json_str,
        _ => {
            return (
                StatusCode::UNAUTHORIZED,
                Json(json!({ "error": "Invalid or expired verification challenge" })),
            )
                .into_response();
        }
    };

    // チャレンジは 1 回限り使用可能
    let _ = state.cache.delete(&cache_key).await;

    let challenge: serde_json::Value = match serde_json::from_str(&challenge_json) {
        Ok(v) => v,
        Err(_) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": "Malformed challenge data" })),
            )
                .into_response();
        }
    };

    let username = challenge["username"]
        .as_str()
        .unwrap_or_default()
        .to_string();
    let provider = challenge["provider"]
        .as_str()
        .unwrap_or_default()
        .to_string();
    let identity: AuthIdentity = match serde_json::from_value(challenge["identity"].clone()) {
        Ok(id) => id,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": format!("Invalid identity data: {}", e) })),
            )
                .into_response();
        }
    };
    let ttl_secs = challenge["ttl_secs"].as_u64();

    let user = match state.db.get_user_by_username(&username).await {
        Ok(Some(u)) => u,
        _ => {
            return (
                StatusCode::UNAUTHORIZED,
                Json(json!({ "error": "User not found" })),
            )
                .into_response();
        }
    };

    let valid_totp = if let Some(secret) = &user.totp_secret {
        aegis_auth::verify_totp_code(secret, &payload.code)
    } else {
        false
    };

    let mut is_backup_code = false;
    let is_valid = if valid_totp {
        true
    } else if let Some(codes_json) = &user.backup_codes {
        if let Some(remaining_json) =
            aegis_auth::verify_and_consume_backup_code(codes_json, &payload.code)
        {
            if let Err(e) =
                aegis_auth::users::update_backup_codes(&*state.db, &user.username, &remaining_json)
                    .await
            {
                tracing::error!(
                    "Failed to update backup codes for user {}: {}",
                    user.username,
                    e
                );
            }
            is_backup_code = true;
            true
        } else {
            false
        }
    } else {
        false
    };

    if !is_valid {
        let _ = state
            .login_limiter
            .record_failure_with_max_ip(&username, ip, state.system().max_login_attempts)
            .await;

        state
            .audit
            .record(
                audit
                    .event(AuditAction::SessionCreate, AuditOutcome::Failure)
                    .with_resource("session", None)
                    .with_details(json!({
                        "username": username,
                        "provider": provider,
                        "error": "invalid_2fa_code",
                    })),
            )
            .await;

        return (
            StatusCode::UNAUTHORIZED,
            Json(json!({ "error": "Invalid verification code" })),
        )
            .into_response();
    }

    if let Err(e) = state.login_limiter.record_success(&username).await {
        tracing::warn!("Failed to reset login failures: {}", e);
    }

    let max_ttl = state.config.cache.default_ttl_secs;
    let ttl_secs = ttl_secs.map(|ttl| ttl.clamp(1, max_ttl));
    match complete_login(&state, &audit, &provider, identity, ttl_secs).await {
        Ok(session) => {
            if is_backup_code {
                tracing::info!("User '{}' logged in using a 2FA backup code", username);
            }
            (StatusCode::CREATED, Json(session)).into_response()
        }
        Err(LoginRefusal::Rejected(message)) => {
            (StatusCode::FORBIDDEN, Json(json!({ "error": message }))).into_response()
        }
        Err(LoginRefusal::Failed(message)) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": message })),
        )
            .into_response(),
    }
}

/// Why a login whose credentials a provider verified did not end in a session
pub(crate) enum LoginRefusal {
    /// An `on-user-authenticated` plugin turned the user away (the plugin's message; 403)
    Rejected(String),
    /// A plugin or the session store failed (500)
    Failed(String),
}

/// 資格情報を検証済みのアイデンティティに `on-user-authenticated` プラグイン（ロールマッピング・
/// 属性の補完・ログインの拒否）を適用し、セッションを発行する。結果は `session.create`
/// （プラグインの失敗は `plugin.error` も）として監査する。拒否・失敗はログイン失敗回数には数えない
pub(crate) async fn complete_login(
    state: &AppState,
    audit: &AuditContext,
    provider: &str,
    identity: AuthIdentity,
    ttl_secs: Option<u64>,
) -> Result<SessionData, LoginRefusal> {
    let username = identity.external_id.clone();
    let authenticated_role = identity.role.clone();
    let identity = match run_auth_hook(state, provider, identity).await {
        Ok(identity) => identity,
        Err(e) => return Err(refuse_after_auth_hook(state, audit, &username, provider, e).await),
    };

    let actor = Some(AuditActor {
        user_id: identity.external_id.clone(),
        role: identity.role.clone(),
    });
    // プラグイン・プロバイダの属性。サーバーが設定するキーは上書きさせない
    let mut metadata: HashMap<String, serde_json::Value> = identity
        .metadata
        .iter()
        .filter(|(key, _)| !RESERVED_METADATA_KEYS.contains(&key.as_str()))
        .map(|(key, value)| (key.clone(), value.clone()))
        .collect();
    metadata.insert("provider".to_string(), json!(provider));
    if let Some(display_name) = &identity.display_name {
        metadata.insert("display_name".to_string(), json!(display_name));
    }
    if !identity.email.is_empty() {
        metadata.insert("email".to_string(), json!(identity.email));
    }

    let result = state
        .sessions
        .create_session(&identity.external_id, &identity.role, metadata, ttl_secs)
        .await;
    match result {
        Ok(session) => {
            let mut details = json!({
                "expires_at": session.expires_at,
                "provider": provider,
            });
            if identity.role != authenticated_role {
                details["role_mapped"] = json!({ "from": authenticated_role, "to": identity.role });
            }
            state
                .audit
                .record(
                    audit
                        .event(AuditAction::SessionCreate, AuditOutcome::Success)
                        .with_actor(actor)
                        .with_resource("session", Some(session_fingerprint(&session.id)))
                        .with_details(details),
                )
                .await;
            Ok(session)
        }
        Err(e) => {
            state
                .audit
                .record(
                    audit
                        .event(AuditAction::SessionCreate, AuditOutcome::Failure)
                        .with_actor(actor)
                        .with_resource("session", None)
                        .with_details(json!({ "error": e.to_string() })),
                )
                .await;
            Err(LoginRefusal::Failed(format!(
                "Failed to create session: {}",
                e
            )))
        }
    }
}

/// `on-user-authenticated` がログインを拒否した・失敗したときの監査記録
async fn refuse_after_auth_hook(
    state: &AppState,
    audit: &AuditContext,
    username: &str,
    provider: &str,
    error: AegisError,
) -> LoginRefusal {
    let rejected = matches!(error, AegisError::PluginRejected(_));
    if !rejected {
        state
            .audit
            .record(
                audit
                    .event(AuditAction::PluginError, AuditOutcome::Failure)
                    .with_resource("session", None)
                    .with_details(json!({
                        "hook": "on-user-authenticated",
                        "username": username,
                        "error": error.to_string(),
                    })),
            )
            .await;
    }
    state
        .audit
        .record(
            audit
                .event(
                    AuditAction::SessionCreate,
                    if rejected {
                        AuditOutcome::Denied
                    } else {
                        AuditOutcome::Failure
                    },
                )
                .with_resource("session", None)
                .with_details(json!({
                    "username": username,
                    "provider": provider,
                    "error": if rejected { "plugin_rejected" } else { "plugin_failed" },
                    "reason": error.to_string(),
                })),
        )
        .await;
    if rejected {
        LoginRefusal::Rejected(error.to_string())
    } else {
        LoginRefusal::Failed(format!("Login hook failed: {}", error))
    }
}

/// ログインのロック中に返す `429`。応答ではユーザー名と IP のどちらでロックされたかを区別しない
fn too_many_attempts(retry_after_secs: u64) -> Response {
    (
        StatusCode::TOO_MANY_REQUESTS,
        [(header::RETRY_AFTER, retry_after_secs.to_string())],
        Json(json!({
            "error": "Too many failed login attempts. Try again later.",
            "retry_after_secs": retry_after_secs,
        })),
    )
        .into_response()
}

pub async fn get_current_session(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Result<impl IntoResponse, (StatusCode, Json<serde_json::Value>)> {
    let session_id = extract_session_id(&headers).ok_or_else(|| {
        (
            StatusCode::UNAUTHORIZED,
            Json(json!({ "error": "Missing Authorization Bearer token or X-Session-Id header" })),
        )
    })?;

    let session = state
        .sessions
        .get_session(&session_id)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": format!("Session query error: {}", e) })),
            )
        })?
        .ok_or_else(|| {
            (
                StatusCode::UNAUTHORIZED,
                Json(json!({ "error": "Invalid or expired session" })),
            )
        })?;

    Ok((StatusCode::OK, Json(session)))
}

pub async fn delete_session(
    State(state): State<Arc<AppState>>,
    audit: AuditContext,
    headers: HeaderMap,
) -> Result<impl IntoResponse, (StatusCode, Json<serde_json::Value>)> {
    let session_id = extract_session_id(&headers).ok_or_else(|| {
        (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "Missing session identifier" })),
        )
    })?;

    let result = state.sessions.destroy_session(&session_id).await;
    let outcome = if result.is_ok() {
        AuditOutcome::Success
    } else {
        AuditOutcome::Failure
    };
    let details = match &result {
        Ok(()) => json!({}),
        Err(e) => json!({ "error": e.to_string() }),
    };
    state
        .audit
        .record(
            audit
                .event(AuditAction::SessionDelete, outcome)
                .with_resource("session", Some(session_fingerprint(&session_id)))
                .with_details(details),
        )
        .await;

    result.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": format!("Failed to delete session: {}", e) })),
        )
    })?;

    Ok((StatusCode::OK, Json(json!({ "status": "logged_out" }))))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::AegisConfig;
    use aegis_auth::users::{create_user, NewUser};
    use aegis_auth::LoginRateLimitConfig;

    async fn login(state: &Arc<AppState>, username: &str, password: &str, ip: &str) -> Response {
        let payload = CreateSessionRequest {
            username: username.to_string(),
            password: password.to_string(),
            ttl_secs: None,
        };
        create_session(
            State(state.clone()),
            AuditContext::default(),
            ClientIp(Some(ip.to_string())),
            Json(payload),
        )
        .await
    }

    #[tokio::test]
    async fn test_login_is_locked_after_repeated_failures() {
        let mut config = AegisConfig::default();
        config.auth.login_rate_limit = LoginRateLimitConfig {
            max_failures_per_user: 3,
            max_failures_per_ip: 0,
            ..Default::default()
        };
        let state = crate::test_support::test_state_with_config(
            config,
            aegis_runtime::PluginSupervisor::new(),
        )
        .await;
        create_user(
            state.db.as_ref(),
            NewUser {
                username: "alice".to_string(),
                password: "correct horse battery".to_string(),
                role: "editor".to_string(),
                email: None,
                display_name: None,
            },
        )
        .await
        .unwrap();

        // 成功するとそれまでの失敗回数はリセットされる
        let ip = "203.0.113.7";
        login(&state, "alice", "wrong", ip).await;
        login(&state, "alice", "wrong", ip).await;
        let ok = login(&state, "alice", "correct horse battery", ip).await;
        assert_eq!(ok.status(), StatusCode::CREATED);

        for _ in 0..3 {
            let resp = login(&state, "alice", "wrong", ip).await;
            assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
        }

        // ロック中は正しいパスワードでも拒否し、Retry-After を返す
        let locked = login(&state, "alice", "correct horse battery", "198.51.100.1").await;
        assert_eq!(locked.status(), StatusCode::TOO_MANY_REQUESTS);
        let retry_after: u64 = locked.headers()[header::RETRY_AFTER]
            .to_str()
            .unwrap()
            .parse()
            .unwrap();
        assert!(retry_after > 0 && retry_after <= 900);

        // 存在しないユーザーへの試行も同じように数える
        for _ in 0..3 {
            login(&state, "nobody", "wrong", ip).await;
        }
        assert_eq!(
            login(&state, "nobody", "wrong", ip).await.status(),
            StatusCode::TOO_MANY_REQUESTS
        );
    }

    async fn state_with_auth_plugin(mode: crate::auth::hook::tests::Mode) -> Arc<AppState> {
        let mut supervisor = aegis_runtime::PluginSupervisor::new();
        supervisor.register(crate::auth::hook::tests::auth_plugin(mode));
        let state = crate::test_support::test_state(supervisor).await;
        create_user(
            state.db.as_ref(),
            NewUser {
                username: "alice".to_string(),
                password: "correct horse battery".to_string(),
                role: "viewer".to_string(),
                email: Some("alice@example.com".to_string()),
                display_name: None,
            },
        )
        .await
        .unwrap();
        state
    }

    async fn body(resp: Response) -> serde_json::Value {
        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        serde_json::from_slice(&bytes).unwrap()
    }

    async fn login_events(state: &Arc<AppState>) -> Vec<aegis_core::AuditEvent> {
        let mut events = state
            .db
            .query_audit_events(&aegis_core::AuditQuery {
                limit: 50,
                ..Default::default()
            })
            .await
            .unwrap();
        events.reverse();
        events
    }

    #[tokio::test]
    async fn test_auth_hook_maps_the_session() {
        use crate::auth::hook::tests::Mode;
        // The local viewer has no groups, so the plugin maps her to editor
        let state = state_with_auth_plugin(Mode::Map).await;
        let resp = login(&state, "alice", "correct horse battery", "203.0.113.7").await;
        assert_eq!(resp.status(), StatusCode::CREATED);
        let session = body(resp).await;
        assert_eq!(session["user_id"], "alice");
        assert_eq!(session["role"], "editor");
        assert_eq!(session["metadata"]["display_name"], "Alice A.");
        assert_eq!(session["metadata"]["department"], "docs");
        assert_eq!(session["metadata"]["provider"], "local");

        let events = login_events(&state).await;
        assert_eq!(events[0].outcome, AuditOutcome::Success);
        assert_eq!(events[0].actor.as_ref().unwrap().role, "editor");
        assert_eq!(
            events[0].details["role_mapped"],
            json!({ "from": "viewer", "to": "editor" })
        );
    }

    #[tokio::test]
    async fn test_auth_hook_rejection_is_a_403_without_a_session() {
        use crate::auth::hook::tests::Mode;
        let state = state_with_auth_plugin(Mode::Reject).await;
        let resp = login(&state, "alice", "correct horse battery", "203.0.113.7").await;
        assert_eq!(resp.status(), StatusCode::FORBIDDEN);
        assert_eq!(
            body(resp).await["error"],
            "Rejected by plugin: guard: account is suspended"
        );

        let events = login_events(&state).await;
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].action, AuditAction::SessionCreate);
        assert_eq!(events[0].outcome, AuditOutcome::Denied);
        assert_eq!(events[0].details["error"], "plugin_rejected");
    }

    #[tokio::test]
    async fn test_auth_hook_failure_refuses_the_login() {
        use crate::auth::hook::tests::Mode;
        let state = state_with_auth_plugin(Mode::Fail).await;
        let resp = login(&state, "alice", "correct horse battery", "203.0.113.7").await;
        assert_eq!(resp.status(), StatusCode::INTERNAL_SERVER_ERROR);
        let actions: Vec<_> = login_events(&state)
            .await
            .into_iter()
            .map(|e| (e.action, e.outcome))
            .collect();
        assert_eq!(
            actions,
            vec![
                (AuditAction::PluginError, AuditOutcome::Failure),
                (AuditAction::SessionCreate, AuditOutcome::Failure),
            ]
        );
    }

    #[tokio::test]
    async fn test_two_factor_auth_flow_and_backup_codes() {
        use axum::body::to_bytes;
        let config = AegisConfig::default();
        let state = crate::test_support::test_state_with_config(
            config,
            aegis_runtime::PluginSupervisor::new(),
        )
        .await;

        create_user(
            state.db.as_ref(),
            NewUser {
                username: "alice".to_string(),
                password: "correct-password".to_string(),
                role: "editor".to_string(),
                email: None,
                display_name: None,
            },
        )
        .await
        .unwrap();

        // 2FA 有効化
        let setup = aegis_auth::generate_totp_setup("alice", "AegisCMS").unwrap();
        let (plain_codes, backup_json) = aegis_auth::generate_backup_codes();
        assert_eq!(plain_codes.len(), 6);
        aegis_auth::users::enable_2fa(state.db.as_ref(), "alice", &setup.secret, &backup_json)
            .await
            .unwrap();

        let ip = "203.0.113.10";

        // 1. パスワードログイン -> mfa_required: true と challenge_token が返る
        let resp = login(&state, "alice", "correct-password", ip).await;
        assert_eq!(resp.status(), StatusCode::OK);
        let body_bytes = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let body: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
        assert_eq!(body["mfa_required"], true);
        let challenge_token = body["challenge_token"].as_str().unwrap().to_string();

        // 2. 誤ったコードで verify_2fa -> 401 Unauthorized
        let bad_verify = verify_2fa(
            State(state.clone()),
            AuditContext::default(),
            ClientIp(Some(ip.to_string())),
            Json(Verify2faRequest {
                challenge_token: challenge_token.clone(),
                code: "000000".to_string(),
            }),
        )
        .await;
        assert_eq!(bad_verify.status(), StatusCode::UNAUTHORIZED);

        // チャレンジトークンは 1 回きりなので再利用できない
        let bad_retry = verify_2fa(
            State(state.clone()),
            AuditContext::default(),
            ClientIp(Some(ip.to_string())),
            Json(Verify2faRequest {
                challenge_token: challenge_token.clone(),
                code: "000000".to_string(),
            }),
        )
        .await;
        assert_eq!(bad_retry.status(), StatusCode::UNAUTHORIZED);

        // 3. 再ログインして challenge_token を再取得
        let resp2 = login(&state, "alice", "correct-password", ip).await;
        assert_eq!(resp2.status(), StatusCode::OK);
        let body2: serde_json::Value =
            serde_json::from_slice(&to_bytes(resp2.into_body(), usize::MAX).await.unwrap())
                .unwrap();
        let challenge_token2 = body2["challenge_token"].as_str().unwrap().to_string();

        // 4. 正しい TOTP コードで verify_2fa -> 201 Created でセッション発行
        let current_code = aegis_auth::generate_current_totp_code(&setup.secret).unwrap();

        let ok_verify = verify_2fa(
            State(state.clone()),
            AuditContext::default(),
            ClientIp(Some(ip.to_string())),
            Json(Verify2faRequest {
                challenge_token: challenge_token2,
                code: current_code,
            }),
        )
        .await;
        assert_eq!(ok_verify.status(), StatusCode::CREATED);

        // 5. バックアップコードでのログイン検証
        let resp3 = login(&state, "alice", "correct-password", ip).await;
        assert_eq!(resp3.status(), StatusCode::OK);
        let body3: serde_json::Value =
            serde_json::from_slice(&to_bytes(resp3.into_body(), usize::MAX).await.unwrap())
                .unwrap();
        let challenge_token3 = body3["challenge_token"].as_str().unwrap().to_string();

        let backup_code = &plain_codes[0];
        let backup_verify = verify_2fa(
            State(state.clone()),
            AuditContext::default(),
            ClientIp(Some(ip.to_string())),
            Json(Verify2faRequest {
                challenge_token: challenge_token3,
                code: backup_code.clone(),
            }),
        )
        .await;
        assert_eq!(backup_verify.status(), StatusCode::CREATED);

        // 6. 使用済みバックアップコードは 2 回目使えない
        let resp4 = login(&state, "alice", "correct-password", ip).await;
        let body4: serde_json::Value =
            serde_json::from_slice(&to_bytes(resp4.into_body(), usize::MAX).await.unwrap())
                .unwrap();
        let challenge_token4 = body4["challenge_token"].as_str().unwrap().to_string();

        let reuse_verify = verify_2fa(
            State(state.clone()),
            AuditContext::default(),
            ClientIp(Some(ip.to_string())),
            Json(Verify2faRequest {
                challenge_token: challenge_token4,
                code: backup_code.clone(),
            }),
        )
        .await;
        assert_eq!(reuse_verify.status(), StatusCode::UNAUTHORIZED);
    }
}
