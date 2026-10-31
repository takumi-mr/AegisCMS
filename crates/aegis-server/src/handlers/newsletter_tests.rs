//! ニュースレターの公開ページ・申し込み・確認と解除・管理 API・配信の結合テスト（実際のルーターを通す）

use std::sync::{Arc, Mutex};

use aegis_core::{NotificationEvent, Notifier, Recipient};
use aegis_db::ContentRecord;
use aegis_runtime::PluginSupervisor;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use serde_json::{json, Value};
use tower::ServiceExt;

use crate::config::AegisConfig;
use crate::state::AppState;

#[derive(Default)]
struct RecordingNotifier(Mutex<Vec<NotificationEvent>>);

impl Notifier for RecordingNotifier {
    fn notify(&self, event: NotificationEvent) {
        self.0.lock().unwrap().push(event);
    }
}

impl RecordingNotifier {
    fn take(&self) -> Vec<NotificationEvent> {
        std::mem::take(&mut *self.0.lock().unwrap())
    }
}

struct Site {
    state: Arc<AppState>,
    mails: Arc<RecordingNotifier>,
    admin: String,
    editor: String,
}

struct Reply {
    status: StatusCode,
    headers: axum::http::HeaderMap,
    body: String,
}

impl Reply {
    fn json(&self) -> Value {
        serde_json::from_str(&self.body).unwrap_or(Value::Null)
    }

    fn location(&self) -> &str {
        self.headers
            .get("location")
            .and_then(|v| v.to_str().ok())
            .unwrap_or_default()
    }
}

async fn site(configure: impl FnOnce(&mut AegisConfig)) -> Site {
    let mut config = AegisConfig::default();
    config.audit.enabled = true;
    config.newsletter.enabled = true;
    config.commerce.public_url = "https://example.com".to_string();
    configure(&mut config);
    let state =
        crate::test_support::test_app_state(config, PluginSupervisor::new(), Vec::new()).await;
    let mails = Arc::new(RecordingNotifier::default());
    let mut state = state.with_notifier(mails.clone());
    if state.config.newsletter.enabled {
        let service = aegis_newsletter::NewsletterService::new(
            state.config.newsletter.clone(),
            state.newsletter_deps(),
        )
        .unwrap();
        state = state.with_newsletter(service);
    }
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
    let admin = session("admin1", "admin").await;
    let editor = session("editor1", "editor").await;
    Site {
        state,
        mails,
        admin,
        editor,
    }
}

impl Site {
    async fn send(
        &self,
        method: &str,
        path: &str,
        content_type: Option<&str>,
        body: String,
        token: Option<&str>,
    ) -> Reply {
        let mut request = Request::builder()
            .method(method)
            .uri(path)
            .header("user-agent", "test")
            .extension(axum::extract::ConnectInfo(std::net::SocketAddr::from((
                [203, 0, 113, 7],
                40000,
            ))));
        if let Some(content_type) = content_type {
            request = request.header("content-type", content_type);
        }
        if let Some(token) = token {
            request = request.header("authorization", format!("Bearer {}", token));
        }
        let response = crate::app::router(&self.state.config, self.state.clone())
            .oneshot(request.body(Body::from(body)).unwrap())
            .await
            .unwrap();
        let status = response.status();
        let headers = response.headers().clone();
        let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        Reply {
            status,
            headers,
            body: String::from_utf8_lossy(&bytes).to_string(),
        }
    }

    async fn get(&self, path: &str) -> Reply {
        self.send("GET", path, None, String::new(), None).await
    }

    async fn post_form(&self, path: &str, pairs: &[(&str, &str)]) -> Reply {
        let body = url::form_urlencoded::Serializer::new(String::new())
            .extend_pairs(pairs)
            .finish();
        self.send(
            "POST",
            path,
            Some("application/x-www-form-urlencoded"),
            body,
            None,
        )
        .await
    }

    async fn api(&self, method: &str, path: &str, body: Option<Value>, token: &str) -> Reply {
        self.send(
            method,
            path,
            body.as_ref().map(|_| "application/json"),
            body.map(|b| b.to_string()).unwrap_or_default(),
            Some(token),
        )
        .await
    }

