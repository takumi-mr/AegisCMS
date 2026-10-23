use axum::{
    extract::{rejection::FormRejection, State},
    http::{header, StatusCode},
    response::{IntoResponse, Response},
    Form,
};
use std::collections::HashMap;
use std::sync::Arc;

use super::sso::{back_to_admin, finish_sso_login, redirect_to_idp, sso_disabled};
use crate::audit::AuditContext;
use crate::state::AppState;
use aegis_auth::SAML_RESPONSE_CREDENTIALS;
use aegis_core::AuthCredentials;

/// `GET /api/v1/auth/saml/login` — `AuthnRequest` を付けて IdP の SSO サービスへリダイレクトする
pub async fn saml_login(State(state): State<Arc<AppState>>) -> Response {
    match &state.saml {
        Some(saml) => redirect_to_idp(&saml.sso).await,
        None => sso_disabled("SAML"),
    }
}

/// `POST /api/v1/auth/saml/acs` — Assertion Consumer Service。IdP がブラウザ経由で POST する
/// `SAMLResponse` を検証してセッションを発行し、Admin Studio に `#session=<トークン>`
/// （失敗時は `#login_error=<理由>`）で戻す
pub async fn saml_acs(
    State(state): State<Arc<AppState>>,
    audit: AuditContext,
    form: Result<Form<HashMap<String, String>>, FormRejection>,
) -> Response {
    let Some(saml) = &state.saml else {
        return sso_disabled("SAML");
    };
    let Ok(Form(parameters)) = form else {
        return back_to_admin(
            &saml.sso,
            "login_error",
            aegis_auth::sso::REASON_INVALID_TOKEN,
        );
    };
    let credentials = AuthCredentials {
        kind: SAML_RESPONSE_CREDENTIALS.to_string(),
        parameters,
        ..Default::default()
    };
    finish_sso_login(&state, &audit, &saml.sso, credentials).await
}

/// `GET /api/v1/auth/saml/metadata` — IdP に登録する SP メタデータ（Entity ID と ACS の URL）
pub async fn saml_metadata(State(state): State<Arc<AppState>>) -> Response {
    match &state.saml {
        Some(saml) => (
            StatusCode::OK,
            [(header::CONTENT_TYPE, "application/samlmetadata+xml")],
            saml.metadata.clone(),
        )
            .into_response(),
        None => sso_disabled("SAML"),
    }
}

#[cfg(all(test, feature = "saml"))]
mod tests {
    use super::*;
    use crate::handlers::sso::list_auth_providers;
    use crate::state::{SamlLogin, SsoLogin};
    use aegis_auth::saml::mock::{MockSamlIdp, MockUser};
    use aegis_auth::saml::SamlAuthProvider;
    use aegis_core::{AuditAction, AuditOutcome};
    use aegis_runtime::PluginSupervisor;
    use axum::Json;

    const ADMIN: &str = "http://localhost:5173/admin/";

    async fn state_with_saml(idp: &MockSamlIdp, supervisor: PluginSupervisor) -> Arc<AppState> {
        let state = crate::test_support::test_state(supervisor).await;
        let provider =
            SamlAuthProvider::new(idp.config(), state.cache.clone(), state.db.clone()).unwrap();
        let metadata = provider.sp_metadata();
        let state = Arc::try_unwrap(state)
            .ok()
            .expect("state is not shared yet");
        Arc::new(state.with_saml(SamlLogin {
            sso: SsoLogin {
                provider: Arc::new(provider),
                display_name: "Example SAML".to_string(),
                post_login_redirect: ADMIN.to_string(),
            },
            metadata,
        }))
    }

    fn location(resp: &Response) -> String {
        assert_eq!(resp.status(), StatusCode::SEE_OTHER);
        resp.headers()[header::LOCATION]
            .to_str()
            .unwrap()
            .to_string()
    }

    async fn post(state: &Arc<AppState>, saml_response: String) -> String {
        let form = HashMap::from([
            ("SAMLResponse".to_string(), saml_response),
            ("RelayState".to_string(), "ignored".to_string()),
        ]);
        location(
            &saml_acs(
                State(state.clone()),
                AuditContext::default(),
                Ok(Form(form)),
            )
            .await,
        )
    }

