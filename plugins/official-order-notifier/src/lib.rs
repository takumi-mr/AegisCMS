//! 公式プラグイン: Slack / Webhook への通知。
//!
//! - `on-order-status-changed`: 注文の作成・状態の変化を送る（`notify_on` の状態のみ）
//! - `send-notification`: 通知のチャネルになる（サーバーの `[notifications].channels` に
//!   `"plugin:official-order-notifier"`）。ホストがテンプレートから作った通知（注文・監査イベント）を送る。
//!   チャネルとして使うなら、注文が二重に届かないよう `notify_on = "none"` にする
//!
//! 設定（サーバーの `[plugins.settings.official-order-notifier]`）:
//!
//! | キー | 内容 |
//! | --- | --- |
//! | `webhook_url` | 送信先（必須）。Slack の Incoming Webhook など。`env:SLACK_WEBHOOK_URL` で環境変数から |
//! | `format` | `slack`（既定、`{"text": ...}`）か `json`（`OrderStatusChange` / `NotificationMessage` をそのまま送る） |
//! | `notify_on` | 通知する状態（カンマ区切り。既定 `pending_payment,paid,cancelled`。`none` で送らない） |
//! | `channel_recipients` | チャネルとして送る宛先: `staff`（既定。運営者向けの通知だけ）か `all`（顧客向けも） |
//! | `channel_kinds` | チャネルとして送る通知の種類（カンマ区切りの前方一致。例 `audit.,order.placed`。空ならすべて） |
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

/// チャネルとして送るか（`channel_recipients` / `channel_kinds`）
fn wants_notification(message: &NotificationMessage) -> bool {
    let all_recipients = config::get("channel_recipients")
        .is_some_and(|value| value.trim().eq_ignore_ascii_case("all"));
    if !all_recipients && message.recipient != Recipient::Staff {
        return false;
    }
    let kinds = config::get("channel_kinds").unwrap_or_default();
    let prefixes: Vec<&str> = kinds
        .split(',')
        .map(str::trim)
        .filter(|k| !k.is_empty())
        .collect();
    prefixes.is_empty() || prefixes.iter().any(|p| message.kind.starts_with(p))
}

/// Slack の 1 メッセージに収める（本文が長すぎると Slack が断る）
fn slack_message_text(message: &NotificationMessage) -> String {
    const MAX: usize = 3500;
    let mut text = format!("*{}*\n{}", message.subject, message.body.trim_end());
    if text.chars().count() > MAX {
        text = text.chars().take(MAX).collect::<String>() + "\n…";
    }
    text
}

/// Webhook に POST する。4xx（429 を除く）は設定の誤りなので再試行しない
fn post(url: &str, body: &serde_json::Value) -> Result<()> {
    let response = http::fetch(&http::Request::post_json(url, body)?)?;
    if response.is_success() {
        return Ok(());
    }
    let detail = format!(
        "The webhook answered {}: {}",
        response.status,
        response.body.chars().take(200).collect::<String>()
    );
    if (400..500).contains(&response.status) && response.status != 429 {
        Err(PluginError::Rejected(detail))
    } else {
        Err(PluginError::Execution(detail))
    }
}

fn webhook_url() -> Result<String> {
    config::get("webhook_url")
        .map(|url| url.trim().to_string())
        .filter(|url| !url.is_empty())
        .ok_or_else(|| PluginError::Rejected("webhook_url is not configured".to_string()))
}

impl Plugin for OrderNotifierPlugin {
    fn send_notification(&mut self, message: NotificationMessage) -> Result<Delivery> {
        if !wants_notification(&message) {
            return Ok(Delivery::Skipped);
        }
        let url = webhook_url()?;
        let body = match config::get("format").as_deref().map(str::trim) {
            Some("json") => serde_json::to_value(&message)?,
            _ => serde_json::json!({ "text": slack_message_text(&message) }),
        };
        post(&url, &body)?;
        Ok(Delivery::Sent)
    }

