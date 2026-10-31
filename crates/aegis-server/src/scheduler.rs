//! コンテンツの予約公開。`status: "scheduled"` のコンテンツを、`published_at` の日時になったら
//! `published` にする（監査ログ `content.publish`、著者への通知 `content.published`）

use std::sync::Arc;
use std::time::Duration;

use aegis_core::{AuditAction, AuditEvent, AuditOutcome, NotificationEvent, Recipient};
use aegis_db::{ContentListQuery, ContentRecord};
use aegis_schema::SchemaDefinition;
use chrono::{DateTime, Utc};
use serde_json::json;

use crate::handlers::content::STATUS_SCHEDULED;
use crate::state::AppState;

/// 予約日時を確認する間隔（公開は予約した日時から最大でこの分だけ遅れる）
const PUBLISH_INTERVAL: Duration = Duration::from_secs(30);

/// `now` までに予約日時を迎えたコンテンツをすべてのスキーマで公開し、公開した件数を返す
pub async fn publish_due_contents(state: &AppState, now: DateTime<Utc>) -> usize {
    let schemas: Vec<SchemaDefinition> = state.schemas.read().await.values().cloned().collect();
    let query = ContentListQuery {
        status: Some(STATUS_SCHEDULED.to_string()),
        ..Default::default()
    };
    let mut published = 0;
    for schema in schemas {
        let records = match state.db.list_contents_page(&schema, &query).await {
            Ok(page) => page.records,
            Err(e) => {
                tracing::warn!(
                    "Failed to list scheduled {} contents: {}",
                    schema.content_type,
                    e
                );
                continue;
            }
        };
        for record in records {
            let Some(at) = record.published_at.as_deref() else {
                continue;
            };
            match DateTime::parse_from_rfc3339(at) {
                Ok(due) if due <= now => {}
                Ok(_) => continue,
                Err(e) => {
                    tracing::warn!(
                        "Scheduled content {} has an invalid published_at '{}': {}",
                        record.id,
                        at,
                        e
                    );
                    continue;
                }
            }

            let live_before =
                crate::handlers::redirects::live_slug(state, &schema, &record.id).await;
            let result = state
                .db
                .publish_scheduled_content(&schema, &record.id, at, &now.to_rfc3339())
                .await;
            // 公開版の slug が変わったら、旧 URL からの転送を残す
            if matches!(result, Ok(true)) {
                if let (Some(old), Some(new)) = (
                    live_before,
                    crate::handlers::redirects::live_slug(state, &schema, &record.id).await,
                ) {
                    if old != new {
                        crate::handlers::redirects::record_slug_change(
                            state,
                            &aegis_audit::AuditOrigin::default(),
                            &record.content_type,
                            &old,
                            &new,
                        )
                        .await;
                    }
                }
            }
            let mut details = json!({
                "content_type": record.content_type,
                "slug": record.slug,
                "title": record.title,
                "published_at": at,
            });
            let outcome = match &result {
                Ok(true) => {
                    published += 1;
                    AuditOutcome::Success
                }
                // 一覧の取得後に予約日時の変更・取り消し・削除があった
                Ok(false) => continue,
                Err(e) => {
                    tracing::warn!("Failed to publish scheduled content {}: {}", record.id, e);
                    details["error"] = json!(e.to_string());
                    AuditOutcome::Failure
                }
            };
            state
                .audit
                .record(
                    AuditEvent::new(AuditAction::ContentPublish, outcome)
                        .with_resource("content", Some(record.id.clone()))
                        .with_details(details),
                )
                .await;

            if outcome != AuditOutcome::Success {
                continue;
            }
            crate::cdn::purge_content(state, &record.content_type, &record.id, &record.slug, None)
                .await;
            #[cfg(feature = "search")]
            crate::search::announce_change(state, &record.content_type, &record.id).await;
            notify_author(state, "content.published", &schema, &record, at).await;

            let published_content = aegis_core::PublishedContent {
                id: record.id.clone(),
                content_type: record.content_type.clone(),
                title: record.title.clone(),
                slug: record.slug.clone(),
                published_at: at.to_string(),
                author_id: record.author_id.clone(),
                fields: record.custom_fields.clone(),
            };
            let failures = {
                let supervisor = state.supervisor.read().await;
                supervisor
                    .notify_content_published(&published_content)
                    .await
            };
            for (plugin, error) in failures {
                tracing::warn!("Plugin '{}' on-content-published failed: {}", plugin, error);
                state
                    .audit
                    .record(
                        AuditEvent::new(AuditAction::PluginError, AuditOutcome::Failure)
                            .with_resource("content", Some(record.id.clone()))
                            .with_details(json!({
                                "hook": "on-content-published",
                                "plugin": plugin,
                                "content_type": record.content_type,
                                "error": error.to_string(),
                            })),
                    )
                    .await;
            }
        }
    }
    published
}

