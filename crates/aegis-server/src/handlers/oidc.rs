use axum::{
    extract::{Query, State},
    http::{header, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use serde_json::json;
use std::collections::HashMap;
use std::sync::Arc;

use super::auth::{complete_login, LoginRefusal};
use crate::audit::AuditContext;
use crate::state::{AppState, OidcLogin};
use aegis_auth::{OIDC_CALLBACK_CREDENTIALS, OIDC_PROVIDER};
use aegis_core::{AegisError, AuditAction, AuditOutcome, AuthCredentials};

/// `login_error` for failures on the server side (details go to the log and the audit log)
const SERVER_ERROR: &str = "server_error";

/// `GET /api/v1/auth/providers` — ログイン画面に表示するログイン方法
pub async fn list_auth_providers(State(state): State<Arc<AppState>>) -> Json<serde_json::Value> {
    Json(json!({
        "local": true,
        "oidc": state.oidc.as_ref().map(|oidc| json!({
            "display_name": oidc.display_name,
            "login_url": "/api/v1/auth/oidc/login",
        })),
    }))
}

fn oidc_disabled() -> Response {
    (
        StatusCode::NOT_FOUND,
        Json(json!({ "error": "OIDC login is not enabled" })),
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
fn back_to_admin(oidc: &OidcLogin, key: &str, value: &str) -> Response {
    let base = oidc
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

/// `GET /api/v1/auth/oidc/login` — IdP の認可エンドポイントへリダイレクトする
pub async fn oidc_login(State(state): State<Arc<AppState>>) -> Response {
    let Some(oidc) = &state.oidc else {
        return oidc_disabled();
    };
    match oidc.provider.initiate_login("").await {
        Ok(Some(redirect)) => (
            StatusCode::SEE_OTHER,
            [
                (header::LOCATION, redirect.url),
                (header::CACHE_CONTROL, "no-store".to_string()),
            ],
        )
            .into_response(),
        Ok(None) => back_to_admin(oidc, "login_error", SERVER_ERROR),
        Err(e) => {
            tracing::error!("Failed to start the OIDC login: {}", e);
            back_to_admin(oidc, "login_error", SERVER_ERROR)
        }
    }
}

/// `GET /api/v1/auth/oidc/callback` — IdP からの戻り。認可コードを検証してセッションを発行し、
/// Admin Studio に `#session=<トークン>`（失敗時は `#login_error=<理由>`）で戻す
pub async fn oidc_callback(
    State(state): State<Arc<AppState>>,
    audit: AuditContext,
    Query(parameters): Query<HashMap<String, String>>,
) -> Response {
    let Some(oidc) = &state.oidc else {
        return oidc_disabled();
    };
    let credentials = AuthCredentials {
        kind: OIDC_CALLBACK_CREDENTIALS.to_string(),
        parameters,
        ..Default::default()
    };
    let identity = match oidc.provider.authenticate(&credentials).await {
        Ok(identity) => identity,
        Err(e) => {
            let reason = match &e {
                AegisError::Authentication(reason) => reason.clone(),
                other => {
                    tracing::error!("OIDC login failed: {}", other);
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
                            "provider": OIDC_PROVIDER,
                            "error": reason,
                            "detail": e.to_string(),
                        })),
                )
                .await;
            return back_to_admin(oidc, "login_error", &reason);
        }
    };
    match complete_login(&state, &audit, OIDC_PROVIDER, identity, None).await {
        Ok(session) => back_to_admin(oidc, "session", &session.id),
        Err(LoginRefusal::Rejected(message)) => back_to_admin(oidc, "login_error", &message),
        Err(LoginRefusal::Failed(_)) => back_to_admin(oidc, "login_error", SERVER_ERROR),
    }
}

#[cfg(all(test, feature = "oidc"))]
mod tests {
    use super::*;
    use aegis_auth::oidc::OidcAuthProvider;
    use aegis_auth::oidc_mock::MockIdp;
    use aegis_runtime::PluginSupervisor;

    async fn state_with_oidc(idp: &MockIdp, supervisor: PluginSupervisor) -> Arc<AppState> {
        let state = crate::test_support::test_state(supervisor).await;
        let mut config = idp.config();
        config.post_login_redirect = "http://localhost:5173/admin/".to_string();
        let provider =
            OidcAuthProvider::new(config, state.cache.clone(), state.db.clone()).unwrap();
        let state = Arc::try_unwrap(state)
            .ok()
            .expect("state is not shared yet");
        Arc::new(state.with_oidc(OidcLogin {
            provider: Arc::new(provider),
            display_name: "Example SSO".to_string(),
            post_login_redirect: "http://localhost:5173/admin/".to_string(),
        }))
    }

    fn location(resp: &Response) -> String {
        assert_eq!(resp.status(), StatusCode::SEE_OTHER);
        resp.headers()[header::LOCATION]
            .to_str()
            .unwrap()
            .to_string()
    }

    async fn sign_in(state: &Arc<AppState>, idp: &MockIdp, claims: serde_json::Value) -> String {
        let to_idp = location(&oidc_login(State(state.clone())).await);
        let parameters = idp.sign_in(&to_idp, claims);
        let resp = oidc_callback(
            State(state.clone()),
            AuditContext::default(),
            Query(parameters),
        )
        .await;
        location(&resp)
    }

    #[tokio::test]
    async fn test_providers_and_disabled_endpoints() {
        let state = crate::test_support::test_state(PluginSupervisor::new()).await;
        let Json(providers) = list_auth_providers(State(state.clone())).await;
        assert_eq!(providers, json!({ "local": true, "oidc": null }));
        assert_eq!(
            oidc_login(State(state)).await.status(),
            StatusCode::NOT_FOUND
        );

        let idp = MockIdp::start().await;
        let state = state_with_oidc(&idp, PluginSupervisor::new()).await;
        let Json(providers) = list_auth_providers(State(state)).await;
        assert_eq!(providers["oidc"]["display_name"], "Example SSO");
    }

    #[tokio::test]
    async fn test_callback_issues_a_session_in_the_fragment() {
        let idp = MockIdp::start().await;
        let state = state_with_oidc(&idp, PluginSupervisor::new()).await;
        let back = sign_in(
            &state,
            &idp,
            json!({ "sub": "u-1", "preferred_username": "carol", "email": "carol@example.com" }),
        )
        .await;
        let token = back
            .strip_prefix("http://localhost:5173/admin/#session=")
            .unwrap_or_else(|| panic!("unexpected redirect {}", back));
        let session = state.sessions.get_session(token).await.unwrap().unwrap();
        assert_eq!(session.user_id, "carol");
        assert_eq!(session.role, "viewer");
        assert_eq!(session.metadata["provider"], "oidc");
        assert_eq!(session.metadata["email"], "carol@example.com");

        let events = state
            .db
            .query_audit_events(&aegis_core::AuditQuery {
                limit: 10,
                ..Default::default()
            })
            .await
            .unwrap();
        assert_eq!(events[0].action, AuditAction::SessionCreate);
        assert_eq!(events[0].outcome, AuditOutcome::Success);
        assert_eq!(events[0].details["provider"], "oidc");
    }

    #[tokio::test]
    async fn test_failures_return_to_the_login_screen() {
        let idp = MockIdp::start().await;
        let state = state_with_oidc(&idp, PluginSupervisor::new()).await;

        let resp = oidc_callback(
            State(state.clone()),
            AuditContext::default(),
            Query(HashMap::from([
                ("code".to_string(), "c".to_string()),
                ("state".to_string(), "forged".to_string()),
            ])),
        )
        .await;
        assert_eq!(
            location(&resp),
            "http://localhost:5173/admin/#login_error=invalid_state"
        );

        // The on-user-authenticated hook still applies (and its message is passed back)
        let mut supervisor = PluginSupervisor::new();
        supervisor.register(crate::auth::hook::tests::auth_plugin(
            crate::auth::hook::tests::Mode::Reject,
        ));
        let state = state_with_oidc(&idp, supervisor).await;
        let back = sign_in(
            &state,
            &idp,
            json!({ "sub": "u-2", "preferred_username": "dave" }),
        )
        .await;
        assert_eq!(
            back,
            "http://localhost:5173/admin/#login_error=Rejected%20by%20plugin%3A%20guard%3A%20account%20is%20suspended"
        );
    }
}
