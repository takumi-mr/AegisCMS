//! コンテンツのレビュー（`pending`）: `contributor` が書いた下書きを `editor` 以上が公開する、最小の承認の流れ。
//!
//! - 状態 `pending`（レビュー待ち）は公開側に出ない（公開側は `published` だけを見る）
//! - `contributor` は自分のコンテンツを `draft` / `pending` の間だけ作成・変更・削除でき、`draft` / `pending` にしか
//!   できない。公開・予約公開・アーカイブ、他人のコンテンツは `editor` 以上
//! - 承認は `pending` → `published` / `scheduled`、差し戻しは `pending` → `draft`（`editor` 以上が行う）。
//!   どれも通常の保存（`PUT /api/v1/contents/{type}/{id}`）の状態の変更で、`review_note` に理由を書ける
//! - 状態の変わり目を監査ログ `content.review`（`details.decision`）に残し、通知する
//!   （依頼 → `editor` 以上のユーザーに `content.submitted`、承認・差し戻し → 著者に `content.approved` /
//!   `content.returned`）
//!
//! 段階を自由に組めるワークフローは持たない（必要になったら `on-before-save` などで外に出す）。
//! 設計は docs/architecture.md の 3.4a

use axum::http::StatusCode;
use axum::Json;
use serde_json::json;

use crate::audit::AuditContext;
use crate::auth::authorize::{role_level, EDITOR};
use crate::state::AppState;
use aegis_auth::SessionData;
use aegis_core::{AuditAction, AuditOutcome, NotificationEvent, Recipient};
use aegis_db::ContentRecord;
use aegis_schema::SchemaDefinition;

/// 下書き
pub const STATUS_DRAFT: &str = "draft";
/// レビュー待ち。`editor` 以上が公開・予約公開（承認）するか、下書きに戻す（差し戻し）
pub const STATUS_PENDING: &str = "pending";

/// レビューのメモ（`review_note`）の最大文字数
pub const MAX_REVIEW_NOTE_CHARS: usize = 2000;

type ApiError = (StatusCode, Json<serde_json::Value>);

fn forbidden(message: &str) -> ApiError {
    (StatusCode::FORBIDDEN, Json(json!({ "error": message })))
}

/// 公開・他人のコンテンツの変更ができる（`editor` 以上）
pub fn can_publish(session: &SessionData) -> bool {
    role_level(&session.role) >= EDITOR
}

/// `contributor` が保存できる状態（`draft` / `pending`）
pub fn is_contributor_status(status: &str) -> bool {
    matches!(status, STATUS_DRAFT | STATUS_PENDING)
}

/// このセッションが `status` で保存してよいか。`contributor` は `draft` / `pending` だけ
pub fn ensure_may_set_status(session: &SessionData, status: &str) -> Result<(), ApiError> {
    if can_publish(session) || is_contributor_status(status) {
        return Ok(());
    }
    Err(forbidden(
        "Contributors can only save content as 'draft' or 'pending'; an editor publishes it",
    ))
}

/// このセッションが保存済みのコンテンツを変更・削除してよいか。`contributor` は自分のコンテンツで、
/// 公開前（`draft` / `pending`）のものだけ
pub fn ensure_may_change(session: &SessionData, current: &ContentRecord) -> Result<(), ApiError> {
    if can_publish(session) {
        return Ok(());
    }
    if current.author_id.as_deref() != Some(session.user_id.as_str()) {
        return Err(forbidden("Contributors can only change their own content"));
    }
    if !is_contributor_status(&current.status) {
        return Err(forbidden(
            "Contributors cannot change content that is published, scheduled or archived; ask an editor",
        ));
    }
    Ok(())
}

/// `review_note` を整える（前後の空白を除き、空なら無し）。長すぎれば 400
pub fn normalize_note(note: Option<String>) -> Result<Option<String>, ApiError> {
    let Some(note) = note.map(|n| n.trim().to_string()).filter(|n| !n.is_empty()) else {
        return Ok(None);
    };
    if note.chars().count() > MAX_REVIEW_NOTE_CHARS {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(json!({
                "error": format!("'review_note' must be at most {} characters", MAX_REVIEW_NOTE_CHARS)
            })),
        ));
    }
    Ok(Some(note))
}