/// `now` までに非公開日時（`unpublish_at`）を迎えた公開中コンテンツをすべてのスキーマでアーカイブ化し、非公開化した件数を返す
pub async fn unpublish_due_contents(state: &AppState, now: DateTime<Utc>) -> usize {
    let schemas: Vec<SchemaDefinition> = state.schemas.read().await.values().cloned().collect();
    let query = ContentListQuery {
        status: Some("published".to_string()),
        ..Default::default()
    };
    let mut unpublished = 0;
    for schema in schemas {
        let records = match state.db.list_contents_page(&schema, &query).await {
            Ok(page) => page.records,
            Err(e) => {
                tracing::warn!(
                    "Failed to list published {} contents for unpublishing: {}",
                    schema.content_type,
                    e
                );
                continue;
            }
        };
        for record in records {
            let Some(at) = record.unpublish_at.as_deref() else {
                continue;
            };
            match DateTime::parse_from_rfc3339(at) {
                Ok(due) if due <= now => {}
                Ok(_) => continue,
                Err(e) => {
                    tracing::warn!(
                        "Published content {} has an invalid unpublish_at '{}': {}",
                        record.id,
                        at,
                        e
                    );
                    continue;
                }
            }

            let result = state
                .db
                .unpublish_scheduled_content(&schema, &record.id, at, &now.to_rfc3339())
                .await;
            let mut details = json!({
                "content_type": record.content_type,
                "slug": record.slug,
                "title": record.title,
                "unpublish_at": at,
            });
            let outcome = match &result {
                Ok(true) => {
                    unpublished += 1;
                    AuditOutcome::Success
                }
                // 一覧の取得後に非公開日時の変更・取り消し・ステータス変更があった
                Ok(false) => continue,
                Err(e) => {
                    tracing::warn!("Failed to unpublish scheduled content {}: {}", record.id, e);
                    details["error"] = json!(e.to_string());
                    AuditOutcome::Failure
                }
            };
            state
                .audit
                .record(
                    AuditEvent::new(AuditAction::ContentUnpublish, outcome)
                        .with_resource("content", Some(record.id.clone()))
                        .with_details(details),
                )
                .await;
            if outcome == AuditOutcome::Success {
                crate::cdn::purge_content(
                    state,
                    &record.content_type,
                    &record.id,
                    &record.slug,
                    None,
                )
                .await;
                #[cfg(feature = "search")]
                crate::search::announce_change(state, &record.content_type, &record.id).await;
                notify_author(state, "content.unpublished", &schema, &record, at).await;
            }
        }
    }
    unpublished
}

