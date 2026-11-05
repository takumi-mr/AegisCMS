//! 会員のパスワードの再設定（API とページ）と、セッションの失効の結合テスト（ルーター経由）

use std::sync::Arc;

use aegis_core::Recipient;
use aegis_runtime::PluginSupervisor;
use aegis_schema::SchemaDefinition;
use axum::body::Body;
use axum::http::{HeaderMap, Request, StatusCode};
use serde_json::{json, Value};
use tower::ServiceExt;

use crate::app::router;
use crate::config::AegisConfig;
use crate::state::AppState;
use crate::test_support::{test_app_state, RecordingNotifier};

const PUBLIC_URL: &str = "https://shop.example.com";

fn product_schema() -> SchemaDefinition {
    serde_json::from_value(json!({
        "content_type": "product",
        "label": "Products",
        "fields": [{ "name": "price", "type": "integer", "required": true }]
    }))
    .unwrap()
}

struct Site {
    app: axum::Router,
    mails: Arc<RecordingNotifier>,
    admin_token: String,
}

struct Reply {
    status: StatusCode,
    headers: HeaderMap,
    body: String,
}

impl Reply {
    fn json(&self) -> Value {
        serde_json::from_str(&self.body).unwrap_or(Value::Null)
    }

    fn set_cookie_pair(&self) -> String {
        self.headers
            .get("set-cookie")
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.split(';').next())
            .unwrap_or_default()
            .to_string()
    }
}

/// `with_mail`: メールを送れる（`[notifications]` が有効）状態にする
async fn site_with(with_mail: bool, configure: impl FnOnce(&mut AegisConfig)) -> Site {
    let mut config = AegisConfig::default();
    config.commerce.enabled = true;
    config.members.enabled = true;
    config.commerce.product_type = "product".to_string();
    config.members.public_url = PUBLIC_URL.to_string();
    configure(&mut config);

    let mails = Arc::new(RecordingNotifier::default());
    let state = test_app_state(
        config.clone(),
        PluginSupervisor::new(),
        vec![product_schema()],
    )
    .await;
    let commerce =
        aegis_commerce::CommerceService::new(config.commerce.clone(), state.commerce_deps())
            .unwrap();
    let members = aegis_commerce::MemberService::from_config(
        &config.members,
        state.db.clone(),
        state.cache.clone(),
        state.audit.clone(),
    );
    let mut state = state.with_commerce(commerce);
    if with_mail {
        state = state.with_notifier(mails.clone());
    }
    let state: Arc<AppState> = Arc::new(match members {
        Some(members) => state.with_members(members),
        None => state,
    });
    let admin_token = state
        .sessions
        .create_session("admin", "admin", Default::default(), None)
        .await
        .unwrap()
        .id;
    Site {
        app: router(&config, state),
        mails,
        admin_token,
    }
}

