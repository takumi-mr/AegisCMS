//! Stripe Checkout（`[commerce].provider = "stripe"`）。決済ページ（Checkout Session）を作り、
//! 入金・期限切れを署名付きの Webhook で受け取る。カード情報は AegisCMS を通らない

use super::config::StripeConfig;
use aegis_core::{AegisError, OrderAdjustment, OrderContext, OrderLine, Result};
use async_trait::async_trait;
use hmac::{Hmac, Mac};
use http::HeaderMap;
use serde_json::Value;
use sha2::Sha256;

use super::payment::{PaymentEvent, PaymentEventKind, PaymentProvider, PaymentStart, ReturnUrls};
use aegis_db::OrderRecord;

/// Webhook の署名の時刻と現在時刻の許容差（秒）。古い通知の再送（リプレイ）を受け付けない
const SIGNATURE_TOLERANCE_SECS: i64 = 300;

pub struct StripePaymentProvider {
    secret_key: String,
    webhook_secret: String,
    api_base: String,
    client: reqwest::Client,
}

impl StripePaymentProvider {
    pub fn new(config: &StripeConfig) -> Result<Self> {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(20))
            .build()
            .map_err(|e| AegisError::Config(format!("Failed to build the HTTP client: {}", e)))?;
        Ok(Self {
            secret_key: config.secret_key(),
            webhook_secret: config.webhook_secret(),
            api_base: config.api_base.trim_end_matches('/').to_string(),
            client,
        })
    }
}

/// Checkout Session の作成パラメータ（`application/x-www-form-urlencoded`）。
/// 商品と、送料などの加算を明細にする。割引（負の金額）があると Stripe の明細では表せないため、
/// 注文全体を 1 行にまとめる（金額は同じ）
fn checkout_session_form(
    order: &OrderRecord,
    ctx: &OrderContext,
    urls: &ReturnUrls,
    expires_at: i64,
) -> Vec<(String, String)> {
    let mut form: Vec<(String, String)> = vec![
        ("mode".into(), "payment".into()),
        ("success_url".into(), urls.success_url.clone()),
        ("cancel_url".into(), urls.cancel_url.clone()),
        ("client_reference_id".into(), order.id.clone()),
        ("expires_at".into(), expires_at.to_string()),
        ("metadata[order_id]".into(), order.id.clone()),
        ("metadata[order_number]".into(), order.number.clone()),
        (
            "payment_intent_data[metadata][order_id]".into(),
            order.id.clone(),
        ),
    ];
    if !order.customer_email.is_empty() {
        form.push(("customer_email".into(), order.customer_email.clone()));
    }

    let mut push_item = |index: usize, name: &str, unit_amount: i64, quantity: u32| {
        let key = |field: &str| format!("line_items[{}]{}", index, field);
        form.push((key("[price_data][currency]"), order.currency.clone()));
        form.push((key("[price_data][unit_amount]"), unit_amount.to_string()));
        form.push((key("[price_data][product_data][name]"), name.to_string()));
        form.push((key("[quantity]"), quantity.to_string()));
    };
    let itemized = ctx.adjustments.iter().all(|a| a.amount >= 0);
    if itemized {
        let lines = ctx
            .lines
            .iter()
            .map(|l: &OrderLine| (l.title.as_str(), l.unit_price, l.quantity));
        let adjustments = ctx
            .adjustments
            .iter()
            .filter(|a| a.amount > 0)
            .map(|a: &OrderAdjustment| (a.label.as_str(), a.amount, 1));
        for (index, (name, amount, quantity)) in lines.chain(adjustments).enumerate() {
            push_item(index, name, amount, quantity);
        }
    } else {
        push_item(0, &format!("Order {}", order.number), order.total, 1);
    }
    form
}

