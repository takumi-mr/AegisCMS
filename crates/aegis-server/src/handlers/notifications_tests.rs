//! 通知の管理 API の結合テスト（実際のルーターを通す）: 権限、テンプレートの編集・プレビュー・元に戻す、
//! テスト送信、送れなかった通知の再送・破棄、ほかのサーバーでの編集の反映

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use aegis_cache::BusMessage;
use aegis_core::{AuditQuery, Delivered, NotificationEvent, NotificationMessage, SendError};
use aegis_notify::{NotificationChannel, NotificationService, NotificationsConfig, SiteInfo};
use aegis_runtime::PluginSupervisor;
use async_trait::async_trait;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use serde_json::{json, Value};
use tower::ServiceExt;

use super::*;
use crate::config::AegisConfig;

/// 送った通知を覚えておくチャネル。`fail` の間は再試行しない失敗を返す
#[derive(Default)]
struct Outbox {
    sent: Mutex<Vec<NotificationMessage>>,
    fail: AtomicBool,
}

#[async_trait]
impl NotificationChannel for Outbox {
    fn name(&self) -> &str {
        "email"
    }

    fn accepts(&self, _recipient: &Recipient) -> bool {
        true
    }

    async fn send(&self, message: &NotificationMessage) -> Result<Delivered, SendError> {
        if self.fail.load(Ordering::SeqCst) {
            return Err(SendError::permanent("550 mailbox unavailable"));
        }
        self.sent.lock().unwrap().push(message.clone());
        Ok(Delivered::Sent)
    }
}

impl Outbox {
    fn subjects(&self) -> Vec<String> {
        std::mem::take(&mut *self.sent.lock().unwrap())
            .into_iter()
            .map(|m| m.subject)
            .collect()
    }
}

struct Site {
    state: Arc<AppState>,
    outbox: Arc<Outbox>,
    admin: String,
    editor: String,
}

async fn site(enabled: bool) -> Site {
    let mut config = AegisConfig::default();
    config.audit.enabled = true;
    config.notifications.enabled = enabled;
    let state =
        crate::test_support::test_app_state(config, PluginSupervisor::new(), Vec::new()).await;
    let outbox = Arc::new(Outbox::default());
    let state = if enabled {
        let config = NotificationsConfig {
            enabled: true,
            ..Default::default()
        };
        let service = NotificationService::with_channels(
            &config,
            SiteInfo {
                name: "Aegis Coffee".to_string(),
                url: "https://shop.example.com".to_string(),
                admin_url: "https://shop.example.com/admin/".to_string(),
            },
            Some(state.audit.clone()),
            vec![outbox.clone()],
        )
        .with_queue(state.db.clone());
        state.with_notifications(service)
    } else {
        state
    };
    let state = Arc::new(state);
    let session = |user: &'static str, role: &'static str| {
        let state = state.clone();
        async move {
            state
                .sessions
                .create_session(user, role, Default::default(), None)
                .await
                .unwrap()
                .id
        }
    };
    Site {
        admin: session("admin", "admin").await,
        editor: session("editor1", "editor").await,
        state,
        outbox,
    }
}

impl Site {
    async fn call(
        &self,
        method: &str,
        path: &str,
        body: Option<Value>,
        token: &str,
    ) -> (StatusCode, Value) {
        let request = Request::builder()
            .method(method)
            .uri(path)
            .header("authorization", format!("Bearer {}", token));
        let request = match body {
            Some(body) => request
                .header("content-type", "application/json")
                .body(Body::from(body.to_string())),
            None => request.body(Body::empty()),
        }
        .unwrap();
        let response = crate::app::router(&self.state.config, self.state.clone())
            .oneshot(request)
            .await
            .unwrap();
        let status = response.status();
        let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        (
            status,
            serde_json::from_slice(&bytes).unwrap_or(Value::Null),
        )
    }

    async fn audit_details(&self, action: AuditAction) -> Vec<Value> {
        self.state
            .db
            .query_audit_events(&AuditQuery {
                action: Some(action),
                limit: 100,
                ..Default::default()
            })
            .await
            .unwrap()
            .into_iter()
            .map(|e| e.details)
            .collect()
    }
}

