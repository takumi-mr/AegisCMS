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
use crate::auth::hook::{run_auth_hook, RESERVED_METADATA_KEYS};
use crate::auth::local::PASSWORD_CREDENTIALS;
use crate::auth::{extract_session_id, session_fingerprint, SessionData};
use crate::state::AppState;
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
                match state.login_limiter.record_failure(&username, ip).await {
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
    use crate::auth::users::{create_user, NewUser};
    use aegis_core::{AegisConfig, LoginRateLimitConfig};

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
}
