//! Admin Studio からのアカウント登録（`[auth.registration]`）: 申し込み（確認メールを送る）と、
//! 確認リンクのトークンによるアカウントの作成。どちらもログインせずに呼べる

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
use crate::state::{AppState, Registration};
use aegis_auth::registration::{RegistrationError, RegistrationRequest, Requested};
use aegis_core::{AuditAction, AuditOutcome, NotificationEvent, Recipient};

/// 同じクライアント IP からの申し込みを数える期間（秒）
const IP_WINDOW_SECS: u64 = 3600;

fn registration_disabled() -> Response {
    (
        StatusCode::NOT_FOUND,
        Json(json!({ "error": "Account registration is not enabled" })),
    )
        .into_response()
}

fn error_response(error: &RegistrationError) -> Response {
    let status = match error {
        RegistrationError::UsernameTaken | RegistrationError::EmailTaken => StatusCode::CONFLICT,
        RegistrationError::EmailDomainNotAllowed => StatusCode::FORBIDDEN,
        RegistrationError::Internal(_) => StatusCode::INTERNAL_SERVER_ERROR,
        _ => StatusCode::BAD_REQUEST,
    };
    (
        status,
        Json(json!({ "error": error.to_string(), "code": error.code() })),
    )
        .into_response()
}

/// 失敗の監査の結果（ポリシーによる拒否は `denied`、サーバーのエラーは `failure`）
fn outcome_of(error: &RegistrationError) -> AuditOutcome {
    match error {
        RegistrationError::Internal(_) => AuditOutcome::Failure,
        _ => AuditOutcome::Denied,
    }
}

/// `POST /api/v1/auth/registration` — 登録を申し込む。メールアドレスに確認リンクを送り、`202` を返す。
/// メールアドレスがすでに使われていても応答は同じ（登録済みかどうかを伏せる。メールは送らない）。
/// 同じクライアント IP からは 1 時間に `max_per_ip_per_hour` 件まで
pub async fn register(
    State(state): State<Arc<AppState>>,
    audit: AuditContext,
    ClientIp(client_ip): ClientIp,
    Json(payload): Json<RegistrationRequest>,
) -> Response {
    let Some(registration) = state.registration.clone() else {
        return registration_disabled();
    };
    if !state.system().registration_enabled {
        return registration_disabled();
    }
    let username = payload.username.trim().to_string();

    let limit = registration.service.config().max_per_ip_per_hour;
    if let (true, Some(ip)) = (limit > 0, client_ip.as_deref()) {
        let count = state
            .cache
            .increment(&format!("registration:ip:{}", ip), Some(IP_WINDOW_SECS))
            .await;
        match count {
            Ok(count) if count > limit => {
                record(
                    &state,
                    &audit,
                    AuditOutcome::Denied,
                    None,
                    json!({
                        "stage": "requested",
                        "username": username,
                        "error": "rate_limited",
                    }),
                )
                .await;
                return (
                    StatusCode::TOO_MANY_REQUESTS,
                    [(header::RETRY_AFTER, IP_WINDOW_SECS.to_string())],
                    Json(json!({
                        "error": "Too many registrations from this address. Try again later.",
                        "code": "rate_limited",
                        "retry_after_secs": IP_WINDOW_SECS,
                    })),
                )
                    .into_response();
            }
            Ok(_) => {}
            Err(e) => tracing::warn!("Failed to count registrations: {}", e),
        }
    }

    match registration.service.request(payload).await {
        Ok(Requested::Pending {
            token,
            username,
            email,
            display_name,
        }) => {
            send_verification(&registration, &token, &username, &email, display_name);
            record(
                &state,
                &audit,
                AuditOutcome::Success,
                None,
                json!({
                    "stage": "requested",
                    "username": username,
                }),
            )
            .await;
            accepted(&registration)
        }
        Ok(Requested::EmailInUse { username }) => {
            record(
                &state,
                &audit,
                AuditOutcome::Denied,
                None,
                json!({
                    "stage": "requested",
                    "username": username,
                    "error": "email_in_use",
                }),
            )
            .await;
            accepted(&registration)
        }
        Err(e) => {
            record(
                &state,
                &audit,
                outcome_of(&e),
                None,
                json!({
                    "stage": "requested",
                    "username": username,
                    "error": e.code(),
                }),
            )
            .await;
            error_response(&e)
        }
    }
}

