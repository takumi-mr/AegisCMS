use axum::{
    extract::{Query, State},
    response::Response,
};
use std::collections::HashMap;
use std::sync::Arc;

use super::sso::{finish_sso_login, redirect_to_idp, sso_disabled};
use crate::audit::AuditContext;
use crate::state::AppState;
use aegis_auth::OIDC_CALLBACK_CREDENTIALS;
use aegis_core::AuthCredentials;

/// `GET /api/v1/auth/oidc/login` — IdP の認可エンドポイントへリダイレクトする
pub async fn oidc_login(State(state): State<Arc<AppState>>) -> Response {
    match &state.oidc {
        Some(oidc) => redirect_to_idp(oidc).await,
        None => sso_disabled("OIDC"),
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
        return sso_disabled("OIDC");
    };
    let credentials = AuthCredentials {
        kind: OIDC_CALLBACK_CREDENTIALS.to_string(),
        parameters,
        ..Default::default()
    };
    finish_sso_login(&state, &audit, oidc, credentials).await
}

#[cfg(all(test, feature = "oidc"))]
mod tests {
    use super::*;
    use crate::handlers::sso::list_auth_providers;
    use crate::state::SsoLogin;
    use aegis_auth::oidc::OidcAuthProvider;
    use aegis_auth::oidc_mock::MockIdp;
    use aegis_core::{AuditAction, AuditOutcome};
    use aegis_runtime::PluginSupervisor;
    use axum::{
        http::{header, StatusCode},
        Json,
    };
    use serde_json::json;

    async fn state_with_oidc(idp: &MockIdp, supervisor: PluginSupervisor) -> Arc<AppState> {
        let state = crate::test_support::test_state(supervisor).await;
        let mut config = idp.config();
        config.post_login_redirect = "http://localhost:5173/admin/".to_string();
        let provider =
            OidcAuthProvider::new(config, state.cache.clone(), state.db.clone()).unwrap();
        let state = Arc::try_unwrap(state)
            .ok()
            .expect("state is not shared yet");
        Arc::new(state.with_oidc(SsoLogin {
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
        assert_eq!(
            providers,
            json!({
                "local": true,
                "oidc": null,
                "saml": null,
                "registration": false,
                "password_reset": false,
                "invitation": false,
            })
        );
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
