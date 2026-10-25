//! コメントの API と公開ページの結合テスト（実際のルーターを通す）

use std::sync::{Arc, Mutex};

use aegis_core::{NotificationEvent, Notifier};
use aegis_runtime::PluginSupervisor;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use serde_json::{json, Value};
use tower::ServiceExt;

use super::*;
use crate::config::AegisConfig;
use aegis_db::ContentRecord;

#[derive(Default)]
struct RecordingNotifier {
    events: Mutex<Vec<NotificationEvent>>,
}

impl Notifier for RecordingNotifier {
    fn notify(&self, event: NotificationEvent) {
        self.events.lock().unwrap().push(event);
    }
}

impl RecordingNotifier {
    fn kinds(&self) -> Vec<String> {
        std::mem::take(&mut *self.events.lock().unwrap())
            .into_iter()
            .map(|e| e.kind)
            .collect()
    }
}

struct Site {
    state: Arc<AppState>,
    mails: Arc<RecordingNotifier>,
    editor: String,
    admin: String,
}

async fn site(configure: impl FnOnce(&mut AegisConfig)) -> Site {
    let mut config = AegisConfig::default();
    config.comments.enabled = true;
    config.audit.enabled = true;
    configure(&mut config);
    let state =
        crate::test_support::test_app_state(config, PluginSupervisor::new(), Vec::new()).await;
    let mails = Arc::new(RecordingNotifier::default());
    let state = state.with_notifier(mails.clone());
    let comments =
        CommentService::new(state.config.comments.clone(), state.comment_deps()).unwrap();
    let state = Arc::new(state.with_comments(comments));

    let schema = state.schemas.read().await["article"].clone();
    for (id, slug, status) in [("a1", "hello", "published"), ("a2", "draft", "draft")] {
        state
            .db
            .insert_content(
                &schema,
                &ContentRecord {
                    id: id.to_string(),
                    content_type: "article".to_string(),
                    title: slug.to_uppercase(),
                    slug: slug.to_string(),
                    status: status.to_string(),
                    blocks_json: "[]".to_string(),
                    author_id: None,
                    published_at: Some("2026-10-01T00:00:00Z".to_string()),
                    unpublish_at: None,
                    created_at: "2026-10-01T00:00:00Z".to_string(),
                    updated_at: "2026-10-01T00:00:00Z".to_string(),
                    custom_fields: Default::default(),
                },
            )
            .await
            .unwrap();
    }
    let editor = state
        .sessions
        .create_session("editor1", "editor", Default::default(), None)
        .await
        .unwrap()
        .id;
    let admin = state
        .sessions
        .create_session("admin", "admin", Default::default(), None)
        .await
        .unwrap()
        .id;
    Site {
        state,
        mails,
        editor,
        admin,
    }
}

impl Site {
    async fn send(&self, request: Request<Body>) -> (StatusCode, String) {
        let response = crate::app::router(&self.state.config, self.state.clone())
            .oneshot(request)
            .await
            .unwrap();
        let status = response.status();
        let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        (status, String::from_utf8_lossy(&bytes).to_string())
    }

    async fn call(
        &self,
        method: &str,
        path: &str,
        body: Option<Value>,
        token: Option<&str>,
    ) -> (StatusCode, Value) {
        let mut request =
            Request::builder()
                .method(method)
                .uri(path)
                .extension(axum::extract::ConnectInfo(std::net::SocketAddr::from((
                    [203, 0, 113, 7],
                    40000,
                ))));
        if let Some(token) = token {
            request = request.header("authorization", format!("Bearer {}", token));
        }
        let request = match body {
            Some(body) => request
                .header("content-type", "application/json")
                .body(Body::from(body.to_string())),
            None => request.body(Body::empty()),
        }
        .unwrap();
        let (status, text) = self.send(request).await;
        (status, serde_json::from_str(&text).unwrap_or(Value::Null))
    }

    async fn page(&self, path: &str) -> (StatusCode, String) {
        self.send(Request::get(path).body(Body::empty()).unwrap())
            .await
    }