    /// 申し込んで確認する。メールアドレスの購読者の（確認・解除の）トークンを返す
    async fn subscribe_and_confirm(&self, email: &str) -> String {
        let reply = self
            .post_form("/newsletter/subscribe", &[("email", email)])
            .await;
        assert_eq!(reply.status, StatusCode::SEE_OTHER, "{}", reply.body);
        let mails = self.mails.take();
        let token = token_of(&mails[0].data["confirm_url"]);
        let confirm = self
            .post_form("/newsletter/confirm", &[("token", &token)])
            .await;
        assert_eq!(confirm.status, StatusCode::OK, "{}", confirm.body);
        token
    }

    async fn publish_article(&self, slug: &str, status: &str) -> String {
        let schema = self.state.schemas.read().await["article"].clone();
        let id = format!("id-{}", slug);
        self.state
            .db
            .insert_content(
                &schema,
                &ContentRecord {
                    id: id.clone(),
                    content_type: "article".to_string(),
                    title: format!("Title of {}", slug),
                    slug: slug.to_string(),
                    status: status.to_string(),
                    blocks_json: json!([
                        { "name": "core/paragraph", "text": "The **autumn** blend has arrived." },
                        { "name": "core/paragraph", "text": "Second paragraph." }
                    ])
                    .to_string(),
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
        id
    }
}

fn token_of(url: &Value) -> String {
    url.as_str()
        .unwrap()
        .split("token=")
        .nth(1)
        .unwrap()
        .to_string()
}

#[tokio::test]
async fn test_signup_page_confirm_and_unsubscribe_flow() {
    let site = site(|config| config.server.base_path = "/cms".to_string()).await;

    let page = site.get("/newsletter").await;
    assert_eq!(page.status, StatusCode::OK);
    assert_eq!(page.headers["cache-control"], "private, no-store");
    assert!(page.body.contains("<h1"), "{}", page.body);
    assert!(page.body.contains(r#"action="/cms/newsletter/subscribe""#));
    assert!(page.body.contains(r#"name="website" tabindex="-1""#));
    assert!(page.body.contains(r#"type="email" name="email""#));

    // Signing up redirects (PRG) and sends one confirmation mail
    let sent = site
        .post_form(
            "/newsletter/subscribe",
            &[("email", " Reader@Example.com ")],
        )
        .await;
    assert_eq!(sent.status, StatusCode::SEE_OTHER);
    assert_eq!(sent.location(), "/cms/newsletter?sent=1");
    let mails = site.mails.take();
    assert_eq!(mails.len(), 1);
    assert_eq!(mails[0].kind, "newsletter.confirm");
    let confirm_url = mails[0].data["confirm_url"].as_str().unwrap().to_string();
    assert!(confirm_url.starts_with("https://example.com/newsletter/confirm?token="));
    let token = token_of(&mails[0].data["confirm_url"]);

    let thanks = site.get("/newsletter?sent=1").await;
    assert!(thanks.body.contains("We sent a confirmation email"));
    assert!(!thanks.body.contains("<form"));

    // Opening the link only shows a button: a mail scanner opening it changes nothing
    let link = site
        .get(&format!("/newsletter/confirm?token={}", token))
        .await;
    assert_eq!(link.status, StatusCode::OK);
    assert!(link.body.contains("Confirm subscription"));
    assert!(link
        .body
        .contains(&format!(r#"name="token" value="{}""#, token)));
    assert_eq!(link.headers["cache-control"], "private, no-store");
    let pending = site
        .state
        .db
        .get_newsletter_subscriber_by_token(&token)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(pending.status, "pending");
    assert_eq!(pending.email, "reader@example.com");
    assert_eq!(pending.ip.as_deref(), Some("203.0.113.7"));

    let confirmed = site
        .post_form("/newsletter/confirm", &[("token", &token)])
        .await;
    assert_eq!(confirmed.status, StatusCode::OK);
    assert!(confirmed
        .body
        .contains("Thank you. You will get new posts by email."));
    assert_eq!(
        site.state
            .db
            .get_newsletter_subscriber_by_token(&token)
            .await
            .unwrap()
            .unwrap()
            .status,
        "active"
    );

    // Unsubscribing works the same way
    let unsubscribe_link = site
        .get(&format!("/newsletter/unsubscribe?token={}", token))
        .await;
    assert!(unsubscribe_link.body.contains("Unsubscribe"));
    assert_eq!(
        site.state
            .db
            .get_newsletter_subscriber_by_token(&token)
            .await
            .unwrap()
            .unwrap()
            .status,
        "active"
    );
    let done = site
        .post_form("/newsletter/unsubscribe", &[("token", &token)])
        .await;
    assert_eq!(done.status, StatusCode::OK);
    assert!(done.body.contains("You will no longer get the newsletter"));
    assert_eq!(
        site.state
            .db
            .get_newsletter_subscriber_by_token(&token)
            .await
            .unwrap()
            .unwrap()
            .status,
        "unsubscribed"
    );
}

#[tokio::test]
async fn test_signup_errors_bots_and_invalid_links() {
    let site = site(|config| config.newsletter.max_per_ip_per_hour = 2).await;

    // A bad address is shown back (escaped) with the reason
    let bad = site
        .post_form("/newsletter/subscribe", &[("email", "<b>nope</b>")])
        .await;
    assert_eq!(bad.status, StatusCode::BAD_REQUEST);
    assert!(bad.body.contains("Please enter a valid email address."));
    assert!(bad.body.contains("&lt;b&gt;nope&lt;/b&gt;"));
    assert!(!bad.body.contains("<b>nope</b>"));
    assert!(site.mails.take().is_empty());

    // The hidden field: looks accepted, nothing is stored or sent
    let bot = site
        .post_form(
            "/newsletter/subscribe",
            &[
                ("email", "bot@example.com"),
                ("website", "http://spam.example"),
            ],
        )
        .await;
    assert_eq!(bot.status, StatusCode::SEE_OTHER);
    assert!(site.mails.take().is_empty());
    assert!(site
        .state
        .db
        .get_newsletter_subscriber_by_email("bot@example.com")
        .await
        .unwrap()
        .is_none());

    // Too many sign-ups from one address
    for email in ["a@example.com", "b@example.com"] {
        let ok = site
            .post_form("/newsletter/subscribe", &[("email", email)])
            .await;
        assert_eq!(ok.status, StatusCode::SEE_OTHER);
    }
    let limited = site
        .post_form("/newsletter/subscribe", &[("email", "c@example.com")])
        .await;
    assert_eq!(limited.status, StatusCode::TOO_MANY_REQUESTS);
    assert!(limited.body.contains("Too many sign-ups"));

    // An unknown or malformed link
    for token in ["0123456789abcdef", "../etc/passwd", ""] {
        let reply = site
            .post_form("/newsletter/confirm", &[("token", token)])
            .await;
        assert_eq!(reply.status, StatusCode::NOT_FOUND, "{:?}", token);
        assert!(reply.body.contains("invalid or has expired"));
        let reply = site
            .post_form("/newsletter/unsubscribe", &[("token", token)])
            .await;
        assert_eq!(reply.status, StatusCode::NOT_FOUND);
    }
}

#[tokio::test]
async fn test_japanese_pages() {
    let site = site(|config| config.newsletter.locale = "ja".to_string()).await;
    let page = site.get("/newsletter").await;
    assert!(page.body.contains("ニュースレター"));
    assert!(page.body.contains("購読する"));
    let bad = site
        .post_form("/newsletter/subscribe", &[("email", "nope")])
        .await;
    assert!(bad.body.contains("正しいメールアドレス"));
}

#[tokio::test]
async fn test_json_api_signup() {
    let site = site(|_| {}).await;
    let ok = site
        .send(
            "POST",
            "/api/v1/newsletter/subscribe",
            Some("application/json"),
            json!({ "email": "api@example.com" }).to_string(),
            None,
        )
        .await;
    assert_eq!(ok.status, StatusCode::ACCEPTED);
    assert_eq!(ok.json()["status"], "pending");
    assert_eq!(site.mails.take().len(), 1);

    // The same answer for an address that is already waiting
    let again = site
        .send(
            "POST",
            "/api/v1/newsletter/subscribe",
            Some("application/json"),
            json!({ "email": "API@example.com" }).to_string(),
            None,
        )
        .await;
    assert_eq!(again.status, StatusCode::ACCEPTED);
    assert!(site.mails.take().is_empty());

    let bad = site
        .send(
            "POST",
            "/api/v1/newsletter/subscribe",
            Some("application/json"),
            json!({ "email": "nope" }).to_string(),
            None,
        )
        .await;
    assert_eq!(bad.status, StatusCode::BAD_REQUEST);
    assert_eq!(bad.json()["code"], "invalid_email");
    assert_eq!(bad.json()["error"], "Please enter a valid email address.");
}

#[tokio::test]
async fn test_disabled_newsletter_has_no_pages() {
    let site = site(|config| config.newsletter.enabled = false).await;
    assert_eq!(site.get("/newsletter").await.status, StatusCode::NOT_FOUND);
    assert_eq!(
        site.post_form("/newsletter/subscribe", &[("email", "a@example.com")])
            .await
            .status,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        site.get("/newsletter/confirm?token=abc").await.status,
        StatusCode::NOT_FOUND
    );
    let api = site
        .api("GET", "/api/v1/newsletter/subscribers", None, &site.admin)
        .await;
    assert_eq!(api.status, StatusCode::NOT_FOUND);
    assert_eq!(api.json()["code"], "newsletter_disabled");
}

/// 手元で動く siteverify（トークン `good` だけ通す）
async fn mock_siteverify() -> String {
    let app = axum::Router::new().route(
        "/siteverify",
        axum::routing::post(
            |axum::Form(form): axum::Form<std::collections::HashMap<String, String>>| async move {
                match form.get("response").map(String::as_str) {
                    Some("good") => (StatusCode::OK, r#"{"success": true}"#),
                    _ => (StatusCode::OK, r#"{"success": false}"#),
                }
            },
        ),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}/siteverify", listener.local_addr().unwrap());
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    url
}

#[tokio::test]
async fn test_captcha_on_the_signup_form() {
    let verify_url = mock_siteverify().await;
    let site = site(|config| {
        config.newsletter.captcha = Some(aegis_captcha::CaptchaConfig {
            provider: aegis_captcha::Provider::Turnstile,
            site_key: "site-key".to_string(),
            secret: "secret-key".to_string(),
            fail_open: false,
            timeout_secs: 5,
            verify_url,
            script_url: String::new(),
            widget_class: String::new(),
            response_field: String::new(),
        });
    })
    .await;

    let page = site.get("/newsletter").await;
    assert!(page
        .body
        .contains(r#"class="cf-turnstile" data-sitekey="site-key""#));
    let csp = page.headers["content-security-policy"].to_str().unwrap();
    assert!(csp.contains("https://challenges.cloudflare.com"), "{}", csp);

    let missing = site
        .post_form("/newsletter/subscribe", &[("email", "a@example.com")])
        .await;
    assert_eq!(missing.status, StatusCode::BAD_REQUEST);
    assert!(missing.body.contains("The verification failed."));
    assert!(site.mails.take().is_empty());

    let ok = site
        .post_form(
            "/newsletter/subscribe",
            &[
                ("email", "a@example.com"),
                ("cf-turnstile-response", "good"),
            ],
        )
        .await;
    assert_eq!(ok.status, StatusCode::SEE_OTHER);
    assert_eq!(site.mails.take().len(), 1);

    // The JSON API needs the token too
    let api = site
        .send(
            "POST",
            "/api/v1/newsletter/subscribe",
            Some("application/json"),
            json!({ "email": "b@example.com" }).to_string(),
            None,
        )
        .await;
    assert_eq!(api.status, StatusCode::BAD_REQUEST);
    assert_eq!(api.json()["code"], "captcha_failed");
}

#[tokio::test]
async fn test_admin_lists_filters_exports_and_deletes_subscribers() {
    let site = site(|config| config.newsletter.max_per_ip_per_hour = 0).await;
    let denied = site
        .api("GET", "/api/v1/newsletter/subscribers", None, &site.editor)
        .await;
    assert_eq!(denied.status, StatusCode::FORBIDDEN);
    assert_eq!(
        site.get("/api/v1/newsletter/subscribers").await.status,
        StatusCode::UNAUTHORIZED
    );

    site.subscribe_and_confirm("a@example.com").await;
    let b = site.subscribe_and_confirm("b@example.com").await;
    site.post_form("/newsletter/subscribe", &[("email", "pending@example.com")])
        .await;
    site.post_form("/newsletter/unsubscribe", &[("token", &b)])
        .await;

    let all = site
        .api("GET", "/api/v1/newsletter/subscribers", None, &site.admin)
        .await
        .json();
    assert_eq!(all["total"], 3);
    assert_eq!(
        all["counts"],
        json!({ "pending": 1, "active": 1, "unsubscribed": 1 })
    );
    // The tokens never leave the server
    let text = all.to_string();
    assert!(!text.contains("token"), "{}", text);
    assert!(all["subscribers"][0]["ip"].is_string());

    let active = site
        .api(
            "GET",
            "/api/v1/newsletter/subscribers?status=active&q=A@EX",
            None,
            &site.admin,
        )
        .await
        .json();
    assert_eq!(active["total"], 1);
    assert_eq!(active["subscribers"][0]["email"], "a@example.com");
    let bad = site
        .api(
            "GET",
            "/api/v1/newsletter/subscribers?status=nope",
            None,
            &site.admin,
        )
        .await;
    assert_eq!(bad.status, StatusCode::BAD_REQUEST);
    assert_eq!(bad.json()["code"], "invalid_status");

    // CSV of the people who get mail
    let csv = site
        .api(
            "GET",
            "/api/v1/newsletter/subscribers/export",
            None,
            &site.admin,
        )
        .await;
    assert_eq!(csv.status, StatusCode::OK);
    assert_eq!(csv.headers["content-type"], "text/csv; charset=utf-8");
    assert!(csv.headers["content-disposition"]
        .to_str()
        .unwrap()
        .starts_with("attachment;"));
    assert_eq!(csv.body, "email\r\na@example.com\r\n");

    // Delete
    let ids: Vec<String> = all["subscribers"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|s| s["status"] != "active")
        .map(|s| s["id"].as_str().unwrap().to_string())
        .collect();
    let deleted = site
        .api(
            "POST",
            "/api/v1/newsletter/subscribers/delete",
            Some(json!({ "ids": ids })),
            &site.admin,
        )
        .await;
    assert_eq!(deleted.status, StatusCode::OK);
    assert_eq!(deleted.json()["count"], 2);
    let left = site
        .api("GET", "/api/v1/newsletter/subscribers", None, &site.admin)
        .await
        .json();
    assert_eq!(left["total"], 1);
    let none = site
        .api(
            "POST",
            "/api/v1/newsletter/subscribers/delete",
            Some(json!({ "ids": [] })),
            &site.admin,
        )
        .await;
    assert_eq!(none.status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_admin_sends_published_content_to_subscribers() {
    let site = site(|config| config.newsletter.max_per_ip_per_hour = 0).await;
    site.publish_article("autumn-blend", "published").await;
    site.publish_article("draft-post", "draft").await;
    let token_a = site.subscribe_and_confirm("a@example.com").await;
    site.subscribe_and_confirm("b@example.com").await;
    let send = |body: Value| {
        let site = &site;
        async move {
            site.api("POST", "/api/v1/newsletter/issues", Some(body), &site.admin)
                .await
        }
    };
    let denied = site
        .api(
            "POST",
            "/api/v1/newsletter/issues",
            Some(json!({ "content_type": "article", "slug_or_id": "autumn-blend" })),
            &site.editor,
        )
        .await;
    assert_eq!(denied.status, StatusCode::FORBIDDEN);

    // Only published content can be sent
    let draft = send(json!({ "content_type": "article", "slug_or_id": "draft-post" })).await;
    assert_eq!(draft.status, StatusCode::BAD_REQUEST);
    assert_eq!(draft.json()["code"], "content_not_published");
    let missing = send(json!({ "content_type": "nope", "slug_or_id": "x" })).await;
    assert_eq!(missing.status, StatusCode::NOT_FOUND);

    // A test mail goes to one address only: no history, a sample unsubscribe link
    let test = send(json!({
        "content_type": "article", "slug_or_id": "autumn-blend", "test_to": "Editor@Example.com"
    }))
    .await;
    assert_eq!(test.status, StatusCode::ACCEPTED);
    assert_eq!(test.json()["status"], "test_sent");
    let mails = site.mails.take();
    assert_eq!(mails.len(), 1);
    assert!(
        matches!(&mails[0].recipients[0], Recipient::Person { email, .. } if email == "editor@example.com")
    );
    assert_eq!(mails[0].data["title"], "Title of autumn-blend");
    assert_eq!(mails[0].data["summary"], "The autumn blend has arrived.");
    assert_eq!(mails[0].data["url"], "https://example.com/autumn-blend");
    assert!(mails[0].data["unsubscribe_url"]
        .as_str()
        .unwrap()
        .ends_with("token=test"));
    let history = site
        .api("GET", "/api/v1/newsletter/issues", None, &site.admin)
        .await
        .json();
    assert_eq!(history["total"], 0);

    // The real send: one mail per subscriber, each with their own unsubscribe link
    let sent = send(json!({
        "content_type": "article", "slug_or_id": "autumn-blend", "summary": "Custom summary"
    }))
    .await;
    assert_eq!(sent.status, StatusCode::CREATED, "{}", sent.body);
    assert_eq!(sent.json()["recipients"], 2);
    assert_eq!(sent.json()["created_by"], "admin1");
    let mails = site.mails.take();
    assert_eq!(mails.len(), 2);
    assert!(mails
        .iter()
        .all(|m| m.kind == "newsletter.issue" && m.data["summary"] == "Custom summary"));
    assert!(mails.iter().any(|m| m.data["unsubscribe_url"]
        == format!(
            "https://example.com/newsletter/unsubscribe?token={}",
            token_a
        )));

    // Not twice, unless forced
    let again = send(json!({ "content_type": "article", "slug_or_id": "autumn-blend" })).await;
    assert_eq!(again.status, StatusCode::CONFLICT);
    assert_eq!(again.json()["code"], "already_sent");
    assert!(site.mails.take().is_empty());
    let forced = send(json!({
        "content_type": "article", "slug_or_id": "autumn-blend", "force": true
    }))
    .await;
    assert_eq!(forced.status, StatusCode::CREATED);
    site.mails.take();

    let history = site
        .api("GET", "/api/v1/newsletter/issues", None, &site.admin)
        .await
        .json();
    assert_eq!(history["total"], 2);
    assert_eq!(history["issues"][0]["title"], "Title of autumn-blend");

    // The audit log has the send
    let events = site
        .api(
            "GET",
            "/api/v1/audit-logs?action=newsletter.send",
            None,
            &site.admin,
        )
        .await;
    assert_eq!(events.status, StatusCode::OK, "{}", events.body);
    assert!(events.body.contains("newsletter.send"), "{}", events.body);
}

#[tokio::test]
async fn test_sending_without_subscribers_is_refused() {
    let site = site(|_| {}).await;
    site.publish_article("autumn-blend", "published").await;
    let reply = site
        .api(
            "POST",
            "/api/v1/newsletter/issues",
            Some(json!({ "content_type": "article", "slug_or_id": "autumn-blend" })),
            &site.admin,
        )
        .await;
    assert_eq!(reply.status, StatusCode::CONFLICT);
    assert_eq!(reply.json()["code"], "no_subscribers");
}

#[tokio::test]
async fn test_newsletter_is_a_reserved_name() {
    // A page or article cannot take the URL of the sign-up page as its slug for the admin path either
    let mut config = AegisConfig::default();
    config.server.admin_path = "/newsletter".to_string();
    assert!(config.server.normalized_admin_path().is_err());
}