/// 保存による状態の変わり目のうち、レビューに当たるもの
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReviewDecision {
    /// レビューを頼んだ（→ `pending`）
    Submitted,
    /// 承認した（`pending` → `published` / `scheduled`）
    Approved,
    /// 差し戻した（`pending` → `draft`。著者以外が行った）
    Returned,
    /// 著者が依頼を取り下げた（`pending` → `draft`）
    Withdrawn,
}

impl ReviewDecision {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Submitted => "submitted",
            Self::Approved => "approved",
            Self::Returned => "returned",
            Self::Withdrawn => "withdrawn",
        }
    }

    /// `previous`（新規作成なら `None`）から `record` の状態になったときのレビューの出来事
    pub fn of(previous: Option<&str>, record: &ContentRecord, actor: &str) -> Option<Self> {
        let next = record.status.as_str();
        match (previous, next) {
            (Some(STATUS_PENDING), STATUS_PENDING) => None,
            (_, STATUS_PENDING) => Some(Self::Submitted),
            (Some(STATUS_PENDING), "published" | "scheduled") => Some(Self::Approved),
            (Some(STATUS_PENDING), STATUS_DRAFT) => {
                if record.author_id.as_deref() == Some(actor) {
                    Some(Self::Withdrawn)
                } else {
                    Some(Self::Returned)
                }
            }
            _ => None,
        }
    }
}

/// レビューの出来事を監査ログ（`content.review`）に残し、通知する
pub async fn record(
    state: &AppState,
    audit: &AuditContext,
    schema: &SchemaDefinition,
    record: &ContentRecord,
    decision: ReviewDecision,
    actor: &SessionData,
    note: Option<&str>,
) {
    let mut details = json!({
        "decision": decision.as_str(),
        "content_type": record.content_type,
        "slug": record.slug,
        "title": record.title,
        "status": record.status,
    });
    if let Some(note) = note {
        details["note"] = json!(note);
    }
    state
        .audit
        .record(
            audit
                .event(AuditAction::ContentReview, AuditOutcome::Success)
                .with_resource("content", Some(record.id.clone()))
                .with_details(details),
        )
        .await;

    match decision {
        ReviewDecision::Submitted => notify_reviewers(state, schema, record, actor, note).await,
        ReviewDecision::Approved => {
            notify_author(state, "content.approved", schema, record, actor, note).await
        }
        ReviewDecision::Returned => {
            notify_author(state, "content.returned", schema, record, actor, note).await
        }
        ReviewDecision::Withdrawn => {}
    }
}

/// 通知に渡すコンテンツの値（`content.published` と同じ形）
fn content_data(schema: &SchemaDefinition, record: &ContentRecord) -> serde_json::Value {
    json!({
        "id": record.id,
        "content_type": record.content_type,
        "label": schema.label,
        "title": record.title,
        "slug": record.slug,
        "status": record.status,
        "published_at": record.published_at,
    })
}

/// Admin Studio のレビュー待ちの一覧を開く URL（`admin_url` が決まらなければ空）
fn review_url(state: &AppState) -> String {
    match state.admin_url.as_deref().filter(|u| !u.is_empty()) {
        Some(url) => format!("{}#review", url),
        None => String::new(),
    }
}

/// 操作した人の名前（表示名、無ければユーザー名）
fn actor_name(actor: &SessionData) -> String {
    actor
        .metadata
        .get("display_name")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| actor.user_id.clone())
}

/// レビューを頼まれたことを `editor` 以上の有効なユーザー（メールアドレスのある人。頼んだ本人を除く）に知らせる
async fn notify_reviewers(
    state: &AppState,
    schema: &SchemaDefinition,
    record: &ContentRecord,
    actor: &SessionData,
    note: Option<&str>,
) {
    let Some(notifier) = &state.notifier else {
        return;
    };
    let users = match state.db.list_users().await {
        Ok(users) => users,
        Err(e) => {
            tracing::warn!("Could not list the reviewers to notify: {}", e);
            return;
        }
    };
    let recipients: Vec<Recipient> = users
        .into_iter()
        .filter(|u| !u.disabled && u.username != actor.user_id)
        .filter(|u| role_level(&u.role) >= EDITOR)
        .filter_map(|u| {
            let name = u.display_name.clone().unwrap_or_else(|| u.username.clone());
            u.email.map(|email| Recipient::person(email, name))
        })
        .collect();
    if recipients.is_empty() {
        return;
    }
    notifier.notify(NotificationEvent::new(
        "content.submitted",
        recipients,
        json!({
            "content": content_data(schema, record),
            "submitted_by": actor_name(actor),
            "note": note,
            "review_url": review_url(state),
        }),
    ));
}

