//! `sdk/examples/*-hooks` のビルド済み `plugin.wasm` が、AssemblyScript / TinyGo SDK の
//! 注文・通知・コメント・ブロック描画のフックとホスト API（`kv` / `config` / `http`）を、
//! 実際のホスト（`WasmEngine`）上で Rust SDK と同じ ABI で扱えることを確認する。
//! 各言語の例は同じ挙動を実装している（TinyGo の例は `sdk/tinygo/examples/hooks`）

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use aegis_core::{
    AegisError, CommentSubmission, CommentVerdict, Delivered, NotificationMessage, OrderContext,
    OrderStatusChange, PluginManifest, PluginResourceLimits, PublishedContent, Recipient,
};
use aegis_runtime::{
    AegisPlugin, HostServices, HttpRequest, HttpResponse, WasmEngine, WasmPluginInstance,
};
use serde_json::json;

/// KV はメモリ、HTTP は記録して `http_status` で答え、設定値は 1 つだけ持つ
struct FakeServices {
    plugin: String,
    kv: Mutex<HashMap<String, String>>,
    requests: Mutex<Vec<HttpRequest>>,
    http_status: Mutex<u16>,
    settings: Mutex<HashMap<String, String>>,
}

impl FakeServices {
    fn new(plugin: &str) -> Arc<Self> {
        Arc::new(Self {
            plugin: plugin.to_string(),
            kv: Mutex::new(HashMap::new()),
            requests: Mutex::new(Vec::new()),
            http_status: Mutex::new(200),
            settings: Mutex::new(HashMap::new()),
        })
    }
}

#[async_trait::async_trait]
impl HostServices for FakeServices {
    async fn kv_get(&self, key: &str) -> aegis_core::Result<Option<String>> {
        Ok(self.kv.lock().unwrap().get(key).cloned())
    }

    async fn kv_set(&self, key: &str, value: &str) -> aegis_core::Result<()> {
        self.kv
            .lock()
            .unwrap()
            .insert(key.to_string(), value.to_string());
        Ok(())
    }

    async fn http_fetch(
        &self,
        plugin: &str,
        request: HttpRequest,
    ) -> aegis_core::Result<HttpResponse> {
        assert_eq!(plugin, self.plugin);
        self.requests.lock().unwrap().push(request);
        Ok(HttpResponse {
            status: *self.http_status.lock().unwrap(),
            headers: Default::default(),
            body: "{}".to_string(),
        })
    }

    fn plugin_setting(&self, plugin: &str, key: &str) -> Option<String> {
        assert_eq!(plugin, self.plugin);
        self.settings.lock().unwrap().get(key).cloned()
    }
}

fn sdk_path(relative: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../sdk")
        .join(relative)
}

fn load(dir: &Path, services: Arc<FakeServices>) -> WasmPluginInstance {
    let manifest = PluginManifest::from_json_with_defaults(
        &std::fs::read_to_string(dir.join("plugin.manifest.json")).unwrap(),
        &PluginResourceLimits::default(),
    )
    .unwrap();
    let wasm = std::fs::read(dir.join(&manifest.entrypoint))
        .unwrap_or_else(|e| panic!("{}: plugin.wasm is not built: {}", dir.display(), e));
    let services: Arc<dyn HostServices> = services;
    WasmPluginInstance::from_bytes(WasmEngine::new().unwrap(), manifest, &wasm, services)
        .unwrap_or_else(|e| panic!("{}: {}", dir.display(), e))
}

fn order(subtotal: i64, coupon: Option<&str>) -> OrderContext {
    serde_json::from_value(json!({
        "currency": "jpy",
        "lines": [{
            "product_id": "p1", "slug": "tee", "title": "Tee",
            "unit_price": subtotal, "quantity": 1, "line_total": subtotal
        }],
        "subtotal": subtotal,
        "total": subtotal,
        "coupon_code": coupon,
        "order_number": "A-100",
    }))
    .unwrap()
}

fn comment(body: &str) -> CommentSubmission {
    CommentSubmission {
        content_type: "articles".to_string(),
        content_id: "c1".to_string(),
        content_title: "Post".to_string(),
        content_slug: "post".to_string(),
        author_name: "Visitor".to_string(),
        author_email: "visitor@example.com".to_string(),
        body: body.to_string(),
        status: "approved".to_string(),
        ..Default::default()
    }
}

fn notification(recipient: Recipient) -> NotificationMessage {
    NotificationMessage {
        kind: "audit.session.create".to_string(),
        recipient,
        subject: "Login locked".to_string(),
        body: "root was locked".to_string(),
        html: None,
        data: json!({ "site_name": "Shop" }),
    }
}

