//! 問い合わせフォームの公開ページ・送信・受信箱の結合テスト（実際のルーターを通す）

use std::sync::{Arc, Mutex};

use aegis_core::{NotificationEvent, Notifier};
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
    fn kinds(&self) -> Vec<String> {
        std::mem::take(&mut *self.0.lock().unwrap())
            .into_iter()
            .map(|e| e.kind)
            .collect()
    }
}

struct Site {
    state: Arc<AppState>,
    mails: Arc<RecordingNotifier>,
    editor: String,
    viewer: String,
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
}

async fn site(configure: impl FnOnce(&mut AegisConfig)) -> Site {
    let mut config = AegisConfig::default();
    config.contact.enabled = true;
    config.audit.enabled = true;
    configure(&mut config);
    let state =
        crate::test_support::test_app_state(config, PluginSupervisor::new(), Vec::new()).await;
    let mails = Arc::new(RecordingNotifier::default());
    let mut state = state.with_notifier(mails.clone());
    if state.config.contact.enabled {
        let contact =
            aegis_contact::ContactService::new(state.config.contact.clone(), state.contact_deps())
                .unwrap();
        state = state.with_contact(contact);
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
    let editor = session("editor1", "editor").await;
    let viewer = session("viewer1", "viewer").await;
    Site {
        state,
        mails,
        editor,
        viewer,
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

    async fn inbox(&self) -> Value {
        self.api("GET", "/api/v1/contact/messages", None, &self.editor)
            .await
            .json()
    }
}

const VALID: &[(&str, &str)] = &[
    ("name", "山田 花子"),
    ("email", "hanako@example.com"),
    ("subject", "導入の相談"),
    ("message", "見積もりをお願いします。"),
];

#[tokio::test]
async fn test_form_page_post_redirect_and_inbox() {
    let site = site(|config| config.server.base_path = "/cms".to_string()).await;
    let page = site.get("/contact").await;
    assert_eq!(page.status, StatusCode::OK);
    assert!(page.body.contains("<h1"), "{}", page.body);
    assert!(page.body.contains(r#"action="/cms/contact/default""#));
    assert!(page.body.contains(r#"name="website" tabindex="-1""#));
    assert_eq!(site.get("/contact/default").await.status, StatusCode::OK);
    assert_eq!(
        site.get("/contact/nope").await.status,
        StatusCode::NOT_FOUND
    );

    let sent = site.post_form("/contact/default", VALID).await;
    assert_eq!(sent.status, StatusCode::SEE_OTHER);
    assert_eq!(sent.headers["location"], "/cms/contact/default?sent=1");
    assert_eq!(site.mails.kinds(), vec!["contact.received"]);

    let thanks = site.get("/contact/default?sent=1").await;
    assert!(thanks.body.contains("We have received your message"));
    assert!(!thanks.body.contains("<form"));
    assert_eq!(thanks.headers["cache-control"], "private, no-store");

    let inbox = site.inbox().await;
    assert_eq!(inbox["total"], 1);
    let message = &inbox["messages"][0];
    assert_eq!(message["name"], "山田 花子");
    assert_eq!(message["subject"], "導入の相談");
    assert_eq!(message["status"], "new");
    assert_eq!(message["ip"], "203.0.113.7");
    assert_eq!(message["fields"][3]["value"], "見積もりをお願いします。");
    assert_eq!(inbox["counts"]["new"], 1);
}

#[tokio::test]
async fn test_invalid_input_is_shown_back_escaped() {
    let site = site(|_| {}).await;
    let reply = site
        .post_form(
            "/contact/default",
            &[("name", "<b>Taro</b>"), ("email", "nope"), ("message", "")],
        )
        .await;
    assert_eq!(reply.status, StatusCode::BAD_REQUEST);
    assert_eq!(reply.headers["cache-control"], "private, no-store");
    assert!(reply.body.contains("Please check the fields below."));
    assert!(
        reply.body.contains(r#"value="&lt;b&gt;Taro&lt;/b&gt;""#),
        "{}",
        reply.body
    );
    assert!(!reply.body.contains("<b>Taro</b>"));
    assert!(reply.body.contains("must be a valid email address"));
    assert!(reply.body.contains("&quot;Message&quot; is required"));
    assert_eq!(site.inbox().await["total"], 0);
    assert!(site.mails.kinds().is_empty());
}

#[tokio::test]
async fn test_honeypot_json_api_and_rate_limit() {
    let site = site(|config| config.contact.max_per_ip_per_hour = 2).await;
    let mut bot: Vec<(&str, &str)> = VALID.to_vec();
    bot.push(("website", "http://spam.example"));
    let reply = site.post_form("/contact/default", &bot).await;
    assert_eq!(reply.status, StatusCode::SEE_OTHER);
    assert_eq!(site.inbox().await["total"], 0);

    let ok = site
        .send(
            "POST",
            "/api/v1/contact/forms/default",
            Some("application/json"),
            json!({ "name": "A", "email": "a@example.com", "message": "Hi" }).to_string(),
            None,
        )
        .await;
    assert_eq!(ok.status, StatusCode::CREATED, "{}", ok.body);
    let bad = site
        .send(
            "POST",
            "/api/v1/contact/forms/default",
            Some("application/json"),
            json!({ "name": "A" }).to_string(),
            None,
        )
        .await;
    assert_eq!(bad.status, StatusCode::BAD_REQUEST);
    assert_eq!(bad.json()["errors"][0]["field"], "email");
    assert_eq!(bad.json()["errors"][0]["code"], "required");

    let second = site.post_form("/contact/default", VALID).await;
    assert_eq!(second.status, StatusCode::SEE_OTHER);
    let third = site.post_form("/contact/default", VALID).await;
    assert_eq!(third.status, StatusCode::TOO_MANY_REQUESTS);
    assert!(third.body.contains("Too many messages"));

    let forms = site.get("/api/v1/contact/forms").await.json();
    assert_eq!(forms[0]["name"], "default");
    assert_eq!(forms[0]["path"], "/contact/default");
    assert_eq!(forms[0]["fields"][1]["type"], "email");
}

#[tokio::test]
async fn test_inbox_permissions_status_and_delete() {
    let site = site(|_| {}).await;
    site.post_form("/contact/default", VALID).await;
    site.post_form("/contact/default", VALID).await;
    let ids: Vec<String> = site.inbox().await["messages"]
        .as_array()
        .unwrap()
        .iter()
        .map(|m| m["id"].as_str().unwrap().to_string())
        .collect();

    assert_eq!(
        site.get("/api/v1/contact/messages").await.status,
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        site.api("GET", "/api/v1/contact/messages", None, &site.viewer)
            .await
            .status,
        StatusCode::FORBIDDEN
    );

    let path = format!("/api/v1/contact/messages/{}", ids[0]);
    assert_eq!(
        site.api(
            "PATCH",
            &path,
            Some(json!({ "status": "done" })),
            &site.editor
        )
        .await
        .status,
        StatusCode::NO_CONTENT
    );
    assert_eq!(
        site.api("GET", &path, None, &site.editor).await.json()["status"],
        "done"
    );
    assert_eq!(
        site.api(
            "PATCH",
            &path,
            Some(json!({ "status": "archived" })),
            &site.editor
        )
        .await
        .status,
        StatusCode::BAD_REQUEST
    );
    let bulk = site
        .api(
            "POST",
            "/api/v1/contact/messages/bulk",
            Some(json!({ "ids": ids, "action": "read" })),
            &site.editor,
        )
        .await;
    assert_eq!(bulk.json()["count"], 2);
    assert_eq!(
        site.api("DELETE", &path, None, &site.editor).await.status,
        StatusCode::NO_CONTENT
    );
    assert_eq!(
        site.api("DELETE", &path, None, &site.editor).await.status,
        StatusCode::NOT_FOUND
    );
    let inbox = site.inbox().await;
    assert_eq!(inbox["total"], 1);
    assert_eq!(inbox["counts"]["read"], 1);
}

#[tokio::test]
async fn test_contact_form_block_in_an_article() {
    let site = site(|config| config.server.base_path = "/cms".to_string()).await;
    let schema = site.state.schemas.read().await["article"].clone();
    site.state
        .db
        .insert_content(
            &schema,
            &ContentRecord {
                id: "a1".to_string(),
                content_type: "article".to_string(),
                title: "Contact us".to_string(),
                slug: "contact-us".to_string(),
                status: "published".to_string(),
                blocks_json: json!([
                    { "name": "core/paragraph", "text": "お気軽にどうぞ。" },
                    { "name": "core/contact-form", "attributes": { "form": "default" } },
                    { "name": "core/contact-form", "attributes": { "form": "missing" } }
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
    let page = site.get("/articles/contact-us").await;
    assert_eq!(page.status, StatusCode::OK);
    assert_eq!(
        page.body
            .matches(r#"<form class="aegis-contact-form""#)
            .count(),
        1,
        "{}",
        page.body
    );
    assert!(page.body.contains(r#"action="/cms/contact/default""#));
    assert!(page.body.contains("お気軽にどうぞ。"));
    assert!(!page.body.contains("contact-form>"), "{}", page.body);

    // 無効にするとブロックは何も出さず、ページも無い
    let disabled = self::site(|config| config.contact.enabled = false).await;
    assert_eq!(disabled.get("/contact").await.status, StatusCode::NOT_FOUND);
    assert_eq!(
        disabled
            .send(
                "POST",
                "/api/v1/contact/forms/default",
                Some("application/json"),
                "{}".to_string(),
                None
            )
            .await
            .status,
        StatusCode::NOT_FOUND
    );
}

#[tokio::test]
async fn test_japanese_messages_and_subject_field() {
    let site = site(|config| {
        config.contact.locale = "ja".to_string();
        config.contact.forms[0].subject_field = "topic".to_string();
        config.contact.forms[0].fields = vec![
            aegis_contact::FieldConfig {
                name: "email".into(),
                label: "メールアドレス".into(),
                kind: aegis_contact::FieldKind::Email,
                required: true,
                placeholder: String::new(),
                options: vec![],
                max_length: None,
            },
            aegis_contact::FieldConfig {
                name: "topic".into(),
                label: "ご用件".into(),
                kind: aegis_contact::FieldKind::Select,
                required: true,
                placeholder: String::new(),
                options: vec!["導入のご相談".into(), "その他".into()],
                max_length: None,
            },
        ];
    })
    .await;
    let page = site.get("/contact").await;
    assert!(page.body.contains("お問い合わせ</h1>"), "{}", page.body);
    assert!(page.body.contains(">送信する</button>"));

    let reply = site.post_form("/contact/default", &[("email", "x")]).await;
    assert_eq!(reply.status, StatusCode::BAD_REQUEST);
    assert!(reply.body.contains("入力内容を確認してください。"));
    assert!(reply.body.contains("「ご用件」を入力してください"));

    site.post_form(
        "/contact/default",
        &[("email", "a@example.com"), ("topic", "導入のご相談")],
    )
    .await;
    let inbox = site.inbox().await;
    assert_eq!(inbox["messages"][0]["subject"], "導入のご相談");
    let thanks = site.get("/contact/default?sent=1").await;
    assert!(thanks.body.contains("お問い合わせありがとうございます。"));
}

fn multipart(parts: &[(&str, Option<(&str, &str)>, &[u8])]) -> (String, Vec<u8>) {
    let boundary = "----aegis-test-boundary";
    let mut body = Vec::new();
    for (name, file, data) in parts {
        body.extend_from_slice(format!("--{}\r\n", boundary).as_bytes());
        match file {
            Some((filename, content_type)) => body.extend_from_slice(
                format!(
                    "Content-Disposition: form-data; name=\"{}\"; filename=\"{}\"\r\nContent-Type: {}\r\n\r\n",
                    name, filename, content_type
                )
                .as_bytes(),
            ),
            None => body.extend_from_slice(
                format!("Content-Disposition: form-data; name=\"{}\"\r\n\r\n", name).as_bytes(),
            ),
        }
        body.extend_from_slice(data);
        body.extend_from_slice(b"\r\n");
    }
    body.extend_from_slice(format!("--{}--\r\n", boundary).as_bytes());
    (format!("multipart/form-data; boundary={}", boundary), body)
}

#[tokio::test]
async fn test_attachment_upload_download_and_search() {
    let site = site(|config| {
        config.storage.local.upload_dir = "target/test_uploads".into();
        let mut fields = aegis_contact::config::default_fields("en");
        fields.push(aegis_contact::FieldConfig {
            name: "attachment".into(),
            label: "Attachment".into(),
            kind: aegis_contact::FieldKind::File,
            required: false,
            placeholder: String::new(),
            options: vec![],
            max_length: None,
        });
        config.contact.forms[0].fields = fields;
    })
    .await;
    let page = site.get("/contact").await;
    assert!(page.body.contains(r#"enctype="multipart/form-data""#));
    assert!(page.body.contains(r#"type="file""#));
    assert!(page.body.contains(r#"accept=".jpg,"#), "{}", page.body);

    let mut parts: Vec<(&str, Option<(&str, &str)>, &[u8])> = VALID
        .iter()
        .map(|(k, v)| (*k, None, v.as_bytes()))
        .collect();
    parts.push((
        "attachment",
        Some(("estimate.pdf", "application/pdf")),
        b"%PDF-1.4 test",
    ));
    let (content_type, body) = multipart(&parts);
    let request = Request::builder()
        .method("POST")
        .uri("/contact/default")
        .header("content-type", content_type)
        .extension(axum::extract::ConnectInfo(std::net::SocketAddr::from((
            [203, 0, 113, 8],
            40000,
        ))))
        .body(Body::from(body))
        .unwrap();
    let sent = crate::app::router(&site.state.config, site.state.clone())
        .oneshot(request)
        .await
        .unwrap();
    assert_eq!(sent.status(), StatusCode::SEE_OTHER);

    let inbox = site.inbox().await;
    let message = &inbox["messages"][0];
    let id = message["id"].as_str().unwrap();
    let file = message["fields"]
        .as_array()
        .unwrap()
        .iter()
        .find(|f| f["name"] == "attachment")
        .unwrap();
    assert_eq!(file["value"], "estimate.pdf");
    assert_eq!(file["file"]["size"], 13);

    let search = site
        .api(
            "GET",
            "/api/v1/contact/messages?q=ESTIMATE",
            None,
            &site.editor,
        )
        .await
        .json();
    assert_eq!(search["total"], 1);
    let none = site
        .api(
            "GET",
            "/api/v1/contact/messages?q=nothing",
            None,
            &site.editor,
        )
        .await
        .json();
    assert_eq!(none["total"], 0);

    let url = format!("/api/v1/contact/messages/{}/attachments/attachment", id);
    let download = site.api("GET", &url, None, &site.editor).await;
    assert_eq!(download.status, StatusCode::OK);
    assert_eq!(download.body, "%PDF-1.4 test");
    assert_eq!(download.headers["content-type"], "application/pdf");
    assert!(download.headers["content-disposition"]
        .to_str()
        .unwrap()
        .starts_with("attachment;"));
    assert_eq!(download.headers["x-content-type-options"], "nosniff");
    assert_eq!(
        site.api("GET", &url, None, &site.viewer).await.status,
        StatusCode::FORBIDDEN
    );
    assert_eq!(site.get(&url).await.status, StatusCode::UNAUTHORIZED);

    // 保存先は公開されない（大文字・パーセントエンコードでも）
    let key = file["file"]["key"].as_str().unwrap();
    for path in [
        format!("/uploads/{}", key),
        format!("/uploads/{}", key.replace("contact/", "Contact/")),
        format!("/uploads/{}", key.replace("contact/", "%63ontact/")),
    ] {
        assert_eq!(
            site.get(&path).await.status,
            StatusCode::NOT_FOUND,
            "{}",
            path
        );
    }

    site.api(
        "DELETE",
        &format!("/api/v1/contact/messages/{}", id),
        None,
        &site.editor,
    )
    .await;
    assert!(!site.state.storage.exists(key).await.unwrap());
}

#[tokio::test]
async fn test_attachment_rejected_by_type_and_too_large_body() {
    let site = site(|config| {
        config.contact.max_attachment_mb = 1;
        let mut fields = aegis_contact::config::default_fields("en");
        fields.push(aegis_contact::FieldConfig {
            name: "attachment".into(),
            label: "Attachment".into(),
            kind: aegis_contact::FieldKind::File,
            required: false,
            placeholder: String::new(),
            options: vec![],
            max_length: None,
        });
        config.contact.forms[0].fields = fields;
    })
    .await;
    let post = |parts: Vec<(&'static str, Option<(&'static str, &'static str)>, Vec<u8>)>| {
        let site = &site;
        async move {
            let borrowed: Vec<(&str, Option<(&str, &str)>, &[u8])> = parts
                .iter()
                .map(|(n, f, d)| (*n, *f, d.as_slice()))
                .collect();
            let (content_type, body) = multipart(&borrowed);
            let request = Request::builder()
                .method("POST")
                .uri("/contact/default")
                .header("content-type", content_type)
                .extension(axum::extract::ConnectInfo(std::net::SocketAddr::from((
                    [203, 0, 113, 9],
                    40000,
                ))))
                .body(Body::from(body))
                .unwrap();
            crate::app::router(&site.state.config, site.state.clone())
                .oneshot(request)
                .await
                .unwrap()
        }
    };
    let base = || -> Vec<(&'static str, Option<(&'static str, &'static str)>, Vec<u8>)> {
        VALID
            .iter()
            .map(|(k, v)| (*k, None, v.as_bytes().to_vec()))
            .collect()
    };

    let mut exe = base();
    exe.push((
        "attachment",
        Some(("run.exe", "application/octet-stream")),
        b"MZ".to_vec(),
    ));
    let reply = post(exe).await;
    assert_eq!(reply.status(), StatusCode::BAD_REQUEST);
    let body = axum::body::to_bytes(reply.into_body(), usize::MAX)
        .await
        .unwrap();
    assert!(String::from_utf8_lossy(&body).contains("accepts these file types only"));

    let mut huge = base();
    huge.push((
        "attachment",
        Some(("big.png", "image/png")),
        vec![0u8; 3 * 1024 * 1024],
    ));
    let reply = post(huge).await;
    assert!(
        matches!(
            reply.status(),
            StatusCode::BAD_REQUEST | StatusCode::PAYLOAD_TOO_LARGE
        ),
        "{}",
        reply.status()
    );
    assert_eq!(site.inbox().await["total"], 0);
}

/// 手元で動く siteverify。トークン `good` だけ通し、`down` は 500 を返す
async fn mock_siteverify() -> String {
    let app = axum::Router::new().route(
        "/siteverify",
        axum::routing::post(
            |axum::Form(form): axum::Form<std::collections::HashMap<String, String>>| async move {
                match form.get("response").map(String::as_str) {
                    Some("good") => (StatusCode::OK, r#"{"success": true}"#),
                    Some("down") => (StatusCode::INTERNAL_SERVER_ERROR, "down"),
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

fn captcha(verify_url: String) -> aegis_contact::config::ContactConfig {
    aegis_contact::config::ContactConfig {
        enabled: true,
        captcha: Some(aegis_captcha::CaptchaConfig {
            provider: aegis_captcha::Provider::Turnstile,
            site_key: "site-key".to_string(),
            secret: "secret-key".to_string(),
            fail_open: false,
            timeout_secs: 5,
            verify_url,
            script_url: String::new(),
            widget_class: String::new(),
            response_field: String::new(),
        }),
        ..Default::default()
    }
}

#[tokio::test]
async fn test_captcha_widget_csp_and_html_form_verification() {
    let url = mock_siteverify().await;
    let site = site(|config| config.contact = captcha(url)).await;

    // ページのフォームと、ブロック・自前のテンプレート用の値にウィジェットが入り、CSP が通す
    let page = site.get("/contact").await;
    assert_eq!(page.status, StatusCode::OK);
    assert!(page
        .body
        .contains(r#"class="cf-turnstile" data-sitekey="site-key""#));
    let csp = page.headers["content-security-policy"].to_str().unwrap();
    assert!(
        csp.contains(
            "script-src 'self' 'unsafe-inline' 'unsafe-eval' https://challenges.cloudflare.com"
        ),
        "{}",
        csp
    );
    assert!(
        csp.contains("frame-src 'self' https://challenges.cloudflare.com"),
        "{}",
        csp
    );

    // トークンが無い・違う → 入力を残したまま 400
    for pairs in [
        VALID.to_vec(),
        [VALID, &[("cf-turnstile-response", "wrong")]].concat(),
    ] {
        let reply = site.post_form("/contact/default", &pairs).await;
        assert_eq!(reply.status, StatusCode::BAD_REQUEST);
        assert!(
            reply.body.contains("The verification failed."),
            "{}",
            reply.body
        );
        assert!(reply.body.contains("hanako@example.com"));
        assert_eq!(reply.headers["cache-control"], "private, no-store");
    }
    assert_eq!(site.inbox().await["total"], 0);

    // 検証サービスが落ちている → 503（fail-closed）
    let down = site
        .post_form(
            "/contact/default",
            &[VALID, &[("cf-turnstile-response", "down")]].concat(),
        )
        .await;
    assert_eq!(down.status, StatusCode::SERVICE_UNAVAILABLE);
    assert!(down.body.contains("The verification is unavailable."));

    let ok = site
        .post_form(
            "/contact/default",
            &[VALID, &[("cf-turnstile-response", "good")]].concat(),
        )
        .await;
    assert_eq!(ok.status, StatusCode::SEE_OTHER);
    assert_eq!(site.inbox().await["total"], 1);
}

#[tokio::test]
async fn test_captcha_json_api_needs_a_token_too() {
    let url = mock_siteverify().await;
    let site = site(|config| config.contact = captcha(url)).await;
    let body = |token: Option<&str>| {
        let mut body = json!({
            "name": "Hanako", "email": "hanako@example.com", "message": "Hello"
        });
        if let Some(token) = token {
            body["cf-turnstile-response"] = json!(token);
        }
        body
    };
    let path = "/api/v1/contact/forms/default";
    let missing = site
        .send(
            "POST",
            path,
            Some("application/json"),
            body(None).to_string(),
            None,
        )
        .await;
    assert_eq!(missing.status, StatusCode::BAD_REQUEST);
    assert_eq!(missing.json()["code"], "captcha_failed");
    assert_eq!(missing.json()["error"], "Captcha verification failed");

    let down = site
        .send(
            "POST",
            path,
            Some("application/json"),
            body(Some("down")).to_string(),
            None,
        )
        .await;
    assert_eq!(down.status, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(down.json()["code"], "captcha_unavailable");

    let ok = site
        .send(
            "POST",
            path,
            Some("application/json"),
            body(Some("good")).to_string(),
            None,
        )
        .await;
    assert_eq!(ok.status, StatusCode::CREATED);
}

#[tokio::test]
async fn test_no_captcha_means_no_widget_and_default_csp() {
    let site = site(|_| {}).await;
    let page = site.get("/contact").await;
    assert!(!page.body.contains("cf-turnstile"));
    assert!(!page.headers["content-security-policy"]
        .to_str()
        .unwrap()
        .contains("challenges.cloudflare.com"));
}