#[async_trait]
impl PaymentProvider for StripePaymentProvider {
    fn id(&self) -> &'static str {
        "stripe"
    }

    fn allows_manual_confirmation(&self) -> bool {
        false
    }

    async fn start_payment(
        &self,
        order: &OrderRecord,
        ctx: &OrderContext,
        urls: &ReturnUrls,
    ) -> Result<PaymentStart> {
        let expires_at = order
            .expires_at
            .as_deref()
            .and_then(|at| chrono::DateTime::parse_from_rfc3339(at).ok())
            .map(|at| at.timestamp())
            .unwrap_or_else(|| chrono::Utc::now().timestamp() + 3600);
        let response = self
            .client
            .post(format!("{}/v1/checkout/sessions", self.api_base))
            .bearer_auth(&self.secret_key)
            // 同じ注文で 2 回作らない
            .header("Idempotency-Key", format!("aegis-order-{}", order.id))
            .form(&checkout_session_form(order, ctx, urls, expires_at))
            .send()
            .await
            .map_err(|e| AegisError::Internal(format!("Stripe request failed: {}", e)))?;
        let status = response.status();
        let bytes = response
            .bytes()
            .await
            .map_err(|e| AegisError::Internal(format!("Stripe request failed: {}", e)))?;
        let body: Value = serde_json::from_slice(&bytes)
            .map_err(|e| AegisError::Internal(format!("Invalid Stripe response: {}", e)))?;
        if !status.is_success() {
            let message = body["error"]["message"].as_str().unwrap_or("unknown error");
            return Err(AegisError::Internal(format!(
                "Stripe returned {}: {}",
                status, message
            )));
        }
        let (Some(id), Some(url)) = (body["id"].as_str(), body["url"].as_str()) else {
            return Err(AegisError::Internal(
                "Stripe response has no session id or url".to_string(),
            ));
        };
        Ok(PaymentStart {
            redirect_url: Some(url.to_string()),
            payment_ref: Some(id.to_string()),
        })
    }

    fn parse_webhook(&self, headers: &HeaderMap, body: &[u8]) -> Result<Option<PaymentEvent>> {
        let signature = headers
            .get("stripe-signature")
            .and_then(|v| v.to_str().ok())
            .ok_or_else(|| AegisError::Authentication("Missing Stripe-Signature".to_string()))?;
        verify_signature(
            &self.webhook_secret,
            signature,
            body,
            chrono::Utc::now().timestamp(),
        )?;
        let event: Value = serde_json::from_slice(body)
            .map_err(|e| AegisError::Validation(format!("Invalid Stripe event: {}", e)))?;
        Ok(event_to_payment(&event))
    }
}

/// `Stripe-Signature: t=<unix>,v1=<hex>[,v1=...]` を確かめる（`{t}.{body}` の HMAC-SHA256）
fn verify_signature(secret: &str, header: &str, body: &[u8], now: i64) -> Result<()> {
    let invalid =
        |reason: &str| AegisError::Authentication(format!("Stripe signature: {}", reason));
    let mut timestamp = None;
    let mut signatures = Vec::new();
    for part in header.split(',') {
        match part.trim().split_once('=') {
            Some(("t", value)) => timestamp = value.parse::<i64>().ok(),
            Some(("v1", value)) => signatures.push(value.to_string()),
            _ => {}
        }
    }
    let timestamp = timestamp.ok_or_else(|| invalid("no timestamp"))?;
    if (now - timestamp).abs() > SIGNATURE_TOLERANCE_SECS {
        return Err(invalid("timestamp outside the tolerance"));
    }
    for signature in signatures {
        let Ok(expected) = hex::decode(&signature) else {
            continue;
        };
        let mut mac =
            Hmac::<Sha256>::new_from_slice(secret.as_bytes()).map_err(|_| invalid("bad secret"))?;
        mac.update(timestamp.to_string().as_bytes());
        mac.update(b".");
        mac.update(body);
        // 比較は定数時間
        if mac.verify_slice(&expected).is_ok() {
            return Ok(());
        }
    }
    Err(invalid("no matching v1 signature"))
}

/// Checkout Session のイベントを注文の出来事にする（それ以外のイベントは `None`）
fn event_to_payment(event: &Value) -> Option<PaymentEvent> {
    let event_type = event["type"].as_str()?;
    let session = &event["data"]["object"];
    let order_id = session["client_reference_id"]
        .as_str()
        .or_else(|| session["metadata"]["order_id"].as_str())?
        .to_string();
    let payment_ref = session["id"].as_str()?.to_string();
    let paid = || PaymentEventKind::Paid {
        amount: session["amount_total"].as_i64().unwrap_or(-1),
        currency: session["currency"].as_str().unwrap_or_default().to_string(),
    };
    let kind = match event_type {
        "checkout.session.completed" => match session["payment_status"].as_str() {
            Some("paid") | Some("no_payment_required") => paid(),
            // コンビニ払いなど。`async_payment_succeeded` / `failed` を待つ
            _ => PaymentEventKind::AwaitingPayment,
        },
        "checkout.session.async_payment_succeeded" => paid(),
        "checkout.session.async_payment_failed" => PaymentEventKind::Failed,
        "checkout.session.expired" => PaymentEventKind::Expired,
        _ => return None,
    };
    Some(PaymentEvent {
        event_id: event["id"].as_str().unwrap_or_default().to_string(),
        order_id,
        payment_ref,
        kind,
    })
}