async fn check_hooks(dir: PathBuf, plugin_name: &str, kv_namespace: &str, calculated_by: &str) {
    let services = FakeServices::new(plugin_name);
    let plugin = load(&dir, services.clone());
    let name = dir.display().to_string();

    // calculate-order: 設定のしきい値未満なら送料を足す。metadata も返る
    let calculated = plugin.calculate_order(order(3000, None)).await.unwrap();
    assert_eq!(calculated.adjustments.len(), 1, "{}", name);
    assert_eq!(calculated.adjustments[0].kind, "shipping", "{}", name);
    assert_eq!(calculated.adjustments[0].amount, 500, "{}", name);
    assert_eq!(
        calculated.metadata.get("calculated_by"),
        Some(&json!(calculated_by)),
        "{}",
        name
    );

    // config_get: しきい値を下げると送料は付かない
    services
        .settings
        .lock()
        .unwrap()
        .insert("free_shipping_threshold".to_string(), "1000".to_string());
    let free = plugin.calculate_order(order(3000, None)).await.unwrap();
    assert!(free.adjustments.is_empty(), "{}", name);

    // 注文を断る（メッセージはそのまま出る）
    match plugin.calculate_order(order(3000, Some("EXPIRED"))).await {
        Err(AegisError::PluginRejected(message)) => {
            assert!(message.contains("expired"), "{}: {}", name, message)
        }
        other => panic!("{}: expected a rejection, got {:?}", name, other),
    }

    // on-order-status-changed: kv_get / kv_set で数え、http_fetch で知らせる
    let change = OrderStatusChange {
        order: order(3000, None),
        from_status: None,
        to_status: None,
        occurred_at: "2026-11-02T00:00:00Z".to_string(),
    };
    plugin
        .on_order_status_changed(change.clone())
        .await
        .unwrap();
    plugin.on_order_status_changed(change).await.unwrap();
    assert_eq!(
        services
            .kv
            .lock()
            .unwrap()
            .get(&format!("{}:order_events", kv_namespace))
            .map(String::as_str),
        Some("2"),
        "{}",
        name
    );
    {
        let requests = services.requests.lock().unwrap();
        assert_eq!(requests.len(), 2, "{}", name);
        assert_eq!(requests[0].method, "POST", "{}", name);
        assert_eq!(
            requests[0].url, "https://hooks.example.com/orders",
            "{}",
            name
        );
        assert_eq!(
            requests[0].headers.get("Content-Type").map(String::as_str),
            Some("application/json"),
            "{}",
            name
        );
        let body: serde_json::Value =
            serde_json::from_str(requests[0].body.as_deref().unwrap()).unwrap();
        assert_eq!(body["text"], "Order A-100: created", "{}", name);
    }
    services.requests.lock().unwrap().clear();

    // send-notification: 顧客宛ては扱わず、運営者宛ては送る
    let skipped = plugin
        .send_notification(notification(Recipient::person("a@example.com", "A")))
        .await
        .unwrap();
    assert_eq!(skipped, Delivered::Skipped, "{}", name);
    assert!(services.requests.lock().unwrap().is_empty(), "{}", name);
    let sent = plugin
        .send_notification(notification(Recipient::Staff))
        .await
        .unwrap();
    assert_eq!(sent, Delivered::Sent, "{}", name);
    assert_eq!(services.requests.lock().unwrap().len(), 1, "{}", name);

    // 4xx は再試行しない失敗、5xx は再試行する失敗
    *services.http_status.lock().unwrap() = 404;
    let permanent = plugin
        .send_notification(notification(Recipient::Staff))
        .await
        .unwrap_err();
    assert!(permanent.permanent, "{}: {:?}", name, permanent);
    *services.http_status.lock().unwrap() = 503;
    let transient = plugin
        .send_notification(notification(Recipient::Staff))
        .await
        .unwrap_err();
    assert!(!transient.permanent, "{}: {:?}", name, transient);

    // on-content-published
    plugin
        .on_content_published(PublishedContent::new(
            "id-1",
            "articles",
            "Hello",
            "hello",
            "2026-11-02T00:00:00Z",
        ))
        .await
        .unwrap();
    assert_eq!(
        services
            .kv
            .lock()
            .unwrap()
            .get(&format!("{}:last_published", kv_namespace))
            .map(String::as_str),
        Some("articles/hello"),
        "{}",
        name
    );

    // check-comment
    assert_eq!(
        plugin.check_comment(comment("Nice post")).await.unwrap(),
        CommentVerdict::Allow,
        "{}",
        name
    );
    assert_eq!(
        plugin.check_comment(comment("Buy VIAGRA")).await.unwrap(),
        CommentVerdict::Spam {
            reason: Some("blocked word".to_string())
        },
        "{}",
        name
    );
    assert_eq!(
        plugin
            .check_comment(comment("http://a http://b http://c"))
            .await
            .unwrap(),
        CommentVerdict::Hold {
            reason: Some("too many links".to_string())
        },
        "{}",
        name
    );
    match plugin.check_comment(comment("   ")).await {
        Err(AegisError::PluginRejected(message)) => {
            assert!(message.contains("empty"), "{}: {}", name, message)
        }
        other => panic!("{}: expected a rejection, got {:?}", name, other),
    }

    // render-blocks: 入力と同じ順。足りないブロックは None、HTML は呼び出し側でエスケープ済み
    let blocks = serde_json::from_value(json!([
        { "name": "example/callout", "attributes": { "message": "<b>Hi</b> & \"bye\"" }, "children": [] },
        { "name": "example/callout", "attributes": { "count": 3 }, "children": [] },
    ]))
    .unwrap();
    let html = plugin.render_blocks(blocks).await.unwrap();
    assert_eq!(
        html,
        vec![
            Some(
                "<aside class=\"callout\">&lt;b&gt;Hi&lt;/b&gt; &amp; &quot;bye&quot;</aside>"
                    .to_string()
            ),
            None
        ],
        "{}",
        name
    );
}

#[tokio::test]
async fn test_assemblyscript_example_handles_the_commerce_notification_and_comment_hooks() {
    check_hooks(
        sdk_path("examples/assemblyscript-hooks"),
        "example-as-hooks",
        "example_as_hooks_state",
        "example-as-hooks",
    )
    .await;
}

#[tokio::test]
async fn test_tinygo_example_handles_the_commerce_notification_and_comment_hooks() {
    check_hooks(
        sdk_path("tinygo/examples/hooks"),
        "example-tinygo-hooks",
        "example_tinygo_hooks_state",
        "example-tinygo-hooks",
    )
    .await;
}
