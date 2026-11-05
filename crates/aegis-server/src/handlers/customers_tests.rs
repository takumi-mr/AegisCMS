//! 顧客アカウント・商品レビューの HTTP 統合テスト（ルーター経由）

use std::sync::Arc;

use aegis_core::OrderStatus;
use aegis_db::{ContentRecord, DatabaseAdapter};
use aegis_runtime::PluginSupervisor;
use aegis_schema::SchemaDefinition;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use serde_json::{json, Value};
use tower::ServiceExt;

use crate::app::router;
use crate::config::AegisConfig;
use crate::test_support::test_app_state;

fn product_schema() -> SchemaDefinition {
    serde_json::from_value(json!({
        "content_type": "product",
        "label": "Products",
        "fields": [
            { "name": "title", "type": "string", "required": true },
            { "name": "price", "type": "integer", "required": true },
            { "name": "stock", "type": "integer", "nullable": true }
        ]
    }))
    .unwrap()
}

struct TestContext {
    app: axum::Router,
    db: Arc<dyn DatabaseAdapter>,
    admin_token: String,
}

async fn setup_app() -> TestContext {
    setup_app_with(|_| {}).await
}

async fn setup_app_with(configure: impl FnOnce(&mut AegisConfig)) -> TestContext {
    let mut config = AegisConfig::default();
    configure(&mut config);
    config.commerce.enabled = true;
    config.commerce.product_type = "product".to_string();
    config.comments.moderation.require_approval = false; // 自動承認

    let state = test_app_state(
        config.clone(),
        PluginSupervisor::new(),
        vec![product_schema()],
    )
    .await;

    // Commerce サービス初期化
    let commerce =
        aegis_commerce::CommerceService::new(config.commerce.clone(), state.commerce_deps())
            .unwrap();
    // サーバーと同じく、設定（[commerce.customers]）から作る
    let customers = aegis_commerce::CustomerService::from_config(
        &config.commerce.customers,
        state.db.clone(),
        state.cache.clone(),
        state.audit.clone(),
    );
    let reviews = aegis_commerce::ReviewService::new(
        state.db.clone(),
        state.audit.clone(),
        state.notifier.clone(),
        config.comments.moderation.clone(),
    );

    let state = state.with_commerce(commerce).with_reviews(reviews);
    let state = Arc::new(match customers {
        Some(customers) => state.with_customers(customers),
        None => state,
    });

    let db = state.db.clone();

    // 管理者セッショントークンを取得
    let admin_session = state
        .sessions
        .create_session("admin", "admin", Default::default(), None)
        .await
        .unwrap();

    let app = router(&config, state);

    TestContext {
        app,
        db,
        admin_token: admin_session.id,
    }
}

async fn json_response(res: axum::response::Response) -> Value {
    let bytes = axum::body::to_bytes(res.into_body(), usize::MAX)
        .await
        .unwrap();
    serde_json::from_slice(&bytes).unwrap_or(Value::Null)
}