    async fn post_comment(&self, body: Value) -> (StatusCode, Value) {
        self.call(
            "POST",
            "/api/v1/contents/article/hello/comments",
            Some(body),
            None,
        )
        .await
    }
}

#[tokio::test]
async fn test_post_moderate_and_show_on_the_article_page() {
    let site = site(|_| {}).await;

    // The article page has an empty comment section with a form
    let (status, html) = site.page("/articles/hello").await;
    assert_eq!(status, StatusCode::OK);
    assert!(html.contains("id=\"comments\""), "{}", html);
    assert!(html.contains("No comments yet."));
    // minijinja escapes `/` in attributes (`&#x2f;`); the browser decodes it
    assert!(
        html.contains(
            "data-post-url=\"&#x2f;api&#x2f;v1&#x2f;contents&#x2f;article&#x2f;a1&#x2f;comments\""
        ),
        "{}",
        html
    );

    let (status, body) = site
        .post_comment(json!({
            "name": "Hanako", "email": "hanako@example.com", "body": "<b>Hi</b> there",
            "notify_replies": true
        }))
        .await;
    assert_eq!(status, StatusCode::ACCEPTED, "{}", body);
    assert_eq!(body, json!({ "status": "pending" }));
    assert_eq!(site.mails.kinds(), ["comment.pending"]);

    // Waiting comments are not public
    let (_, public) = site
        .call("GET", "/api/v1/contents/article/hello/comments", None, None)
        .await;
    assert_eq!(public["count"], 0);

    // Moderation needs a session
    let (status, _) = site.call("GET", "/api/v1/comments", None, None).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    let (status, list) = site
        .call(
            "GET",
            "/api/v1/comments?status=pending",
            None,
            Some(&site.editor),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{}", list);
    assert_eq!(list["total"], 1);
    assert_eq!(list["counts"]["pending"], 1);
    let comment = &list["comments"][0];
    assert_eq!(comment["ip"], "203.0.113.7");
    assert_eq!(comment["content"]["title"], "HELLO");
    assert!(comment.get("notify_token").is_none());
    let id = comment["id"].as_str().unwrap().to_string();

    let (status, _) = site
        .call(
            "PATCH",
            &format!("/api/v1/comments/{}", id),
            Some(json!({ "status": "approved" })),
            Some(&site.editor),
        )
        .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let (status, _) = site
        .call(
            "PATCH",
            "/api/v1/comments/missing",
            Some(json!({ "status": "approved" })),
            Some(&site.editor),
        )
        .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (status, _) = site
        .call(
            "PATCH",
            &format!("/api/v1/comments/{}", id),
            Some(json!({ "status": "published" })),
            Some(&site.editor),
        )
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    // Now it is on the page, escaped
    let (_, html) = site.page("/articles/hello").await;
    assert!(html.contains("1 comment"), "{}", html);
    assert!(html.contains("&lt;b&gt;Hi&lt;/b&gt; there"), "{}", html);
    assert!(!html.contains("<b>Hi</b>"));
    assert!(!html.contains("hanako@example.com"));

    // An editor replies from Admin Studio; the reader asked to hear about replies
    let (status, reply) = site
        .call(
            "POST",
            &format!("/api/v1/comments/{}/replies", id),
            Some(json!({ "body": "Thanks!" })),
            Some(&site.editor),
        )
        .await;
    assert_eq!(status, StatusCode::CREATED, "{}", reply);
    assert_eq!(site.mails.kinds(), ["comment.replied"]);
    let (_, html) = site.page("/articles/hello").await;
    assert!(html.contains("2 comments"));
    assert!(html.contains("aegis-comment-badge"));

    // The moderation is in the audit log
    let (_, logs) = site
        .call(
            "GET",
            "/api/v1/audit-logs?action=comment.moderate",
            None,
            Some(&site.admin),
        )
        .await;
    // (the attempt on the missing comment is recorded too, with `count: 0`)
    assert_eq!(logs["items"].as_array().map(Vec::len), Some(2), "{}", logs);
}

#[tokio::test]
async fn test_public_errors_and_spam_protection() {
    let site = site(|config| {
        config.comments.moderation.max_per_ip_per_hour = 2;
    })
    .await;

    // Drafts and types without comments are not found
    let (status, _) = site
        .call(
            "POST",
            "/api/v1/contents/article/draft/comments",
            Some(json!({ "name": "A", "email": "a@example.com", "body": "x" })),
            None,
        )
        .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (status, _) = site
        .call("GET", "/api/v1/contents/page/hello/comments", None, None)
        .await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    let (status, body) = site
        .post_comment(json!({ "name": "A", "email": "nope", "body": "x" }))
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["error"], "A valid email address is required");

    // The honeypot looks accepted but stores nothing
    let (status, body) = site
        .post_comment(json!({
            "name": "Bot", "email": "bot@example.com", "body": "buy", "website": "http://x"
        }))
        .await;
    assert_eq!(status, StatusCode::ACCEPTED);
    assert_eq!(body["status"], "pending");

    for _ in 0..2 {
        let (status, _) = site
            .post_comment(json!({ "name": "A", "email": "a@example.com", "body": "x" }))
            .await;
        assert_eq!(status, StatusCode::ACCEPTED);
    }
    let (status, body) = site
        .post_comment(json!({ "name": "A", "email": "a@example.com", "body": "x" }))
        .await;
    assert_eq!(status, StatusCode::TOO_MANY_REQUESTS, "{}", body);

    let (_, list) = site
        .call("GET", "/api/v1/comments", None, Some(&site.editor))
        .await;
    assert_eq!(list["total"], 2);
}

