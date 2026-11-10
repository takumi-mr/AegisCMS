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
async fn mock_stripe() -> (String, Arc<Mutex<Vec<String>>>, Arc<Mutex<Vec<String>>>) {
    let forms = Arc::new(Mutex::new(Vec::new()));
    let recorded = forms.clone();
    // Checkout 以外の呼び出し（`METHOD /path body`）
    let calls = Arc::new(Mutex::new(Vec::new()));
    let (cancelled, refunded, portal) = (calls.clone(), calls.clone(), calls.clone());
    let app = axum::Router::new()
        .route(
            "/v1/subscriptions/{id}",
            axum::routing::get(
                |axum::extract::Path(id): axum::extract::Path<String>| async move {
                    axum::Json(json!({
                        "id": id,
                        "status": "active",
                        "current_period_end": chrono::Utc::now().timestamp() + 30 * 86_400,
                        "cancel_at_period_end": false
                    }))
                },
            )
            .delete(
                move |axum::extract::Path(id): axum::extract::Path<String>| {
                    let cancelled = cancelled.clone();
                    async move {
                        cancelled
                            .lock()
                            .unwrap()
                            .push(format!("DELETE /v1/subscriptions/{id}"));
                        axum::Json(json!({ "id": id, "status": "canceled" }))
                    }
                },
            ),
        )
        .route(
            "/v1/refunds",
            axum::routing::post(move |body: String| {
                let refunded = refunded.clone();
                async move {
                    refunded
                        .lock()
                        .unwrap()
                        .push(format!("POST /v1/refunds {body}"));
                    axum::Json(json!({ "id": "re_1" }))
                }
            }),
        )
        .route(
            "/v1/billing_portal/sessions",
            axum::routing::post(move |body: String| {
                let portal = portal.clone();
                async move {
                    portal
                        .lock()
                        .unwrap()
                        .push(format!("POST /v1/billing_portal/sessions {body}"));
                    axum::Json(json!({ "url": "https://billing.stripe.test/p" }))
                }
            }),
        )
        .route(
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
    (format!("http://{}", address), forms, calls)
}

struct Site {
    app: axum::Router,
    state: Arc<AppState>,
    forms: Arc<Mutex<Vec<String>>>,
    calls: Arc<Mutex<Vec<String>>>,
    admin_token: String,
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
    let (api_base, forms, calls) = mock_stripe().await;
    let mut config = AegisConfig::default();
    config.members.enabled = true;
    config.members.levels = vec!["free".into(), "paid".into()];
    config.members.public_url = "https://example.com".to_string();
    config.members.purchase.enabled = purchase;
    config.members.purchase.currency = "jpy".to_string();
    config.members.purchase.stripe.secret_key = "sk_test_mock".to_string();
    config.members.purchase.stripe.webhook_secret = WEBHOOK_SECRET.to_string();
    config.members.purchase.stripe.api_base = api_base;
    config.members.purchase.plans = vec![
        aegis_members::PlanConfig {
            id: "monthly".to_string(),
            label: "Monthly".to_string(),
            level: "paid".to_string(),
            amount: 800,
            interval: aegis_members::PlanInterval::Month,
            interval_count: 1,
        },
        aegis_members::PlanConfig {
            id: "quarterly".to_string(),
            label: String::new(),
            level: "paid".to_string(),
            amount: 2200,
            interval: aegis_members::PlanInterval::Month,
            interval_count: 3,
        },
    ];
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
    let admin = state
        .sessions
        .create_session("boss", "admin", Default::default(), None)
        .await
        .unwrap();
    Site {
        app: router(&config, state.clone()),
        state,
        forms,
        calls,
        admin_token: admin.id,
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

// --- サブスク（docs/members.md 16） ---

fn subscription_checkout_event(member_id: &str, plan: &str, amount: i64, stripe_id: &str) -> Value {
    json!({
        "id": "evt_sub_checkout",
        "type": "checkout.session.completed",
        "data": { "object": {
            "id": "cs_sub_1",
            "mode": "subscription",
            "client_reference_id": member_id,
            "metadata": { "member_id": member_id, "plan_id": plan },
            "subscription": stripe_id,
            "customer": "cus_1",
            "payment_status": "paid",
            "amount_total": amount,
            "currency": "jpy"
        }}
    })
}

fn subscription_event(kind: &str, stripe_id: &str, status: &str, cancel_at_end: bool) -> Value {
    json!({
        "id": format!("evt_{kind}"),
        "type": kind,
        "data": { "object": {
            "id": stripe_id,
            "status": status,
            "customer": "cus_1",
            "cancel_at_period_end": cancel_at_end,
            "current_period_end": chrono::Utc::now().timestamp() + 60 * 86_400
        }}
    })
}

impl Site {
    async fn member_id(&self, email: &str) -> String {
        self.state
            .db
            .get_member_by_email(email)
            .await
            .unwrap()
            .unwrap()
            .id
    }

    /// 会員を作り、契約の開始の通知まで済ませる
    async fn subscribed_member(&self, email: &str, stripe_id: &str) -> String {
        let token = self.member(email, "free").await;
        let id = self.member_id(email).await;
        let reply = self
            .webhook(
                subscription_checkout_event(&id, "monthly", 800, stripe_id),
                WEBHOOK_SECRET,
            )
            .await;
        assert_eq!(reply.status, StatusCode::OK, "{}", reply.body);
        token
    }

    async fn admin_post(&self, path: &str, body: Value) -> Reply {
        self.post_json(path, body, Some(&self.admin_token)).await
    }
}

#[tokio::test]
async fn test_plans_are_listed_and_a_subscription_starts_at_the_plan_price() {
    let site = site(true).await;
    let plans = site.get("/api/v1/members/plans", None).await;
    assert_eq!(plans.status, StatusCode::OK);
    let plans = plans.json();
    assert_eq!(plans["plans"][0]["id"], "monthly");
    assert_eq!(plans["plans"][0]["amount"], 800);
    assert_eq!(
        plans["plans"][1]["label"], "quarterly",
        "the id when no label"
    );
    assert_eq!(plans["plans"][1]["interval_count"], 3);

    let token = site.member("a@example.com", "free").await;
    async fn start(site: &Site, plan: &str, bearer: Option<&str>) -> Reply {
        site.post_json(
            "/api/v1/members/subscriptions",
            json!({ "plan_id": plan }),
            bearer,
        )
        .await
    }
    assert_eq!(
        start(&site, "monthly", None).await.status,
        StatusCode::UNAUTHORIZED
    );
    let unknown = start(&site, "nope", Some(&token)).await;
    assert_eq!(unknown.status, StatusCode::NOT_FOUND);
    assert_eq!(unknown.json()["code"], "unknown_plan");

    let reply = start(&site, "quarterly", Some(&token)).await;
    assert_eq!(reply.status, StatusCode::CREATED, "{}", reply.body);
    assert_eq!(
        reply.json()["redirect_url"],
        "https://checkout.stripe.test/cs_test_1"
    );
    let form = site.forms.lock().unwrap()[0].clone();
    for expected in [
        "mode=subscription",
        "line_items%5B0%5D%5Bprice_data%5D%5Bunit_amount%5D=2200",
        "line_items%5B0%5D%5Bprice_data%5D%5Brecurring%5D%5Binterval%5D=month",
        "line_items%5B0%5D%5Bprice_data%5D%5Brecurring%5D%5Binterval_count%5D=3",
        "metadata%5Bplan_id%5D=quarterly",
        "success_url=https%3A%2F%2Fexample.com%2Faccount%3Fsubscribed%3D1",
    ] {
        assert!(form.contains(expected), "{expected} in {form}");
    }

    // 契約の通知が来る前は、レベルは上がらない
    let mine = site
        .get("/api/v1/members/subscriptions", Some(&token))
        .await
        .json();
    assert_eq!(mine["level"], "free");
    assert!(mine["subscriptions"].as_array().unwrap().is_empty());

    // ブラウザのリンクは、ログインのあとに戻り、ログイン済みなら決済ページへ
    let guest = site.get("/account/subscribe?plan=monthly", None).await;
    assert_eq!(guest.status, StatusCode::SEE_OTHER);
    assert!(guest.location.unwrap().starts_with("/account/login?next="));
    let member = site
        .get("/account/subscribe?plan=monthly", Some(&token))
        .await;
    assert_eq!(member.status, StatusCode::SEE_OTHER);
    assert_eq!(
        member.location.as_deref(),
        Some("https://checkout.stripe.test/cs_test_2")
    );
}

#[tokio::test]
async fn test_a_subscription_grants_the_level_until_it_ends() {
    let site = site(true).await;
    site.publish("secret", Some("paid"), None).await;
    let other = site.member("other@example.com", "free").await;
    let token = site.member("a@example.com", "free").await;
    let id = site.member_id("a@example.com").await;
    assert!(!site
        .get("/articles/secret", Some(&token))
        .await
        .body
        .contains(SECRET));

    // 金額がプランと合わない・知らないプラン・知らない会員では、契約にならない
    for event in [
        subscription_checkout_event(&id, "monthly", 1, "sub_1"),
        subscription_checkout_event(&id, "gold", 800, "sub_1"),
        subscription_checkout_event("nobody", "monthly", 800, "sub_1"),
    ] {
        assert_eq!(
            site.webhook(event, WEBHOOK_SECRET).await.status,
            StatusCode::OK
        );
    }
    assert!(site
        .state
        .db
        .get_subscription_by_stripe_id("sub_1")
        .await
        .unwrap()
        .is_none());
    // 偽の署名
    let forged = site
        .webhook(
            subscription_checkout_event(&id, "monthly", 800, "sub_1"),
            "whsec_forged",
        )
        .await;
    assert_eq!(forged.status, StatusCode::BAD_REQUEST);

    // 開始（再送されても 1 件）
    for _ in 0..2 {
        site.webhook(
            subscription_checkout_event(&id, "monthly", 800, "sub_1"),
            WEBHOOK_SECRET,
        )
        .await;
    }
    let subs = site.state.db.list_member_subscriptions(&id).await.unwrap();
    assert_eq!(subs.len(), 1);
    assert_eq!(
        (subs[0].status.as_str(), subs[0].level.as_str()),
        ("active", "paid")
    );
    assert_eq!(subs[0].stripe_customer_id, "cus_1");

    assert!(site
        .get("/articles/secret", Some(&token))
        .await
        .body
        .contains(SECRET));
    assert!(!site
        .get("/articles/secret", Some(&other))
        .await
        .body
        .contains(SECRET));
    let mine = site
        .get("/api/v1/members/subscriptions", Some(&token))
        .await
        .json();
    assert_eq!(mine["level"], "paid");
    assert_eq!(mine["subscriptions"][0]["plan_label"], "Monthly");
    assert_eq!(mine["subscriptions"][0]["active"], true);

    // もう一度は契約できない
    let again = site
        .post_json(
            "/api/v1/members/subscriptions",
            json!({ "plan_id": "monthly" }),
            Some(&token),
        )
        .await;
    assert_eq!(again.status, StatusCode::CONFLICT);
    assert_eq!(again.json()["code"], "already_subscribed");

    // 解約の予約と、支払いの失敗では、期間の終わりまで読める
    site.webhook(
        subscription_event("customer.subscription.updated", "sub_1", "active", true),
        WEBHOOK_SECRET,
    )
    .await;
    assert!(
        site.state
            .db
            .get_subscription_by_stripe_id("sub_1")
            .await
            .unwrap()
            .unwrap()
            .cancel_at_period_end
    );
    site.webhook(
        subscription_event("customer.subscription.updated", "sub_1", "past_due", true),
        WEBHOOK_SECRET,
    )
    .await;
    assert_eq!(
        site.state
            .db
            .get_subscription_by_stripe_id("sub_1")
            .await
            .unwrap()
            .unwrap()
            .status,
        "past_due"
    );
    assert!(site
        .get("/articles/secret", Some(&token))
        .await
        .body
        .contains(SECRET));

    // 解約されたら読めない
    site.webhook(
        subscription_event("customer.subscription.deleted", "sub_1", "canceled", false),
        WEBHOOK_SECRET,
    )
    .await;
    assert_eq!(
        site.state
            .db
            .get_subscription_by_stripe_id("sub_1")
            .await
            .unwrap()
            .unwrap()
            .status,
        "cancelled"
    );
    assert!(!site
        .get("/articles/secret", Some(&token))
        .await
        .body
        .contains(SECRET));
    // 知らない契約の通知は何もしない
    let unknown = site
        .webhook(
            subscription_event("customer.subscription.deleted", "sub_x", "canceled", false),
            WEBHOOK_SECRET,
        )
        .await;
    assert_eq!(unknown.status, StatusCode::OK);
    // 解約のあとは、また契約できる（同じ Stripe の顧客に結びつく）
    let restart = site
        .post_json(
            "/api/v1/members/subscriptions",
            json!({ "plan_id": "monthly" }),
            Some(&token),
        )
        .await;
    assert_eq!(restart.status, StatusCode::CREATED);
    let form = site.forms.lock().unwrap().last().unwrap().clone();
    assert!(form.contains("customer=cus_1"), "{form}");
    assert!(!form.contains("customer_email"));
}

#[tokio::test]
async fn test_the_billing_portal_needs_a_subscription() {
    let site = site(true).await;
    let token = site.member("a@example.com", "free").await;
    let none = site
        .post_json("/api/v1/members/billing-portal", json!({}), Some(&token))
        .await;
    assert_eq!(none.status, StatusCode::NOT_FOUND);
    assert_eq!(none.json()["code"], "no_subscription");

    let subscriber = site.subscribed_member("sub@example.com", "sub_2").await;
    let portal = site
        .post_json(
            "/api/v1/members/billing-portal",
            json!({}),
            Some(&subscriber),
        )
        .await;
    assert_eq!(portal.status, StatusCode::OK, "{}", portal.body);
    assert_eq!(portal.json()["url"], "https://billing.stripe.test/p");
    assert!(site
        .calls
        .lock()
        .unwrap()
        .iter()
        .any(|c| c.contains("customer=cus_1")));
    let link = site.get("/account/billing", Some(&subscriber)).await;
    assert_eq!(
        link.location.as_deref(),
        Some("https://billing.stripe.test/p")
    );
}

#[tokio::test]
async fn test_leaving_cancels_the_subscription_first() {
    let site = site(true).await;
    let token = site.subscribed_member("a@example.com", "sub_3").await;
    let id = site.member_id("a@example.com").await;
    let leave = || {
        site.send(
            Request::delete("/api/v1/members/me")
                .header("content-type", "application/json")
                .header("authorization", format!("Bearer {token}"))
                .body(Body::from(json!({ "password": "password123" }).to_string()))
                .unwrap(),
        )
    };
    let reply = leave().await;
    assert_eq!(reply.status, StatusCode::NO_CONTENT, "{}", reply.body);
    assert!(site
        .calls
        .lock()
        .unwrap()
        .contains(&"DELETE /v1/subscriptions/sub_3".to_string()));
    let kept = site
        .state
        .db
        .get_subscription_by_stripe_id("sub_3")
        .await
        .unwrap()
        .unwrap();
    assert_eq!((kept.status.as_str(), kept.member_id), ("cancelled", None));
    assert!(site
        .state
        .db
        .list_member_subscriptions(&id)
        .await
        .unwrap()
        .is_empty());
}

#[tokio::test]
async fn test_admins_grant_revoke_refund_and_cancel() {
    let site = site(true).await;
    site.publish("secret", Some("paid"), Some(500)).await;
    let token = site.member("a@example.com", "free").await;
    let id = site.member_id("a@example.com").await;
    let grant_path = format!("/api/v1/admin/members/{id}/purchases");
    let grant = || async {
        site.admin_post(
            &grant_path,
            json!({ "content_type": "article", "slug": "secret" }),
        )
        .await
    };

    // 会員のトークンでは操作できない
    let denied = site
        .post_json(
            &format!("/api/v1/admin/members/{id}/purchases"),
            json!({ "content_type": "article", "slug": "secret" }),
            Some(&token),
        )
        .await;
    assert!(denied.status.is_client_error());

    // 無料で付ける → 読める。二重には付けない
    let granted = grant().await;
    assert_eq!(granted.status, StatusCode::CREATED, "{}", granted.body);
    let purchase_id = granted.json()["purchase_id"].as_str().unwrap().to_string();
    assert!(site
        .get("/articles/secret", Some(&token))
        .await
        .body
        .contains(SECRET));
    assert_eq!(grant().await.status, StatusCode::CONFLICT);
    let detail = site
        .get(
            &format!("/api/v1/admin/members/{id}"),
            Some(&site.admin_token),
        )
        .await
        .json();
    assert_eq!(detail["purchases"][0]["manual"], true);
    assert_eq!(detail["purchases"][0]["amount"], 0);

    // 決済のない権利は、返金できず、取り消せる
    let refund_manual = site
        .admin_post(
            &format!("/api/v1/admin/purchases/{purchase_id}/refund"),
            json!({}),
        )
        .await;
    assert_eq!(refund_manual.status, StatusCode::CONFLICT);
    let revoke = site
        .admin_post(
            &format!("/api/v1/admin/purchases/{purchase_id}/revoke"),
            json!({}),
        )
        .await;
    assert_eq!(revoke.status, StatusCode::NO_CONTENT);
    assert!(!site
        .get("/articles/secret", Some(&token))
        .await
        .body
        .contains(SECRET));
    let twice = site
        .admin_post(
            &format!("/api/v1/admin/purchases/{purchase_id}/revoke"),
            json!({}),
        )
        .await;
    assert_eq!(twice.status, StatusCode::CONFLICT);

    // 払った購入は、Stripe で返金して外す
    site.buy("secret", &token).await;
    site.webhook(
        session_event("checkout.session.completed", "cs_test_1", 500),
        WEBHOOK_SECRET,
    )
    .await;
    let paid = site.purchase("cs_test_1").await;
    assert!(site
        .get("/articles/secret", Some(&token))
        .await
        .body
        .contains(SECRET));
    let refund = site
        .admin_post(
            &format!("/api/v1/admin/purchases/{}/refund", paid.id),
            json!({}),
        )
        .await;
    assert_eq!(refund.status, StatusCode::NO_CONTENT, "{}", refund.body);
    let call = site
        .calls
        .lock()
        .unwrap()
        .iter()
        .find(|c| c.contains("/v1/refunds"))
        .cloned()
        .unwrap();
    assert!(
        call.contains("payment_intent=pi_test_1") && call.contains("amount=500"),
        "{call}"
    );
    assert_eq!(site.purchase("cs_test_1").await.status, "refunded");
    assert!(!site
        .get("/articles/secret", Some(&token))
        .await
        .body
        .contains(SECRET));

    // 契約の解約
    let subscriber = site.subscribed_member("sub@example.com", "sub_9").await;
    let sub = site
        .state
        .db
        .get_subscription_by_stripe_id("sub_9")
        .await
        .unwrap()
        .unwrap();
    let cancel = site
        .admin_post(
            &format!("/api/v1/admin/subscriptions/{}/cancel", sub.id),
            json!({}),
        )
        .await;
    assert_eq!(cancel.status, StatusCode::NO_CONTENT, "{}", cancel.body);
    assert_eq!(
        site.state
            .db
            .get_subscription(&sub.id)
            .await
            .unwrap()
            .unwrap()
            .status,
        "cancelled"
    );
    let mine = site
        .get("/api/v1/members/subscriptions", Some(&subscriber))
        .await
        .json();
    assert_eq!(mine["level"], "free");
}
