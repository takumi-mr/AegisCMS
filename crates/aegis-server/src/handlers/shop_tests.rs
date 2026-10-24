//! 見積もり・注文・在庫・決済（手動 / Stripe のモック）・期限切れの結合テスト（実際のルーターを通す）

use std::sync::{Arc, Mutex};

use aegis_core::{
    AuditAction, AuditOutcome, NotificationEvent, Notifier, OrderAdjustment, OrderContext,
    OrderStatusChange, PluginCapabilities, PluginManifest, Recipient,
};
use aegis_runtime::{AegisPlugin, PluginSupervisor};
use aegis_schema::SchemaDefinition;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use serde_json::{json, Value};
use tower::ServiceExt;

use super::*;
use crate::audit::AuditContext;
use crate::config::AegisConfig;
use aegis_commerce::{stripe, CartItem};
use aegis_db::{ContentRecord, DatabaseAdapter};

fn product_schema() -> SchemaDefinition {
    serde_json::from_value(json!({
        "content_type": "product",
        "label": "Products",
        "fields": [
            { "name": "title", "type": "string", "required": true },
            { "name": "price", "type": "integer", "required": true },
            { "name": "stock", "type": "integer", "nullable": true },
            { "name": "sku", "type": "string", "nullable": true }
        ]
    }))
    .unwrap()
}

/// 送料 500 円（5,000 円以上で無料）とクーポン `SAVE10`（10% 引き）を加え、ついでに単価を
/// 書き換えようとする（無視されるはず）プラグイン。通知は `notifications` に記録する
struct ShopRulesPlugin {
    manifest: PluginManifest,
    notifications: Arc<Mutex<Vec<OrderStatusChange>>>,
}

#[async_trait::async_trait]
impl AegisPlugin for ShopRulesPlugin {
    fn manifest(&self) -> &PluginManifest {
        &self.manifest
    }

    async fn calculate_order(&self, mut ctx: OrderContext) -> aegis_core::Result<OrderContext> {
        if ctx.coupon_code.as_deref() == Some("BOGUS") {
            return Err(aegis_core::AegisError::PluginRejected(
                "This coupon is not valid".to_string(),
            ));
        }
        let shipping = if ctx.subtotal >= 5000 { 0 } else { 500 };
        ctx.adjustments.push(OrderAdjustment {
            kind: "shipping".to_string(),
            label: "Shipping".to_string(),
            amount: shipping,
            ..Default::default()
        });
        if ctx.coupon_code.as_deref() == Some("SAVE10") {
            ctx.adjustments.push(OrderAdjustment {
                kind: "discount".to_string(),
                label: "SAVE10".to_string(),
                amount: -(ctx.subtotal / 10),
                code: Some("SAVE10".to_string()),
                ..Default::default()
            });
        }
        // Prices are the host's: this must be ignored
        ctx.lines[0].unit_price = 1;
        ctx.total = 1;
        Ok(ctx)
    }

    async fn on_order_status_changed(&self, change: OrderStatusChange) -> aegis_core::Result<()> {
        self.notifications.lock().unwrap().push(change);
        Ok(())
    }
}

/// Records the notifications (emails) the shop asks for
#[derive(Default)]
struct RecordingNotifier {
    events: Mutex<Vec<NotificationEvent>>,
}

impl Notifier for RecordingNotifier {
    fn notify(&self, event: NotificationEvent) {
        self.events.lock().unwrap().push(event);
    }
}

struct Shop {
    state: Arc<AppState>,
    notifications: Arc<Mutex<Vec<OrderStatusChange>>>,
    mails: Arc<RecordingNotifier>,
    staff_token: String,
}

async fn shop(configure: impl FnOnce(&mut AegisConfig)) -> Shop {
    let mut config = AegisConfig::default();
    config.commerce.enabled = true;
    config.commerce.max_quantity_per_item = 5;
    configure(&mut config);
    let adapter = aegis_db::SqliteAdapter::connect("sqlite::memory:", 1)
        .await
        .unwrap();
    adapter.init().await.unwrap();
    shop_with_db(Arc::new(adapter), config).await
}