#[tokio::test]
async fn test_settings_bulk_actions_and_unsubscribe() {
    let site = site(|_| {}).await;

    // Editors can read the settings, only admins can change them
    let (status, settings) = site
        .call("GET", "/api/v1/comments/settings", None, Some(&site.editor))
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(settings["policy"]["require_approval"], true);
    let (status, _) = site
        .call(
            "PUT",
            "/api/v1/comments/settings",
            Some(json!({ "require_approval": false })),
            Some(&site.editor),
        )
        .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    let (status, settings) = site
        .call(
            "PUT",
            "/api/v1/comments/settings",
            Some(json!({ "require_approval": false })),
            Some(&site.admin),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{}", settings);
    assert_eq!(settings["policy"]["require_approval"], false);
    assert_eq!(settings["defaults"]["require_approval"], true);

    // Approval is off: published right away
    let (status, body) = site
        .post_comment(json!({
            "name": "Hanako", "email": "hanako@example.com", "body": "Hello", "notify_replies": true
        }))
        .await;
    assert_eq!(status, StatusCode::CREATED, "{}", body);
    assert_eq!(body["status"], "approved");
    assert_eq!(body["comment"]["author_name"], "Hanako");
    let parent = body["comment"]["id"].as_str().unwrap().to_string();
    assert_eq!(site.mails.kinds(), Vec::<String>::new());

    let (status, body) = site
        .post_comment(json!({
            "name": "Taro", "email": "taro@example.com", "body": "Reply", "parent_id": parent
        }))
        .await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(body["comment"]["depth"], 1);
    let events = std::mem::take(&mut *site.mails.events.lock().unwrap());
    assert_eq!(events.len(), 1);
    // Without [commerce].public_url there is no link to stop the emails
    assert_eq!(events[0].data["unsubscribe_url"], "");

    // Unsubscribe page: wrong token → 404, right token → confirmation page
    let token = site
        .state
        .db
        .get_comment(&parent)
        .await
        .unwrap()
        .unwrap()
        .notify_token
        .unwrap();
    let (status, _) = site
        .page(&format!("/comments/unsubscribe?id={}&token=wrong", parent))
        .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (status, html) = site
        .page(&format!(
            "/comments/unsubscribe?id={}&token={}",
            parent, token
        ))
        .await;
    assert_eq!(status, StatusCode::OK);
    assert!(html.contains("no longer receive emails"));
    assert!(
        !site
            .state
            .db
            .get_comment(&parent)
            .await
            .unwrap()
            .unwrap()
            .notify_replies
    );

    // Bulk: trash both, then delete them
    let (_, list) = site
        .call("GET", "/api/v1/comments", None, Some(&site.editor))
        .await;
    let ids: Vec<Value> = list["comments"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["id"].clone())
        .collect();
    let (status, body) = site
        .call(
            "POST",
            "/api/v1/comments/bulk",
            Some(json!({ "ids": ids, "action": "trash" })),
            Some(&site.editor),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["count"], 2);
    let (_, html) = site.page("/articles/hello").await;
    assert!(html.contains("0 comments"));
    let (_, body) = site
        .call(
            "POST",
            "/api/v1/comments/bulk",
            Some(json!({ "ids": ids, "action": "delete" })),
            Some(&site.editor),
        )
        .await;
    assert_eq!(body["count"], 2);
}

#[tokio::test]
async fn test_deleting_content_deletes_its_comments_and_closed_comments_have_no_form() {
    let site = site(|config| {
        config.comments.moderation.require_approval = false;
        config.comments.close_after_days = 1;
    })
    .await;
    // Published on 2026-10-01: closed by now
    let (status, body) = site
        .post_comment(json!({ "name": "A", "email": "a@example.com", "body": "late" }))
        .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{}", body);
    let (_, html) = site.page("/articles/hello").await;
    assert!(html.contains("Comments are closed."));
    assert!(!html.contains("<form class=\"aegis-comment-form\""));

    // A comment from before closing is removed with its article
    site.state
        .db
        .insert_comment(&aegis_db::CommentRecord {
            id: "c1".to_string(),
            content_type: "article".to_string(),
            content_id: "a1".to_string(),
            author_name: "A".to_string(),
            author_email: "a@example.com".to_string(),
            body: "early".to_string(),
            status: "approved".to_string(),
            created_at: "2026-10-01T01:00:00Z".to_string(),
            updated_at: "2026-10-01T01:00:00Z".to_string(),
            ..Default::default()
        })
        .await
        .unwrap();
    let (status, _) = site
        .call(
            "DELETE",
            "/api/v1/contents/article/hello",
            None,
            Some(&site.editor),
        )
        .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    assert!(site.state.db.get_comment("c1").await.unwrap().is_none());
}

#[tokio::test]
async fn test_comments_disabled() {
    let site = site(|config| config.comments.enabled = false).await;
    let state = Arc::new(AppState {
        comments: None,
        ..(*site.state).clone()
    });
    let site = Site { state, ..site };
    let (status, _) = site
        .call("GET", "/api/v1/contents/article/hello/comments", None, None)
        .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (_, html) = site.page("/articles/hello").await;
    assert!(!html.contains("id=\"comments\""));
}

#[tokio::test]
async fn test_script_in_name_and_body_is_escaped_on_the_page() {
    let site = site(|config| config.comments.moderation.require_approval = false).await;
    let (status, body) = site
        .post_comment(json!({
            "name": "<script>alert('name')</script>",
            "email": "x@example.com",
            "body": "<script>alert(\"body\")</script>\n<img src=x onerror=alert(1)>"
        }))
        .await;
    assert_eq!(status, StatusCode::CREATED, "{}", body);

    let (_, html) = site.page("/articles/hello").await;
    assert!(html.contains("1 comment"), "{}", html);
    // Neither the name (text and the reply button's data-reply-name) nor the body runs
    assert!(!html.contains("<script>alert"), "{}", html);
    assert!(!html.contains("<img src=x"), "{}", html);
    assert!(
        html.contains("<strong>&lt;script&gt;alert(&#x27;name&#x27;)&lt;&#x2f;script&gt;</strong>"),
        "{}",
        html
    );
    assert!(html
        .contains("data-reply-name=\"&lt;script&gt;alert(&#x27;name&#x27;)&lt;&#x2f;script&gt;\""));
    assert!(html.contains(
        "<p>&lt;script&gt;alert(&quot;body&quot;)&lt;/script&gt;<br>&lt;img src=x onerror=alert(1)&gt;</p>"
    ));
}