fn accepted(registration: &Registration) -> Response {
    (
        StatusCode::ACCEPTED,
        Json(json!({
            "status": "pending_verification",
            "expires_in_secs": registration.service.config().token_ttl_secs,
        })),
    )
        .into_response()
}

fn send_verification(
    registration: &Registration,
    token: &str,
    username: &str,
    email: &str,
    display_name: Option<String>,
) {
    let ttl = registration.service.config().token_ttl_secs;
    let name = display_name.clone().unwrap_or_else(|| username.to_string());
    registration.notifier.notify(NotificationEvent::new(
        "account.verify",
        vec![Recipient::person(email, name)],
        json!({
            "username": username,
            "display_name": display_name,
            "verify_url": format!("{}#verify-email={}", registration.admin_url, token),
            "expires_hours": ttl.div_ceil(3600),
        }),
    ));
}

#[derive(Debug, Deserialize)]
pub struct VerifyRequest {
    pub token: String,
}

/// `POST /api/v1/auth/registration/verify` — 確認リンクのトークンでアカウントを作る。
/// `status` は `active`（すぐにログインできる）か `pending_approval`（admin が有効にするまで無効）。
/// 運営者（`staff_recipients`）に新しいアカウントを知らせる
pub async fn verify_registration(
    State(state): State<Arc<AppState>>,
    audit: AuditContext,
    Json(payload): Json<VerifyRequest>,
) -> Response {
    let Some(registration) = state.registration.clone() else {
        return registration_disabled();
    };
    if !state.system().registration_enabled {
        return registration_disabled();
    }
    match registration.service.complete(&payload.token).await {
        Ok(user) => {
            let pending_approval = user.disabled;
            registration.notifier.notify(NotificationEvent::new(
                "account.registered",
                vec![Recipient::Staff],
                json!({
                    "username": user.username,
                    "display_name": user.display_name,
                    "email": user.email,
                    "role": user.role,
                    "pending_approval": pending_approval,
                }),
            ));
            record(
                &state,
                &audit,
                AuditOutcome::Success,
                Some(user.username.clone()),
                json!({
                    "stage": "confirmed",
                    "username": user.username,
                    "role": user.role,
                    "pending_approval": pending_approval,
                }),
            )
            .await;
            (
                StatusCode::CREATED,
                Json(json!({
                    "status": if pending_approval { "pending_approval" } else { "active" },
                    "username": user.username,
                })),
            )
                .into_response()
        }
        Err(e) => {
            record(
                &state,
                &audit,
                outcome_of(&e),
                None,
                json!({
                    "stage": "confirmed",
                    "error": e.code(),
                }),
            )
            .await;
            error_response(&e)
        }
    }
}