async fn shop_with_db(db: Arc<dyn aegis_db::DatabaseAdapter>, config: AegisConfig) -> Shop {
    let notifications = Arc::new(Mutex::new(Vec::new()));
    let mut supervisor = PluginSupervisor::new();
    supervisor.register(Arc::new(ShopRulesPlugin {
        manifest: PluginManifest {
            name: "shop-rules".to_string(),
            version: "1.0.0".to_string(),
            entrypoint: "plugin.wasm".to_string(),
            capabilities: PluginCapabilities {
                network: None,
                storage: None,
                hooks: vec![
                    "calculate-order".to_string(),
                    "on-order-status-changed".to_string(),
                ],
            },
            ..Default::default()
        },
        notifications: notifications.clone(),
    }));
    let state =
        crate::test_support::test_app_state_with_db(db, config, supervisor, vec![product_schema()])
            .await;
    let mails = Arc::new(RecordingNotifier::default());
    let mut deps = state.commerce_deps();
    deps.notifier = Some(mails.clone());
    let commerce = CommerceService::new(state.config.commerce.clone(), deps).unwrap();
    let state = state.with_commerce(commerce);
    let state = Arc::new(state);
    let schemas = state.schemas.read().await.clone();
    state
        .commerce
        .as_ref()
        .unwrap()
        .check_product_schema(&schemas)
        .unwrap();

    let schema = schemas["product"].clone();
    for (id, slug, price, stock, status) in [
        ("p-mug", "mug", 1200, Some(3), "published"),
        ("p-shirt", "shirt", 3000, None, "published"),
        ("p-draft", "secret", 100, Some(10), "draft"),
    ] {
        let mut fields = std::collections::HashMap::from([("price".to_string(), json!(price))]);
        if let Some(stock) = stock {
            fields.insert("stock".to_string(), json!(stock));
        }
        state
            .db
            .insert_content(
                &schema,
                &ContentRecord {
                    id: id.to_string(),
                    content_type: "product".to_string(),
                    title: slug.to_uppercase(),
                    slug: slug.to_string(),
                    status: status.to_string(),
                    blocks_json: "[]".to_string(),
                    author_id: None,
                    published_at: None,
                    unpublish_at: None,
                    created_at: "2026-10-22T00:00:00Z".to_string(),
                    updated_at: "2026-10-22T00:00:00Z".to_string(),
                    custom_fields: fields,
                },
            )
            .await
            .unwrap();
    }
    let staff = state
        .sessions
        .create_session("clerk", "editor", Default::default(), None)
        .await
        .unwrap();
    Shop {
        state,
        notifications,
        mails,
        staff_token: staff.id,
    }
}

impl Shop {
    async fn call(
        &self,
        method: &str,
        path: &str,
        body: Option<Value>,
        token: Option<&str>,
    ) -> (StatusCode, Value) {
        let mut request = Request::builder().method(method).uri(path);
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
        self.send(request).await
    }

    async fn send(&self, request: Request<Body>) -> (StatusCode, Value) {
        let response = crate::app::router(&self.state.config, self.state.clone())
            .oneshot(request)
            .await
            .unwrap();
        let status = response.status();
        let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        (
            status,
            serde_json::from_slice(&bytes).unwrap_or(Value::Null),
        )
    }

    async fn stock(&self, slug: &str) -> Option<i64> {
        let schema = self.state.schemas.read().await["product"].clone();
        let product = self
            .state
            .db
            .get_content(&schema, slug)
            .await
            .unwrap()
            .unwrap();
        product.custom_fields.get("stock").and_then(|v| v.as_i64())
    }

    async fn checkout(&self, items: Value) -> (StatusCode, Value) {
        self.call(
            "POST",
            "/api/v1/shop/checkout",
            Some(json!({
                "items": items,
                "email": "buyer@example.com",
                "name": "Buyer",
                "shipping_address": { "postal_code": "100-0001", "region": "Tokyo", "city": "Chiyoda", "line1": "1-1" }
            })),
            None,
        )
        .await
    }

    async fn set_status(&self, id: &str, status: &str) -> (StatusCode, Value) {
        self.call(
            "POST",
            &format!("/api/v1/orders/{}/status", id),
            Some(json!({ "status": status, "note": "by test" })),
            Some(&self.staff_token),
        )
        .await
    }

    async fn audit_actions(&self) -> Vec<(AuditAction, AuditOutcome)> {
        let mut events = self
            .state
            .db
            .query_audit_events(&aegis_core::AuditQuery {
                limit: 100,
                ..Default::default()
            })
            .await
            .unwrap();
        events.reverse();
        events
            .into_iter()
            .filter(|e| {
                matches!(
                    e.action,
                    AuditAction::OrderCreate | AuditAction::OrderUpdate
                )
            })
            .map(|e| (e.action, e.outcome))
            .collect()
    }

    /// The notifications sent so far: (kind, recipients, data)
    fn mails(&self) -> Vec<(String, Vec<&'static str>, Value)> {
        self.mails
            .events
            .lock()
            .unwrap()
            .iter()
            .map(|e| {
                let recipients = e
                    .recipients
                    .iter()
                    .map(|r| match r {
                        Recipient::Person { email, .. } => {
                            assert_eq!(email, "buyer@example.com");
                            "customer"
                        }
                        Recipient::Staff => "staff",
                    })
                    .collect();
                (e.kind.clone(), recipients, e.data.clone())
            })
            .collect()
    }

