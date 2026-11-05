//! 顧客アカウントのページ（`/account`、ログインの Cookie、`/shop/account.js`）の結合テスト（ルーター経由）

use std::sync::Arc;

use aegis_runtime::PluginSupervisor;
use aegis_schema::SchemaDefinition;
use axum::body::Body;
use axum::http::{HeaderMap, Request, StatusCode};
use serde_json::{json, Value};
use tower::ServiceExt;

use crate::app::router;
use crate::config::AegisConfig;
use crate::state::AppState;
use crate::test_support::test_app_state;

fn product_schema() -> SchemaDefinition {
    serde_json::from_value(json!({
        "content_type": "product",
        "label": "Products",
        "fields": [
            { "name": "price", "type": "integer", "required": true },
            { "name": "stock", "type": "integer", "nullable": true }
        ]
    }))
    .unwrap()
}

struct Site {
    app: axum::Router,
    admin_token: String,
}

struct Reply {
    status: StatusCode,
    headers: HeaderMap,
    body: String,
}

impl Reply {
    fn set_cookie(&self) -> String {
        self.headers
            .get("set-cookie")
            .and_then(|v| v.to_str().ok())
            .unwrap_or_default()
            .to_string()
    }

    fn location(&self) -> &str {
        self.headers
            .get("location")
            .and_then(|v| v.to_str().ok())
            .unwrap_or_default()
    }

    fn cache_control(&self) -> &str {
        self.headers
            .get("cache-control")
            .and_then(|v| v.to_str().ok())
            .unwrap_or_default()
    }
}