impl Site {
    async fn send(&self, request: Request<Body>) -> Reply {
        let response = self.app.clone().oneshot(request).await.unwrap();
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

    async fn get(&self, path: &str, cookie: Option<&str>) -> Reply {
        let mut request = Request::get(path);
        if let Some(cookie) = cookie {
            request = request.header("cookie", cookie);
        }
        self.send(request.body(Body::empty()).unwrap()).await
    }

    async fn call(&self, method: &str, path: &str, body: Value, extra: &[(&str, &str)]) -> Reply {
        let mut request = Request::builder()
            .method(method)
            .uri(path)
            .header("content-type", "application/json");
        for (name, value) in extra {
            request = request.header(*name, *value);
        }
        self.send(request.body(Body::from(body.to_string())).unwrap())
            .await
    }

    async fn post(&self, path: &str, body: Value) -> Reply {
        self.call("POST", path, body, &[]).await
    }

    /// 登録してログインし、Cookie（`名前=値`）を返す
    async fn sign_up(&self, email: &str, password: &str) -> String {
        let registered = self
            .post(
                "/api/v1/members/register",
                json!({ "email": email, "password": password, "name": "Hanako" }),
            )
            .await;
        assert_eq!(
            registered.status,
            StatusCode::CREATED,
            "{}",
            registered.body
        );
        self.log_in(email, password).await
    }

    async fn log_in(&self, email: &str, password: &str) -> String {
        let login = self
            .post(
                "/api/v1/members/login",
                json!({ "email": email, "password": password }),
            )
            .await;
        assert_eq!(login.status, StatusCode::OK, "{}", login.body);
        login.set_cookie_pair()
    }

    /// メールに載ったトークン（直近の `member.password_reset`）
    fn reset_token(&self) -> String {
        let events = self.mails.take();
        let event = events
            .iter()
            .rev()
            .find(|e| e.kind == "member.password_reset")
            .expect("a reset email was sent");
        let url = event.data["reset_url"].as_str().unwrap();
        assert!(
            url.starts_with(&format!("{PUBLIC_URL}/account/reset?token=")),
            "the link comes from [members].public_url: {url}"
        );
        url.split("token=").nth(1).unwrap().to_string()
    }
}

#[tokio::test]
async fn test_reset_is_off_without_mail_or_a_public_url() {
    for site in [
        site_with(false, |_| {}).await,
        site_with(true, |config| config.members.public_url = String::new()).await,
        site_with(true, |config| config.members.password_reset = false).await,
    ] {
        let request = site
            .post(
                "/api/v1/members/password-reset",
                json!({ "email": "a@example.com" }),
            )
            .await;
        assert_eq!(request.status, StatusCode::NOT_FOUND);
        for path in ["/account/forgot", "/account/reset?token=x"] {
            assert_eq!(
                site.get(path, None).await.status,
                StatusCode::NOT_FOUND,
                "{path}"
            );
        }
        // ログインの画面は「忘れたとき」のリンクを出さない
        let login = site.get("/account/login", None).await;
        assert!(
            login.body.contains("data-forgot-url=\"\""),
            "{}",
            login.body
        );
    }
}

#[tokio::test]
async fn test_reset_pages_exist_and_are_never_cached() {
    let site = site_with(true, |_| {}).await;
    let login = site.get("/account/login", None).await;
    assert!(
        login
            .body
            .contains("data-forgot-url=\"&#x2f;account&#x2f;forgot\""),
        "the login page links to the form"
    );
    let forgot = site.get("/account/forgot", None).await;
    assert_eq!(forgot.status, StatusCode::OK);
    assert!(forgot.body.contains("data-view=\"forgot\""));
    let reset = site.get("/account/reset?token=abc", None).await;
    assert_eq!(reset.status, StatusCode::OK);
    assert!(reset.body.contains("data-view=\"reset\""));
    // 共有のキャッシュに載せない（ログインの状態で変わる）
    assert_eq!(
        reset
            .headers
            .get("cache-control")
            .unwrap()
            .to_str()
            .unwrap(),
        "private, no-store"
    );
}

#[tokio::test]
async fn test_reset_by_email_ends_every_session() {
    let site = site_with(true, |_| {}).await;
    let cookie = site.sign_up("hanako@example.com", "password123").await;
    assert_eq!(
        site.get("/api/v1/members/me", Some(&cookie)).await.status,
        StatusCode::OK
    );

    // 登録のないアドレスも同じ応答で、メールは送らない
    let unknown = site
        .post(
            "/api/v1/members/password-reset",
            json!({ "email": "nobody@example.com" }),
        )
        .await;
    assert_eq!(unknown.status, StatusCode::ACCEPTED);
    assert!(site.mails.take().is_empty());

    let known = site
        .post(
            "/api/v1/members/password-reset",
            json!({ "email": "hanako@example.com" }),
        )
        .await;
    assert_eq!(known.status, StatusCode::ACCEPTED);
    assert_eq!(known.json()["status"], "pending_reset");
    assert_eq!(
        known.json(),
        unknown.json(),
        "the answer does not say which exists"
    );

    let token = site.reset_token();
    // 短いパスワードは拒否される
    let short = site
        .post(
            "/api/v1/members/password-reset/complete",
            json!({ "token": token, "password": "short" }),
        )
        .await;
    assert_eq!(short.status, StatusCode::BAD_REQUEST);

    let done = site
        .post(
            "/api/v1/members/password-reset/complete",
            json!({ "token": token, "password": "a-brand-new-password" }),
        )
        .await;
    assert_eq!(done.status, StatusCode::OK, "{}", done.body);

    // 以前のログインは終わっている。古いパスワードでは入れず、新しいパスワードで入れる
    assert_eq!(
        site.get("/api/v1/members/me", Some(&cookie)).await.status,
        StatusCode::UNAUTHORIZED
    );
    let old = site
        .post(
            "/api/v1/members/login",
            json!({ "email": "hanako@example.com", "password": "password123" }),
        )
        .await;
    assert_eq!(old.status, StatusCode::UNAUTHORIZED);
    let fresh = site
        .log_in("hanako@example.com", "a-brand-new-password")
        .await;
    assert_eq!(
        site.get("/api/v1/members/me", Some(&fresh)).await.status,
        StatusCode::OK
    );

    // リンクは 1 回だけ使える
    let again = site
        .post(
            "/api/v1/members/password-reset/complete",
            json!({ "token": token, "password": "yet-another-password" }),
        )
        .await;
    assert_eq!(again.status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_reset_request_validates_and_is_limited_per_ip() {
    let site = site_with(true, |config| config.audit.trust_forwarded_for = true).await;
    let bad = site
        .post(
            "/api/v1/members/password-reset",
            json!({ "email": "not an email" }),
        )
        .await;
    assert_eq!(bad.status, StatusCode::BAD_REQUEST);

    let from = |ip: &'static str, email: String| {
        let site = &site;
        async move {
            site.call(
                "POST",
                "/api/v1/members/password-reset",
                json!({ "email": email }),
                &[("x-forwarded-for", ip)],
            )
            .await
            .status
        }
    };
    for n in 0..5 {
        assert_eq!(
            from("203.0.113.5", format!("user{n}@example.com")).await,
            StatusCode::ACCEPTED
        );
    }
    assert_eq!(
        from("203.0.113.5", "user9@example.com".to_string()).await,
        StatusCode::TOO_MANY_REQUESTS
    );
    assert_eq!(
        from("203.0.113.6", "user9@example.com".to_string()).await,
        StatusCode::ACCEPTED
    );
}

#[tokio::test]
async fn test_changing_the_password_keeps_this_browser_and_ends_the_others() {
    let site = site_with(true, |_| {}).await;
    let this_browser = site.sign_up("hanako@example.com", "password123").await;
    let other_browser = site.log_in("hanako@example.com", "password123").await;

    let changed = site
        .call(
            "PUT",
            "/api/v1/members/me",
            json!({ "current_password": "password123", "new_password": "changed-password-1" }),
            &[("cookie", &this_browser)],
        )
        .await;
    assert_eq!(changed.status, StatusCode::OK, "{}", changed.body);
    // 変えた本人のブラウザには、新しい Cookie が渡る
    let fresh = changed.set_cookie_pair();
    assert!(fresh.starts_with("cms_member=mses_"), "{fresh}");
    assert_ne!(fresh, this_browser);

    assert_eq!(
        site.get("/api/v1/members/me", Some(&fresh)).await.status,
        StatusCode::OK
    );
    for old in [&this_browser, &other_browser] {
        assert_eq!(
            site.get("/api/v1/members/me", Some(old)).await.status,
            StatusCode::UNAUTHORIZED,
            "an old session is over"
        );
    }
    // 名前だけの更新では、セッションはそのまま
    let renamed = site
        .call(
            "PUT",
            "/api/v1/members/me",
            json!({ "name": "Hanako Renamed" }),
            &[("cookie", &fresh)],
        )
        .await;
    assert_eq!(renamed.status, StatusCode::OK);
    assert!(renamed.headers.get("set-cookie").is_none());
}

#[tokio::test]
async fn test_suspending_a_member_ends_the_session_at_once() {
    let site = site_with(true, |_| {}).await;
    let cookie = site.sign_up("hanako@example.com", "password123").await;
    let me = site.get("/api/v1/members/me", Some(&cookie)).await;
    let id = me.json()["id"].as_str().unwrap().to_string();

    let suspended = site
        .call(
            "PATCH",
            &format!("/api/v1/admin/members/{id}"),
            json!({ "status": "suspended" }),
            &[("authorization", &format!("Bearer {}", site.admin_token))],
        )
        .await;
    assert!(suspended.status.is_success(), "{}", suspended.body);

    assert_eq!(
        site.get("/api/v1/members/me", Some(&cookie)).await.status,
        StatusCode::UNAUTHORIZED
    );
    // 停止中は、再設定のメールも送らない
    let request = site
        .post(
            "/api/v1/members/password-reset",
            json!({ "email": "hanako@example.com" }),
        )
        .await;
    assert_eq!(request.status, StatusCode::ACCEPTED);
    assert!(site.mails.take().is_empty());
    let _ = Recipient::Staff;
}