    async fn sign_in(state: &Arc<AppState>, idp: &MockSamlIdp, user: &MockUser) -> String {
        let to_idp = location(&saml_login(State(state.clone())).await);
        post(state, idp.sign_in(&to_idp, user)).await
    }

    #[tokio::test]
    async fn test_providers_metadata_and_disabled_endpoints() {
        let state = crate::test_support::test_state(PluginSupervisor::new()).await;
        assert_eq!(
            saml_login(State(state.clone())).await.status(),
            StatusCode::NOT_FOUND
        );
        assert_eq!(
            saml_metadata(State(state)).await.status(),
            StatusCode::NOT_FOUND
        );

        let idp = MockSamlIdp::new();
        let state = state_with_saml(&idp, PluginSupervisor::new()).await;
        let Json(providers) = list_auth_providers(State(state.clone())).await;
        assert_eq!(providers["saml"]["display_name"], "Example SAML");
        assert_eq!(providers["saml"]["login_url"], "/api/v1/auth/saml/login");

        let resp = saml_metadata(State(state)).await;
        assert_eq!(
            resp.headers()[header::CONTENT_TYPE],
            "application/samlmetadata+xml"
        );
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        assert!(String::from_utf8_lossy(&body).contains("AssertionConsumerService"));
    }

    #[tokio::test]
    async fn test_acs_issues_a_session_in_the_fragment() {
        let idp = MockSamlIdp::new();
        let state = state_with_saml(&idp, PluginSupervisor::new()).await;
        let user = MockUser::new("carol").attribute("email", &["carol@example.com"]);
        let back = sign_in(&state, &idp, &user).await;
        let token = back
            .strip_prefix(&format!("{}#session=", ADMIN))
            .unwrap_or_else(|| panic!("unexpected redirect {}", back));
        let session = state.sessions.get_session(token).await.unwrap().unwrap();
        assert_eq!(session.user_id, "carol");
        assert_eq!(session.role, "viewer");
        assert_eq!(session.metadata["provider"], "saml");
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
        assert_eq!(events[0].details["provider"], "saml");
    }

    #[tokio::test]
    async fn test_failures_return_to_the_login_screen() {
        let idp = MockSamlIdp::new();
        let state = state_with_saml(&idp, PluginSupervisor::new()).await;

        // Signed by an IdP this SP does not trust
        let to_idp = location(&saml_login(State(state.clone())).await);
        let other = MockSamlIdp::with_new_key();
        let back = post(&state, other.sign_in(&to_idp, &MockUser::new("eve"))).await;
        assert_eq!(back, format!("{}#login_error=invalid_token", ADMIN));
        let events = state
            .db
            .query_audit_events(&aegis_core::AuditQuery {
                limit: 10,
                ..Default::default()
            })
            .await
            .unwrap();
        assert_eq!(events[0].outcome, AuditOutcome::Failure);
        assert_eq!(events[0].details["provider"], "saml");
        assert_eq!(events[0].details["error"], "invalid_token");

        // Not a form post
        let resp = saml_acs(
            State(state.clone()),
            AuditContext::default(),
            Err(FormRejection::InvalidFormContentType(Default::default())),
        )
        .await;
        assert_eq!(
            location(&resp),
            format!("{}#login_error=invalid_token", ADMIN)
        );

        // The on-user-authenticated hook still applies (and its message is passed back)
        let mut supervisor = PluginSupervisor::new();
        supervisor.register(crate::auth::hook::tests::auth_plugin(
            crate::auth::hook::tests::Mode::Reject,
        ));
        let state = state_with_saml(&idp, supervisor).await;
        let back = sign_in(&state, &idp, &MockUser::new("dave")).await;
        assert_eq!(
            back,
            format!(
                "{}#login_error=Rejected%20by%20plugin%3A%20guard%3A%20account%20is%20suspended",
                ADMIN
            )
        );
    }
}