#[tokio::test]
async fn test_status_needs_an_admin_and_reports_the_setup() {
    let site = site(true).await;
    let (status, _) = site
        .call("GET", "/api/v1/notifications", None, &site.editor)
        .await;
    assert_eq!(status, StatusCode::FORBIDDEN);

    let (status, body) = site
        .call("GET", "/api/v1/notifications", None, &site.admin)
        .await;
    assert_eq!(status, StatusCode::OK, "{}", body);
    assert_eq!(body["enabled"], true);
    assert_eq!(body["channels"], json!(["email"]));
    assert_eq!(body["html"], true);
    assert_eq!(body["persistent"], true);
    assert_eq!(body["queue"], json!({ "pending": 0, "failed": 0 }));
    let kinds = body["kinds"].as_array().unwrap();
    let registered = kinds
        .iter()
        .find(|k| k["kind"] == "account.registered")
        .unwrap();
    // 運営者向けだけの種類
    assert_eq!(registered["person"], false);
    assert_eq!(registered["staff"], true);

    // 無効なら enabled: false。テンプレートなどの API は 404
    drop(site);
    let off = self::site(false).await;
    let (status, body) = off
        .call("GET", "/api/v1/notifications", None, &off.admin)
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["enabled"], false);
    let (status, _) = off
        .call("GET", "/api/v1/notifications/templates", None, &off.admin)
        .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_edit_preview_send_and_reset_a_template() {
    let site = site(true).await;
    let (status, detail) = site
        .call(
            "GET",
            "/api/v1/notifications/templates/test.subject",
            None,
            &site.admin,
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(detail["current"]["source"], "builtin");
    let builtin = detail["current"]["content"].as_str().unwrap().to_string();

    // 保存前のプレビュー
    let (status, preview) = site
        .call(
            "POST",
            "/api/v1/notifications/preview",
            Some(json!({
                "kind": "test",
                "templates": { "test.subject": "Draft for {{ site_name }}" }
            })),
            &site.admin,
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{}", preview);
    assert_eq!(preview["subject"], "Draft for Aegis Coffee");
    assert!(preview["html"]
        .as_str()
        .unwrap()
        .contains("<!DOCTYPE html>"));

    // 文法の誤り・名前の誤りは保存しない
    let (status, body) = site
        .call(
            "PUT",
            "/api/v1/notifications/templates/test.subject",
            Some(json!({ "content": "Hello {% if %}" })),
            &site.admin,
        )
        .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{}", body);
    let (status, _) = site
        .call(
            "PUT",
            "/api/v1/notifications/templates/..%2Fsecret",
            Some(json!({ "content": "x" })),
            &site.admin,
        )
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    let (status, saved) = site
        .call(
            "PUT",
            "/api/v1/notifications/templates/test.subject",
            Some(json!({ "content": "Checked by {{ site_name }}" })),
            &site.admin,
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{}", saved);
    assert_eq!(saved["current"]["source"], "custom");
    assert_eq!(saved["default"]["content"], builtin);
    let (_, list) = site
        .call("GET", "/api/v1/notifications/templates", None, &site.admin)
        .await;
    assert!(list
        .as_array()
        .unwrap()
        .contains(&json!({ "name": "test.subject", "source": "custom" })));

    // テスト送信は編集したテンプレートで、その場で送る
    let (status, results) = site
        .call(
            "POST",
            "/api/v1/notifications/test",
            Some(json!({ "to": "you@example.com" })),
            &site.admin,
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{}", results);
    assert_eq!(
        results,
        json!([{
            "channel": "email", "recipient": "you@example.com",
            "subject": "Checked by Aegis Coffee", "ok": true, "error": null
        }])
    );
    assert_eq!(site.outbox.subjects(), ["Checked by Aegis Coffee"]);

    // 見本の値でほかの種類も送れる
    let (status, results) = site
        .call(
            "POST",
            "/api/v1/notifications/test",
            Some(json!({ "kind": "order.placed", "staff": true })),
            &site.admin,
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{}", results);
    assert_eq!(results[0]["recipient"], "staff");
    assert!(site.outbox.subjects()[0].starts_with("New order 261025-SAMPLE"));

    // 元に戻す
    let (status, reset) = site
        .call(
            "DELETE",
            "/api/v1/notifications/templates/test.subject",
            None,
            &site.admin,
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(reset["current"]["source"], "builtin");
    let (status, _) = site
        .call(
            "DELETE",
            "/api/v1/notifications/templates/test.subject",
            None,
            &site.admin,
        )
        .await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    let operations: Vec<Value> = site
        .audit_details(AuditAction::NotificationTemplateUpdate)
        .await
        .into_iter()
        .map(|d| d["operation"].clone())
        .collect();
    assert_eq!(operations, [json!("reset"), json!("save")]);
}

#[tokio::test]
async fn test_test_send_needs_a_valid_recipient() {
    let site = site(true).await;
    for body in [json!({}), json!({ "to": "not an address" })] {
        let (status, _) = site
            .call(
                "POST",
                "/api/v1/notifications/test",
                Some(body),
                &site.admin,
            )
            .await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
    }
    // テンプレートの無い種類は何も送らない
    let (status, _) = site
        .call(
            "POST",
            "/api/v1/notifications/test",
            Some(json!({ "kind": "no.such.kind", "to": "you@example.com" })),
            &site.admin,
        )
        .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
}

#[tokio::test]
async fn test_retry_and_discard_failed_notifications() {
    let site = site(true).await;
    let service = site.state.notifications.clone().unwrap();
    site.outbox.fail.store(true, Ordering::SeqCst);
    for to in ["a@example.com", "b@example.com"] {
        service
            .enqueue(&NotificationEvent::new(
                "test",
                vec![Recipient::person(to, "")],
                json!({}),
            ))
            .await
            .unwrap();
    }
    service.process_queue().await;

    let (status, page) = site
        .call(
            "GET",
            "/api/v1/notifications/queue?status=failed",
            None,
            &site.admin,
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{}", page);
    assert_eq!(page["total"], 2);
    let item = &page["items"][0];
    assert_eq!(item["channel"], "email");
    assert_eq!(item["last_error"], "550 mailbox unavailable");
    assert!(item["subject"].as_str().unwrap().contains("Aegis Coffee"));
    let ids: Vec<String> = page["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|i| i["id"].as_str().unwrap().to_string())
        .collect();
    let (_, status_body) = site
        .call("GET", "/api/v1/notifications", None, &site.admin)
        .await;
    assert_eq!(status_body["queue"]["failed"], 2);

    // 再送: 直ったので送れる
    site.outbox.fail.store(false, Ordering::SeqCst);
    let (status, _) = site
        .call(
            "POST",
            &format!("/api/v1/notifications/queue/{}/retry", ids[0]),
            None,
            &site.admin,
        )
        .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    service.process_queue().await;
    assert_eq!(site.outbox.subjects().len(), 1);
    // 送れたものはもう無い
    let (status, _) = site
        .call(
            "POST",
            &format!("/api/v1/notifications/queue/{}/retry", ids[0]),
            None,
            &site.admin,
        )
        .await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    // 破棄
    let (status, _) = site
        .call(
            "DELETE",
            &format!("/api/v1/notifications/queue/{}", ids[1]),
            None,
            &site.admin,
        )
        .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let (_, page) = site
        .call("GET", "/api/v1/notifications/queue", None, &site.admin)
        .await;
    assert_eq!(page["total"], 0);
    let operations: Vec<Value> = site
        .audit_details(AuditAction::NotificationQueueUpdate)
        .await
        .into_iter()
        .map(|d| d["operation"].clone())
        .collect();
    assert_eq!(operations, [json!("discard"), json!("retry")]);
}

#[tokio::test]
async fn test_templates_edited_on_another_server_are_reloaded() {
    let site = site(true).await;
    // ほかのサーバーが保存した
    site.state
        .db
        .set_setting(
            &format!("{}test.subject", TEMPLATE_SETTING_PREFIX),
            "From another server",
        )
        .await
        .unwrap();
    let service = site.state.notifications.clone().unwrap();
    assert!(service.templates().overrides().is_empty());
    crate::cluster::handle(
        &site.state,
        BusMessage::Event {
            topic: crate::cluster::TOPIC_NOTIFICATION_TEMPLATES.to_string(),
            payload: String::new(),
        },
    )
    .await;
    let rendered = service
        .preview("test", &Recipient::Staff, Default::default())
        .unwrap()
        .unwrap();
    assert_eq!(rendered.subject, "From another server");
}