async fn site_with(configure: impl FnOnce(&mut AegisConfig)) -> Site {
    let mut config = AegisConfig::default();
    config.commerce.enabled = true;
    config.commerce.product_type = "product".to_string();
    configure(&mut config);

    let state = test_app_state(
        config.clone(),
        PluginSupervisor::new(),
        vec![product_schema()],
    )
    .await;
    let commerce =
        aegis_commerce::CommerceService::new(config.commerce.clone(), state.commerce_deps())
            .unwrap();
    let customers = aegis_commerce::CustomerService::from_config(
        &config.commerce.customers,
        state.db.clone(),
        state.cache.clone(),
        state.audit.clone(),
    );
    let state = state.with_commerce(commerce);
    let state: Arc<AppState> = Arc::new(match customers {
        Some(customers) => state.with_customers(customers),
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
        admin_token,
    }
}

async fn site() -> Site {
    site_with(|_| {}).await
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

    async fn get(&self, path: &str, cookie: Option<&str>, bearer: Option<&str>) -> Reply {
        let mut request = Request::get(path);
        if let Some(cookie) = cookie {
            request = request.header("cookie", cookie);
        }
        if let Some(bearer) = bearer {
            request = request.header("authorization", format!("Bearer {bearer}"));
        }
        self.send(request.body(Body::empty()).unwrap()).await
    }

    async fn post(&self, path: &str, body: Value, extra: &[(&str, &str)]) -> Reply {
        let mut request = Request::post(path).header("content-type", "application/json");
        for (name, value) in extra {
            request = request.header(*name, *value);
        }
        self.send(request.body(Body::from(body.to_string())).unwrap())
            .await
    }

    /// 登録してログインし、(Cookie のヘッダー値, 応答本文のトークン) を返す
    async fn sign_up(&self, email: &str) -> (String, String) {
        let registered = self
            .post(
                "/api/v1/customers/register",
                json!({ "email": email, "password": "password123", "name": "Hanako" }),
                &[],
            )
            .await;
        assert_eq!(
            registered.status,
            StatusCode::CREATED,
            "{}",
            registered.body
        );
        let login = self
            .post(
                "/api/v1/customers/login",
                json!({ "email": email, "password": "password123" }),
                &[],
            )
            .await;
        assert_eq!(login.status, StatusCode::OK, "{}", login.body);
        let cookie = login.set_cookie();
        let pair = cookie.split(';').next().unwrap().to_string();
        let token = serde_json::from_str::<Value>(&login.body).unwrap()["token"]
            .as_str()
            .unwrap()
            .to_string();
        (pair, token)
    }
}

#[tokio::test]
async fn test_login_sets_an_httponly_cookie_and_still_returns_the_token() {
    let site = site().await;
    site.post(
        "/api/v1/customers/register",
        json!({ "email": "a@example.com", "password": "password123", "name": "A" }),
        &[],
    )
    .await;
    let login = site
        .post(
            "/api/v1/customers/login",
            json!({ "email": "a@example.com", "password": "password123" }),
            &[],
        )
        .await;
    let cookie = login.set_cookie();
    assert!(cookie.starts_with("cms_customer=cses_"), "{cookie}");
    for attribute in ["HttpOnly", "SameSite=Lax", "Path=/", "Max-Age=2592000"] {
        assert!(cookie.contains(attribute), "{attribute} in {cookie}");
    }
    assert!(!cookie.contains("Secure"), "plain HTTP: {cookie}");
    // API のクライアント向けに、本文のトークンも従来どおり返す
    let body: Value = serde_json::from_str(&login.body).unwrap();
    assert!(body["token"].as_str().unwrap().starts_with("cses_"));

    // HTTPS（リバースプロキシの背後）なら Secure
    let behind_proxy = site
        .post(
            "/api/v1/customers/login",
            json!({ "email": "a@example.com", "password": "password123" }),
            &[("x-forwarded-proto", "https")],
        )
        .await;
    assert!(behind_proxy.set_cookie().contains("; Secure"));
}

#[tokio::test]
async fn test_the_cookie_and_the_bearer_token_both_authenticate() {
    let site = site().await;
    let (cookie, token) = site.sign_up("b@example.com").await;

    let by_cookie = site.get("/api/v1/customers/me", Some(&cookie), None).await;
    assert_eq!(by_cookie.status, StatusCode::OK, "{}", by_cookie.body);
    let by_bearer = site.get("/api/v1/customers/me", None, Some(&token)).await;
    assert_eq!(by_bearer.status, StatusCode::OK);

    let anonymous = site.get("/api/v1/customers/me", None, None).await;
    assert_eq!(anonymous.status, StatusCode::UNAUTHORIZED);
    // 作り物の Cookie は通らない
    let forged = site
        .get(
            "/api/v1/customers/me",
            Some("cms_customer=cses_forged"),
            None,
        )
        .await;
    assert_eq!(forged.status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn test_customer_and_admin_sessions_do_not_mix() {
    let site = site().await;
    let (cookie, token) = site.sign_up("c@example.com").await;

    // 顧客のトークンでも Cookie でも、管理の API は通らない
    for (cookie, bearer) in [(Some(cookie.as_str()), None), (None, Some(token.as_str()))] {
        let admin_api = site.get("/api/v1/admin/customers", cookie, bearer).await;
        assert_eq!(admin_api.status, StatusCode::UNAUTHORIZED);
    }
    // 管理者のトークンでは、顧客の API は通らない
    let as_customer = site
        .get("/api/v1/customers/me", None, Some(&site.admin_token))
        .await;
    assert_eq!(as_customer.status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn test_logout_clears_the_cookie_and_ends_the_session() {
    let site = site().await;
    let (cookie, _) = site.sign_up("d@example.com").await;

    let logout = site
        .send(
            Request::post("/api/v1/customers/logout")
                .header("cookie", &cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await;
    assert_eq!(logout.status, StatusCode::NO_CONTENT);
    let cleared = logout.set_cookie();
    assert!(cleared.starts_with("cms_customer=;"), "{cleared}");
    assert!(cleared.contains("Max-Age=0"), "{cleared}");

    // ログアウトしたセッションは、残っている Cookie でも通らない
    let after = site.get("/api/v1/customers/me", Some(&cookie), None).await;
    assert_eq!(after.status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn test_account_pages_follow_the_login_state() {
    let site = site().await;

    // 未ログイン: マイページはログインへ。ログインと登録の画面は出る
    let home = site.get("/account", None, None).await;
    assert!(home.status.is_redirection(), "{}", home.status);
    assert_eq!(home.location(), "/account/login");
    assert_eq!(home.cache_control(), "private, no-store");

    for (path, view) in [
        ("/account/login", "login"),
        ("/account/register", "register"),
    ] {
        let page = site.get(path, None, None).await;
        assert_eq!(page.status, StatusCode::OK, "{path}");
        assert!(page.body.contains("account.js"), "{path}");
        assert!(
            page.body.contains(&format!("data-view=\"{view}\"")),
            "{path}"
        );
        assert_eq!(page.cache_control(), "private, no-store");
    }

    // ログイン済み: マイページが出て、ログイン・登録の画面はマイページへ
    let (cookie, _) = site.sign_up("e@example.com").await;
    let home = site.get("/account", Some(&cookie), None).await;
    assert_eq!(home.status, StatusCode::OK);
    assert!(home.body.contains("data-view=\"home\""));
    for path in ["/account/login", "/account/register"] {
        let page = site.get(path, Some(&cookie), None).await;
        assert!(page.status.is_redirection(), "{path}");
        assert_eq!(page.location(), "/account");
    }

    // 古い・作り物の Cookie はログインしていない扱い
    let forged = site
        .get("/account", Some("cms_customer=cses_x"), None)
        .await;
    assert_eq!(forged.location(), "/account/login");
}

#[tokio::test]
async fn test_the_shop_pages_link_to_the_account() {
    let site = site().await;
    let cart = site.get("/cart", None, None).await;
    assert_eq!(cart.status, StatusCode::OK);
    assert!(
        cart.body.contains("-account-link"),
        "the nav links to the account"
    );
}

#[tokio::test]
async fn test_account_script_carries_the_texts_of_the_site_language() {
    let site = site_with(|config| config.theme.locale = "ja".to_string()).await;
    let script = site.get("/shop/account.js", None, None).await;
    assert_eq!(script.status, StatusCode::OK);
    assert!(script
        .headers
        .get("content-type")
        .unwrap()
        .to_str()
        .unwrap()
        .starts_with("application/javascript"));
    assert!(
        script.body.contains("_CART_I18N = "),
        "{}",
        script.body.chars().take(200).collect::<String>()
    );
    assert!(script.body.contains("ログインする"));
    // トークンに触れない（Cookie は HttpOnly）
    assert!(!script.body.contains("localStorage.setItem('aegis_customer"));
}

#[tokio::test]
async fn test_closed_registration_and_disabled_accounts_hide_the_pages() {
    let closed = site_with(|config| {
        config.commerce.customers.registration = aegis_commerce::CustomerRegistration::Closed;
    })
    .await;
    assert_eq!(
        closed.get("/account/register", None, None).await.status,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        closed.get("/account/login", None, None).await.status,
        StatusCode::OK
    );
    // 登録を止めているときは、登録のリンク先が空（スクリプトは「登録はこちら」を出さない）
    let login = closed.get("/account/login", None, None).await;
    assert!(login.body.contains("data-register-url=\"\""));

    let disabled = site_with(|config| config.commerce.customers.enabled = false).await;
    for path in [
        "/account",
        "/account/login",
        "/account/register",
        "/shop/account.js",
    ] {
        assert_eq!(
            disabled.get(path, None, None).await.status,
            StatusCode::NOT_FOUND,
            "{path}"
        );
    }
    // ゲスト購入の画面は、アカウントがなくても出る。ナビゲーションはアカウントへのリンクを出さない
    let cart = disabled.get("/cart", None, None).await;
    assert_eq!(cart.status, StatusCode::OK);
    assert!(!cart.body.contains("-account-link"));
}

fn delete_request(cookie: Option<&str>, password: &str) -> Request<Body> {
    let mut request = Request::builder()
        .method("DELETE")
        .uri("/api/v1/customers/me")
        .header("content-type", "application/json");
    if let Some(cookie) = cookie {
        request = request.header("cookie", cookie);
    }
    request
        .body(Body::from(json!({ "password": password }).to_string()))
        .unwrap()
}

#[tokio::test]
async fn test_a_customer_can_delete_their_account() {
    let site = site().await;
    let (cookie, _) = site.sign_up("bye@example.com").await;

    // ログインしていないと、消せない
    let anonymous = site.send(delete_request(None, "password123")).await;
    assert_eq!(anonymous.status, StatusCode::UNAUTHORIZED);
    // パスワードが違うと、消さない（アカウントもセッションもそのまま）
    let wrong = site
        .send(delete_request(Some(&cookie), "wrong-password-1"))
        .await;
    assert_eq!(wrong.status, StatusCode::UNAUTHORIZED);
    assert_eq!(
        site.get("/api/v1/customers/me", Some(&cookie), None)
            .await
            .status,
        StatusCode::OK
    );

    let deleted = site
        .send(delete_request(Some(&cookie), "password123"))
        .await;
    assert_eq!(deleted.status, StatusCode::NO_CONTENT);
    // Cookie も消える。そのセッションは、もう通らない
    let cleared = deleted.set_cookie();
    assert!(cleared.starts_with("cms_customer=;"), "{cleared}");
    assert!(cleared.contains("Max-Age=0"), "{cleared}");
    assert_eq!(
        site.get("/api/v1/customers/me", Some(&cookie), None)
            .await
            .status,
        StatusCode::UNAUTHORIZED
    );
    // ログインもできない。同じアドレスで、あらためて登録はできる
    let login = site
        .post(
            "/api/v1/customers/login",
            json!({ "email": "bye@example.com", "password": "password123" }),
            &[],
        )
        .await;
    assert_eq!(login.status, StatusCode::UNAUTHORIZED);
    let again = site
        .post(
            "/api/v1/customers/register",
            json!({ "email": "bye@example.com", "password": "password456", "name": "New" }),
            &[],
        )
        .await;
    assert_eq!(again.status, StatusCode::CREATED);
}
