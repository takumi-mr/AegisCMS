//! AegisCMS 本体のテストのためのプラグイン（同梱しない・ストアに載せない）。
//!
//! 設定:
//! - `webhook_url`: 送り先
//! - `notify_on`: 注文がこの状態（カンマ区切り）になったら、`{"status", "number", "total", "currency"}` を送る
//!
//! 通知のチャネルとしては、運営者向けの通知だけを `{"text": "*件名*\n本文"}` で送り、ほかは `Skipped`。
//! 4xx（429 を除く）は設定の誤りとして `Rejected`（ホストは再試行しない）、それ以外の失敗は `Execution`

use aegis_plugin_sdk::prelude::*;

fn webhook_url() -> Result<String> {
    config::get("webhook_url")
        .map(|url| url.trim().to_string())
        .filter(|url| !url.is_empty())
        .ok_or_else(|| PluginError::Rejected("webhook_url is not configured".to_string()))
}

fn post(body: &aegis_plugin_sdk::serde_json::Value) -> Result<()> {
    let response = http::fetch(&http::Request::post_json(&webhook_url()?, body)?)?;
    if response.is_success() {
        return Ok(());
    }
    let detail = format!("The webhook answered {}", response.status);
    if (400..500).contains(&response.status) && response.status != 429 {
        Err(PluginError::Rejected(detail))
    } else {
        Err(PluginError::Execution(detail))
    }
}

#[derive(Default)]
pub struct TestNotifyPlugin;

impl Plugin for TestNotifyPlugin {
    fn send_notification(&mut self, message: NotificationMessage) -> Result<Delivery> {
        if message.recipient != Recipient::Staff {
            return Ok(Delivery::Skipped);
        }
        let text = format!("*{}*\n{}", message.subject, message.body.trim_end());
        post(&aegis_plugin_sdk::serde_json::json!({ "text": text }))?;
        Ok(Delivery::Sent)
    }

    fn on_order_status_changed(&mut self, change: OrderStatusChange) -> Result<()> {
        let notify_on = config::get("notify_on").unwrap_or_default();
        let wanted = change
            .to_status
            .as_deref()
            .is_some_and(|status| notify_on.split(',').any(|s| s.trim() == status));
        if !wanted {
            return Ok(());
        }
        post(&aegis_plugin_sdk::serde_json::json!({
            "status": change.to_status,
            "number": change.order.order_number,
            "total": change.order.total,
            "currency": change.order.currency,
        }))
    }
}

aegis_plugin_sdk::register_plugin!(TestNotifyPlugin);

#[cfg(test)]
mod tests {
    use super::*;
    use aegis_plugin_sdk::host::mock;

    fn message(recipient: Recipient) -> NotificationMessage {
        NotificationMessage {
            kind: "order.placed".to_string(),
            recipient,
            subject: "Subject".to_string(),
            body: "Body\n".to_string(),
            html: None,
            data: aegis_plugin_sdk::serde_json::Value::Null,
        }
    }

    #[test]
    fn posts_staff_notifications_only() {
        mock::reset();
        mock::set_config("webhook_url", "https://hooks.example.com/x");
        let sent = TestNotifyPlugin
            .send_notification(message(Recipient::Staff))
            .unwrap();
        assert_eq!(sent, Delivery::Sent);
        let requests = mock::take_http_requests();
        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0].url, "https://hooks.example.com/x");

        let skipped = TestNotifyPlugin
            .send_notification(message(Recipient::Person {
                email: "a@example.com".to_string(),
                name: None,
            }))
            .unwrap();
        assert_eq!(skipped, Delivery::Skipped);
        assert!(mock::take_http_requests().is_empty());
    }

    #[test]
    fn posts_only_the_listed_order_statuses() {
        mock::reset();
        mock::set_config("webhook_url", "https://hooks.example.com/x");
        mock::set_config("notify_on", "paid");
        let change = |to: &str| OrderStatusChange {
            to_status: Some(to.to_string()),
            ..Default::default()
        };
        TestNotifyPlugin
            .on_order_status_changed(change("pending"))
            .unwrap();
        assert!(mock::take_http_requests().is_empty());
        TestNotifyPlugin
            .on_order_status_changed(change("paid"))
            .unwrap();
        assert_eq!(mock::take_http_requests().len(), 1);
    }
}
