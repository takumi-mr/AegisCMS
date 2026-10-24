//! ユーザー招待 API（`/api/v1/auth/invitations`）: 管理者による招待リンク発行と、
//! 招待された本人によるアカウント登録（ユーザー名・パスワード決定）

use axum::{
    extract::{Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde::Deserialize;
use serde_json::json;
use std::sync::Arc;

use crate::audit::AuditContext;
use crate::auth::authorize::RequireRole;
use crate::auth::AdminSession;
use crate::state::AppState;
use aegis_auth::invitation::{AcceptInviteRequest, InvitationError, InviteRequest};
use aegis_core::{AuditAction, AuditOutcome, NotificationEvent, Recipient};

fn invitation_disabled() -> Response {
    (
        StatusCode::NOT_FOUND,
        Json(json!({ "error": "User invitation is not enabled" })),
    )
        .into_response()
}

fn error_response(error: &InvitationError) -> Response {
    let status = match error {
        InvitationError::EmailTaken | InvitationError::UsernameTaken => StatusCode::CONFLICT,
        InvitationError::Internal(_) => StatusCode::INTERNAL_SERVER_ERROR,
        _ => StatusCode::BAD_REQUEST,
    };
    (
        status,
        Json(json!({ "error": error.to_string(), "code": error.code() })),
    )
        .into_response()
}

fn outcome_of(error: &InvitationError) -> AuditOutcome {
    match error {
        InvitationError::Internal(_) => AuditOutcome::Failure,
        _ => AuditOutcome::Denied,
    }
}

/// `POST /api/v1/auth/invitations` — 管理者がメールアドレスとロールを指定してユーザーを招待する
pub async fn create_invitation(
    State(state): State<Arc<AppState>>,
    RequireRole(session): AdminSession,
    audit: AuditContext,
    Json(payload): Json<InviteRequest>,
) -> Response {
    let Some(invitation) = state.invitation.clone() else {
        return invitation_disabled();
    };

    let email = payload.email.trim().to_string();
    let role = payload.role.trim().to_string();

    match invitation
        .service
        .create_invitation(&session.user_id, payload)
        .await
    {
        Ok(requested) => {
            let ttl = invitation.service.config().token_ttl_secs;
            let recipient_name = requested
                .display_name
                .clone()
                .unwrap_or_else(|| requested.email.clone());
            invitation.notifier.notify(NotificationEvent::new(
                "account.invite",
                vec![Recipient::person(&requested.email, recipient_name)],
                json!({
                    "email": requested.email,
                    "role": requested.role,
                    "display_name": requested.display_name,
                    "invite_url": format!("{}#invite={}", invitation.admin_url, requested.token),
                    "expires_days": ttl.div_ceil(86400),
                }),
            ));
            state
                .audit
                .record(
                    audit
                        .event(AuditAction::UserInvite, AuditOutcome::Success)
                        .with_details(json!({
                            "stage": "created",
                            "email": requested.email,
                            "role": requested.role,
                            "invited_by": session.user_id,
                        })),
                )
                .await;
            (
                StatusCode::CREATED,
                Json(json!({
                    "status": "invited",
                    "email": email,
                    "role": role,
                    "expires_in_secs": ttl,
                })),
            )
                .into_response()
        }
        Err(e) => {
            state
                .audit
                .record(
                    audit
                        .event(AuditAction::UserInvite, outcome_of(&e))
                        .with_details(json!({
                            "stage": "created",
                            "email": email,
                            "role": role,
                            "error": e.code(),
                        })),
                )
                .await;
            error_response(&e)
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct TokenQuery {
    pub token: String,
}

/// `GET /api/v1/auth/invitations/info` — 招待トークンの有効性を確認し、招待情報を返す（受諾画面表示用）
pub async fn get_invitation_info(
    State(state): State<Arc<AppState>>,
    Query(query): Query<TokenQuery>,
) -> Response {
    let Some(invitation) = state.invitation.clone() else {
        return invitation_disabled();
    };

    match invitation.service.get_invitation_info(&query.token).await {
        Ok(info) => (StatusCode::OK, Json(json!(info))).into_response(),
        Err(e) => error_response(&e),
    }
}

/// `POST /api/v1/auth/invitations/accept` — 招待を受諾してユーザーアカウントを作成する
pub async fn accept_invitation(
    State(state): State<Arc<AppState>>,
    audit: AuditContext,
    Json(payload): Json<AcceptInviteRequest>,
) -> Response {
    let Some(invitation) = state.invitation.clone() else {
        return invitation_disabled();
    };

    match invitation.service.accept_invitation(payload).await {
        Ok(user) => {
            // 運営者に登録通知
            invitation.notifier.notify(NotificationEvent::new(
                "account.registered",
                vec![Recipient::Staff],
                json!({
                    "username": user.username,
                    "display_name": user.display_name,
                    "email": user.email,
                    "role": user.role,
                    "pending_approval": false,
                }),
            ));
            state
                .audit
                .record(
                    audit
                        .event(AuditAction::UserInvite, AuditOutcome::Success)
                        .with_resource("user", Some(user.username.clone()))
                        .with_details(json!({
                            "stage": "accepted",
                            "username": user.username,
                            "role": user.role,
                        })),
                )
                .await;
            (
                StatusCode::CREATED,
                Json(json!({
                    "status": "active",
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
                        .event(AuditAction::UserInvite, outcome_of(&e))
                        .with_details(json!({
                            "stage": "accepted",
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
    use aegis_auth::invitation::InvitationService;
    use aegis_auth::InvitationConfig;
    use aegis_auth::SessionData;

    async fn setup() -> (Arc<AppState>, Arc<RecordingNotifier>) {
        let state = crate::test_support::test_state_with_plugins(
            aegis_runtime::PluginSupervisor::new(),
            Vec::new(),
        )
        .await;
        let mails = Arc::new(RecordingNotifier::default());
        let service = InvitationService::new(
            state.db.clone(),
            state.cache.clone(),
            InvitationConfig {
                enabled: true,
                token_ttl_secs: 604_800,
            },
        );
        let state = (*state).clone().with_invitation(crate::state::Invitation {
            service,
            notifier: mails.clone(),
            admin_url: "https://cms.example.com/admin/".to_string(),
        });
        (Arc::new(state), mails)
    }

    fn admin_session() -> AdminSession {
        RequireRole(SessionData {
            id: "ses_admin".to_string(),
            user_id: "admin".to_string(),
            role: "admin".to_string(),
            created_at: String::new(),
            expires_at: String::new(),
            metadata: Default::default(),
        })
    }

    #[tokio::test]
    async fn test_invitation_flow() {
        let (state, mails) = setup().await;

        // 招待作成
        let resp = create_invitation(
            State(state.clone()),
            admin_session(),
            AuditContext::default(),
            Json(InviteRequest {
                email: "invited@example.com".to_string(),
                role: "editor".to_string(),
                display_name: Some("Invited Person".to_string()),
            }),
        )
        .await;
        assert_eq!(resp.status(), StatusCode::CREATED);

        let events = mails.take();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].kind, "account.invite");
        let invite_url = events[0].data["invite_url"].as_str().unwrap();
        let token = invite_url.split("#invite=").nth(1).unwrap();

        // 招待情報の取得（リンクを開いた画面）
        let resp = get_invitation_info(
            State(state.clone()),
            Query(TokenQuery {
                token: token.to_string(),
            }),
        )
        .await;
        assert_eq!(resp.status(), StatusCode::OK);

        // 招待の受諾
        let resp = accept_invitation(
            State(state.clone()),
            AuditContext::default(),
            Json(AcceptInviteRequest {
                token: token.to_string(),
                username: "newuser".to_string(),
                password: "new-password-123".to_string(),
                display_name: None,
            }),
        )
        .await;
        assert_eq!(resp.status(), StatusCode::CREATED);

        // ユーザーが作成され、最初から有効であることを確認
        let user = state
            .db
            .get_user_by_username("newuser")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(user.email.as_deref(), Some("invited@example.com"));
        assert_eq!(user.role, "editor");
        assert!(!user.disabled);
        assert!(!user.pending_approval);

        // 運営者への登録通知が飛んでいることを確認
        let events = mails.take();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].kind, "account.registered");
    }
}