/// 予約公開・予約非公開が済んだことを著者に知らせる（`content.published` / `content.unpublished`）。
/// 通知が無効、著者がいない・メールアドレスが無い・無効なアカウントなら送らない
async fn notify_author(
    state: &AppState,
    kind: &str,
    schema: &SchemaDefinition,
    record: &ContentRecord,
    at: &str,
) {
    let Some(notifier) = &state.notifier else {
        return;
    };
    let Some(author_id) = record.author_id.as_deref().filter(|a| !a.is_empty()) else {
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
    notifier.notify(NotificationEvent::new(
        kind,
        vec![Recipient::person(email, name)],
        json!({
            "username": author.username,
            "display_name": author.display_name,
            "at": at,
            "content": {
                "id": record.id,
                "content_type": record.content_type,
                "label": schema.label,
                "title": record.title,
                "slug": record.slug,
            },
            "content_url": crate::site::content_url(&state.config, &record.content_type, &record.slug),
        }),
    ));
}

/// 予約公開および予約非公開を定期的に行うタスクを起動する
pub fn spawn_publish_task(state: Arc<AppState>) {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(PUBLISH_INTERVAL);
        loop {
            interval.tick().await;
            let now = Utc::now();
            match publish_due_contents(&state, now).await {
                0 => {}
                n => tracing::info!("Published {} scheduled content(s)", n),
            }
            match unpublish_due_contents(&state, now).await {
                0 => {}
                n => tracing::info!("Unpublished {} scheduled content(s)", n),
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audit::AuditContext;
    use crate::auth::authorize::RequireRole;
    use crate::handlers::content::{create_content, update_content, ContentResponse};
    use crate::test_support::test_state;
    use aegis_runtime::PluginSupervisor;
    use axum::extract::{Path, State};
    use axum::http::StatusCode;
    use axum::response::IntoResponse;
    use axum::Json;
    use serde_json::Value;

    fn editor() -> crate::auth::EditorSession {
        RequireRole(aegis_auth::SessionData {
            id: "ses_editor".to_string(),
            user_id: "editor1".to_string(),
            role: "editor".to_string(),
            created_at: String::new(),
            expires_at: String::new(),
            metadata: Default::default(),
        })
    }

    async fn create(state: &Arc<AppState>, payload: Value) -> Result<ContentResponse, StatusCode> {
        let response = create_content(
            State(state.clone()),
            editor(),
            Path("article".to_string()),
            AuditContext::default(),
            Json(serde_json::from_value(payload).unwrap()),
        )
        .await
        .map_err(|(status, _)| status)?
        .into_response();
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        Ok(serde_json::from_slice(&body).unwrap())
    }

    async fn update(
        state: &Arc<AppState>,
        slug: &str,
        payload: Value,
    ) -> Result<ContentResponse, StatusCode> {
        update_content(
            State(state.clone()),
            editor(),
            Path(("article".to_string(), slug.to_string())),
            AuditContext::default(),
            Json(serde_json::from_value(payload).unwrap()),
        )
        .await
        .map(|Json(body)| body)
        .map_err(|(status, _)| status)
    }

    async fn status_of(state: &AppState, slug: &str) -> (String, Option<String>) {
        let schema = state.schemas.read().await["article"].clone();
        let record = state.db.get_content(&schema, slug).await.unwrap().unwrap();
        (record.status, record.published_at)
    }

    #[tokio::test]
    async fn test_scheduled_content_is_published_when_due() {
        let state = test_state(PluginSupervisor::new()).await;
        let created = create(
            &state,
            serde_json::json!({ "title": "Later", "slug": "later", "status": "scheduled",
                                "published_at": "2099-01-02T03:04:05+09:00" }),
        )
        .await
        .unwrap();
        // 予約日時は UTC に揃えて保存する
        assert_eq!(created.status, "scheduled");
        assert_eq!(
            created.published_at.as_deref(),
            Some("2099-01-01T18:04:05Z")
        );

        // 予約日時の前は公開しない
        let before: DateTime<Utc> = "2099-01-01T18:04:04Z".parse().unwrap();
        assert_eq!(publish_due_contents(&state, before).await, 0);
        assert_eq!(status_of(&state, "later").await.0, "scheduled");

        let due: DateTime<Utc> = "2099-01-01T18:04:05Z".parse().unwrap();
        assert_eq!(publish_due_contents(&state, due).await, 1);
        assert_eq!(
            status_of(&state, "later").await,
            (
                "published".to_string(),
                Some("2099-01-01T18:04:05Z".to_string())
            )
        );
        // 2 回目は何もしない
        assert_eq!(publish_due_contents(&state, due).await, 0);

        // 公開した後に編集しても、公開日時は予約した日時のまま
        let edited = update(
            &state,
            "later",
            serde_json::json!({ "title": "Edited", "slug": "later", "status": "published" }),
        )
        .await
        .unwrap();
        assert_eq!(edited.published_at.as_deref(), Some("2099-01-01T18:04:05Z"));

        let events = state
            .db
            .query_audit_events(&aegis_core::AuditQuery {
                action: Some(AuditAction::ContentPublish),
                limit: 10,
                ..Default::default()
            })
            .await
            .unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].outcome, AuditOutcome::Success);
        assert_eq!(events[0].actor, None);
        assert_eq!(events[0].resource_id.as_deref(), Some(created.id.as_str()));
    }

    #[tokio::test]
    async fn test_rescheduled_or_unscheduled_content_is_left_alone() {
        let state = test_state(PluginSupervisor::new()).await;
        create(
            &state,
            serde_json::json!({ "title": "A", "slug": "a", "status": "scheduled",
                                "published_at": "2099-01-01T00:00:00Z" }),
        )
        .await
        .unwrap();
        create(
            &state,
            serde_json::json!({ "title": "B", "slug": "b", "status": "scheduled",
                                "published_at": "2099-01-01T00:00:00Z" }),
        )
        .await
        .unwrap();

        // A は延期、B は下書きに戻す
        update(
            &state,
            "a",
            serde_json::json!({ "title": "A", "slug": "a", "status": "scheduled",
                                "published_at": "2099-06-01T00:00:00Z" }),
        )
        .await
        .unwrap();
        let draft = update(
            &state,
            "b",
            serde_json::json!({ "title": "B", "slug": "b", "status": "draft" }),
        )
        .await
        .unwrap();
        assert_eq!(draft.published_at, None);

        let now: DateTime<Utc> = "2099-02-01T00:00:00Z".parse().unwrap();
        assert_eq!(publish_due_contents(&state, now).await, 0);
        assert_eq!(status_of(&state, "a").await.0, "scheduled");
        assert_eq!(status_of(&state, "b").await.0, "draft");

        // 予約日時の変更と公開が行き違っても、古い予約日時では公開しない
        let schema = state.schemas.read().await["article"].clone();
        let a = state.db.get_content(&schema, "a").await.unwrap().unwrap();
        assert!(!state
            .db
            .publish_scheduled_content(&schema, &a.id, "2099-01-01T00:00:00Z", "now")
            .await
            .unwrap());
    }

    #[tokio::test]
    async fn test_scheduling_requires_a_valid_time() {
        let state = test_state(PluginSupervisor::new()).await;
        for published_at in [Value::Null, "".into(), "tomorrow".into()] {
            assert_eq!(
                create(
                    &state,
                    serde_json::json!({ "title": "x", "slug": "x", "status": "scheduled",
                                        "published_at": published_at }),
                )
                .await
                .unwrap_err(),
                StatusCode::BAD_REQUEST
            );
        }

        // 過ぎた日時を予約するとその場で公開する
        let past = create(
            &state,
            serde_json::json!({ "title": "Now", "slug": "now", "status": "scheduled",
                                "published_at": "2000-01-01T00:00:00Z" }),
        )
        .await
        .unwrap();
        assert_eq!(past.status, "published");
        assert!(past.published_at.unwrap().as_str() > "2026");

        // 予約以外の状態では published_at を無視する
        let draft = create(
            &state,
            serde_json::json!({ "title": "Draft", "slug": "draft", "status": "draft",
                                "published_at": "2099-01-01T00:00:00Z" }),
        )
        .await
        .unwrap();
        assert_eq!(draft.published_at, None);
    }

    #[tokio::test]
    async fn test_scheduled_content_is_unpublished_when_due() {
        let state = test_state(PluginSupervisor::new()).await;
        let created = create(
            &state,
            serde_json::json!({
                "title": "Campaign",
                "slug": "campaign",
                "status": "published",
                "published_at": "2099-01-01T00:00:00Z",
                "unpublish_at": "2099-01-10T00:00:00Z"
            }),
        )
        .await
        .unwrap();
        assert_eq!(created.status, "published");
        assert_eq!(
            created.unpublish_at.as_deref(),
            Some("2099-01-10T00:00:00Z")
        );

        // 期日前は非公開化されない
        let before: DateTime<Utc> = "2099-01-09T23:59:59Z".parse().unwrap();
        assert_eq!(unpublish_due_contents(&state, before).await, 0);
        let schema = state.schemas.read().await["article"].clone();
        let record = state
            .db
            .get_content(&schema, "campaign")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(record.status, "published");

        // 期日到来でアーカイブ化される
        let due: DateTime<Utc> = "2099-01-10T00:00:00Z".parse().unwrap();
        assert_eq!(unpublish_due_contents(&state, due).await, 1);
        let record = state
            .db
            .get_content(&schema, "campaign")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(record.status, "archived");

        // 監査ログが記録されていることを確認
        let events = state
            .db
            .query_audit_events(&aegis_core::AuditQuery {
                action: Some(AuditAction::ContentUnpublish),
                limit: 10,
                ..Default::default()
            })
            .await
            .unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].outcome, AuditOutcome::Success);
        assert_eq!(events[0].resource_id.as_deref(), Some(created.id.as_str()));

        // 予約公開（status: "scheduled"）で unpublish_at <= published_at は 400 Bad Request
        let bad_scheduled = create(
            &state,
            serde_json::json!({
                "title": "Bad Scheduled",
                "slug": "bad-scheduled",
                "status": "scheduled",
                "published_at": "2099-01-10T00:00:00Z",
                "unpublish_at": "2099-01-05T00:00:00Z"
            }),
        )
        .await;
        assert_eq!(bad_scheduled.unwrap_err(), StatusCode::BAD_REQUEST);

        // 公開中（status: "published"）で過去の日時（unpublish_at <= published_at/now）は 400 Bad Request
        let bad_published = create(
            &state,
            serde_json::json!({
                "title": "Bad Published",
                "slug": "bad-published",
                "status": "published",
                "unpublish_at": "2000-01-01T00:00:00Z"
            }),
        )
        .await;
        assert_eq!(bad_published.unwrap_err(), StatusCode::BAD_REQUEST);
    }

    struct MockPublishPlugin {
        manifest: aegis_core::PluginManifest,
        published: Arc<std::sync::Mutex<Vec<aegis_core::PublishedContent>>>,
        should_fail: bool,
    }

    #[async_trait::async_trait]
    impl aegis_runtime::AegisPlugin for MockPublishPlugin {
        fn manifest(&self) -> &aegis_core::PluginManifest {
            &self.manifest
        }

        async fn on_content_published(
            &self,
            content: aegis_core::PublishedContent,
        ) -> aegis_core::Result<()> {
            if self.should_fail {
                return Err(aegis_core::AegisError::PluginRuntime(
                    "simulated webhook failure".to_string(),
                ));
            }
            self.published.lock().unwrap().push(content);
            Ok(())
        }
    }

    /// `test_state` with a recording notifier, a public URL and an author `editor1` with `email`
    async fn state_with_notifier(
        email: Option<&str>,
    ) -> (Arc<AppState>, Arc<crate::test_support::RecordingNotifier>) {
        let state = test_state(PluginSupervisor::new()).await;
        aegis_auth::users::insert_new_user(
            state.db.as_ref(),
            aegis_auth::users::NewUser {
                username: "editor1".to_string(),
                password: String::new(),
                role: "editor".to_string(),
                email: email.map(str::to_string),
                display_name: Some("Hanako".to_string()),
            },
            "hash".to_string(),
            false,
            false,
        )
        .await
        .unwrap();
        let notifier = Arc::new(crate::test_support::RecordingNotifier::default());
        let mut state = (*state).clone();
        state.notifier = Some(notifier.clone());
        state.config.commerce.public_url = "https://example.com/".to_string();
        (Arc::new(state), notifier)
    }

    #[tokio::test]
    async fn test_author_is_notified_when_scheduled_content_is_published_and_unpublished() {
        let (state, notifier) = state_with_notifier(Some("hanako@example.com")).await;
        create(
            &state,
            serde_json::json!({ "title": "Spring Sale", "slug": "spring-sale", "status": "scheduled",
                                "published_at": "2099-03-01T00:00:00Z",
                                "unpublish_at": "2099-03-31T00:00:00Z" }),
        )
        .await
        .unwrap();

        // Nothing before the date
        let before: DateTime<Utc> = "2099-02-28T23:59:59Z".parse().unwrap();
        assert_eq!(publish_due_contents(&state, before).await, 0);
        assert!(notifier.take().is_empty());

        let due: DateTime<Utc> = "2099-03-01T00:00:00Z".parse().unwrap();
        assert_eq!(publish_due_contents(&state, due).await, 1);
        let events = notifier.take();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].kind, "content.published");
        assert_eq!(
            events[0].recipients,
            vec![Recipient::person("hanako@example.com", "Hanako")]
        );
        let data = &events[0].data;
        assert_eq!(data["content"]["title"], "Spring Sale");
        assert_eq!(data["content"]["label"], "Articles");
        assert_eq!(data["at"], "2099-03-01T00:00:00Z");
        assert_eq!(data["content_url"], "https://example.com/spring-sale");

        // Publishing again sends nothing (it is no longer scheduled)
        assert_eq!(publish_due_contents(&state, due).await, 0);
        assert!(notifier.take().is_empty());

        let end: DateTime<Utc> = "2099-03-31T00:00:00Z".parse().unwrap();
        assert_eq!(unpublish_due_contents(&state, end).await, 1);
        let events = notifier.take();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].kind, "content.unpublished");
        assert_eq!(events[0].data["at"], "2099-03-31T00:00:00Z");
    }

    #[tokio::test]
    async fn test_no_notification_without_an_author_email() {
        let (state, notifier) = state_with_notifier(None).await;
        create(
            &state,
            serde_json::json!({ "title": "Quiet", "slug": "quiet", "status": "scheduled",
                                "published_at": "2099-04-01T00:00:00Z" }),
        )
        .await
        .unwrap();
        let due: DateTime<Utc> = "2099-04-01T00:00:00Z".parse().unwrap();
        assert_eq!(publish_due_contents(&state, due).await, 1);
        assert!(notifier.take().is_empty());
    }

    #[tokio::test]
    async fn test_scheduled_content_triggers_on_content_published_hook() {
        let mut supervisor = PluginSupervisor::new();
        let published_items = Arc::new(std::sync::Mutex::new(Vec::new()));
        let manifest = aegis_core::PluginManifest {
            name: "publisher-webhook".to_string(),
            capabilities: aegis_core::PluginCapabilities {
                hooks: vec!["on-content-published".to_string()],
                ..Default::default()
            },
            ..Default::default()
        };

        supervisor.register(Arc::new(MockPublishPlugin {
            manifest,
            published: published_items.clone(),
            should_fail: false,
        }));

        let state = test_state(supervisor).await;
        let created = create(
            &state,
            serde_json::json!({
                "title": "Release Notes",
                "slug": "release-notes",
                "status": "scheduled",
                "published_at": "2099-05-01T12:00:00Z"
            }),
        )
        .await
        .unwrap();

        // 予約日時前はフックは呼ばれない
        let before: DateTime<Utc> = "2099-05-01T11:59:59Z".parse().unwrap();
        assert_eq!(publish_due_contents(&state, before).await, 0);
        assert!(published_items.lock().unwrap().is_empty());

        // 予約日時に公開され、フックが呼ばれる
        let due: DateTime<Utc> = "2099-05-01T12:00:00Z".parse().unwrap();
        assert_eq!(publish_due_contents(&state, due).await, 1);

        let items = published_items.lock().unwrap().clone();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].id, created.id);
        assert_eq!(items[0].slug, "release-notes");
        assert_eq!(items[0].title, "Release Notes");
        assert_eq!(items[0].published_at, "2099-05-01T12:00:00Z");
        assert_eq!(items[0].content_type, "article");
    }

    #[tokio::test]
    async fn test_scheduled_content_plugin_error_is_audited_and_does_not_block_publishing() {
        let mut supervisor = PluginSupervisor::new();
        let manifest = aegis_core::PluginManifest {
            name: "failing-plugin".to_string(),
            capabilities: aegis_core::PluginCapabilities {
                hooks: vec!["on-content-published".to_string()],
                ..Default::default()
            },
            ..Default::default()
        };

        supervisor.register(Arc::new(MockPublishPlugin {
            manifest,
            published: Arc::new(std::sync::Mutex::new(Vec::new())),
            should_fail: true,
        }));

        let state = test_state(supervisor).await;
        let created = create(
            &state,
            serde_json::json!({
                "title": "Important News",
                "slug": "important-news",
                "status": "scheduled",
                "published_at": "2099-06-01T00:00:00Z"
            }),
        )
        .await
        .unwrap();

        let due: DateTime<Utc> = "2099-06-01T00:00:00Z".parse().unwrap();
        // 公開自体は成功する
        assert_eq!(publish_due_contents(&state, due).await, 1);
        assert_eq!(status_of(&state, "important-news").await.0, "published");

        // 監査ログに ContentPublish と PluginError の両方が記録されている
        let publish_events = state
            .db
            .query_audit_events(&aegis_core::AuditQuery {
                action: Some(AuditAction::ContentPublish),
                limit: 10,
                ..Default::default()
            })
            .await
            .unwrap();
        assert_eq!(publish_events.len(), 1);
        assert_eq!(publish_events[0].outcome, AuditOutcome::Success);
        assert_eq!(
            publish_events[0].resource_id.as_deref(),
            Some(created.id.as_str())
        );

        let error_events = state
            .db
            .query_audit_events(&aegis_core::AuditQuery {
                action: Some(AuditAction::PluginError),
                limit: 10,
                ..Default::default()
            })
            .await
            .unwrap();
        assert_eq!(error_events.len(), 1);
        assert_eq!(error_events[0].outcome, AuditOutcome::Failure);
        assert_eq!(
            error_events[0].resource_id.as_deref(),
            Some(created.id.as_str())
        );
        let details = &error_events[0].details;
        assert_eq!(details["hook"], "on-content-published");
        assert_eq!(details["plugin"], "failing-plugin");
    }
}