async fn record(
    state: &AppState,
    audit: &AuditContext,
    outcome: AuditOutcome,
    user_id: Option<String>,
    details: serde_json::Value,
) {
    state
        .audit
        .record(
            audit
                .event(AuditAction::UserRegister, outcome)
                .with_resource("user", user_id)
                .with_details(details),
        )
        .await;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::AegisConfig;
    use aegis_auth::registration::RegistrationService;
    use aegis_auth::RegistrationConfig;
    use aegis_core::Notifier;
    use std::sync::Mutex;

    #[derive(Default)]
    struct RecordingNotifier {
        events: Mutex<Vec<NotificationEvent>>,
    }

    impl Notifier for RecordingNotifier {
        fn notify(&self, event: NotificationEvent) {
            self.events.lock().unwrap().push(event);
        }
    }

    async fn setup(config: RegistrationConfig) -> (Arc<AppState>, Arc<RecordingNotifier>) {
        let mut aegis = AegisConfig::default();
        aegis.audit.enabled = true;
        let state = crate::test_support::test_app_state(
            aegis,
            aegis_runtime::PluginSupervisor::new(),
            Vec::new(),
        )
        .await;
        let mails = Arc::new(RecordingNotifier::default());
        let service = RegistrationService::new(
            state.db.clone(),
            state.cache.clone(),
            RegistrationConfig {
                enabled: true,
                ..config
            },
        );
        let state = state.with_registration(Registration {
            service,
            notifier: mails.clone(),
            admin_url: "https://cms.example.com/admin/".to_string(),
        });
        (Arc::new(state), mails)
    }

    fn request(username: &str, email: &str) -> RegistrationRequest {
        RegistrationRequest {
            username: username.to_string(),
            email: email.to_string(),
            password: "long-enough-pass".to_string(),
            display_name: None,
        }
    }

    async fn post_register(state: &Arc<AppState>, req: RegistrationRequest, ip: &str) -> Response {
        register(
            State(state.clone()),
            AuditContext::default(),
            ClientIp(Some(ip.to_string())),
            Json(req),
        )
        .await
    }

    async fn post_verify(state: &Arc<AppState>, token: &str) -> Response {
        verify_registration(
            State(state.clone()),
            AuditContext::default(),
            Json(VerifyRequest {
                token: token.to_string(),
            }),
        )
        .await
    }

    async fn body(resp: Response) -> serde_json::Value {
        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        serde_json::from_slice(&bytes).unwrap()
    }

    /// `user.register` events, oldest first
    async fn register_events(state: &Arc<AppState>) -> Vec<aegis_core::AuditEvent> {
        let mut events = state
            .db
            .query_audit_events(&aegis_core::AuditQuery {
                action: Some(AuditAction::UserRegister),
                limit: 50,
                ..Default::default()
            })
            .await
            .unwrap();
        events.reverse();
        events
    }

    #[tokio::test]
    async fn test_register_then_verify_creates_a_disabled_account() {
        let (state, mails) = setup(RegistrationConfig::default()).await;
        let axum::Json(providers) =
            crate::handlers::list_auth_providers(State(state.clone())).await;
        assert_eq!(providers["registration"], true);

        let resp = post_register(&state, request("hanako", "hanako@example.com"), "10.0.0.1").await;
        assert_eq!(resp.status(), StatusCode::ACCEPTED);
        assert_eq!(body(resp).await["status"], "pending_verification");
        assert!(state
            .db
            .get_user_by_username("hanako")
            .await
            .unwrap()
            .is_none());

        let event = mails.events.lock().unwrap()[0].clone();
        assert_eq!(event.kind, "account.verify");
        assert_eq!(
            event.recipients,
            vec![Recipient::person("hanako@example.com", "hanako")]
        );
        assert_eq!(event.data["expires_hours"], 24);
        let url = event.data["verify_url"].as_str().unwrap();
        let token = url
            .strip_prefix("https://cms.example.com/admin/#verify-email=")
            .unwrap();

        let resp = post_verify(&state, token).await;
        assert_eq!(resp.status(), StatusCode::CREATED);
        let created = body(resp).await;
        assert_eq!(created["status"], "pending_approval");
        assert_eq!(created["username"], "hanako");
        let user = state
            .db
            .get_user_by_username("hanako")
            .await
            .unwrap()
            .unwrap();
        assert!(user.disabled);
        assert_eq!(user.role, "viewer");

        let staff = mails.events.lock().unwrap()[1].clone();
        assert_eq!(staff.kind, "account.registered");
        assert_eq!(staff.recipients, vec![Recipient::Staff]);
        assert_eq!(staff.data["pending_approval"], true);

        // The link is single-use
        let resp = post_verify(&state, token).await;
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
        assert_eq!(body(resp).await["code"], "invalid_token");

        // The account cannot log in until an admin enables it
        let login = state
            .auth_provider
            .authenticate(&aegis_core::AuthCredentials {
                kind: aegis_auth::local::PASSWORD_CREDENTIALS.to_string(),
                identifier: Some("hanako".to_string()),
                secret: Some("long-enough-pass".to_string()),
                ..Default::default()
            })
            .await;
        assert!(login.is_err());

        let events = register_events(&state).await;
        let stages: Vec<(String, AuditOutcome)> = events
            .iter()
            .map(|e| (e.details["stage"].as_str().unwrap().to_string(), e.outcome))
            .collect();
        assert_eq!(
            stages,
            [
                ("requested".to_string(), AuditOutcome::Success),
                ("confirmed".to_string(), AuditOutcome::Success),
                ("confirmed".to_string(), AuditOutcome::Denied),
            ]
        );
        // Neither the email address nor the token is written to the audit log
        let logged = serde_json::to_string(&events).unwrap();
        assert!(!logged.contains("hanako@example.com"));
        assert!(!logged.contains(token));
    }

    #[tokio::test]
    async fn test_without_approval_the_account_is_active() {
        let (state, mails) = setup(RegistrationConfig {
            require_approval: false,
            ..Default::default()
        })
        .await;
        post_register(&state, request("taro", "taro@example.com"), "10.0.0.1").await;
        let url = mails.events.lock().unwrap()[0].data["verify_url"]
            .as_str()
            .unwrap()
            .to_string();
        let token = url.rsplit_once('=').unwrap().1;

        let resp = post_verify(&state, token).await;
        assert_eq!(resp.status(), StatusCode::CREATED);
        assert_eq!(body(resp).await["status"], "active");
        let staff = mails.events.lock().unwrap()[1].clone();
        assert_eq!(staff.data["pending_approval"], false);
        let user = state
            .db
            .get_user_by_username("taro")
            .await
            .unwrap()
            .unwrap();
        assert!(!user.disabled);
    }

    #[tokio::test]
    async fn test_existing_email_gets_the_same_answer_without_a_mail() {
        let (state, mails) = setup(RegistrationConfig::default()).await;
        aegis_auth::users::create_user(
            state.db.as_ref(),
            aegis_auth::users::NewUser {
                username: "root".to_string(),
                password: "initial-pass".to_string(),
                role: "admin".to_string(),
                email: Some("root@example.com".to_string()),
                display_name: None,
            },
        )
        .await
        .unwrap();
        let resp = post_register(&state, request("other", "ROOT@example.com"), "10.0.0.1").await;
        assert_eq!(resp.status(), StatusCode::ACCEPTED);
        assert!(mails.events.lock().unwrap().is_empty());

        let resp = post_register(&state, request("root", "new@example.com"), "10.0.0.1").await;
        assert_eq!(resp.status(), StatusCode::CONFLICT);
        assert_eq!(body(resp).await["code"], "username_taken");
    }

    #[tokio::test]
    async fn test_registrations_per_ip_are_limited() {
        let (state, mails) = setup(RegistrationConfig {
            max_per_ip_per_hour: 2,
            ..Default::default()
        })
        .await;
        for (i, expected) in [
            StatusCode::ACCEPTED,
            StatusCode::ACCEPTED,
            StatusCode::TOO_MANY_REQUESTS,
        ]
        .into_iter()
        .enumerate()
        {
            let req = request(&format!("user{}", i), &format!("user{}@example.com", i));
            assert_eq!(
                post_register(&state, req, "10.0.0.1").await.status(),
                expected
            );
        }
        // Another address is counted separately
        let resp = post_register(&state, request("user9", "user9@example.com"), "10.0.0.2").await;
        assert_eq!(resp.status(), StatusCode::ACCEPTED);
        assert_eq!(mails.events.lock().unwrap().len(), 3);
    }

    #[tokio::test]
    async fn test_disabled_registration_is_not_found() {
        let state = crate::test_support::test_state(aegis_runtime::PluginSupervisor::new()).await;
        let resp = post_register(&state, request("a", "a@example.com"), "10.0.0.1").await;
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
        assert_eq!(
            post_verify(&state, "x").await.status(),
            StatusCode::NOT_FOUND
        );
    }
}
