//! 会員アカウント・商品レビューの HTTP 統合テスト（ルーター経由）

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
    config.members.enabled = true;
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
    // サーバーと同じく、設定（[members]）から作る
    let members = aegis_members::MemberService::from_config(
        &config.members,
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
    let state = Arc::new(match members {
        Some(members) => state.with_members(members),
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
async fn test_member_lifecycle_and_reviews_e2e() {
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

    // 1. 会員アカウント登録 (POST /api/v1/members/register)
    let register_req = Request::builder()
        .method("POST")
        .uri("/api/v1/members/register")
        .header("content-type", "application/json")
        .body(Body::from(
            json!({
                "email": "member@example.com",
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
    let member_id = body["member"]["id"].as_str().unwrap().to_string();

    // 2. 会員ログイン (POST /api/v1/members/login)
    let login_req = Request::builder()
        .method("POST")
        .uri("/api/v1/members/login")
        .header("content-type", "application/json")
        .body(Body::from(
            json!({
                "email": "member@example.com",
                "password": "SecretPassword123"
            })
            .to_string(),
        ))
        .unwrap();

    let res = ctx.app.clone().oneshot(login_req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let body = json_response(res).await;
    let member_token = body["token"].as_str().unwrap().to_string();
    assert!(member_token.starts_with("mses_"));

    // 3. 会員プロフィール取得 (GET /api/v1/members/me)
    let me_req = Request::builder()
        .method("GET")
        .uri("/api/v1/members/me")
        .header("authorization", format!("Bearer {}", member_token))
        .body(Body::empty())
        .unwrap();

    let res = ctx.app.clone().oneshot(me_req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let me = json_response(res).await;
    assert_eq!(me["email"], "member@example.com");
    assert_eq!(me["name"], "Taro Yamada");
    assert!(me["nickname"].is_null());

    // 3b. 公開名を決める (PUT /api/v1/members/me)。本名は変わらない。長すぎる名前は 400
    let put_me = |body: Value| {
        Request::builder()
            .method("PUT")
            .uri("/api/v1/members/me")
            .header("authorization", format!("Bearer {}", member_token))
            .header("content-type", "application/json")
            .body(Body::from(body.to_string()))
            .unwrap()
    };
    let res = ctx
        .app
        .clone()
        .oneshot(put_me(json!({ "nickname": "x".repeat(31) })))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);
    let res = ctx
        .app
        .clone()
        .oneshot(put_me(json!({ "nickname": "taro" })))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let updated = json_response(res).await;
    assert_eq!(updated["nickname"], "taro");
    assert_eq!(updated["name"], "Taro Yamada");

    // 4. カートチェックアウト (POST /api/v1/shop/checkout) — 会員トークンで注文作成
    let checkout_req = Request::builder()
        .method("POST")
        .uri("/api/v1/shop/checkout")
        .header("authorization", format!("Bearer {}", member_token))
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

    // 5. 会員の注文履歴 (GET /api/v1/members/orders)
    let orders_req = Request::builder()
        .method("GET")
        .uri("/api/v1/members/orders")
        .header("authorization", format!("Bearer {}", member_token))
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
        .header("authorization", format!("Bearer {}", member_token))
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
    // 公開されたか承認待ちかを、応答で伝える（レビュー欄の「ありがとうございます」の文を決める）
    assert!(matches!(
        rev_body["status"].as_str(),
        Some("approved" | "pending")
    ));
    let review_id = rev_body["review"]["id"].as_str().unwrap().to_string();

    // 7. 同一注文・同一商品への二重投稿の拒否 (1注文1件制約)
    let dup_review_req = Request::builder()
        .method("POST")
        .uri("/api/v1/products/prod_kb_01/reviews")
        .header("authorization", format!("Bearer {}", member_token))
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
        .header("authorization", format!("Bearer {}", member_token))
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
    // 公開の場には、本名ではなく公開名が出る
    assert_eq!(pub_reviews["reviews"][0]["author_name"], "taro");
    assert_eq!(pub_reviews["stats"]["review_count"], 1);
    assert_eq!(pub_reviews["stats"]["average_rating"], 5.0);
    assert_eq!(pub_reviews["json_ld"]["@type"], "AggregateRating");
    // schema.org のプロパティ名（camelCase）
    assert_eq!(pub_reviews["json_ld"]["ratingValue"], 5.0);
    assert_eq!(pub_reviews["json_ld"]["reviewCount"], 1);

    // 10. 管理者: 会員一覧・詳細取得 (GET /api/v1/admin/members)
    let admin_members_req = Request::builder()
        .method("GET")
        .uri("/api/v1/admin/members")
        .header("authorization", format!("Bearer {}", ctx.admin_token))
        .body(Body::empty())
        .unwrap();

    let res = ctx.app.clone().oneshot(admin_members_req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let admin_cust_body = json_response(res).await;
    assert_eq!(admin_cust_body["total"], 1);
    assert_eq!(admin_cust_body["members"][0]["id"], member_id);
    assert!(admin_cust_body["members"][0].get("password_hash").is_none());

    // 詳細: 注文は一覧に要る項目だけ（トークンのハッシュなどを載せない）
    let detail_req = Request::builder()
        .method("GET")
        .uri(format!("/api/v1/admin/members/{}", member_id))
        .header("authorization", format!("Bearer {}", ctx.admin_token))
        .body(Body::empty())
        .unwrap();
    let res = ctx.app.clone().oneshot(detail_req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let detail = json_response(res).await;
    assert_eq!(detail["member"]["id"], member_id);
    assert!(detail["member"].get("password_hash").is_none());
    for order in detail["orders"].as_array().unwrap() {
        let mut keys: Vec<&String> = order.as_object().unwrap().keys().collect();
        keys.sort();
        assert_eq!(
            keys,
            ["created_at", "currency", "id", "number", "status", "total"]
        );
    }

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

    // 会員がアバターを設定すると、過去のレビューにも出る（表示のたびに今の値を引く）。会員 ID は応答に載せない
    assert!(pub_reviews2["reviews"][0]["avatar_url"].is_null());
    assert!(pub_reviews2["reviews"][0].get("member_id").is_none());
    let (status, _) = put_avatar(&ctx, &member_token, b"\x89PNG\r\n\x1a\n-pixels").await;
    assert_eq!(status, StatusCode::OK);
    let res = ctx
        .app
        .clone()
        .oneshot(
            Request::get("/api/v1/products/prod_kb_01/reviews")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let with_avatar = json_response(res).await;
    let url = with_avatar["reviews"][0]["avatar_url"].as_str().unwrap();
    assert!(url.contains(&member_id), "{url}");
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
    json!({ "email": email, "password": "password123", "name": "Test Member" })
}

/// `[members].enabled = false`: 会員の API は 404（レビューなど、ほかの EC の機能は動く）
#[tokio::test]
async fn test_member_api_is_off_when_disabled() {
    let ctx = setup_app_with(|config| config.members.enabled = false).await;
    for path in ["/api/v1/members/register", "/api/v1/members/login"] {
        let res = post_json(&ctx, path, registration("off@example.com")).await;
        assert_eq!(res.status(), StatusCode::NOT_FOUND, "{path}");
        let body = json_response(res).await;
        assert!(body["error"].as_str().unwrap().contains("not enabled"));
    }
}

/// `registration = "closed"`: 新規の登録は 403。登録済みの会員はログインできる
#[tokio::test]
async fn test_closed_registration_over_http() {
    // 開いた設定では登録できる
    let open = setup_app().await;
    let res = post_json(
        &open,
        "/api/v1/members/register",
        registration("a@example.com"),
    )
    .await;
    assert_eq!(res.status(), StatusCode::CREATED);

    let closed = setup_app_with(|config| {
        config.members.registration = aegis_members::MemberRegistration::Closed;
    })
    .await;
    let res = post_json(
        &closed,
        "/api/v1/members/register",
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

async fn put_avatar(ctx: &TestContext, token: &str, file: &[u8]) -> (StatusCode, Value) {
    let boundary = "aegis-test-boundary";
    let mut body = format!(
        "--{b}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"me.png\"\r\n\
         Content-Type: image/png\r\n\r\n",
        b = boundary
    )
    .into_bytes();
    body.extend_from_slice(file);
    body.extend_from_slice(format!("\r\n--{}--\r\n", boundary).as_bytes());
    let res = ctx
        .app
        .clone()
        .oneshot(
            Request::put("/api/v1/members/me/avatar")
                .header("authorization", format!("Bearer {}", token))
                .header(
                    "content-type",
                    format!("multipart/form-data; boundary={}", boundary),
                )
                .body(Body::from(body))
                .unwrap(),
        )
        .await
        .unwrap();
    let status = res.status();
    (status, json_response(res).await)
}

/// 会員は自分のアバター画像を設定・差し替え・外せる。管理画面の会員詳細にも URL が出る
#[tokio::test]
async fn test_member_avatar_over_http() {
    let ctx = setup_app().await;
    let res = post_json(
        &ctx,
        "/api/v1/members/register",
        registration("avatar@example.com"),
    )
    .await;
    assert_eq!(res.status(), StatusCode::CREATED);
    let member_id = json_response(res).await["member"]["id"]
        .as_str()
        .unwrap()
        .to_string();
    let res = post_json(
        &ctx,
        "/api/v1/members/login",
        json!({ "email": "avatar@example.com", "password": "password123" }),
    )
    .await;
    let token = json_response(res).await["token"]
        .as_str()
        .unwrap()
        .to_string();
    let png = b"\x89PNG\r\n\x1a\n-not-really-pixels";

    let (status, first) = put_avatar(&ctx, &token, png).await;
    assert_eq!(status, StatusCode::OK, "{first}");
    let first_url = first["avatar_url"].as_str().unwrap().to_string();
    assert!(first_url.starts_with("/uploads/avatars/"), "{first_url}");
    assert!(first_url.contains(&member_id));
    assert!(first.get("avatar_key").is_none());

    let res = ctx
        .app
        .clone()
        .oneshot(
            Request::get("/api/v1/members/me")
                .header("authorization", format!("Bearer {}", token))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(json_response(res).await["avatar_url"], first_url);

    // 差し替えるとキーが変わる。SVG は受け付けない
    let (_, second) = put_avatar(&ctx, &token, png).await;
    assert_ne!(second["avatar_url"].as_str().unwrap(), first_url);
    let (status, _) = put_avatar(&ctx, &token, b"<svg/>").await;
    assert_eq!(status, StatusCode::UNSUPPORTED_MEDIA_TYPE);

    // 管理画面の会員詳細にも出る
    let res = ctx
        .app
        .clone()
        .oneshot(
            Request::get(format!("/api/v1/admin/members/{member_id}"))
                .header("authorization", format!("Bearer {}", ctx.admin_token))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    assert_eq!(
        json_response(res).await["member"]["avatar_url"],
        second["avatar_url"]
    );

    let res = ctx
        .app
        .clone()
        .oneshot(
            Request::delete("/api/v1/members/me/avatar")
                .header("authorization", format!("Bearer {}", token))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    assert!(json_response(res).await["avatar_url"].is_null());

    // ログインしていなければ 401
    let (status, _) = put_avatar(&ctx, "mses_invalid", png).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}