/// 承認・差し戻しを著者に知らせる。著者が操作した本人・いない・メールアドレスが無い・無効なら送らない
async fn notify_author(
    state: &AppState,
    kind: &str,
    schema: &SchemaDefinition,
    record: &ContentRecord,
    actor: &SessionData,
    note: Option<&str>,
) {
    let Some(notifier) = &state.notifier else {
        return;
    };
    let Some(author_id) = record
        .author_id
        .as_deref()
        .filter(|a| !a.is_empty() && *a != actor.user_id)
    else {
        return;
    };
    let author = match state.db.get_user_by_username(author_id).await {
        Ok(Some(user)) if !user.disabled => user,
        Ok(_) => return,
        Err(e) => {
            tracing::warn!(
                "Could not look up the author '{}' to notify: {}",
                author_id,
                e
            );
            return;
        }
    };
    let Some(email) = author.email.as_deref() else {
        return;
    };
    let name = author.display_name.as_deref().unwrap_or(&author.username);
    let content_url = if record.status == "published" {
        crate::site::content_url(&state.config, &record.content_type, &record.slug)
    } else {
        String::new()
    };
    notifier.notify(NotificationEvent::new(
        kind,
        vec![Recipient::person(email, name)],
        json!({
            "username": author.username,
            "display_name": author.display_name,
            "content": content_data(schema, record),
            "reviewed_by": actor_name(actor),
            "note": note,
            "content_url": content_url,
            "edit_url": state.admin_url.clone().unwrap_or_default(),
        }),
    ));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn session(user: &str, role: &str) -> SessionData {
        SessionData {
            id: "ses".to_string(),
            user_id: user.to_string(),
            role: role.to_string(),
            created_at: String::new(),
            expires_at: String::new(),
            metadata: Default::default(),
        }
    }

    fn record(status: &str, author: &str) -> ContentRecord {
        ContentRecord {
            id: "c1".to_string(),
            content_type: "article".to_string(),
            title: "T".to_string(),
            slug: "t".to_string(),
            status: status.to_string(),
            blocks_json: "[]".to_string(),
            author_id: Some(author.to_string()),
            published_at: None,
            unpublish_at: None,
            created_at: String::new(),
            updated_at: String::new(),
            custom_fields: Default::default(),
        }
    }

    #[test]
    fn contributors_save_only_drafts_and_pending() {
        let contributor = session("alice", "contributor");
        assert!(ensure_may_set_status(&contributor, "draft").is_ok());
        assert!(ensure_may_set_status(&contributor, "pending").is_ok());
        for status in ["published", "scheduled", "archived", "other"] {
            assert_eq!(
                ensure_may_set_status(&contributor, status).unwrap_err().0,
                StatusCode::FORBIDDEN
            );
        }
        let editor = session("bob", "editor");
        assert!(ensure_may_set_status(&editor, "published").is_ok());
    }

    #[test]
    fn contributors_change_only_their_unpublished_content() {
        let contributor = session("alice", "contributor");
        assert!(ensure_may_change(&contributor, &record("draft", "alice")).is_ok());
        assert!(ensure_may_change(&contributor, &record("pending", "alice")).is_ok());
        assert!(ensure_may_change(&contributor, &record("draft", "bob")).is_err());
        assert!(ensure_may_change(&contributor, &record("published", "alice")).is_err());
        assert!(ensure_may_change(&contributor, &record("scheduled", "alice")).is_err());
        let editor = session("bob", "editor");
        assert!(ensure_may_change(&editor, &record("published", "alice")).is_ok());
    }

    #[test]
    fn decisions_follow_status_changes() {
        let of = |prev: Option<&str>, status: &str, actor: &str| {
            ReviewDecision::of(prev, &record(status, "alice"), actor)
        };
        assert_eq!(
            of(None, "pending", "alice"),
            Some(ReviewDecision::Submitted)
        );
        assert_eq!(
            of(Some("draft"), "pending", "alice"),
            Some(ReviewDecision::Submitted)
        );
        assert_eq!(of(Some("pending"), "pending", "alice"), None);
        assert_eq!(
            of(Some("pending"), "published", "bob"),
            Some(ReviewDecision::Approved)
        );
        assert_eq!(
            of(Some("pending"), "scheduled", "bob"),
            Some(ReviewDecision::Approved)
        );
        assert_eq!(
            of(Some("pending"), "draft", "bob"),
            Some(ReviewDecision::Returned)
        );
        assert_eq!(
            of(Some("pending"), "draft", "alice"),
            Some(ReviewDecision::Withdrawn)
        );
        assert_eq!(of(Some("draft"), "published", "bob"), None);
        assert_eq!(of(None, "draft", "alice"), None);
    }

    #[test]
    fn notes_are_trimmed_and_bounded() {
        assert_eq!(normalize_note(None).unwrap(), None);
        assert_eq!(normalize_note(Some("  ".into())).unwrap(), None);
        assert_eq!(
            normalize_note(Some(" fix the title ".into())).unwrap(),
            Some("fix the title".to_string())
        );
        let long = "あ".repeat(MAX_REVIEW_NOTE_CHARS + 1);
        assert_eq!(
            normalize_note(Some(long)).unwrap_err().0,
            StatusCode::BAD_REQUEST
        );
    }

    mod flow {
        use crate::audit::AuditContext;
        use crate::auth::authorize::RequireRole;
        use crate::handlers::content::{
            create_content, delete_content, list_content_versions, update_content, ContentResponse,
        };
        use crate::state::AppState;
        use crate::test_support::{test_state, RecordingNotifier};
        use aegis_core::{AuditAction, AuditQuery, NotificationEvent, Recipient};
        use aegis_runtime::PluginSupervisor;
        use axum::extract::{Path, State};
        use axum::http::StatusCode;
        use axum::response::IntoResponse;
        use axum::Json;
        use serde_json::{json, Value};
        use std::sync::Arc;

        fn as_user<const LEVEL: u8>(user: &str, role: &str) -> RequireRole<LEVEL> {
            RequireRole(super::session(user, role))
        }

        async fn setup() -> (Arc<AppState>, Arc<RecordingNotifier>) {
            let state = test_state(PluginSupervisor::new()).await;
            for (name, role, email, disabled) in [
                ("alice", "contributor", Some("alice@example.com"), false),
                ("bob", "editor", Some("bob@example.com"), false),
                ("carol", "admin", Some("carol@example.com"), false),
                ("dave", "editor", Some("dave@example.com"), true),
                ("erin", "viewer", Some("erin@example.com"), false),
                ("frank", "editor", None, false),
            ] {
                aegis_auth::users::insert_new_user(
                    state.db.as_ref(),
                    aegis_auth::users::NewUser {
                        username: name.to_string(),
                        password: String::new(),
                        role: role.to_string(),
                        email: email.map(str::to_string),
                        display_name: None,
                    },
                    "hash".to_string(),
                    disabled,
                    false,
                )
                .await
                .unwrap();
            }
            let notifier = Arc::new(RecordingNotifier::default());
            let mut state = (*state).clone();
            state.notifier = Some(notifier.clone());
            state.admin_url = Some("https://example.com/admin/".to_string());
            state.config.commerce.public_url = "https://example.com".to_string();
            (Arc::new(state), notifier)
        }

        async fn create(
            state: &Arc<AppState>,
            user: (&str, &str),
            payload: Value,
        ) -> Result<ContentResponse, StatusCode> {
            let response = create_content(
                State(state.clone()),
                as_user(user.0, user.1),
                Path("article".to_string()),
                AuditContext::default(),
                Json(serde_json::from_value(payload).unwrap()),
            )
            .await
            .map_err(|(status, _)| status)?
            .into_response();
            assert_eq!(response.status(), StatusCode::CREATED);
            let body = axum::body::to_bytes(response.into_body(), usize::MAX)
                .await
                .unwrap();
            Ok(serde_json::from_slice(&body).unwrap())
        }

        async fn update(
            state: &Arc<AppState>,
            user: (&str, &str),
            slug: &str,
            payload: Value,
        ) -> Result<ContentResponse, StatusCode> {
            update_content(
                State(state.clone()),
                as_user(user.0, user.1),
                Path(("article".to_string(), slug.to_string())),
                AuditContext::default(),
                Json(serde_json::from_value(payload).unwrap()),
            )
            .await
            .map(|Json(body)| body)
            .map_err(|(status, _)| status)
        }

        async fn delete(
            state: &Arc<AppState>,
            user: (&str, &str),
            slug: &str,
        ) -> Result<StatusCode, StatusCode> {
            delete_content(
                State(state.clone()),
                as_user(user.0, user.1),
                Path(("article".to_string(), slug.to_string())),
                AuditContext::default(),
            )
            .await
            .map_err(|(status, _)| status)
        }

        async fn review_decisions(state: &AppState) -> Vec<(String, Option<String>)> {
            let mut events = state
                .db
                .query_audit_events(&AuditQuery {
                    limit: 100,
                    ..Default::default()
                })
                .await
                .unwrap();
            events.reverse();
            events
                .into_iter()
                .filter(|e| e.action == AuditAction::ContentReview)
                .map(|e| {
                    (
                        e.details["decision"].as_str().unwrap().to_string(),
                        e.details["note"].as_str().map(str::to_string),
                    )
                })
                .collect()
        }

        fn emails(event: &NotificationEvent) -> Vec<String> {
            let mut emails: Vec<String> = event
                .recipients
                .iter()
                .map(|r| match r {
                    Recipient::Person { email, .. } => email.clone(),
                    Recipient::Staff => "staff".to_string(),
                })
                .collect();
            emails.sort();
            emails
        }

        const ALICE: (&str, &str) = ("alice", "contributor");
        const BOB: (&str, &str) = ("bob", "editor");

        #[tokio::test]
        async fn contributors_submit_and_editors_approve_or_return() {
            let (state, notifier) = setup().await;
            let article = |status: &str, note: Option<&str>| {
                json!({ "title": "Spring Sale", "slug": "spring", "status": status,
                        "review_note": note })
            };

            // A contributor cannot publish or schedule, even when creating
            for status in ["published", "scheduled", "archived"] {
                assert_eq!(
                    create(&state, ALICE, article(status, None))
                        .await
                        .unwrap_err(),
                    StatusCode::FORBIDDEN
                );
            }

            // Requesting a review notifies the active editors and admins with an email
            let created = create(&state, ALICE, article("pending", Some(" Please check ")))
                .await
                .unwrap();
            assert_eq!(created.status, "pending");
            assert_eq!(created.author_id.as_deref(), Some("alice"));
            assert_eq!(created.published_at, None);
            let events = notifier.take();
            assert_eq!(events.len(), 1);
            assert_eq!(events[0].kind, "content.submitted");
            assert_eq!(emails(&events[0]), ["bob@example.com", "carol@example.com"]);
            assert_eq!(events[0].data["submitted_by"], "alice");
            assert_eq!(events[0].data["note"], "Please check");
            assert_eq!(
                events[0].data["review_url"],
                "https://example.com/admin/#review"
            );
            assert_eq!(events[0].data["content"]["title"], "Spring Sale");

            // Saving it again while it waits is not another request
            update(&state, ALICE, "spring", article("pending", None))
                .await
                .unwrap();
            assert!(notifier.take().is_empty());

            // The editor returns it with a note: the author is told why, and the note is in the history
            let returned = update(
                &state,
                BOB,
                "spring",
                article("draft", Some("Fix the price")),
            )
            .await
            .unwrap();
            assert_eq!(returned.status, "draft");
            let events = notifier.take();
            assert_eq!(events.len(), 1);
            assert_eq!(events[0].kind, "content.returned");
            assert_eq!(emails(&events[0]), ["alice@example.com"]);
            assert_eq!(events[0].data["reviewed_by"], "bob");
            assert_eq!(events[0].data["note"], "Fix the price");
            let Json(versions) = list_content_versions(
                State(state.clone()),
                as_user("erin", "viewer"),
                Path(("article".to_string(), "spring".to_string())),
            )
            .await
            .unwrap();
            assert!(versions
                .iter()
                .any(|v| v.change_note.as_deref() == Some("Fix the price")));

            // Sent again and approved: published, and the author gets the link
            update(&state, ALICE, "spring", article("pending", None))
                .await
                .unwrap();
            assert_eq!(notifier.take()[0].kind, "content.submitted");
            let approved = update(&state, BOB, "spring", article("published", None))
                .await
                .unwrap();
            assert_eq!(approved.status, "published");
            assert!(approved.published_at.is_some());
            let events = notifier.take();
            assert_eq!(events.len(), 1);
            assert_eq!(events[0].kind, "content.approved");
            assert_eq!(emails(&events[0]), ["alice@example.com"]);
            assert_eq!(events[0].data["content_url"], "https://example.com/spring");

            // Once published, the contributor can neither change nor delete it
            assert_eq!(
                update(&state, ALICE, "spring", article("pending", None))
                    .await
                    .unwrap_err(),
                StatusCode::FORBIDDEN
            );
            assert_eq!(
                delete(&state, ALICE, "spring").await.unwrap_err(),
                StatusCode::FORBIDDEN
            );

            assert_eq!(
                review_decisions(&state).await,
                vec![
                    ("submitted".to_string(), Some("Please check".to_string())),
                    ("returned".to_string(), Some("Fix the price".to_string())),
                    ("submitted".to_string(), None),
                    ("approved".to_string(), None),
                ]
            );
        }

        #[tokio::test]
        async fn contributors_only_touch_their_own_unpublished_content() {
            let (state, notifier) = setup().await;
            create(
                &state,
                BOB,
                json!({ "title": "Bob's", "slug": "bobs", "status": "draft" }),
            )
            .await
            .unwrap();
            let draft = json!({ "title": "Mine", "slug": "bobs", "status": "draft" });
            assert_eq!(
                update(&state, ALICE, "bobs", draft).await.unwrap_err(),
                StatusCode::FORBIDDEN
            );
            assert_eq!(
                delete(&state, ALICE, "bobs").await.unwrap_err(),
                StatusCode::FORBIDDEN
            );
            // A contributor cannot hand their content to someone else
            assert_eq!(
                create(
                    &state,
                    ALICE,
                    json!({ "title": "X", "slug": "x", "author_id": "bob" })
                )
                .await
                .unwrap_err(),
                StatusCode::FORBIDDEN
            );

            // Withdrawing their own request is recorded but notifies nobody
            create(
                &state,
                ALICE,
                json!({ "title": "Mine", "slug": "mine", "status": "pending" }),
            )
            .await
            .unwrap();
            notifier.take();
            update(
                &state,
                ALICE,
                "mine",
                json!({ "title": "Mine", "slug": "mine", "status": "draft" }),
            )
            .await
            .unwrap();
            assert!(notifier.take().is_empty());
            assert_eq!(
                review_decisions(&state).await.last().unwrap().0,
                "withdrawn"
            );
            assert_eq!(
                delete(&state, ALICE, "mine").await.unwrap(),
                StatusCode::NO_CONTENT
            );

            // An overlong note is refused before anything is saved
            assert_eq!(
                create(
                    &state,
                    ALICE,
                    json!({ "title": "Long", "slug": "long", "status": "pending",
                            "review_note": "x".repeat(super::MAX_REVIEW_NOTE_CHARS + 1) })
                )
                .await
                .unwrap_err(),
                StatusCode::BAD_REQUEST
            );
        }

        #[tokio::test]
        async fn pending_content_is_not_public() {
            let (state, _) = setup().await;
            create(
                &state,
                BOB,
                json!({ "title": "Live", "slug": "live", "status": "published" }),
            )
            .await
            .unwrap();
            // An editor sending a published item back to review takes it offline, like a draft
            let back = update(
                &state,
                BOB,
                "live",
                json!({ "title": "Live", "slug": "live", "status": "pending" }),
            )
            .await
            .unwrap();
            assert_eq!(back.status, "pending");
            assert_eq!(back.published_at, None);
            assert!(!crate::handlers::content::is_currently_published(
                &back.status,
                back.unpublish_at.as_deref(),
                chrono::Utc::now()
            ));
        }
    }
}
