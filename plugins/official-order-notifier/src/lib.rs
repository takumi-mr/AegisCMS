//! 公式プラグイン: 注文の通知（`on-order-status-changed` → `http::fetch`）。
//!
//! 設定（サーバーの `[plugins.settings.official-order-notifier]`）:
//!
//! | キー | 内容 |
//! | --- | --- |
//! | `webhook_url` | 送信先（必須）。Slack の Incoming Webhook など。`env:SLACK_WEBHOOK_URL` で環境変数から |
//! | `format` | `slack`（既定、`{"text": ...}`）か `json`（`OrderStatusChange` をそのまま送る） |
//! | `notify_on` | 通知する状態（カンマ区切り。既定 `pending_payment,paid,cancelled`） |
//!
//! 送信先のドメインはマニフェストの `capabilities.network.allowed_domains` に含めること
//! （既定は `hooks.slack.com` のみ）

use aegis_plugin_sdk::prelude::*;

#[derive(Default)]
pub struct OrderNotifierPlugin;

const DEFAULT_NOTIFY_ON: &str = "pending_payment,paid,cancelled";

/// 小数部を持たない通貨（Stripe の zero-decimal currencies）
const ZERO_DECIMAL_CURRENCIES: [&str; 16] = [
    "bif", "clp", "djf", "gnf", "jpy", "kmf", "krw", "mga", "pyg", "rwf", "ugx", "vnd", "vuv",
    "xaf", "xof", "xpf",
];

/// 最小単位の金額を表示用にする（`1200, "jpy"` → `¥1,200`、`1250, "usd"` → `USD 12.50`）
fn money(amount: i64, currency: &str) -> String {
    let sign = if amount < 0 { "-" } else { "" };
    let abs = amount.unsigned_abs();
    if ZERO_DECIMAL_CURRENCIES.contains(&currency) {
        let symbol = if currency == "jpy" {
            "¥".to_string()
        } else {
            format!("{} ", currency.to_ascii_uppercase())
        };
        format!("{}{}{}", sign, symbol, group(&abs.to_string()))
    } else {
        format!(
            "{}{} {}.{:02}",
            sign,
            currency.to_ascii_uppercase(),
            group(&(abs / 100).to_string()),
            abs % 100
        )
    }
}

fn group(digits: &str) -> String {
    let mut out = String::new();
    for (i, ch) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(ch);
    }
    out
}

fn status_label(status: Option<&str>) -> &'static str {
    match status {
        Some("pending_payment") => "New order (awaiting payment)",
        Some("paid") => "Order paid",
        Some("shipped") => "Order shipped",
        Some("completed") => "Order completed",
        Some("cancelled") => "Order cancelled",
        _ => "Order updated",
    }
}

/// Slack に送る文面
fn slack_text(change: &OrderStatusChange) -> String {
    let order = &change.order;
    let mut text = format!(
        "*{}* {} — {} ({})",
        status_label(change.to_status.as_deref()),
        order.order_number.as_deref().unwrap_or("-"),
        money(order.total, &order.currency),
        if order.customer_name.is_empty() {
            "-"
        } else {
            &order.customer_name
        }
    );
    for line in &order.lines {
        text.push_str(&format!("\n• {} × {}", line.title, line.quantity));
    }
    if let Some(code) = &order.coupon_code {
        text.push_str(&format!("\nCoupon: {}", code));
    }
    text
}

impl Plugin for OrderNotifierPlugin {
    fn on_order_status_changed(&mut self, change: OrderStatusChange) -> Result<()> {
        let notify_on = config::get("notify_on").unwrap_or_else(|| DEFAULT_NOTIFY_ON.to_string());
        let wanted = change
            .to_status
            .as_deref()
            .is_some_and(|status| notify_on.split(',').any(|s| s.trim() == status));
        if !wanted {
            return Ok(());
        }
        let url = config::get("webhook_url")
            .filter(|url| !url.trim().is_empty())
            .ok_or_else(|| PluginError::Execution("webhook_url is not configured".to_string()))?;
        let request = match config::get("format").as_deref().map(str::trim) {
            Some("json") => http::Request::post_json(url.trim(), &change)?,
            _ => http::Request::post_json(
                url.trim(),
                &serde_json::json!({ "text": slack_text(&change) }),
            )?,
        };
        let response = http::fetch(&request)?;
        if !response.is_success() {
            return Err(PluginError::Execution(format!(
                "The webhook answered {}: {}",
                response.status,
                response.body.chars().take(200).collect::<String>()
            )));
        }
        Ok(())
    }
}

aegis_plugin_sdk::register_plugin!(OrderNotifierPlugin);

#[cfg(test)]
mod tests {
    use super::*;
    use aegis_plugin_sdk::host::mock;

    fn change(to: &str) -> OrderStatusChange {
        OrderStatusChange {
            order: OrderContext {
                order_number: Some("261023-ABC234".to_string()),
                currency: "jpy".to_string(),
                lines: vec![OrderLine {
                    title: "Mug".to_string(),
                    quantity: 2,
                    ..Default::default()
                }],
                total: 2900,
                customer_name: "Buyer".to_string(),
                ..Default::default()
            },
            from_status: None,
            to_status: Some(to.to_string()),
            occurred_at: "2026-10-23T00:00:00Z".to_string(),
        }
    }

    #[test]
    fn test_money() {
        assert_eq!(money(1_234_567, "jpy"), "¥1,234,567");
        assert_eq!(money(1250, "usd"), "USD 12.50");
        assert_eq!(money(5, "eur"), "EUR 0.05");
    }

    #[test]
    fn test_posts_selected_statuses_to_slack() {
        mock::reset();
        mock::set_config("webhook_url", "https://hooks.slack.com/services/T/B/X");
        let mut plugin = OrderNotifierPlugin;

        plugin
            .on_order_status_changed(change("pending_payment"))
            .unwrap();
        plugin.on_order_status_changed(change("shipped")).unwrap();
        let requests = mock::take_http_requests();
        assert_eq!(requests.len(), 1, "shipped is not in the default notify_on");
        assert_eq!(requests[0].method, "POST");
        assert_eq!(requests[0].url, "https://hooks.slack.com/services/T/B/X");
        let body: serde_json::Value =
            serde_json::from_str(requests[0].body.as_deref().unwrap()).unwrap();
        let text = body["text"].as_str().unwrap();
        assert!(text.contains("261023-ABC234"), "{}", text);
        assert!(text.contains("¥2,900"), "{}", text);
        assert!(text.contains("Mug × 2"), "{}", text);

        mock::set_config("format", "json");
        mock::set_config("notify_on", "shipped");
        plugin.on_order_status_changed(change("shipped")).unwrap();
        let requests = mock::take_http_requests();
        let body: serde_json::Value =
            serde_json::from_str(requests[0].body.as_deref().unwrap()).unwrap();
        assert_eq!(body["to_status"], "shipped");
    }

    #[test]
    fn test_missing_webhook_url_is_an_error() {
        mock::reset();
        let mut plugin = OrderNotifierPlugin;
        assert!(plugin.on_order_status_changed(change("paid")).is_err());
    }
}
