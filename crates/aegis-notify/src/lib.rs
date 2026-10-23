//! 通知（`[notifications]`）。`aegis_core::Notifier` の実装。
//!
//! 通知を出す側（EC など）は `NotificationEvent`（種類・宛先・テンプレートに渡す値）を渡すだけで、
//! ここで宛先ごとに件名・本文をテンプレートから作り、設定された配信チャネル（メール・ログ）で送る。
//! 配信はバックグラウンドで行い、一時的な失敗は間隔を空けて再試行する。結果は監査ログ
//! （`notification.send`）に残す。配信待ちはメモリ上にあるため、再起動すると送られずに失われる

pub mod channel;
pub mod config;
pub mod email;
pub mod log;
pub mod templates;

use std::sync::Arc;
use std::time::Duration;

use aegis_audit::{AuditOrigin, AuditService};
use aegis_core::{AuditAction, AuditOutcome, NotificationEvent, Notifier, Recipient, Result};
use serde_json::json;

pub use channel::{NotificationChannel, NotificationMessage, SendError};
pub use config::{ChannelKind, EmailConfig, NotificationsConfig, SmtpTls};
pub use email::EmailChannel;
pub use log::LogChannel;
pub use templates::{Rendered, Templates};

/// テンプレートに渡すサイトの情報（`site_name` / `site_url` / `admin_url`）
#[derive(Debug, Clone, Default)]
pub struct SiteInfo {
    pub name: String,
    /// 公開サイトの URL（空なら本文に出さない）
    pub url: String,
    /// Admin Studio の URL（運営者向けの通知に出す。空なら出さない）
    pub admin_url: String,
}

/// 1 つの宛先・チャネルへの配信結果
#[derive(Debug, Clone)]
pub struct Delivery {
    pub channel: String,
    pub recipient: Recipient,
    pub subject: String,
    pub attempts: u32,
    pub result: std::result::Result<(), SendError>,
}

#[derive(Clone)]
pub struct NotificationService {
    inner: Arc<Inner>,
}

struct Inner {
    config: NotificationsConfig,
    site: SiteInfo,
    templates: Templates,
    channels: Vec<Arc<dyn NotificationChannel>>,
    audit: Option<Arc<AuditService>>,
    /// 再試行の間隔の基準（1 回目の失敗の後に待つ時間。以降 4 倍ずつ）
    retry_delay: Duration,
}

impl NotificationService {
    /// 設定（`channels`）のチャネルを作る。設定の検証も行う
    pub fn from_config(
        config: &NotificationsConfig,
        site: SiteInfo,
        audit: Option<Arc<AuditService>>,
    ) -> Result<Self> {
        config.validate()?;
        let mut channels: Vec<Arc<dyn NotificationChannel>> = Vec::new();
        for kind in &config.channels {
            match kind {
                ChannelKind::Email => channels.push(Arc::new(EmailChannel::new(&config.email)?)),
                ChannelKind::Log => channels.push(Arc::new(LogChannel)),
            }
        }
        Ok(Self::with_channels(config, site, audit, channels))
    }

    /// チャネルを直接渡して作る（独自のチャネル・テスト用）
    pub fn with_channels(
        config: &NotificationsConfig,
        site: SiteInfo,
        audit: Option<Arc<AuditService>>,
        channels: Vec<Arc<dyn NotificationChannel>>,
    ) -> Self {
        Self {
            inner: Arc::new(Inner {
                templates: Templates::new(&config.locale, config.templates_dir.as_deref()),
                config: config.clone(),
                site,
                channels,
                audit,
                retry_delay: Duration::from_secs(2),
            }),
        }
    }

    /// 再試行の間隔を変える（テスト用）
    pub fn with_retry_delay(self, delay: Duration) -> Self {
        let inner = Arc::try_unwrap(self.inner).unwrap_or_else(|_| {
            panic!("with_retry_delay must be called before the service is shared")
        });
        Self {
            inner: Arc::new(Inner {
                retry_delay: delay,
                ..inner
            }),
        }
    }

    pub fn channel_names(&self) -> Vec<String> {
        self.inner
            .channels
            .iter()
            .map(|c| c.name().to_string())
            .collect()
    }