#[tokio::test]
async fn test_customer_lifecycle_and_reviews_e2e() {
    let ctx = setup_app().await;

    // 商品コンテンツを挿入
    let product_schema = product_schema();
    ctx.db
        .insert_content(
            &product_schema,
            &ContentRecord {
                id: "prod_kb_01".to_string(),
                content_type: "product".to_string(),
                title: "Ergonomic Mechanical Keyboard".to_string(),
                slug: "mech-keyboard".to_string(),
                status: "published".to_string(),
                blocks_json: "[]".to_string(),
                author_id: None,
                published_at: None,
                unpublish_at: None,
                created_at: "2026-10-25T10:00:00Z".to_string(),
                updated_at: "2026-10-25T10:00:00Z".to_string(),
                custom_fields: std::collections::HashMap::from([
                    ("price".to_string(), json!(12000)),
                    ("stock".to_string(), json!(50)),
                ]),
            },
        )
        .await
        .unwrap();

    // 1. 顧客アカウント登録 (POST /api/v1/customers/register)
    let register_req = Request::builder()
        .method("POST")
        .uri("/api/v1/customers/register")
        .header("content-type", "application/json")
        .body(Body::from(
            json!({
                "email": "customer@example.com",
                "password": "SecretPassword123",
                "name": "Taro Yamada",
                "phone": "090-1111-2222"
            })
            .to_string(),
        ))
        .unwrap();

    let res = ctx.app.clone().oneshot(register_req).await.unwrap();
    assert_eq!(res.status(), StatusCode::CREATED);
    let body = json_response(res).await;
    let customer_id = body["customer"]["id"].as_str().unwrap().to_string();

    // 2. 顧客ログイン (POST /api/v1/customers/login)
    let login_req = Request::builder()
        .method("POST")
        .uri("/api/v1/customers/login")
        .header("content-type", "application/json")
        .body(Body::from(
            json!({
                "email": "customer@example.com",
                "password": "SecretPassword123"
            })
            .to_string(),
        ))
        .unwrap();

    let res = ctx.app.clone().oneshot(login_req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let body = json_response(res).await;
    let customer_token = body["token"].as_str().unwrap().to_string();
    assert!(customer_token.starts_with("cses_"));

    // 3. 顧客プロフィール取得 (GET /api/v1/customers/me)
    let me_req = Request::builder()
        .method("GET")
        .uri("/api/v1/customers/me")
        .header("authorization", format!("Bearer {}", customer_token))
        .body(Body::empty())
        .unwrap();

    let res = ctx.app.clone().oneshot(me_req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let me = json_response(res).await;
    assert_eq!(me["email"], "customer@example.com");
    assert_eq!(me["name"], "Taro Yamada");

    // 4. カートチェックアウト (POST /api/v1/shop/checkout) — 顧客トークンで注文作成
    let checkout_req = Request::builder()
        .method("POST")
        .uri("/api/v1/shop/checkout")
        .header("authorization", format!("Bearer {}", customer_token))
        .header("content-type", "application/json")
        .body(Body::from(
            json!({
                "items": [
                    { "product": "mech-keyboard", "quantity": 1 }
                ],
                "shipping_address": {
                    "name": "Taro Yamada",
                    "postal_code": "100-0001",
                    "region": "Tokyo",
                    "city": "Chiyoda",
                    "line1": "1-1-1",
                    "line2": "",
                    "phone": "090-1111-2222",
                    "country": "JP"
                }
            })
            .to_string(),
        ))
        .unwrap();

    let res = ctx.app.clone().oneshot(checkout_req).await.unwrap();
    assert_eq!(res.status(), StatusCode::CREATED);
    let order_body = json_response(res).await;
    let order_id = order_body["order"]["id"].as_str().unwrap().to_string();

    // 5. 顧客の注文履歴 (GET /api/v1/customers/orders)
    let orders_req = Request::builder()
        .method("GET")
        .uri("/api/v1/customers/orders")
        .header("authorization", format!("Bearer {}", customer_token))
        .body(Body::empty())
        .unwrap();

    let res = ctx.app.clone().oneshot(orders_req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let orders = json_response(res).await;
    assert_eq!(orders["orders"].as_array().unwrap().len(), 1);
    assert_eq!(orders["orders"][0]["id"], order_id);

    // 注文ステータスを完了済みに更新
    ctx.db
        .transition_order(&aegis_db::OrderTransition {
            id: order_id.clone(),
            from_status: OrderStatus::PendingPayment.as_str().to_string(),
            to_status: OrderStatus::Completed.as_str().to_string(),
            paid_at: Some("2026-10-25T11:00:00Z".to_string()),
            restock: Vec::new(),
            note: None,
            updated_at: "2026-10-25T11:00:00Z".to_string(),
        })
        .await
        .unwrap();

    // 6. 購入者レビュー投稿 (POST /api/v1/products/:product_id/reviews)
    let review_req = Request::builder()
        .method("POST")
        .uri("/api/v1/products/prod_kb_01/reviews")
        .header("authorization", format!("Bearer {}", customer_token))
        .header("content-type", "application/json")
        .body(Body::from(
            json!({
                "order_id": order_id,
                "rating": 5,
                "title": "Best keyboard ever",
                "body": "Keys feel fantastic, very satisfied."
            })
            .to_string(),
        ))
        .unwrap();

    let res = ctx.app.clone().oneshot(review_req).await.unwrap();
    assert_eq!(res.status(), StatusCode::CREATED);
    let rev_body = json_response(res).await;
    assert_eq!(rev_body["review"]["rating"], 5);
    let review_id = rev_body["review"]["id"].as_str().unwrap().to_string();

    // 7. 同一注文・同一商品への二重投稿の拒否 (1注文1件制約)
    let dup_review_req = Request::builder()
        .method("POST")
        .uri("/api/v1/products/prod_kb_01/reviews")
        .header("authorization", format!("Bearer {}", customer_token))
        .header("content-type", "application/json")
        .body(Body::from(
            json!({
                "order_id": order_id,
                "rating": 4,
                "title": "Another review",
                "body": "Trying to review again."
            })
            .to_string(),
        ))
        .unwrap();

    let res = ctx.app.clone().oneshot(dup_review_req).await.unwrap();
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);

    // 8. 未購入商品へのレビュー拒否
    let unpurchased_review_req = Request::builder()
        .method("POST")
        .uri("/api/v1/products/prod_unpurchased/reviews")
        .header("authorization", format!("Bearer {}", customer_token))
        .header("content-type", "application/json")
        .body(Body::from(
            json!({
                "order_id": order_id,
                "rating": 1,
                "title": "Fake review",
                "body": "I didn't buy this."
            })
            .to_string(),
        ))
        .unwrap();

    let res = ctx
        .app
        .clone()
        .oneshot(unpurchased_review_req)
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::FORBIDDEN);

    // 9. 公開レビュー一覧と集計の取得 (GET /api/v1/products/prod_kb_01/reviews)
    let public_rev_req = Request::builder()
        .method("GET")
        .uri("/api/v1/products/prod_kb_01/reviews")
        .body(Body::empty())
        .unwrap();

    let res = ctx.app.clone().oneshot(public_rev_req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let pub_reviews = json_response(res).await;
    assert_eq!(pub_reviews["reviews"].as_array().unwrap().len(), 1);
    assert_eq!(pub_reviews["stats"]["review_count"], 1);
    assert_eq!(pub_reviews["stats"]["average_rating"], 5.0);
    assert_eq!(pub_reviews["json_ld"]["@type"], "AggregateRating");
    assert_eq!(pub_reviews["json_ld"]["rating_value"], 5.0);
    assert_eq!(pub_reviews["json_ld"]["review_count"], 1);

    // 10. 管理者: 顧客一覧・詳細取得 (GET /api/v1/admin/customers)
    let admin_customers_req = Request::builder()
        .method("GET")
        .uri("/api/v1/admin/customers")
        .header("authorization", format!("Bearer {}", ctx.admin_token))
        .body(Body::empty())
        .unwrap();

    let res = ctx.app.clone().oneshot(admin_customers_req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let admin_cust_body = json_response(res).await;
    assert_eq!(admin_cust_body["total"], 1);
    assert_eq!(admin_cust_body["customers"][0]["id"], customer_id);

    // 11. 管理者: レビュー一覧取得・返信・ステマ規制配慮非承認
    let admin_rev_req = Request::builder()
        .method("GET")
        .uri("/api/v1/admin/reviews")
        .header("authorization", format!("Bearer {}", ctx.admin_token))
        .body(Body::empty())
        .unwrap();

    let res = ctx.app.clone().oneshot(admin_rev_req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let admin_revs = json_response(res).await;
    assert_eq!(admin_revs["total"], 1);

    // 管理者からの公式返信 (POST /api/v1/admin/reviews/:id/reply)
    let reply_req = Request::builder()
        .method("POST")
        .uri(format!("/api/v1/admin/reviews/{}/reply", review_id))
        .header("authorization", format!("Bearer {}", ctx.admin_token))
        .header("content-type", "application/json")
        .body(Body::from(
            json!({
                "reply": "Thank you for purchasing our keyboard! We are delighted you enjoy it."
            })
            .to_string(),
        ))
        .unwrap();

    let res = ctx.app.clone().oneshot(reply_req).await.unwrap();
    assert_eq!(res.status(), StatusCode::NO_CONTENT);

    // 公開レビュー一覧に公式返信が反映されていることを確認
    let public_rev_req2 = Request::builder()
        .method("GET")
        .uri("/api/v1/products/prod_kb_01/reviews")
        .body(Body::empty())
        .unwrap();

    let res = ctx.app.clone().oneshot(public_rev_req2).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let pub_reviews2 = json_response(res).await;
    assert!(pub_reviews2["reviews"][0]["reply"].is_string());
}

async fn post_json(ctx: &TestContext, path: &str, body: Value) -> axum::response::Response {
    ctx.app
        .clone()
        .oneshot(
            Request::post(path)
                .header("content-type", "application/json")
                .body(Body::from(body.to_string()))
                .unwrap(),
        )
        .await
        .unwrap()
}

fn registration(email: &str) -> Value {
    json!({ "email": email, "password": "password123", "name": "Test Customer" })
}

/// `[commerce.customers].enabled = false`: 顧客の API は 404（レビューなど、ほかの EC の機能は動く）
#[tokio::test]
async fn test_customer_api_is_off_when_disabled() {
    let ctx = setup_app_with(|config| config.commerce.customers.enabled = false).await;
    for path in ["/api/v1/customers/register", "/api/v1/customers/login"] {
        let res = post_json(&ctx, path, registration("off@example.com")).await;
        assert_eq!(res.status(), StatusCode::NOT_FOUND, "{path}");
        let body = json_response(res).await;
        assert!(body["error"].as_str().unwrap().contains("not enabled"));
    }
}

/// `registration = "closed"`: 新規の登録は 403。登録済みの顧客はログインできる
#[tokio::test]
async fn test_closed_registration_over_http() {
    // 開いた設定では登録できる
    let open = setup_app().await;
    let res = post_json(
        &open,
        "/api/v1/customers/register",
        registration("a@example.com"),
    )
    .await;
    assert_eq!(res.status(), StatusCode::CREATED);

    let closed = setup_app_with(|config| {
        config.commerce.customers.registration = aegis_commerce::CustomerRegistration::Closed;
    })
    .await;
    let res = post_json(
        &closed,
        "/api/v1/customers/register",
        registration("b@example.com"),
    )
    .await;
    assert_eq!(res.status(), StatusCode::FORBIDDEN);
    let body = json_response(res).await;
    assert!(body["error"]
        .as_str()
        .unwrap()
        .contains("registration is closed"));
}