    fn on_order_status_changed(&mut self, change: OrderStatusChange) -> Result<()> {
        // 空の設定値はホストから「未設定」として届くため、止めるときは `none` と書く
        let notify_on = config::get("notify_on").unwrap_or_else(|| DEFAULT_NOTIFY_ON.to_string());
        if notify_on.trim().eq_ignore_ascii_case("none") {
            return Ok(());
        }
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

    fn notification(kind: &str, recipient: Recipient) -> NotificationMessage {
        NotificationMessage {
            kind: kind.to_string(),
            recipient,
            html: None,
            subject: "【Shop】ログインをロックしました（root）".to_string(),
            body: "日時: 2026-10-23T05:00:00Z\nIP アドレス: 203.0.113.7\n".to_string(),
            data: serde_json::json!({ "audit": { "action": "session.create" } }),
        }
    }

    #[test]
    fn test_sends_staff_notifications_as_a_channel() {
        mock::reset();
        mock::set_config("webhook_url", "https://hooks.slack.com/services/T/B/X");
        let mut plugin = OrderNotifierPlugin;

        let sent = plugin
            .send_notification(notification("audit.session.create", Recipient::Staff))
            .unwrap();
        assert_eq!(sent, Delivery::Sent);
        let requests = mock::take_http_requests();
        assert_eq!(requests.len(), 1);
        let body: serde_json::Value =
            serde_json::from_str(requests[0].body.as_deref().unwrap()).unwrap();
        assert_eq!(
            body["text"],
            "*【Shop】ログインをロックしました（root）*\n日時: 2026-10-23T05:00:00Z\nIP アドレス: 203.0.113.7"
        );

        // 顧客向けは既定では送らない
        let customer = Recipient::Person {
            email: "hanako@example.com".to_string(),
            name: None,
        };
        assert_eq!(
            plugin
                .send_notification(notification("order.shipped", customer.clone()))
                .unwrap(),
            Delivery::Skipped
        );
        mock::set_config("channel_recipients", "all");
        assert_eq!(
            plugin
                .send_notification(notification("order.shipped", customer))
                .unwrap(),
            Delivery::Sent
        );

        // 種類で絞る
        mock::set_config("channel_kinds", "audit., order.placed");
        assert_eq!(
            plugin
                .send_notification(notification("order.shipped", Recipient::Staff))
                .unwrap(),
            Delivery::Skipped
        );
        assert_eq!(
            plugin
                .send_notification(notification("order.placed", Recipient::Staff))
                .unwrap(),
            Delivery::Sent
        );

        // json 形式は通知をそのまま送る
        mock::take_http_requests();
        mock::set_config("format", "json");
        plugin
            .send_notification(notification("audit.session.create", Recipient::Staff))
            .unwrap();
        let requests = mock::take_http_requests();
        let body: serde_json::Value =
            serde_json::from_str(requests[0].body.as_deref().unwrap()).unwrap();
        assert_eq!(body["kind"], "audit.session.create");
        assert_eq!(body["recipient"]["type"], "staff");
        assert_eq!(body["data"]["audit"]["action"], "session.create");
    }

    #[test]
    fn test_channel_errors_tell_the_host_whether_to_retry() {
        mock::reset();
        let mut plugin = OrderNotifierPlugin;
        let error = plugin
            .send_notification(notification("audit.session.create", Recipient::Staff))
            .unwrap_err();
        assert!(
            error.is_permanent(),
            "a missing webhook_url will not fix itself"
        );
    }

    #[test]
    fn test_notify_on_none_disables_the_order_hook() {
        mock::reset();
        mock::set_config("webhook_url", "https://hooks.slack.com/services/T/B/X");
        mock::set_config("notify_on", "none");
        let mut plugin = OrderNotifierPlugin;
        plugin
            .on_order_status_changed(change("pending_payment"))
            .unwrap();
        assert!(mock::take_http_requests().is_empty());
    }

    #[test]
    fn test_missing_webhook_url_is_an_error() {
        mock::reset();
        let mut plugin = OrderNotifierPlugin;
        assert!(plugin.on_order_status_changed(change("paid")).is_err());
    }
}
