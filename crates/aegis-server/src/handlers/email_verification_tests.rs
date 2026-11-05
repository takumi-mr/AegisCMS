//! メールアドレスの確認（API とページ）の結合テスト（ルーター経由）

use std::sync::Arc;

use aegis_runtime::PluginSupervisor;
use aegis_schema::SchemaDefinition;
use axum::body::Body;
use axum::http::{Request, StatusCode};
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
}

struct Reply {
    status: StatusCode,
    body: String,
}

impl Reply {
    fn json(&self) -> Value {
        serde_json::from_str(&self.body).unwrap_or(Value::Null)
    }
}

/// `require`: 確認を必須にする。`with_mail`: メールを送れる状態にする
async fn site(require: bool, with_mail: bool) -> Site {
    let mut config = AegisConfig::default();
    config.commerce.enabled = true;
    config.members.enabled = true;
    config.commerce.product_type = "product".to_string();
    config.members.public_url = PUBLIC_URL.to_string();
    config.members.require_email_verification = require;

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
    let members = aegis_members::MemberService::from_config(
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
    Site {
        app: router(&config, state),
        mails,
    }
}

impl Site {
    async fn request(&self, request: Request<Body>) -> Reply {
        let response = self.app.clone().oneshot(request).await.unwrap();
        let status = response.status();
        let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        Reply {
            status,
            body: String::from_utf8_lossy(&bytes).to_string(),
        }
    }

    async fn post(&self, path: &str, body: Value) -> Reply {
        self.request(
            Request::post(path)
                .header("content-type", "application/json")
                .body(Body::from(body.to_string()))
                .unwrap(),
        )
        .await
    }

    async fn get(&self, path: &str) -> Reply {
        self.request(Request::get(path).body(Body::empty()).unwrap())
            .await
    }

    async fn register(&self, email: &str, password: &str) -> Reply {
        self.post(
            "/api/v1/members/register",
            json!({ "email": email, "password": password, "name": "Hanako" }),
        )
        .await
    }

    async fn login(&self, email: &str, password: &str) -> Reply {
        self.post(
            "/api/v1/members/login",
            json!({ "email": email, "password": password }),
        )
        .await
    }

    /// 直近の確認メールの、リンクのトークン
    fn verify_token(&self) -> String {
        let events = self.mails.take();
        let event = events
            .iter()
            .rev()
            .find(|e| e.kind == "member.verify")
            .expect("a confirmation email was sent");
        let url = event.data["verify_url"].as_str().unwrap();
        assert!(
            url.starts_with(&format!("{PUBLIC_URL}/account/verify?token=")),
            "the link comes from [members].public_url: {url}"
        );
        url.split("token=").nth(1).unwrap().to_string()
    }
}

#[tokio::test]
async fn test_register_confirm_and_log_in() {
    let site = site(true, true).await;

    let registered = site.register("hanako@example.com", "password123").await;
    assert_eq!(
        registered.status,
        StatusCode::ACCEPTED,
        "{}",
        registered.body
    );
    assert_eq!(registered.json()["status"], "verification_sent");
    // 登録しても、ログインはまだ（応答にトークンも会員の情報もない）
    assert!(registered.json().get("member").is_none());
    let token = site.verify_token();

    // 確認が済むまで、パスワードが合っていても入れない。コードで、ページが案内を出せる
    let waiting = site.login("hanako@example.com", "password123").await;
    assert_eq!(waiting.status, StatusCode::FORBIDDEN);
    assert_eq!(waiting.json()["code"], "email_not_verified");
    // パスワードが違うときは、確認待ちかどうかを教えない
    let wrong = site.login("hanako@example.com", "wrong-password-1").await;
    assert_eq!(wrong.status, StatusCode::UNAUTHORIZED);
    assert!(wrong.json().get("code").is_none());

    let confirmed = site
        .post("/api/v1/members/verify", json!({ "token": token }))
        .await;
    assert_eq!(confirmed.status, StatusCode::OK, "{}", confirmed.body);
    assert_eq!(confirmed.json()["status"], "verified");
    assert_eq!(
        site.login("hanako@example.com", "password123").await.status,
        StatusCode::OK
    );

    // リンクは 1 回だけ
    let again = site
        .post("/api/v1/members/verify", json!({ "token": token }))
        .await;
    assert_eq!(again.status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_register_does_not_say_whether_the_address_is_taken() {
    let site = site(true, true).await;
    let first = site.register("taken@example.com", "password123").await;
    let token = site.verify_token();
    site.post("/api/v1/members/verify", json!({ "token": token }))
        .await;

    // 有効になったアドレスでの登録は、同じ応答で、メールも送らない
    let second = site
        .register("taken@example.com", "another-password-1")
        .await;
    assert_eq!(second.status, first.status);
    assert_eq!(second.json(), first.json());
    assert!(site.mails.take().is_empty());
    // 元のパスワードのまま
    assert_eq!(
        site.login("taken@example.com", "password123").await.status,
        StatusCode::OK
    );
}

#[tokio::test]
async fn test_resend_always_answers_the_same() {
    let site = site(true, true).await;
    site.register("waiting@example.com", "password123").await;
    site.mails.take();

    let waiting = site
        .post(
            "/api/v1/members/verify/resend",
            json!({ "email": "waiting@example.com" }),
        )
        .await;
    assert_eq!(waiting.status, StatusCode::ACCEPTED);
    let token = site.verify_token();
    assert_eq!(token.len(), 64);

    let unknown = site
        .post(
            "/api/v1/members/verify/resend",
            json!({ "email": "nobody@example.com" }),
        )
        .await;
    assert_eq!(unknown.status, StatusCode::ACCEPTED);
    assert_eq!(
        unknown.json(),
        waiting.json(),
        "the answer does not say which exists"
    );
    assert!(site.mails.take().is_empty());

    let bad = site
        .post(
            "/api/v1/members/verify/resend",
            json!({ "email": "not an email" }),
        )
        .await;
    assert_eq!(bad.status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_without_the_requirement_registration_works_as_before() {
    let site = site(false, true).await;
    let registered = site.register("plain@example.com", "password123").await;
    assert_eq!(registered.status, StatusCode::CREATED);
    assert_eq!(registered.json()["member"]["status"], "active");
    assert_eq!(
        site.login("plain@example.com", "password123").await.status,
        StatusCode::OK
    );
    assert!(site.mails.take().is_empty());
}

#[tokio::test]
async fn test_confirmation_is_off_without_mail_and_the_pages_follow() {
    // 確認を必須にしているのにメールを送れない（起動時に止める設定）。API は静かに登録させず、503
    let no_mail = site(true, false).await;
    let registered = no_mail.register("a@example.com", "password123").await;
    assert_eq!(registered.status, StatusCode::SERVICE_UNAVAILABLE);
    for path in ["/account/verify?token=x"] {
        assert_eq!(
            no_mail.get(path).await.status,
            StatusCode::NOT_FOUND,
            "{path}"
        );
    }
    let verify = no_mail
        .post("/api/v1/members/verify", json!({ "token": "x" }))
        .await;
    assert_eq!(verify.status, StatusCode::NOT_FOUND);

    // メールを送れるなら、確認のページがある（キャッシュには載せない）
    let with_mail = site(true, true).await;
    let page = with_mail.get("/account/verify?token=abc").await;
    assert_eq!(page.status, StatusCode::OK);
    assert!(page.body.contains("data-view=\"verify\""));
}
