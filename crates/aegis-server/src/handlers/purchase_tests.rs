//! 記事の単独購入の結合テスト（ルーター経由。Stripe はモック）。docs/members.md 16

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use aegis_ast::BlockNode;
use aegis_commerce::stripe::sign_webhook_payload;
use aegis_db::ContentRecord;
use aegis_runtime::PluginSupervisor;
use aegis_schema::SchemaDefinition;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use serde_json::{json, Value};
use tower::ServiceExt;

use crate::app::router;
use crate::config::AegisConfig;
use crate::state::AppState;
use crate::test_support::test_app_state;

const SECRET: &str = "Only buyers read this sentence about zxqunique.";
const WEBHOOK_SECRET: &str = "whsec_members_mock";

fn schema(content_type: &str) -> SchemaDefinition {
    serde_json::from_value(json!({
        "content_type": content_type,
        "label": content_type,
        "fields": [
            { "name": "excerpt", "type": "string", "required": false, "nullable": true },
            { "name": "access", "type": "string", "required": false, "nullable": true },
            { "name": "purchase_price", "type": "integer", "required": false, "nullable": true }
        ]
    }))
    .unwrap()
}

/// Stripe API のモック（Checkout Session の作成だけ）。受け取ったフォームを記録する
async fn mock_stripe() -> (String, Arc<Mutex<Vec<String>>>) {
    let forms = Arc::new(Mutex::new(Vec::new()));
    let recorded = forms.clone();
    let app = axum::Router::new().route(
        "/v1/checkout/sessions",
        axum::routing::post(move |headers: axum::http::HeaderMap, body: String| {
            let recorded = recorded.clone();
            async move {
                assert_eq!(headers["authorization"], "Bearer sk_test_mock");
                let mut forms = recorded.lock().unwrap();
                forms.push(body);
                let id = format!("cs_test_{}", forms.len());
                axum::Json(
                    json!({ "id": id, "url": format!("https://checkout.stripe.test/{}", id) }),
                )
            }
        }),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    (format!("http://{}", address), forms)
}

struct Site {
    app: axum::Router,
    state: Arc<AppState>,
    forms: Arc<Mutex<Vec<String>>>,
}

struct Reply {
    status: StatusCode,
    location: Option<String>,
    body: String,
}

impl Reply {
    fn json(&self) -> Value {
        serde_json::from_str(&self.body).unwrap_or(Value::Null)
    }
}

async fn site(purchase: bool) -> Site {
    let (api_base, forms) = mock_stripe().await;
    let mut config = AegisConfig::default();
    config.members.enabled = true;
    config.members.levels = vec!["free".into(), "paid".into()];
    config.members.public_url = "https://example.com".to_string();
    config.members.purchase.enabled = purchase;
    config.members.purchase.currency = "jpy".to_string();
    config.members.purchase.stripe.secret_key = "sk_test_mock".to_string();
    config.members.purchase.stripe.webhook_secret = WEBHOOK_SECRET.to_string();
    config.members.purchase.stripe.api_base = api_base;
    let state = test_app_state(
        config.clone(),
        PluginSupervisor::new(),
        vec![schema("article"), schema("page")],
    )
    .await;
    let members = aegis_members::MemberService::from_config(
        &config.members,
        state.db.clone(),
        state.cache.clone(),
        state.audit.clone(),
    );
    let state = Arc::new(state.with_members(members.unwrap()));
    Site {
        app: router(&config, state.clone()),
        state,
        forms,
    }
}

impl Site {
    async fn send(&self, request: Request<Body>) -> Reply {
        let response = self.app.clone().oneshot(request).await.unwrap();
        let status = response.status();
        let location = response
            .headers()
            .get("location")
            .and_then(|v| v.to_str().ok())
            .map(str::to_string);
        let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        Reply {
            status,
            location,
            body: String::from_utf8_lossy(&bytes).to_string(),
        }
    }

    async fn get(&self, path: &str, bearer: Option<&str>) -> Reply {
        let mut request = Request::get(path);
        if let Some(token) = bearer {
            request = request.header("authorization", format!("Bearer {token}"));
        }
        self.send(request.body(Body::empty()).unwrap()).await
    }

    async fn post_json(&self, path: &str, body: Value, bearer: Option<&str>) -> Reply {
        let mut request = Request::post(path).header("content-type", "application/json");
        if let Some(token) = bearer {
            request = request.header("authorization", format!("Bearer {token}"));
        }
        self.send(request.body(Body::from(body.to_string())).unwrap())
            .await
    }

    /// `access` と `purchase_price` を付けた記事を公開する
    async fn publish(&self, slug: &str, access: Option<&str>, price: Option<i64>) {
        let schema = self
            .state
            .schemas
            .read()
            .await
            .get("article")
            .cloned()
            .unwrap();
        let mut custom_fields = HashMap::new();
        custom_fields.insert("excerpt".to_string(), json!("A public teaser."));
        if let Some(access) = access {
            custom_fields.insert("access".to_string(), json!(access));
        }
        if let Some(price) = price {
            custom_fields.insert("purchase_price".to_string(), json!(price));
        }
        let record = ContentRecord {
            id: format!("article-{slug}"),
            content_type: "article".to_string(),
            title: format!("Title of {slug}"),
            slug: slug.to_string(),
            status: "published".to_string(),
            blocks_json: serde_json::to_string(&vec![BlockNode::paragraph(SECRET)]).unwrap(),
            author_id: None,
            published_at: Some("2026-10-22T00:00:00Z".to_string()),
            unpublish_at: None,
            created_at: "2026-10-22T00:00:00Z".to_string(),
            updated_at: "2026-10-22T00:00:00Z".to_string(),
            custom_fields,
        };
        self.state
            .db
            .insert_content(&schema, &record)
            .await
            .unwrap();
    }

    /// 会員を作ってログインし、トークンを返す
    async fn member(&self, email: &str, level: &str) -> String {
        let service = self.state.members.as_ref().unwrap();
        let member = service
            .register(
                aegis_members::MemberRegisterRequest {
                    email: email.to_string(),
                    password: "password123".to_string(),
                    name: "Member".to_string(),
                    phone: None,
                    shipping_address: None,
                },
                None,
                None,
            )
            .await
            .unwrap();
        if level != "free" {
            service
                .admin_set_level(&member.id, level, "admin")
                .await
                .unwrap();
        }
        service
            .login(
                aegis_members::MemberLoginRequest {
                    email: email.to_string(),
                    password: "password123".to_string(),
                },
                None,
                None,
            )
            .await
            .unwrap()
            .0
            .token
    }

    async fn buy(&self, slug: &str, token: &str) -> Reply {
        self.post_json(
            "/api/v1/members/purchases",
            json!({ "content_type": "article", "slug": slug }),
            Some(token),
        )
        .await
    }

    async fn webhook(&self, event: Value, secret: &str) -> Reply {
        let body = event.to_string();
        let signature =
            sign_webhook_payload(secret, body.as_bytes(), chrono::Utc::now().timestamp());
        let request = Request::post("/api/v1/members/webhooks/stripe")
            .header("stripe-signature", signature)
            .body(Body::from(body))
            .unwrap();
        self.send(request).await
    }

    async fn purchase(&self, session: &str) -> aegis_db::PurchaseRecord {
        self.state
            .db
            .get_purchase_by_ref(session)
            .await
            .unwrap()
            .unwrap()
    }
}

fn session_event(kind: &str, session: &str, amount: i64) -> Value {
    json!({
        "id": format!("evt_{kind}"),
        "type": kind,
        "data": { "object": {
            "id": session,
            "payment_status": "paid",
            "payment_intent": "pi_test_1",
            "amount_total": amount,
            "currency": "jpy"
        }}
    })
}

#[tokio::test]
async fn test_purchases_are_off_unless_enabled() {
    let site = site(false).await;
    site.publish("secret", Some("paid"), Some(500)).await;
    let token = site.member("a@example.com", "free").await;
    assert_eq!(
        site.buy("secret", &token).await.status,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        site.get(
            "/account/purchase?content_type=article&slug=secret",
            Some(&token)
        )
        .await
        .status,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        site.webhook(
            session_event("checkout.session.completed", "cs", 500),
            WEBHOOK_SECRET
        )
        .await
        .status,
        StatusCode::NOT_FOUND
    );
    // 案内に購入のリンクを出さない
    let reply = site.get("/articles/secret", Some(&token)).await;
    assert!(!reply.body.contains("/account/purchase"));
}

#[tokio::test]
async fn test_the_notice_offers_to_buy_with_the_price() {
    let site = site(true).await;
    site.publish("secret", Some("paid"), Some(500)).await;
    site.publish("nopurchase", Some("paid"), None).await;
    let token = site.member("a@example.com", "free").await;

    // ゲスト: 金額つきの案内と、ログインのリンク（購入のリンクはまだ出さない）
    let guest = site.get("/articles/secret", None).await;
    assert!(guest.body.contains("¥500"), "{}", guest.body);
    assert!(guest
        .body
        .contains("/account/login?next=%2Farticles%2Fsecret"));
    assert!(!guest.body.contains("/account/purchase"));
    assert!(!guest.body.contains(SECRET));

    // 会員: 購入のリンク
    let member = site.get("/articles/secret", Some(&token)).await;
    assert!(
        member
            .body
            .contains("/account/purchase?content_type=article&amp;slug=secret"),
        "{}",
        member.body
    );
    assert!(member.body.contains("¥500"));
    assert!(!member.body.contains(SECRET));

    // 価格のない記事には、購入の案内を出さない
    let other = site.get("/articles/nopurchase", Some(&token)).await;
    assert!(!other.body.contains("/account/purchase"));
}

#[tokio::test]
async fn test_starting_a_purchase_checks_the_content_and_ignores_client_amounts() {
    let site = site(true).await;
    site.publish("secret", Some("paid"), Some(500)).await;
    site.publish("free-text", None, Some(500)).await;
    site.publish("no-price", Some("paid"), None).await;
    let token = site.member("a@example.com", "free").await;

    // ログインが要る
    let reply = site
        .post_json(
            "/api/v1/members/purchases",
            json!({ "content_type": "article", "slug": "secret" }),
            None,
        )
        .await;
    assert_eq!(reply.status, StatusCode::UNAUTHORIZED);

    // 売り物でないもの（価格が無い・制限が無い・無い記事・知らない型）
    for (content_type, slug) in [
        ("article", "no-price"),
        ("article", "free-text"),
        ("article", "missing"),
        ("nothing", "secret"),
    ] {
        let reply = site
            .post_json(
                "/api/v1/members/purchases",
                json!({ "content_type": content_type, "slug": slug }),
                Some(&token),
            )
            .await;
        assert_eq!(reply.status, StatusCode::NOT_FOUND, "{slug}");
        assert_eq!(reply.json()["code"], "not_for_sale");
    }
    assert!(site.forms.lock().unwrap().is_empty());

    // 金額を送っても使わない
    let reply = site
        .post_json(
            "/api/v1/members/purchases",
            json!({ "content_type": "article", "slug": "secret", "amount": 1, "currency": "usd" }),
            Some(&token),
        )
        .await;
    assert_eq!(reply.status, StatusCode::CREATED, "{}", reply.body);
    assert_eq!(
        reply.json()["redirect_url"],
        "https://checkout.stripe.test/cs_test_1"
    );
    let form = site.forms.lock().unwrap()[0].clone();
    assert!(
        form.contains("line_items%5B0%5D%5Bprice_data%5D%5Bunit_amount%5D=500"),
        "{form}"
    );
    assert!(form.contains("line_items%5B0%5D%5Bprice_data%5D%5Bcurrency%5D=jpy"));
    assert!(
        form.contains("success_url=https%3A%2F%2Fexample.com%2Farticles%2Fsecret%3Fpurchased%3D1")
    );
    assert!(form.contains("cancel_url=https%3A%2F%2Fexample.com%2Farticles%2Fsecret"));
    let purchase = site.purchase("cs_test_1").await;
    assert_eq!(purchase.status, "pending");
    assert_eq!(purchase.amount, 500);
    assert!(form.contains(&format!("client_reference_id={}", purchase.id)));

    // 決済ページを作っただけでは、読めない
    let page = site.get("/articles/secret", Some(&token)).await;
    assert!(!page.body.contains(SECRET));
}

#[tokio::test]
async fn test_a_payment_notification_grants_access_to_that_member_only() {
    let site = site(true).await;
    site.publish("secret", Some("paid"), Some(500)).await;
    let buyer = site.member("buyer@example.com", "free").await;
    let other = site.member("other@example.com", "free").await;
    assert_eq!(site.buy("secret", &buyer).await.status, StatusCode::CREATED);

    // 偽の署名・金額違い・知らないセッションでは、権利は付かない
    let forged = site
        .webhook(
            session_event("checkout.session.completed", "cs_test_1", 500),
            "whsec_forged",
        )
        .await;
    assert_eq!(forged.status, StatusCode::BAD_REQUEST);
    for event in [
        session_event("checkout.session.completed", "cs_test_1", 1),
        session_event("checkout.session.completed", "cs_unknown", 500),
    ] {
        assert_eq!(
            site.webhook(event, WEBHOOK_SECRET).await.status,
            StatusCode::OK
        );
    }
    let mut wrong_currency = session_event("checkout.session.completed", "cs_test_1", 500);
    wrong_currency["data"]["object"]["currency"] = json!("usd");
    assert_eq!(
        site.webhook(wrong_currency, WEBHOOK_SECRET).await.status,
        StatusCode::OK
    );
    assert_eq!(site.purchase("cs_test_1").await.status, "pending");
    assert!(!site
        .get("/articles/secret", Some(&buyer))
        .await
        .body
        .contains(SECRET));

    // 後払い（まだ支払われていない）は待つ
    let mut awaiting = session_event("checkout.session.completed", "cs_test_1", 500);
    awaiting["data"]["object"]["payment_status"] = json!("unpaid");
    assert_eq!(
        site.webhook(awaiting, WEBHOOK_SECRET).await.status,
        StatusCode::OK
    );
    assert_eq!(site.purchase("cs_test_1").await.status, "pending");

    // 入金
    let paid = site
        .webhook(
            session_event("checkout.session.completed", "cs_test_1", 500),
            WEBHOOK_SECRET,
        )
        .await;
    assert_eq!(paid.status, StatusCode::OK);
    let purchase = site.purchase("cs_test_1").await;
    assert_eq!(purchase.status, "paid");
    assert_eq!(purchase.transaction_id.as_deref(), Some("pi_test_1"));
    assert!(purchase.paid_at.is_some());

    // 買った会員だけが読める。応答は共有キャッシュに載せない
    let page = site.get("/articles/secret", Some(&buyer)).await;
    assert!(page.body.contains(SECRET));
    assert!(!site
        .get("/articles/secret", Some(&other))
        .await
        .body
        .contains(SECRET));
    assert!(!site
        .get("/articles/secret", None)
        .await
        .body
        .contains(SECRET));

    // 通知が再送されても、結果は変わらない
    assert_eq!(
        site.webhook(
            session_event("checkout.session.completed", "cs_test_1", 500),
            WEBHOOK_SECRET
        )
        .await
        .status,
        StatusCode::OK
    );
    assert_eq!(site.purchase("cs_test_1").await.status, "paid");

    // 買った記事は、もう買えない。レベルで読める会員も買わせない
    let again = site.buy("secret", &buyer).await;
    assert_eq!(again.status, StatusCode::CONFLICT);
    assert_eq!(again.json()["code"], "already_purchased");
    let paid_member = site.member("paid@example.com", "paid").await;
    let accessible = site.buy("secret", &paid_member).await;
    assert_eq!(accessible.status, StatusCode::CONFLICT);
    assert_eq!(accessible.json()["code"], "already_accessible");

    // 自分の購入の一覧
    let list = site.get("/api/v1/members/purchases", Some(&buyer)).await;
    assert_eq!(list.status, StatusCode::OK);
    let purchases = &list.json()["purchases"];
    assert_eq!(purchases.as_array().unwrap().len(), 1);
    assert_eq!(purchases[0]["status"], "paid");
    assert_eq!(purchases[0]["title"], "Title of secret");
    assert_eq!(purchases[0]["url"], "/articles/secret");
    let others = site.get("/api/v1/members/purchases", Some(&other)).await;
    assert!(others.json()["purchases"].as_array().unwrap().is_empty());
}

#[tokio::test]
async fn test_expiry_and_refunds_change_the_purchase() {
    let site = site(true).await;
    site.publish("secret", Some("paid"), Some(500)).await;
    let buyer = site.member("buyer@example.com", "free").await;

    // 期限切れ: pending → cancelled（そのあと買い直せる）
    site.buy("secret", &buyer).await;
    let expired = json!({
        "id": "evt_exp", "type": "checkout.session.expired",
        "data": { "object": { "id": "cs_test_1" } }
    });
    assert_eq!(
        site.webhook(expired, WEBHOOK_SECRET).await.status,
        StatusCode::OK
    );
    assert_eq!(site.purchase("cs_test_1").await.status, "cancelled");

    site.buy("secret", &buyer).await;
    site.webhook(
        session_event("checkout.session.completed", "cs_test_2", 500),
        WEBHOOK_SECRET,
    )
    .await;
    assert!(site
        .get("/articles/secret", Some(&buyer))
        .await
        .body
        .contains(SECRET));

    // 一部の返金では、権利は残る。全額の返金で外れる
    let refund = |refunded: bool| {
        json!({
            "id": "evt_refund", "type": "charge.refunded",
            "data": { "object": { "payment_intent": "pi_test_1", "refunded": refunded } }
        })
    };
    site.webhook(refund(false), WEBHOOK_SECRET).await;
    assert_eq!(site.purchase("cs_test_2").await.status, "paid");
    assert!(site
        .get("/articles/secret", Some(&buyer))
        .await
        .body
        .contains(SECRET));
    site.webhook(refund(true), WEBHOOK_SECRET).await;
    assert_eq!(site.purchase("cs_test_2").await.status, "refunded");
    assert!(!site
        .get("/articles/secret", Some(&buyer))
        .await
        .body
        .contains(SECRET));
}

#[tokio::test]
async fn test_the_buy_link_redirects_to_the_payment_page_or_the_login() {
    let site = site(true).await;
    site.publish("secret", Some("paid"), Some(500)).await;
    let token = site.member("a@example.com", "free").await;
    let path = "/account/purchase?content_type=article&slug=secret";

    // 未ログイン: ログインのあとにここへ戻る
    let guest = site.get(path, None).await;
    assert_eq!(guest.status, StatusCode::SEE_OTHER);
    let location = guest.location.unwrap();
    assert!(location.starts_with("/account/login?next="), "{location}");
    assert!(location.contains("%2Faccount%2Fpurchase%3Fcontent_type%3Darticle%26slug%3Dsecret"));

    // ログイン済み: 決済ページへ
    let member = site.get(path, Some(&token)).await;
    assert_eq!(member.status, StatusCode::SEE_OTHER);
    assert_eq!(
        member.location.as_deref(),
        Some("https://checkout.stripe.test/cs_test_1")
    );

    // 売り物でないものは、リダイレクトせずに断る
    let missing = site
        .get(
            "/account/purchase?content_type=article&slug=nope",
            Some(&token),
        )
        .await;
    assert_eq!(missing.status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_leaving_detaches_the_purchases_and_drops_the_access() {
    let site = site(true).await;
    site.publish("secret", Some("paid"), Some(500)).await;
    let buyer = site.member("buyer@example.com", "free").await;
    site.buy("secret", &buyer).await;
    site.webhook(
        session_event("checkout.session.completed", "cs_test_1", 500),
        WEBHOOK_SECRET,
    )
    .await;
    let member = site
        .state
        .db
        .get_member_by_email("buyer@example.com")
        .await
        .unwrap()
        .unwrap();
    assert!(site
        .state
        .db
        .has_paid_purchase(&member.id, "article", "article-secret")
        .await
        .unwrap());

    let reply = site
        .send(
            Request::delete("/api/v1/members/me")
                .header("content-type", "application/json")
                .header("authorization", format!("Bearer {buyer}"))
                .body(Body::from(json!({ "password": "password123" }).to_string()))
                .unwrap(),
        )
        .await;
    assert_eq!(reply.status, StatusCode::NO_CONTENT, "{}", reply.body);

    // 購入の記録は残り、会員との紐づきだけが外れる
    let purchase = site.purchase("cs_test_1").await;
    assert_eq!(purchase.status, "paid");
    assert_eq!(purchase.member_id, None);
    assert!(!site
        .state
        .db
        .has_paid_purchase(&member.id, "article", "article-secret")
        .await
        .unwrap());
}
