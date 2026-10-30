//! パスワード再設定 API（`/api/v1/auth/password-reset`）: 申し込み（再設定メール送信）と、
//! トークンによる新パスワードの設定。どちらもログイン不要

use axum::{
    extract::State,
    http::{header, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use serde::Deserialize;
use serde_json::json;
use std::sync::Arc;

use crate::audit::{AuditContext, ClientIp};
use crate::state::{AppState, PasswordReset};
use aegis_auth::password_reset::{PasswordResetError, RequestedReset};
use aegis_core::{AuditAction, AuditOutcome, NotificationEvent, Recipient};

/// 同じクライアント IP からの再設定申し込みを数える期間（秒）
const IP_WINDOW_SECS: u64 = 3600;

fn password_reset_disabled() -> Response {
    (
        StatusCode::NOT_FOUND,
        Json(json!({ "error": "Password reset is not enabled" })),
    )
        .into_response()
}

fn error_response(error: &PasswordResetError) -> Response {
    let status = match error {
        PasswordResetError::Internal(_) => StatusCode::INTERNAL_SERVER_ERROR,
        _ => StatusCode::BAD_REQUEST,
    };
    (
        status,
        Json(json!({ "error": error.to_string(), "code": error.code() })),
    )
        .into_response()
}

fn outcome_of(error: &PasswordResetError) -> AuditOutcome {
    match error {
        PasswordResetError::Internal(_) => AuditOutcome::Failure,
        _ => AuditOutcome::Denied,
    }
}

#[derive(Debug, Deserialize)]
pub struct ResetRequestBody {
    pub email: String,
}

/// `POST /api/v1/auth/password-reset` — 再設定を申し込む。
/// 有効なユーザーがいれば再設定メールを送信し、202 を返す。
/// メールアドレスの存在有無を伏せるため、見つからなくても同じ応答を返す。
pub async fn request_password_reset(
    State(state): State<Arc<AppState>>,
    audit: AuditContext,
    ClientIp(client_ip): ClientIp,
    Json(payload): Json<ResetRequestBody>,
) -> Response {
    let Some(reset) = state.password_reset.clone() else {
        return password_reset_disabled();
    };
    let email = payload.email.trim().to_string();

    let limit = reset.service.config().max_per_ip_per_hour;
    if let (true, Some(ip)) = (limit > 0, client_ip.as_deref()) {
        let count = state
            .cache
            .increment(&format!("password_reset:ip:{}", ip), Some(IP_WINDOW_SECS))
            .await;
        match count {
            Ok(count) if count > limit => {
                state
                    .audit
                    .record(
                        audit
                            .event(AuditAction::UserPasswordReset, AuditOutcome::Denied)
                            .with_details(json!({
                                "stage": "requested",
                                "error": "rate_limited",
                            })),
                    )
                    .await;
                return (
                    StatusCode::TOO_MANY_REQUESTS,
                    [(header::RETRY_AFTER, IP_WINDOW_SECS.to_string())],
                    Json(json!({
                        "error": "Too many password reset requests from this address. Try again later.",
                        "code": "rate_limited",
                        "retry_after_secs": IP_WINDOW_SECS,
                    })),
                )
                    .into_response();
            }
            Ok(_) => {}
            Err(e) => tracing::warn!("Failed to count password reset requests: {}", e),
        }
    }

    match reset.service.request(&email).await {
        Ok(Some(requested)) => {
            send_reset_email(&reset, &requested);
            state
                .audit
                .record(
                    audit
                        .event(AuditAction::UserPasswordReset, AuditOutcome::Success)
                        .with_resource("user", Some(requested.username.clone()))
                        .with_details(json!({
                            "stage": "requested",
                            "username": requested.username,
                        })),
                )
                .await;
            accepted(&reset)
        }
        Ok(None) => {
            // 存在しないメールアドレス、または無効化アカウント
            state
                .audit
                .record(
                    audit
                        .event(AuditAction::UserPasswordReset, AuditOutcome::Denied)
                        .with_details(json!({
                            "stage": "requested",
                            "error": "user_not_found_or_disabled",
                        })),
                )
                .await;
            accepted(&reset)
        }
        Err(e) => {
            state
                .audit
                .record(
                    audit
                        .event(AuditAction::UserPasswordReset, outcome_of(&e))
                        .with_details(json!({
                            "stage": "requested",
                            "error": e.code(),
                        })),
                )
                .await;
            error_response(&e)
        }
    }
}

fn accepted(reset: &PasswordReset) -> Response {
    (
        StatusCode::ACCEPTED,
        Json(json!({
            "status": "pending_reset",
            "expires_in_secs": reset.service.config().token_ttl_secs,
        })),
    )
        .into_response()
}

fn send_reset_email(reset: &PasswordReset, requested: &RequestedReset) {
    let ttl = reset.service.config().token_ttl_secs;
    let name = requested
        .display_name
        .clone()
        .unwrap_or_else(|| requested.username.clone());
    reset.notifier.notify(NotificationEvent::new(
        "account.password_reset",
        vec![Recipient::person(&requested.email, name)],
        json!({
            "username": requested.username,
            "display_name": requested.display_name,
            "reset_url": format!("{}#reset-password={}", reset.admin_url, requested.token),
            "expires_hours": ttl.div_ceil(3600),
        }),
    ));
}

#[derive(Debug, Deserialize)]
pub struct ConfirmResetRequest {
    pub token: String,
    pub password: String,
}

/// `POST /api/v1/auth/password-reset/confirm` — トークンで新しいパスワードを設定する。
/// 完了時に該当ユーザーの既存セッションをすべて失効させる
pub async fn confirm_password_reset(
    State(state): State<Arc<AppState>>,
    audit: AuditContext,
    Json(payload): Json<ConfirmResetRequest>,
) -> Response {
    let Some(reset) = state.password_reset.clone() else {
        return password_reset_disabled();
    };

    match reset
        .service
        .complete(&payload.token, &payload.password)
        .await
    {
        Ok(user) => {
            if let Err(e) = state.sessions.revoke_user_sessions(&user.username).await {
                tracing::warn!("Failed to revoke sessions for '{}': {}", user.username, e);
            }
            state
                .audit
                .record(
                    audit
                        .event(AuditAction::UserPasswordReset, AuditOutcome::Success)
                        .with_resource("user", Some(user.username.clone()))
                        .with_details(json!({
                            "stage": "confirmed",
                            "username": user.username,
                        })),
                )
                .await;
            (
                StatusCode::OK,
                Json(json!({
                    "status": "ok",
                    "username": user.username,
                })),
            )
                .into_response()
        }
        Err(e) => {
            state
                .audit
                .record(
                    audit
                        .event(AuditAction::UserPasswordReset, outcome_of(&e))
                        .with_details(json!({
                            "stage": "confirmed",
                            "error": e.code(),
                        })),
                )
                .await;
            error_response(&e)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::RecordingNotifier;
    use aegis_auth::password::verify_password;
    use aegis_auth::password_reset::PasswordResetService;
    use aegis_auth::users::{create_user, NewUser};
    use aegis_auth::PasswordResetConfig;

    async fn setup() -> (Arc<AppState>, Arc<RecordingNotifier>) {
        let state = crate::test_support::test_state_with_plugins(
            aegis_runtime::PluginSupervisor::new(),
            Vec::new(),
        )
        .await;
        let mails = Arc::new(RecordingNotifier::default());
        let service = PasswordResetService::new(
            state.db.clone(),
            state.cache.clone(),
            PasswordResetConfig {
                enabled: true,
                token_ttl_secs: 3600,
                max_per_ip_per_hour: 5,
                admin_url: String::new(),
            },
        );
        let state = (*state).clone().with_password_reset(PasswordReset {
            service,
            notifier: mails.clone(),
            admin_url: "https://cms.example.com/admin/".to_string(),
        });
        (Arc::new(state), mails)
    }

    #[tokio::test]
    async fn test_password_reset_flow() {
        let (state, mails) = setup().await;
        create_user(
            state.db.as_ref(),
            NewUser {
                username: "alice".to_string(),
                password: "original-pass".to_string(),
                role: "editor".to_string(),
                email: Some("alice@example.com".to_string()),
                display_name: Some("Alice".to_string()),
            },
        )
        .await
        .unwrap();

        // 申し込み
        let resp = request_password_reset(
            State(state.clone()),
            AuditContext::default(),
            ClientIp(Some("127.0.0.1".to_string())),
            Json(ResetRequestBody {
                email: "alice@example.com".to_string(),
            }),
        )
        .await;
        assert_eq!(resp.status(), StatusCode::ACCEPTED);

        let events = mails.take();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].kind, "account.password_reset");
        let reset_url = events[0].data["reset_url"].as_str().unwrap();
        let token = reset_url.split("#reset-password=").nth(1).unwrap();

        // パスワード確認 & 変更
        let resp = confirm_password_reset(
            State(state.clone()),
            AuditContext::default(),
            Json(ConfirmResetRequest {
                token: token.to_string(),
                password: "brand-new-password".to_string(),
            }),
        )
        .await;
        assert_eq!(resp.status(), StatusCode::OK);

        let user = state
            .db
            .get_user_by_username("alice")
            .await
            .unwrap()
            .unwrap();
        assert!(verify_password("brand-new-password", &user.password_hash).unwrap());
    }

    #[tokio::test]
    async fn test_unknown_email_returns_202_without_mail() {
        let (state, mails) = setup().await;
        let resp = request_password_reset(
            State(state.clone()),
            AuditContext::default(),
            ClientIp(Some("127.0.0.1".to_string())),
            Json(ResetRequestBody {
                email: "nonexistent@example.com".to_string(),
            }),
        )
        .await;
        assert_eq!(resp.status(), StatusCode::ACCEPTED);
        assert!(mails.take().is_empty());
    }
}
