//! シングルサインオン（OpenID Connect / SAML）のハンドラーの共通部分: ログイン方法の一覧、IdP への
//! リダイレクト、検証済みのログインの完了と Admin Studio への戻り

use axum::{
    extract::State,
    http::{header, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use serde_json::json;
use std::sync::Arc;

use super::auth::{complete_login, LoginRefusal};
use crate::audit::AuditContext;
use crate::state::{AppState, SsoLogin};
use aegis_core::{AegisError, AuditAction, AuditOutcome, AuthCredentials};

/// `login_error` for failures on the server side (details go to the log and the audit log)
const SERVER_ERROR: &str = "server_error";

/// `GET /api/v1/auth/providers` — ログイン画面に表示するログイン方法
pub async fn list_auth_providers(State(state): State<Arc<AppState>>) -> Json<serde_json::Value> {
    Json(json!({
        "local": true,
        // Admin Studio のログイン画面に「アカウントを作成」を出す（`[auth.registration]`）
        "registration": state.registration.is_some(),
        // ログイン画面に「パスワードを忘れた」を出す（`[auth.password_reset]`）
        "password_reset": state.password_reset.is_some(),
        "invitation": state.invitation.is_some(),
        "oidc": state.oidc.as_ref().map(|oidc| json!({
            "display_name": oidc.display_name,
            "login_url": "/api/v1/auth/oidc/login",
        })),
        "saml": state.saml.as_ref().map(|saml| json!({
            "display_name": saml.sso.display_name,
            "login_url": "/api/v1/auth/saml/login",
        })),
    }))
}

pub(super) fn sso_disabled(name: &str) -> Response {
    (
        StatusCode::NOT_FOUND,
        Json(json!({ "error": format!("{} login is not enabled", name) })),
    )
        .into_response()
}

/// Minimal percent-encoding for a URL fragment value (RFC 3986 unreserved characters kept)
fn encode_fragment_value(value: &str) -> String {
    value
        .bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                (b as char).to_string()
            }
            _ => format!("%{:02X}", b),
        })
        .collect()
}

/// Sends the browser back to Admin Studio with `#{key}={value}` (`session` or `login_error`).
/// A fragment never reaches servers or `Referer` headers, unlike a query string.
pub(super) fn back_to_admin(sso: &SsoLogin, key: &str, value: &str) -> Response {
    let base = sso
        .post_login_redirect
        .split('#')
        .next()
        .unwrap_or_default();
    let location = format!("{}#{}={}", base, key, encode_fragment_value(value));
    (
        StatusCode::SEE_OTHER,
        [
            (header::LOCATION, location),
            (header::CACHE_CONTROL, "no-store".to_string()),
            (header::REFERRER_POLICY, "no-referrer".to_string()),
        ],
    )
        .into_response()
}

/// Redirects to the IdP (`AuthProvider::initiate_login`); failures go back to the login screen
pub(super) async fn redirect_to_idp(sso: &SsoLogin) -> Response {
    match sso.provider.initiate_login("").await {
        Ok(Some(redirect)) => (
            StatusCode::SEE_OTHER,
            [
                (header::LOCATION, redirect.url),
                (header::CACHE_CONTROL, "no-store".to_string()),
            ],
        )
            .into_response(),
        Ok(None) => back_to_admin(sso, "login_error", SERVER_ERROR),
        Err(e) => {
            tracing::error!(
                "Failed to start the {} login: {}",
                sso.provider.provider_id(),
                e
            );
            back_to_admin(sso, "login_error", SERVER_ERROR)
        }
    }
}

/// Verifies what the IdP sent back, issues a session and returns to Admin Studio with
/// `#session=<トークン>`（失敗時は `#login_error=<理由>`）
pub(super) async fn finish_sso_login(
    state: &AppState,
    audit: &AuditContext,
    sso: &SsoLogin,
    credentials: AuthCredentials,
) -> Response {
    let provider = sso.provider.provider_id().to_string();
    let identity = match sso.provider.authenticate(&credentials).await {
        Ok(identity) => identity,
        Err(e) => {
            let reason = match &e {
                AegisError::Authentication(reason) => reason.clone(),
                other => {
                    tracing::error!("{} login failed: {}", provider, other);
                    SERVER_ERROR.to_string()
                }
            };
            state
                .audit
                .record(
                    audit
                        .event(AuditAction::SessionCreate, AuditOutcome::Failure)
                        .with_resource("session", None)
                        .with_details(json!({
                            "provider": provider,
                            "error": reason,
                            "detail": e.to_string(),
                        })),
                )
                .await;
            return back_to_admin(sso, "login_error", &reason);
        }
    };
    match complete_login(state, audit, &provider, identity, None).await {
        Ok(session) => back_to_admin(sso, "session", &session.id),
        Err(LoginRefusal::Rejected(message)) => back_to_admin(sso, "login_error", &message),
        Err(LoginRefusal::Failed(_)) => back_to_admin(sso, "login_error", SERVER_ERROR),
    }
}