    /// 通知を送り、すべての配信が終わるまで待って結果を返す（CLI のテスト送信など）。
    /// 無効な種類（`disabled_events`）・テンプレートの無い種類は何も送らない
    pub async fn deliver(&self, event: &NotificationEvent) -> Vec<Delivery> {
        let inner = &self.inner;
        if !inner.config.is_event_enabled(&event.kind) {
            return Vec::new();
        }
        let context = self.context(event);
        let mut deliveries = Vec::new();
        for recipient in &event.recipients {
            let channels: Vec<_> = inner
                .channels
                .iter()
                .filter(|c| c.accepts(recipient))
                .collect();
            if channels.is_empty() {
                continue;
            }
            let rendered = match inner.templates.render(&event.kind, recipient, &context) {
                Ok(Some(rendered)) => rendered,
                Ok(None) => {
                    tracing::debug!("No notification template for '{}'", event.kind);
                    continue;
                }
                Err(e) => {
                    tracing::error!("{}", e);
                    continue;
                }
            };
            let message = NotificationMessage {
                kind: event.kind.clone(),
                recipient: recipient.clone(),
                subject: rendered.subject,
                body: rendered.body,
            };
            for channel in channels {
                let delivery = self.send_with_retry(channel.as_ref(), &message).await;
                self.record(event, &delivery).await;
                deliveries.push(delivery);
            }
        }
        deliveries
    }

    async fn send_with_retry(
        &self,
        channel: &dyn NotificationChannel,
        message: &NotificationMessage,
    ) -> Delivery {
        let max_attempts = self.inner.config.max_attempts.max(1);
        let mut attempts = 0;
        let result = loop {
            attempts += 1;
            match channel.send(message).await {
                Ok(()) => break Ok(()),
                Err(e) if e.permanent || attempts >= max_attempts => break Err(e),
                Err(e) => {
                    let delay = self.inner.retry_delay * 4u32.pow(attempts - 1);
                    tracing::warn!(
                        "Notification '{}' via {} failed (attempt {}/{}), retrying in {:?}: {}",
                        message.kind,
                        channel.name(),
                        attempts,
                        max_attempts,
                        delay,
                        e
                    );
                    tokio::time::sleep(delay).await;
                }
            }
        };
        if let Err(e) = &result {
            tracing::error!(
                "Notification '{}' via {} was not delivered: {}",
                message.kind,
                channel.name(),
                e
            );
        }
        Delivery {
            channel: channel.name().to_string(),
            recipient: message.recipient.clone(),
            subject: message.subject.clone(),
            attempts,
            result,
        }
    }

    /// テンプレートに渡す値（イベントの値 + サイトの情報）
    fn context(&self, event: &NotificationEvent) -> serde_json::Value {
        let site = &self.inner.site;
        let mut context = match &event.data {
            serde_json::Value::Object(map) => map.clone(),
            serde_json::Value::Null => serde_json::Map::new(),
            other => {
                let mut map = serde_json::Map::new();
                map.insert("data".to_string(), other.clone());
                map
            }
        };
        for (key, value) in [
            ("kind", event.kind.as_str()),
            ("site_name", site.name.as_str()),
            ("site_url", site.url.as_str()),
            ("admin_url", site.admin_url.as_str()),
        ] {
            context
                .entry(key)
                .or_insert_with(|| serde_json::Value::String(value.to_string()));
        }
        serde_json::Value::Object(context)
    }

    async fn record(&self, event: &NotificationEvent, delivery: &Delivery) {
        let Some(audit) = &self.inner.audit else {
            return;
        };
        let (outcome, error) = match &delivery.result {
            Ok(()) => (AuditOutcome::Success, None),
            Err(e) => (AuditOutcome::Failure, Some(e.message.clone())),
        };
        // 監査ログには宛先のアドレスを残さない（顧客か運営者かだけ）
        let recipient = match delivery.recipient {
            Recipient::Person { .. } => "person",
            Recipient::Staff => "staff",
        };
        let mut details = json!({
            "channel": delivery.channel,
            "recipient": recipient,
            "subject": delivery.subject,
            "attempts": delivery.attempts,
            "error": error,
        });
        if let Some(number) = event.data.pointer("/order/number") {
            details["order_number"] = number.clone();
        }
        audit
            .record(
                AuditOrigin::default()
                    .event(AuditAction::NotificationSend, outcome)
                    .with_resource("notification", Some(event.kind.clone()))
                    .with_details(details),
            )
            .await;
    }
}

impl Notifier for NotificationService {
    fn notify(&self, event: NotificationEvent) {
        if !self.inner.config.is_event_enabled(&event.kind) {
            return;
        }
        let Ok(runtime) = tokio::runtime::Handle::try_current() else {
            tracing::error!(
                "Notification '{}' dropped: no async runtime to deliver it",
                event.kind
            );
            return;
        };
        let service = self.clone();
        runtime.spawn(async move {
            service.deliver(&event).await;
        });
    }
}

#[cfg(test)]
mod tests;