/// Webhook の `Stripe-Signature` ヘッダーを作る（テストや、Webhook の動作確認用）
pub fn sign_webhook_payload(secret: &str, body: &[u8], timestamp: i64) -> String {
    let mut mac = Hmac::<Sha256>::new_from_slice(secret.as_bytes()).unwrap();
    mac.update(format!("{}.", timestamp).as_bytes());
    mac.update(body);
    format!(
        "t={},v1={}",
        timestamp,
        hex::encode(mac.finalize().into_bytes())
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_verify_signature() {
        let body = br#"{"id":"evt_1"}"#;
        let header = sign_webhook_payload("whsec_test", body, 1_000_000);
        assert!(verify_signature("whsec_test", &header, body, 1_000_100).is_ok());
        // Wrong secret, tampered body, replayed too late
        assert!(verify_signature("whsec_other", &header, body, 1_000_100).is_err());
        assert!(verify_signature("whsec_test", &header, br#"{"id":"evt_2"}"#, 1_000_100).is_err());
        assert!(verify_signature("whsec_test", &header, body, 1_000_000 + 301).is_err());
        assert!(verify_signature("whsec_test", "v1=abcd", body, 1_000_000).is_err());
        // Several signatures (secret rotation): any one may match
        let rotated = format!("{},v1=00ff", header);
        assert!(verify_signature("whsec_test", &rotated, body, 1_000_000).is_ok());
    }

    #[test]
    fn test_events() {
        let event = |kind: &str, payment_status: &str| {
            json!({
                "id": "evt_1",
                "type": kind,
                "data": { "object": {
                    "id": "cs_test_1",
                    "client_reference_id": "order-1",
                    "payment_status": payment_status,
                    "amount_total": 2400,
                    "currency": "jpy"
                }}
            })
        };
        let paid = event_to_payment(&event("checkout.session.completed", "paid")).unwrap();
        assert_eq!(paid.order_id, "order-1");
        assert_eq!(paid.payment_ref, "cs_test_1");
        assert_eq!(
            paid.kind,
            PaymentEventKind::Paid {
                amount: 2400,
                currency: "jpy".to_string()
            }
        );
        assert_eq!(
            event_to_payment(&event("checkout.session.completed", "unpaid"))
                .unwrap()
                .kind,
            PaymentEventKind::AwaitingPayment
        );
        assert_eq!(
            event_to_payment(&event("checkout.session.expired", "unpaid"))
                .unwrap()
                .kind,
            PaymentEventKind::Expired
        );
        assert!(event_to_payment(&event("customer.created", "")).is_none());
    }

    #[test]
    fn test_checkout_form_collapses_discounts() {
        let order = OrderRecord {
            id: "order-1".to_string(),
            number: "261022-ABC123".to_string(),
            currency: "jpy".to_string(),
            total: 2300,
            ..Default::default()
        };
        let line = OrderLine {
            title: "Mug".to_string(),
            unit_price: 1000,
            quantity: 2,
            line_total: 2000,
            ..Default::default()
        };
        let shipping = OrderAdjustment {
            kind: "shipping".to_string(),
            label: "Shipping".to_string(),
            amount: 500,
            ..Default::default()
        };
        let mut ctx = OrderContext {
            lines: vec![line],
            adjustments: vec![shipping],
            ..Default::default()
        };
        let urls = ReturnUrls {
            success_url: "https://shop/orders/1".to_string(),
            cancel_url: "https://shop/cart".to_string(),
        };
        let form = checkout_session_form(&order, &ctx, &urls, 99);
        let get = |key: &str| form.iter().find(|(k, _)| k == key).map(|(_, v)| v.as_str());
        assert_eq!(get("line_items[0][price_data][unit_amount]"), Some("1000"));
        assert_eq!(get("line_items[0][quantity]"), Some("2"));
        assert_eq!(
            get("line_items[1][price_data][product_data][name]"),
            Some("Shipping")
        );
        assert_eq!(get("expires_at"), Some("99"));

        ctx.adjustments.push(OrderAdjustment {
            kind: "discount".to_string(),
            label: "Coupon".to_string(),
            amount: -200,
            ..Default::default()
        });
        let form = checkout_session_form(&order, &ctx, &urls, 99);
        let get = |key: &str| form.iter().find(|(k, _)| k == key).map(|(_, v)| v.as_str());
        assert_eq!(get("line_items[0][price_data][unit_amount]"), Some("2300"));
        assert_eq!(get("line_items[1][quantity]"), None);
    }
}
