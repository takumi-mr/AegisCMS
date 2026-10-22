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
use crate::auth::local::PASSWORD_CREDENTIALS;
use crate::auth::{extract_session_id, session_fingerprint};
use crate::state::AppState;
use aegis_core::{AegisError, AuditAction, AuditActor, AuditOutcome, AuthCredentials};

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

    let actor = Some(AuditActor {
        user_id: identity.external_id.clone(),
        role: identity.role.clone(),
    });
    let mut metadata = HashMap::new();
    metadata.insert("provider".to_string(), json!(provider));
    if let Some(display_name) = &identity.display_name {
        metadata.insert("display_name".to_string(), json!(display_name));
    }
    if !identity.email.is_empty() {
        metadata.insert("email".to_string(), json!(identity.email));
    }
    let max_ttl = state.config.cache.default_ttl_secs;
    let ttl_secs = payload.ttl_secs.map(|ttl| ttl.clamp(1, max_ttl));

    let result = state
        .sessions
        .create_session(&identity.external_id, &identity.role, metadata, ttl_secs)
        .await;

    let session = match result {
        Ok(session) => {
            state
                .audit
                .record(
                    audit
                        .event(AuditAction::SessionCreate, AuditOutcome::Success)
                        .with_actor(actor)
                        .with_resource("session", Some(session_fingerprint(&session.id)))
                        .with_details(json!({
                            "expires_at": session.expires_at,
                            "provider": provider,
                        })),
                )
                .await;
            session
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
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": format!("Failed to create session: {}", e) })),
            )
                .into_response();
        }
    };

    (StatusCode::CREATED, Json(session)).into_response()
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
        let state = crate::state::test_support::test_state(config).await;
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
}