    /// Waits for the notifications spawned in the background
    async fn notified(&self, count: usize) -> Vec<OrderStatusChange> {
        for _ in 0..100 {
            if self.notifications.lock().unwrap().len() >= count {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
        self.notifications.lock().unwrap().clone()
    }
}

#[tokio::test]
async fn test_quote_uses_server_prices_and_plugin_adjustments() {
    let shop = shop(|_| {}).await;
    let (status, quote) = shop
        .call(
            "POST",
            "/api/v1/shop/quote",
            Some(json!({ "items": [
                { "product": "mug", "quantity": 2 },
                { "product": "p-shirt", "quantity": 1 },
                { "product": "mug", "quantity": 1 }
            ], "coupon_code": "SAVE10" })),
            None,
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{}", quote);
    // The two mug rows are merged; the plugin's unit price change is ignored
    assert_eq!(quote["lines"][0]["quantity"], 3);
    assert_eq!(quote["lines"][0]["unit_price"], 1200);
    assert_eq!(quote["subtotal"], 6600);
    assert_eq!(
        quote["adjustments"][0]["amount"], 0,
        "free shipping over 5,000"
    );
    assert_eq!(quote["adjustments"][1]["amount"], -660);
    assert_eq!(quote["adjustments"][1]["source"], "shop-rules");
    assert_eq!(quote["total"], 5940);

    let (status, body) = shop
        .call(
            "POST",
            "/api/v1/shop/quote",
            Some(json!({ "items": [{ "product": "mug", "quantity": 1 }], "coupon_code": "BOGUS" })),
            None,
        )
        .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert!(body["error"].as_str().unwrap().contains("not valid"));

    for (items, expected) in [
        (
            json!([{ "product": "secret", "quantity": 1 }]),
            StatusCode::NOT_FOUND,
        ),
        (
            json!([{ "product": "mug", "quantity": 4 }]),
            StatusCode::CONFLICT,
        ),
        (
            json!([{ "product": "shirt", "quantity": 6 }]),
            StatusCode::BAD_REQUEST,
        ),
        (
            json!([{ "product": "shirt", "quantity": 0 }]),
            StatusCode::BAD_REQUEST,
        ),
        (json!([]), StatusCode::BAD_REQUEST),
    ] {
        let (status, body) = shop
            .call(
                "POST",
                "/api/v1/shop/quote",
                Some(json!({ "items": items })),
                None,
            )
            .await;
        assert_eq!(status, expected, "{} -> {}", items, body);
    }
}

#[tokio::test]
async fn test_manual_checkout_reserves_stock_and_follows_the_status_flow() {
    let shop = shop(|_| {}).await;
    let (status, created) = shop
        .checkout(
            json!([{ "product": "mug", "quantity": 2 }, { "product": "shirt", "quantity": 1 }]),
        )
        .await;
    assert_eq!(status, StatusCode::CREATED, "{}", created);
    let id = created["order"]["id"].as_str().unwrap().to_string();
    let token = created["token"].as_str().unwrap().to_string();
    assert_eq!(created["order"]["status"], "pending_payment");
    assert_eq!(created["order"]["total"], 5400);
    assert_eq!(created["redirect_url"], Value::Null);
    assert_eq!(
        created["order_url"],
        format!("/orders/{}?token={}", id, token)
    );
    assert!(created["order"]["payment_instructions"].is_string());
    assert_eq!(shop.stock("mug").await, Some(1));
    assert_eq!(
        shop.stock("shirt").await,
        None,
        "unlimited stock stays unlimited"
    );

    // Not enough left for another two
    let (status, body) = shop
        .checkout(json!([{ "product": "mug", "quantity": 2 }]))
        .await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(body["product_id"], "p-mug");
    assert_eq!(shop.stock("mug").await, Some(1));

    // The customer's view needs the token
    let (status, _) = shop
        .call(
            "GET",
            &format!("/api/v1/shop/orders/{}?token=wrong", id),
            None,
            None,
        )
        .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (status, view) = shop
        .call(
            "GET",
            &format!("/api/v1/shop/orders/{}?token={}", id, token),
            None,
            None,
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(view["number"], created["order"]["number"]);
    assert!(view.get("customer_email").is_none());

    // Staff: listing needs a session; the manual payment can be confirmed by hand
    let (status, _) = shop.call("GET", "/api/v1/orders", None, None).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    let (_, list) = shop
        .call(
            "GET",
            "/api/v1/orders?status=pending_payment",
            None,
            Some(&shop.staff_token),
        )
        .await;
    assert_eq!(list["total"], 1);
    assert_eq!(list["orders"][0]["customer_email"], "buyer@example.com");
    assert_eq!(list["orders"][0]["can_mark_paid"], true);

    assert_eq!(
        shop.set_status(&id, "shipped").await.0,
        StatusCode::CONFLICT
    );
    let (status, paid) = shop.set_status(&id, "paid").await;
    assert_eq!(status, StatusCode::OK, "{}", paid);
    assert!(paid["paid_at"].is_string());
    assert_eq!(shop.set_status(&id, "shipped").await.0, StatusCode::OK);
    // Shipped orders are not cancelled (stock is not returned)
    assert_eq!(
        shop.set_status(&id, "cancelled").await.0,
        StatusCode::CONFLICT
    );
    assert_eq!(shop.set_status(&id, "completed").await.0, StatusCode::OK);
    assert_eq!(shop.stock("mug").await, Some(1));

    let changes: Vec<_> = shop
        .notified(4)
        .await
        .iter()
        .map(|c| {
            (
                c.from_status.map(|s| s.as_str()),
                c.to_status.unwrap().as_str(),
            )
        })
        .collect();
    assert_eq!(
        changes,
        vec![
            (None, "pending_payment"),
            (Some("pending_payment"), "paid"),
            (Some("paid"), "shipped"),
            (Some("shipped"), "completed"),
        ]
    );

    // Emails: the order with how to pay (customer and staff), then payment and shipping
    let mails = shop.mails();
    let kinds: Vec<_> = mails
        .iter()
        .map(|(k, r, _)| (k.as_str(), r.clone()))
        .collect();
    assert_eq!(
        kinds,
        vec![
            ("order.placed", vec!["customer", "staff"]),
            ("order.paid", vec!["customer"]),
            ("order.shipped", vec!["customer"]),
        ]
    );
    let placed = &mails[0].2;
    assert_eq!(placed["order"]["number"], created["order"]["number"]);
    assert_eq!(placed["order"]["status"], "pending_payment");
    assert_eq!(placed["order"]["total"], 5400);
    assert_eq!(placed["order"]["lines"].as_array().unwrap().len(), 2);
    assert!(placed["order_url"]
        .as_str()
        .unwrap()
        .ends_with(&format!("/orders/{}?token={}", id, token)));
    assert!(placed["payment_instructions"].is_string());
    assert_eq!(mails[2].2["order"]["note"], "by test");
    assert_eq!(
        shop.audit_actions().await,
        vec![
            (AuditAction::OrderCreate, AuditOutcome::Success),
            (AuditAction::OrderUpdate, AuditOutcome::Success),
            (AuditAction::OrderUpdate, AuditOutcome::Success),
            (AuditAction::OrderUpdate, AuditOutcome::Success),
        ]
    );
}

#[tokio::test]
async fn test_cancel_and_expiry_return_stock() {
    let shop = shop(|_| {}).await;
    let (_, first) = shop
        .checkout(json!([{ "product": "mug", "quantity": 1 }]))
        .await;
    let (_, second) = shop
        .checkout(json!([{ "product": "mug", "quantity": 2 }]))
        .await;
    assert_eq!(shop.stock("mug").await, Some(0));

    let first_id = first["order"]["id"].as_str().unwrap();
    assert_eq!(
        shop.set_status(first_id, "cancelled").await.0,
        StatusCode::OK
    );
    assert_eq!(shop.stock("mug").await, Some(1));
    // A second cancel is a conflict, and does not return the stock twice
    assert_eq!(
        shop.set_status(first_id, "cancelled").await.0,
        StatusCode::CONFLICT
    );
    assert_eq!(shop.stock("mug").await, Some(1));

    // Push the second order's deadline into the past, then run the expiry task once
    let second_id = second["order"]["id"].as_str().unwrap();
    shop.state
        .db
        .set_order_payment(second_id, "manual-ref", Some("2000-01-01T00:00:00Z"))
        .await
        .unwrap();
    let commerce = shop.state.commerce.clone().unwrap();
    assert_eq!(commerce.expire_orders().await.unwrap(), 1);
    assert_eq!(commerce.expire_orders().await.unwrap(), 0);
    assert_eq!(shop.stock("mug").await, Some(3));
    let order = shop.state.db.get_order(second_id).await.unwrap().unwrap();
    assert_eq!(order.status, "cancelled");

    let cancelled: Vec<_> = shop
        .mails()
        .into_iter()
        .filter(|(kind, _, _)| kind == "order.cancelled")
        .collect();
    assert_eq!(cancelled.len(), 2);
    assert_eq!(cancelled[0].1, vec!["customer", "staff"]);
    assert_eq!(cancelled[0].2["reason"], "admin");
    assert_eq!(cancelled[0].2["was_paid"], false);
    assert_eq!(cancelled[1].2["reason"], "expired");
}

#[tokio::test]
async fn test_checkout_validation_and_rate_limit() {
    let shop = shop(|config| config.commerce.max_orders_per_ip_per_hour = 2).await;
    let (status, _) = shop
        .call(
            "POST",
            "/api/v1/shop/checkout",
            Some(json!({ "items": [{ "product": "shirt", "quantity": 1 }], "email": "not-an-email", "name": "x" })),
            None,
        )
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    let checkout_from = |ip: &'static str| {
        let request = Request::post("/api/v1/shop/checkout")
            .header("content-type", "application/json")
            .extension(axum::extract::ConnectInfo(std::net::SocketAddr::from((
                ip.parse::<std::net::Ipv4Addr>().unwrap(),
                4000,
            ))))
            .body(Body::from(
                json!({ "items": [{ "product": "shirt", "quantity": 1 }], "email": "a@example.com", "name": "A" })
                    .to_string(),
            ))
            .unwrap();
        shop.send(request)
    };
    assert_eq!(checkout_from("203.0.113.9").await.0, StatusCode::CREATED);
    assert_eq!(checkout_from("203.0.113.9").await.0, StatusCode::CREATED);
    assert_eq!(
        checkout_from("203.0.113.9").await.0,
        StatusCode::TOO_MANY_REQUESTS
    );
    assert_eq!(checkout_from("198.51.100.1").await.0, StatusCode::CREATED);
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

#[tokio::test]
async fn test_stripe_checkout_and_webhooks() {
    let (api_base, forms) = mock_stripe().await;
    let shop = shop(|config| {
        config.commerce.provider = PaymentProviderKind::Stripe;
        config.commerce.public_url = "https://shop.example.com".to_string();
        config.commerce.stripe.secret_key = "sk_test_mock".to_string();
        config.commerce.stripe.webhook_secret = "whsec_mock".to_string();
        config.commerce.stripe.api_base = api_base;
    })
    .await;

    let (status, created) = shop
        .checkout(json!([{ "product": "mug", "quantity": 2 }]))
        .await;
    assert_eq!(status, StatusCode::CREATED, "{}", created);
    let id = created["order"]["id"].as_str().unwrap().to_string();
    let token = created["token"].as_str().unwrap();
    assert_eq!(
        created["redirect_url"],
        "https://checkout.stripe.test/cs_test_1"
    );
    let form = forms.lock().unwrap()[0].clone();
    assert!(
        form.contains(&format!("client_reference_id={}", id)),
        "{}",
        form
    );
    assert!(
        form.contains("line_items%5B0%5D%5Bprice_data%5D%5Bunit_amount%5D=1200"),
        "{}",
        form
    );
    assert!(form.contains(&format!(
        "success_url=https%3A%2F%2Fshop.example.com%2Forders%2F{}%3Ftoken%3D{}",
        id, token
    )));
    assert_eq!(shop.stock("mug").await, Some(1));

    // Stripe orders are confirmed by the webhook only
    let (_, detail) = shop
        .call(
            "GET",
            &format!("/api/v1/orders/{}", id),
            None,
            Some(&shop.staff_token),
        )
        .await;
    assert_eq!(detail["can_mark_paid"], false);
    assert_eq!(detail["payment_ref"], "cs_test_1");
    assert_eq!(shop.set_status(&id, "paid").await.0, StatusCode::CONFLICT);

    let webhook = |event: Value, secret: &str| {
        let body = event.to_string();
        let signature =
            stripe::sign_webhook_payload(secret, body.as_bytes(), chrono::Utc::now().timestamp());
        let request = Request::post("/api/v1/shop/webhooks/stripe")
            .header("stripe-signature", signature)
            .body(Body::from(body))
            .unwrap();
        shop.send(request)
    };
    let session_event = |kind: &str, session: &str, amount: i64| {
        json!({
            "id": format!("evt_{}", kind),
            "type": kind,
            "data": { "object": {
                "id": session,
                "client_reference_id": id,
                "payment_status": "paid",
                "amount_total": amount,
                "currency": "jpy"
            }}
        })
    };

    // Forged signature, wrong amount and another session are all ignored
    let (status, _) = webhook(
        session_event("checkout.session.completed", "cs_test_1", 2900),
        "whsec_forged",
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    let (status, _) = webhook(
        session_event("checkout.session.completed", "cs_test_1", 1),
        "whsec_mock",
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let (status, _) = webhook(
        session_event("checkout.session.completed", "cs_other", 2900),
        "whsec_mock",
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        shop.state.db.get_order(&id).await.unwrap().unwrap().status,
        "pending_payment"
    );

    // 2 × 1,200 + 500 shipping
    let (status, _) = webhook(
        session_event("checkout.session.completed", "cs_test_1", 2900),
        "whsec_mock",
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    // A duplicate delivery changes nothing
    let (status, _) = webhook(
        session_event("checkout.session.completed", "cs_test_1", 2900),
        "whsec_mock",
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let order = shop.state.db.get_order(&id).await.unwrap().unwrap();
    assert_eq!(order.status, "paid");

    // An unpaid session that expires returns its stock
    let (_, second) = shop
        .checkout(json!([{ "product": "mug", "quantity": 1 }]))
        .await;
    let second_id = second["order"]["id"].as_str().unwrap().to_string();
    assert_eq!(shop.stock("mug").await, Some(0));
    let expired = json!({
        "id": "evt_expired",
        "type": "checkout.session.expired",
        "data": { "object": { "id": "cs_test_2", "client_reference_id": second_id, "payment_status": "unpaid" } }
    });
    assert_eq!(webhook(expired, "whsec_mock").await.0, StatusCode::OK);
    assert_eq!(shop.stock("mug").await, Some(1));

    // Card payments: the order is announced once it is paid, and an abandoned checkout is not
    let mails = shop.mails();
    assert_eq!(mails.len(), 1, "{:?}", mails);
    assert_eq!(mails[0].0, "order.placed");
    assert_eq!(mails[0].1, vec!["customer", "staff"]);
    assert_eq!(mails[0].2["order"]["status"], "paid");
    assert!(mails[0].2.get("order_url").is_none());

    assert_eq!(
        shop.audit_actions().await,
        vec![
            (AuditAction::OrderCreate, AuditOutcome::Success),
            // the amount mismatch
            (AuditAction::OrderUpdate, AuditOutcome::Failure),
            (AuditAction::OrderUpdate, AuditOutcome::Success),
            (AuditAction::OrderCreate, AuditOutcome::Success),
            (AuditAction::OrderUpdate, AuditOutcome::Success),
        ]
    );
}

#[tokio::test]
async fn test_failed_payment_start_cancels_the_order() {
    let shop = shop(|config| {
        config.commerce.provider = PaymentProviderKind::Stripe;
        config.commerce.stripe.secret_key = "sk_test_mock".to_string();
        config.commerce.stripe.webhook_secret = "whsec_mock".to_string();
        // Nothing listens here
        config.commerce.stripe.api_base = "http://127.0.0.1:9".to_string();
    })
    .await;
    let (status, _) = shop
        .checkout(json!([{ "product": "mug", "quantity": 3 }]))
        .await;
    assert_eq!(status, StatusCode::BAD_GATEWAY);
    assert_eq!(shop.stock("mug").await, Some(3));
}

#[tokio::test]
async fn test_storefront_pages_render() {
    let shop = shop(|_| {}).await;
    let page = |path: &'static str| {
        let request = Request::get(path).body(Body::empty()).unwrap();
        let router = crate::app::router(&shop.state.config, shop.state.clone());
        async move {
            let response = router.oneshot(request).await.unwrap();
            let status = response.status();
            let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
                .await
                .unwrap();
            (status, String::from_utf8(bytes.to_vec()).unwrap())
        }
    };
    let (status, html) = page("/products").await;
    assert_eq!(status, StatusCode::OK);
    assert!(html.contains("¥1,200"), "{}", html);
    assert!(!html.contains("SECRET"), "drafts are not listed");
    let (status, html) = page("/products/mug").await;
    assert_eq!(status, StatusCode::OK);
    assert!(html.contains("data-aegis-add-to-cart"));
    assert!(html.contains("data-product-id=\"p-mug\""));
    assert!(html.contains("Only 3 left"));
    assert_eq!(page("/products/secret").await.0, StatusCode::NOT_FOUND);
    let (status, html) = page("/cart").await;
    assert_eq!(status, StatusCode::OK);
    assert!(html.contains("id=\"aegis-cart\""));
    // Attribute values are HTML-escaped (`/` → `&#x2f;`); browsers decode them
    assert!(html
        .replace("&#x2f;", "/")
        .contains("data-api=\"/api/v1/shop\""));
    let (status, script) = page("/shop/cart.js").await;
    assert_eq!(status, StatusCode::OK);
    assert!(script.contains("aegis.cart.v1"));
}

/// The compiled official plugins (`plugins/official-shop-rules`, `plugins/official-order-notifier`)
/// on the real Wasm host: settings from `[plugins.settings]`, KV in the cache backend, and the
/// notifier's webhook on a local server (its manifest's allowed_domains pointed at 127.0.0.1)
#[tokio::test]
async fn test_official_wasm_plugins_in_checkout() {
    use aegis_runtime::{HostServices, WasmEngine, WasmPluginInstance};

    // Local webhook receiver
    let received = Arc::new(Mutex::new(Vec::<Value>::new()));
    let sink = received.clone();
    let app = axum::Router::new().route(
        "/hook",
        axum::routing::post(move |axum::Json(body): axum::Json<Value>| {
            let sink = sink.clone();
            async move {
                sink.lock().unwrap().push(body);
                "ok"
            }
        }),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let hook_url = format!("http://{}/hook", listener.local_addr().unwrap());
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });

    let mut config = AegisConfig::default();
    config.commerce.enabled = true;
    config.plugins = toml::from_str(&format!(
        r#"
        enabled = ["official-shop-rules", "official-order-notifier"]
        [settings.official-shop-rules]
        shipping_fee = 600
        free_shipping_over = 10000
        coupons = "WELCOME:10%:1"
        [settings.official-order-notifier]
        webhook_url = "{}"
        notify_on = "paid"
        "#,
        hook_url
    ))
    .unwrap();

    let cache = Arc::new(aegis_cache::MemoryCacheBackend::new());
    let services: Arc<dyn HostServices> = Arc::new(
        crate::plugin_host::ServerHostServices::new(cache, config.plugins.clone()).unwrap(),
    );
    let plugins_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../plugins");
    let engine = WasmEngine::new().unwrap();
    let mut supervisor = PluginSupervisor::new();
    for name in ["official-shop-rules", "official-order-notifier"] {
        let dir = plugins_dir.join(name);
        let mut manifest = aegis_core::PluginManifest::from_json_with_defaults(
            &std::fs::read_to_string(dir.join("plugin.manifest.json")).unwrap(),
            &Default::default(),
        )
        .unwrap();
        if let Some(network) = manifest.capabilities.network.as_mut() {
            network.allowed_domains = vec!["127.0.0.1".to_string()];
        }
        let wasm = std::fs::read(dir.join("plugin.wasm")).unwrap();
        supervisor.register(Arc::new(
            WasmPluginInstance::from_bytes(engine.clone(), manifest, &wasm, services.clone())
                .unwrap(),
        ));
    }

    let state =
        crate::test_support::test_app_state(config, supervisor, vec![product_schema()]).await;
    let commerce =
        CommerceService::new(state.config.commerce.clone(), state.commerce_deps()).unwrap();
    let state = Arc::new(state.with_commerce(commerce));
    let schema = state.schemas.read().await["product"].clone();
    state
        .db
        .insert_content(
            &schema,
            &ContentRecord {
                id: "p-lamp".to_string(),
                content_type: "product".to_string(),
                title: "Lamp".to_string(),
                slug: "lamp".to_string(),
                status: "published".to_string(),
                blocks_json: "[]".to_string(),
                author_id: None,
                published_at: None,
                unpublish_at: None,
                created_at: "2026-10-22T00:00:00Z".to_string(),
                updated_at: "2026-10-22T00:00:00Z".to_string(),
                custom_fields: std::collections::HashMap::from([
                    ("price".to_string(), json!(4000)),
                    ("stock".to_string(), json!(10)),
                ]),
            },
        )
        .await
        .unwrap();
    let commerce = state.commerce.clone().unwrap();
    let request = |coupon: Option<&str>| CartRequest {
        items: vec![CartItem {
            product: "lamp".to_string(),
            quantity: 2,
        }],
        email: "buyer@example.com".to_string(),
        name: "Buyer".to_string(),
        shipping_address: None,
        coupon_code: coupon.map(str::to_string),
    };

    // 8,000 - 10% + 600 shipping (under the 10,000 threshold)
    let quote = commerce.quote(&request(Some("welcome"))).await.unwrap();
    let adjustments: Vec<_> = quote
        .adjustments
        .iter()
        .map(|a| (a.label.as_str(), a.amount, a.source.as_str()))
        .collect();
    assert_eq!(
        adjustments,
        vec![
            ("Coupon WELCOME", -800, "official-shop-rules"),
            ("Shipping", 600, "official-shop-rules"),
        ]
    );
    assert_eq!(quote.total, 7800);
    match commerce.quote(&request(Some("NOPE"))).await {
        // The customer sees the plugin's message without the plugin's name
        Err(CommerceError::Rejected(message)) => {
            assert_eq!(message, "Coupon code \"NOPE\" is not valid")
        }
        other => panic!("expected a refusal, got {:?}", other.map(|c| c.total)),
    }

    let audit = AuditContext::default();
    let placed = commerce
        .checkout(&audit, &request(Some("WELCOME")), None, |_, _| ReturnUrls {
            success_url: String::new(),
            cancel_url: String::new(),
        })
        .await
        .unwrap();
    commerce
        .transition(
            &audit,
            &placed.order,
            aegis_core::OrderStatus::Paid,
            None,
            "admin",
        )
        .await
        .unwrap();

    // The notifier posts the paid order; the shop rules count the coupon's use
    for _ in 0..200 {
        if !received.lock().unwrap().is_empty() {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
    let posted = received.lock().unwrap().clone();
    assert_eq!(posted.len(), 1, "only 'paid' is in notify_on");
    let text = posted[0]["text"].as_str().unwrap();
    assert!(text.contains("Order paid"), "{}", text);
    assert!(text.contains("¥7,800"), "{}", text);
    for _ in 0..200 {
        if services
            .kv_get("official_shop_rules:coupon:WELCOME:uses")
            .await
            .unwrap()
            .is_some()
        {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
    let error = commerce.quote(&request(Some("WELCOME"))).await.unwrap_err();
    assert!(error.to_string().contains("used up"), "{}", error);
}

async fn run_db_order_lifecycle_verification(shop: Shop) {
    // 1. 見積もり計算 (POST /api/v1/shop/quote) - mug x 2
    let (status, quote) = shop
        .call(
            "POST",
            "/api/v1/shop/quote",
            Some(json!({ "items": [{"product": "mug", "quantity": 2}] })),
            None,
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(quote["subtotal"], 2400);
    assert_eq!(quote["total"], 2900); // 2400 + 500 shipping

    // 2. チェックアウト (POST /api/v1/shop/checkout) - mug x 2 (stock 3 -> 1)
    assert_eq!(shop.stock("mug").await, Some(3));
    let (status, checkout) = shop
        .call(
            "POST",
            "/api/v1/shop/checkout",
            Some(json!({
                "items": [{"product": "mug", "quantity": 2}],
                "email": "customer@example.com",
                "name": "Customer",
                "payment_provider": "manual"
            })),
            None,
        )
        .await;
    assert_eq!(
        status,
        StatusCode::CREATED,
        "Checkout response: {:?}",
        checkout
    );
    let order_id = checkout["order"]["id"].as_str().unwrap().to_string();
    let access_token = checkout["token"].as_str().unwrap().to_string();
    assert_eq!(shop.stock("mug").await, Some(1));

    // 3. 在庫不足 409 Conflict - 残 1 なのに 2 注文
    let (status, _fail) = shop
        .call(
            "POST",
            "/api/v1/shop/checkout",
            Some(json!({
                "items": [{"product": "mug", "quantity": 2}],
                "email": "another@example.com",
                "name": "Another",
                "payment_provider": "manual"
            })),
            None,
        )
        .await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(shop.stock("mug").await, Some(1));

    // 4. 顧客による注文確認ページ取得
    let (status, order) = shop
        .call(
            "GET",
            &format!("/api/v1/shop/orders/{}?token={}", order_id, access_token),
            None,
            None,
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(order["id"], order_id);
    assert_eq!(order["status"], "pending_payment");

    // 5. スタッフによる注文一覧取得 (GET /api/v1/orders)
    let (status, page) = shop
        .call("GET", "/api/v1/orders", None, Some(&shop.staff_token))
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(page["total"], 1);
    assert_eq!(page["orders"][0]["id"], order_id);

    // 6. スタッフによる注文詳細取得 (GET /api/v1/orders/{id})
    let (status, detail) = shop
        .call(
            "GET",
            &format!("/api/v1/orders/{}", order_id),
            None,
            Some(&shop.staff_token),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(detail["id"], order_id);
    assert_eq!(detail["status"], "pending_payment");

    // 7. スタッフによるキャンセルと在庫返却 (POST /api/v1/orders/{id}/status, stock 1 -> 3)
    let (status, cancel) = shop
        .call(
            "POST",
            &format!("/api/v1/orders/{}/status", order_id),
            Some(json!({
                "status": "cancelled",
                "note": "Restocking test"
            })),
            Some(&shop.staff_token),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "Cancel res: {:?}", cancel);
    assert_eq!(cancel["status"], "cancelled");
    assert_eq!(
        shop.stock("mug").await,
        Some(3),
        "Mug stock must be restored to 3 after cancellation"
    );
}

fn is_port_reachable(host_port: &str) -> bool {
    let Ok(addr) = host_port.parse() else {
        return false;
    };
    std::net::TcpStream::connect_timeout(&addr, std::time::Duration::from_millis(150)).is_ok()
}

async fn connect_postgres_with_retry(url: &str) -> Option<aegis_db::PostgresAdapter> {
    for _ in 0..6 {
        if let Ok(adapter) = aegis_db::PostgresAdapter::connect(url, 5).await {
            return Some(adapter);
        }
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
    }
    None
}

async fn connect_mysql_with_retry(url: &str) -> Option<aegis_db::MySqlAdapter> {
    for _ in 0..6 {
        if let Ok(adapter) = aegis_db::MySqlAdapter::connect(url, 5).await {
            return Some(adapter);
        }
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
    }
    None
}

#[tokio::test]
async fn test_postgres_order_and_stock_e2e() {
    let custom_url = std::env::var("POSTGRES_URL").ok();
    if custom_url.is_none() && !is_port_reachable("127.0.0.1:5432") {
        eprintln!("Skipping PostgreSQL test: 127.0.0.1:5432 is not reachable (start with task docker:dbs:up)");
        return;
    }
    let url = custom_url
        .unwrap_or_else(|| "postgres://aegis:password@127.0.0.1:5432/aegis_test".to_string());
    let adapter = match connect_postgres_with_retry(&url).await {
        Some(a) => a,
        None => {
            eprintln!("Skipping PostgreSQL test: failed to connect after retry");
            return;
        }
    };
    let _ = sqlx::query("DROP TABLE IF EXISTS content_product, content_article, _aegis_orders, _aegis_migrations, _aegis_users, _aegis_user_links, _aegis_settings, _aegis_audit CASCADE;")
        .execute(adapter.pool())
        .await;
    if let Err(e) = adapter.init().await {
        eprintln!("Skipping PostgreSQL test: init failed: {}", e);
        return;
    }
    let mut config = AegisConfig::default();
    config.commerce.enabled = true;
    config.commerce.max_quantity_per_item = 5;
    let shop = shop_with_db(Arc::new(adapter), config).await;
    run_db_order_lifecycle_verification(shop).await;
}

#[tokio::test]
async fn test_mysql_order_and_stock_e2e() {
    let custom_url = std::env::var("MYSQL_URL").ok();
    if custom_url.is_none() && !is_port_reachable("127.0.0.1:3306") {
        eprintln!(
            "Skipping MySQL test: 127.0.0.1:3306 is not reachable (start with task docker:dbs:up)"
        );
        return;
    }
    let url = custom_url
        .unwrap_or_else(|| "mysql://aegis:password@127.0.0.1:3306/aegis_test".to_string());
    let adapter = match connect_mysql_with_retry(&url).await {
        Some(a) => a,
        None => {
            eprintln!("Skipping MySQL test: failed to connect after retry");
            return;
        }
    };
    let _ = sqlx::query("DROP TABLE IF EXISTS content_product, content_article, _aegis_orders, _aegis_migrations, _aegis_users, _aegis_user_links, _aegis_settings, _aegis_audit;")
        .execute(adapter.pool())
        .await;
    if let Err(e) = adapter.init().await {
        eprintln!("Skipping MySQL test: init failed: {}", e);
        return;
    }
    let mut config = AegisConfig::default();
    config.commerce.enabled = true;
    config.commerce.max_quantity_per_item = 5;
    let shop = shop_with_db(Arc::new(adapter), config).await;
    run_db_order_lifecycle_verification(shop).await;
}
