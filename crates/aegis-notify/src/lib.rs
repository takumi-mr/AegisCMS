//! 通知（`[notifications]`）。`aegis_core::Notifier` の実装。
//!
//! 通知を出す側（EC など）は `NotificationEvent`（種類・宛先・テンプレートに渡す値）を渡すだけで、
//! ここで宛先ごとに件名・本文（プレーンテキストと HTML）をテンプレートから作り、設定された配信チャネル
//! （メール・ログ・プラグイン）で送る。結果は監査ログ（`notification.send`）に残す。
//!
//! 送信待ちは DB（`with_queue`、`_aegis_notification_queue`）に宛先・チャネルごとに保存し、
//! `spawn_worker` のワーカーが送る。一時的な失敗は間隔を空けて再試行し、再起動しても失われない。
//! 複数台のサーバーでは、条件付きの UPDATE で送信の権利を取った 1 台だけが送る。DB を使わない場合
//! （`[notifications.queue].persistent = false`・CLI）はメモリ上で送る（再起動すると失われる）

pub mod audit;
pub mod channel;
pub mod config;
pub mod email;
pub mod html;
pub mod log;
pub mod samples;
pub mod templates;

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use aegis_audit::{AuditOrigin, AuditService};
use aegis_core::{
    format_audit_timestamp, AuditAction, AuditOutcome, NotificationEvent, Notifier, Recipient,
    Result,
};
use aegis_db::{NotificationQueueStorage, NotificationRelease, QueuedNotification};
use chrono::{DateTime, Utc};
use serde_json::json;

pub use audit::AuditNotificationSink;
pub use channel::{Delivered, NotificationChannel, NotificationMessage, SendError};
pub use config::{
    AuditNotificationsConfig, AuditRule, ChannelKind, EmailConfig, NotificationsConfig,
    QueueConfig, SmtpTls,
};
pub use email::EmailChannel;
pub use log::LogChannel;
pub use templates::{Rendered, TemplateInfo, TemplateSource, Templates};

/// 1 回に取り出す送信待ちの数（まとめて並行に送る）
const QUEUE_BATCH: u64 = 20;
/// 送っている最中の通知を、ほかのサーバーが送り直さずに待つ時間。送信中にサーバーが止まった通知は
/// この時間の後に送り直す（SMTP のタイムアウトより十分長く）
const QUEUE_LEASE: Duration = Duration::from_secs(300);
/// 送れなかった古い通知を消す間隔
const QUEUE_PRUNE_INTERVAL: Duration = Duration::from_secs(3600);

/// メールアドレスとして正しいか（`hanako@example.com`。名前付きの書き方は含めない）
pub fn is_valid_address(value: &str) -> bool {
    value.trim().parse::<lettre::Address>().is_ok()
}

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
    /// 送信待ちの保存先（無ければメモリ上で送る）
    queue: Option<Arc<dyn NotificationQueueStorage>>,
    /// 新しい送信待ちができたことをワーカーに知らせる
    wake: tokio::sync::Notify,
}

impl NotificationService {
    /// 設定（`channels`）のチャネルを作る。設定の検証も行う。プラグインのチャネル
    /// （`plugin:<名前>`）は使えない（`from_config_with_plugins` を使う）
    pub fn from_config(
        config: &NotificationsConfig,
        site: SiteInfo,
        audit: Option<Arc<AuditService>>,
    ) -> Result<Self> {
        Self::from_config_with_plugins(config, site, audit, |name| {
            Err(aegis_core::AegisError::Config(format!(
                "[notifications].channels: plugin channel \"plugin:{}\" is not available here",
                name
            )))
        })
    }

    /// `from_config` と同じで、プラグインのチャネル（`plugin:<名前>`）を `plugin_channel` で作る
    pub fn from_config_with_plugins(
        config: &NotificationsConfig,
        site: SiteInfo,
        audit: Option<Arc<AuditService>>,
        plugin_channel: impl Fn(&str) -> Result<Arc<dyn NotificationChannel>>,
    ) -> Result<Self> {
        config.validate()?;
        let mut channels: Vec<Arc<dyn NotificationChannel>> = Vec::new();
        for kind in &config.channels {
            match kind {
                ChannelKind::Email => channels.push(Arc::new(EmailChannel::new(&config.email)?)),
                ChannelKind::Log => channels.push(Arc::new(LogChannel)),
                ChannelKind::Plugin(name) => channels.push(plugin_channel(name)?),
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
                templates: Templates::new(&config.locale, config.templates_dir.as_deref())
                    .with_html(config.html),
                config: config.clone(),
                site,
                channels,
                audit,
                retry_delay: Duration::from_secs(2),
                queue: None,
                wake: tokio::sync::Notify::new(),
            }),
        }
    }

    fn rebuild(self, change: impl FnOnce(&mut Inner)) -> Self {
        let mut inner = Arc::try_unwrap(self.inner).unwrap_or_else(|_| {
            panic!("NotificationService must be configured before it is shared")
        });
        change(&mut inner);
        Self {
            inner: Arc::new(inner),
        }
    }

    /// 再試行の間隔を変える（テスト用）
    pub fn with_retry_delay(self, delay: Duration) -> Self {
        self.rebuild(|inner| inner.retry_delay = delay)
    }

    /// 送信待ちを DB に保存する（`notify` は保存して戻り、`spawn_worker` のワーカーが送る）
    pub fn with_queue(self, queue: Arc<dyn NotificationQueueStorage>) -> Self {
        self.rebuild(|inner| inner.queue = Some(queue))
    }

    pub fn config(&self) -> &NotificationsConfig {
        &self.inner.config
    }

    pub fn site(&self) -> &SiteInfo {
        &self.inner.site
    }

    pub fn templates(&self) -> &Templates {
        &self.inner.templates
    }

    /// 送信待ちを DB に保存しているか
    pub fn is_persistent(&self) -> bool {
        self.inner.queue.is_some()
    }

    pub fn channel_names(&self) -> Vec<String> {
        self.inner
            .channels
            .iter()
            .map(|c| c.name().to_string())
            .collect()
    }

    /// Admin Studio で編集したテンプレートを差し替える（起動時・編集時・ほかのサーバーでの編集の知らせ）
    pub fn set_template_overrides(&self, overrides: HashMap<String, String>) {
        self.inner.templates.set_overrides(overrides);
    }

    /// 宛先ごとの通知と、それを送るチャネル。無効な種類（`disabled_events`）・テンプレートの無い種類・
    /// 受け付けるチャネルの無い宛先は含めない
    fn messages(
        &self,
        event: &NotificationEvent,
    ) -> Vec<(NotificationMessage, Vec<Arc<dyn NotificationChannel>>)> {
        let inner = &self.inner;
        if !inner.config.is_event_enabled(&event.kind) {
            return Vec::new();
        }
        let context = self.context(event);
        let mut messages = Vec::new();
        for recipient in &event.recipients {
            let channels: Vec<_> = inner
                .channels
                .iter()
                .filter(|c| c.accepts(recipient))
                .cloned()
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
            messages.push((
                NotificationMessage {
                    kind: event.kind.clone(),
                    recipient: recipient.clone(),
                    subject: rendered.subject,
                    body: rendered.body,
                    html: rendered.html,
                    data: context.clone(),
                },
                channels,
            ));
        }
        messages
    }

    /// 通知を送り、すべての配信が終わるまで待って結果を返す（CLI・Admin Studio のテスト送信など）。
    /// 送信待ちには入れない。無効な種類（`disabled_events`）・テンプレートの無い種類は何も送らない
    pub async fn deliver(&self, event: &NotificationEvent) -> Vec<Delivery> {
        self.deliver_with(event, self.inner.config.max_attempts.max(1))
            .await
    }

    /// `deliver` と同じで、失敗しても再試行しない（Admin Studio のテスト送信。結果をすぐ返す）
    pub async fn deliver_now(&self, event: &NotificationEvent) -> Vec<Delivery> {
        self.deliver_with(event, 1).await
    }

    async fn deliver_with(&self, event: &NotificationEvent, max_attempts: u32) -> Vec<Delivery> {
        let mut deliveries = Vec::new();
        for (message, channels) in self.messages(event) {
            for channel in channels {
                // チャネルが扱わない通知（`Skipped`）は結果に含めず、監査ログにも残さない
                if let Some(delivery) = self
                    .send_with_retry(channel.as_ref(), &message, max_attempts)
                    .await
                {
                    self.record(&message, &delivery).await;
                    deliveries.push(delivery);
                }
            }
        }
        deliveries
    }

    async fn send_with_retry(
        &self,
        channel: &dyn NotificationChannel,
        message: &NotificationMessage,
        max_attempts: u32,
    ) -> Option<Delivery> {
        let mut attempts = 0;
        let result = loop {
            attempts += 1;
            match channel.send(message).await {
                Ok(Delivered::Sent) => break Ok(()),
                Ok(Delivered::Skipped) => return None,
                Err(e) if e.permanent || attempts >= max_attempts => break Err(e),
                Err(e) => {
                    let delay = self.retry_delay(attempts);
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
        Some(Delivery {
            channel: channel.name().to_string(),
            recipient: message.recipient.clone(),
            subject: message.subject.clone(),
            attempts,
            result,
        })
    }

    /// `attempts` 回目の失敗の後に待つ時間（2 秒・8 秒・32 秒…）
    fn retry_delay(&self, attempts: u32) -> Duration {
        self.inner
            .retry_delay
            .saturating_mul(4u32.saturating_pow(attempts.saturating_sub(1)))
    }

    /// 通知を宛先・チャネルごとに送信待ちへ入れ、ワーカーを起こす。入れた数を返す
    pub async fn enqueue(&self, event: &NotificationEvent) -> Result<usize> {
        let Some(queue) = &self.inner.queue else {
            return Err(aegis_core::AegisError::Internal(
                "The notification queue is not configured".to_string(),
            ));
        };
        let now = format_audit_timestamp(&Utc::now());
        let mut items = Vec::new();
        for (message, channels) in self.messages(event) {
            let json = serde_json::to_string(&message)
                .map_err(|e| aegis_core::AegisError::Internal(e.to_string()))?;
            for channel in channels {
                items.push(QueuedNotification {
                    id: uuid::Uuid::new_v4().to_string(),
                    kind: message.kind.clone(),
                    channel: channel.name().to_string(),
                    message: json.clone(),
                    status: aegis_db::QUEUE_PENDING.to_string(),
                    attempts: 0,
                    next_attempt_at: now.clone(),
                    locked_until: None,
                    last_error: None,
                    created_at: now.clone(),
                    updated_at: now.clone(),
                });
            }
        }
        queue.enqueue_notifications(&items).await?;
        if !items.is_empty() {
            self.wake();
        }
        Ok(items.len())
    }

    /// ワーカーに送信待ちを確かめさせる（Admin Studio からの再送の後など）
    pub fn wake(&self) {
        self.inner.wake.notify_one();
    }

    /// 送信待ちを送り続けるワーカーを起動する（送信待ちを DB に保存しているときのみ）。
    /// 新しい通知は `notify` がすぐに起こし、再試行の時刻は `poll_interval_secs` ごとに確かめる
    pub fn spawn_worker(&self) -> Option<tokio::task::JoinHandle<()>> {
        let queue = self.inner.queue.clone()?;
        let service = self.clone();
        Some(tokio::spawn(async move {
            let poll = Duration::from_secs(service.inner.config.queue.poll_interval_secs.max(1));
            let mut last_prune: Option<tokio::time::Instant> = None;
            loop {
                service.process_queue().await;
                if last_prune.is_none_or(|at| at.elapsed() >= QUEUE_PRUNE_INTERVAL) {
                    last_prune = Some(tokio::time::Instant::now());
                    service.prune_queue(queue.as_ref(), Utc::now()).await;
                }
                tokio::select! {
                    _ = tokio::time::sleep(poll) => {}
                    _ = service.inner.wake.notified() => {}
                }
            }
        }))
    }

    /// 送る時刻を迎えた送信待ちをすべて送り、試した数を返す
    pub async fn process_queue(&self) -> usize {
        let Some(queue) = &self.inner.queue else {
            return 0;
        };
        let mut attempted = 0;
        // 取り出しても権利を取れないもの（ほかのサーバーが送っている）は次から外れるので、回数は念のため
        for _ in 0..100 {
            let now = Utc::now();
            let due = match queue
                .due_notifications(&format_audit_timestamp(&now), QUEUE_BATCH)
                .await
            {
                Ok(due) => due,
                Err(e) => {
                    tracing::warn!("Could not read the notification queue: {}", e);
                    break;
                }
            };
            let count = due.len();
            let mut tasks = tokio::task::JoinSet::new();
            for item in due {
                let lease = now + chrono::Duration::from_std(QUEUE_LEASE).unwrap_or_default();
                match queue
                    .claim_notification(
                        &item.id,
                        &format_audit_timestamp(&now),
                        &format_audit_timestamp(&lease),
                    )
                    .await
                {
                    Ok(true) => {}
                    Ok(false) => continue,
                    Err(e) => {
                        tracing::warn!("Could not take queued notification {}: {}", item.id, e);
                        continue;
                    }
                }
                let service = self.clone();
                let queue = queue.clone();
                tasks.spawn(async move { service.attempt(queue.as_ref(), item).await });
            }
            while let Some(result) = tasks.join_next().await {
                if result.is_ok() {
                    attempted += 1;
                }
            }
            if (count as u64) < QUEUE_BATCH {
                break;
            }
        }
        attempted
    }

    /// 権利を取った送信待ちを 1 回送り、結果を DB に残す
    async fn attempt(&self, queue: &dyn NotificationQueueStorage, item: QueuedNotification) {
        let attempts = item.attempts.saturating_add(1);
        let max_attempts = self.inner.config.max_attempts.max(1);
        let message = serde_json::from_str::<NotificationMessage>(&item.message);
        let channel = self
            .inner
            .channels
            .iter()
            .find(|c| c.name() == item.channel)
            .cloned();
        let result = match (&message, channel) {
            (Err(e), _) => Err(SendError::permanent(format!(
                "The queued notification is broken: {}",
                e
            ))),
            (_, None) => Err(SendError::permanent(format!(
                "Channel \"{}\" is no longer in [notifications].channels",
                item.channel
            ))),
            (Ok(message), Some(channel)) => channel.send(message).await,
        };
        let now = Utc::now();
        let outcome = match result {
            Ok(Delivered::Skipped) => None,
            Ok(Delivered::Sent) => Some(Ok(())),
            Err(e) if e.permanent || attempts >= max_attempts => {
                tracing::error!(
                    "Notification '{}' via {} was not delivered: {}",
                    item.kind,
                    item.channel,
                    e
                );
                Some(Err(e))
            }
            Err(e) => {
                let delay = self.retry_delay(attempts);
                tracing::warn!(
                    "Notification '{}' via {} failed (attempt {}/{}), retrying in {:?}: {}",
                    item.kind,
                    item.channel,
                    attempts,
                    max_attempts,
                    delay,
                    e
                );
                let next = now + chrono::Duration::from_std(delay).unwrap_or_default();
                self.release(
                    queue,
                    &item,
                    aegis_db::QUEUE_PENDING,
                    attempts,
                    next,
                    now,
                    &e,
                )
                .await;
                return;
            }
        };
        match &outcome {
            // 送れた（扱わない通知も含む）: 消す
            None | Some(Ok(())) => {
                if let Err(e) = queue.delete_notification(&item.id).await {
                    tracing::error!(
                        "Notification {} was delivered but could not be removed from the queue (it may be sent again): {}",
                        item.id,
                        e
                    );
                }
            }
            Some(Err(e)) => {
                self.release(queue, &item, aegis_db::QUEUE_FAILED, attempts, now, now, e)
                    .await
            }
        }
        if let (Some(result), Ok(message)) = (outcome, &message) {
            let delivery = Delivery {
                channel: item.channel.clone(),
                recipient: message.recipient.clone(),
                subject: message.subject.clone(),
                attempts,
                result,
            };
            self.record(message, &delivery).await;
        }
    }

    #[allow(clippy::too_many_arguments)]
    async fn release(
        &self,
        queue: &dyn NotificationQueueStorage,
        item: &QueuedNotification,
        status: &str,
        attempts: u32,
        next_attempt_at: DateTime<Utc>,
        now: DateTime<Utc>,
        error: &SendError,
    ) {
        let release = NotificationRelease {
            id: item.id.clone(),
            status: status.to_string(),
            attempts,
            next_attempt_at: format_audit_timestamp(&next_attempt_at),
            last_error: Some(error.message.clone()),
            updated_at: format_audit_timestamp(&now),
        };
        if let Err(e) = queue.release_notification(&release).await {
            tracing::error!(
                "Could not save the result of notification {} (it is retried after the lease expires): {}",
                item.id,
                e
            );
        }
    }

    /// 送れなかった通知のうち、`retention_days` より古いものを消す
    async fn prune_queue(&self, queue: &dyn NotificationQueueStorage, now: DateTime<Utc>) {
        let days = self.inner.config.queue.retention_days;
        if days == 0 {
            return;
        }
        let cutoff = now - chrono::Duration::days(i64::from(days));
        match queue
            .delete_failed_notifications_before(&format_audit_timestamp(&cutoff))
            .await
        {
            Ok(0) => {}
            Ok(n) => tracing::info!(
                "Removed {} failed notification(s) older than {} days",
                n,
                days
            ),
            Err(e) => tracing::warn!("Could not remove old failed notifications: {}", e),
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

    /// 見本の値で作った通知（`samples::sample_data`）。テスト送信に使う
    pub fn sample_event(&self, kind: &str, recipients: Vec<Recipient>) -> NotificationEvent {
        NotificationEvent::new(
            kind,
            recipients,
            samples::sample_data(kind, self.inner.templates.locale(), &self.inner.site.url),
        )
    }

    /// 見本の値で件名・本文を作る（Admin Studio のプレビュー）。`overrides` は保存前の編集中のテンプレート
    pub fn preview(
        &self,
        kind: &str,
        recipient: &Recipient,
        overrides: HashMap<String, String>,
    ) -> Result<Option<Rendered>> {
        let event = self.sample_event(kind, vec![recipient.clone()]);
        let context = self.context(&event);
        if overrides.is_empty() {
            self.inner.templates.render(kind, recipient, &context)
        } else {
            self.inner
                .templates
                .preview(overrides)
                .render(kind, recipient, &context)
        }
    }

    async fn record(&self, message: &NotificationMessage, delivery: &Delivery) {
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
        if let Some(number) = message.data.pointer("/order/number") {
            details["order_number"] = number.clone();
        }
        audit
            .record(
                AuditOrigin::default()
                    .event(AuditAction::NotificationSend, outcome)
                    .with_resource("notification", Some(message.kind.clone()))
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
            if service.inner.queue.is_none() {
                service.deliver(&event).await;
                return;
            }
            if let Err(e) = service.enqueue(&event).await {
                // DB に入れられなくても通知は落とさない（再起動すると失われる）
                tracing::warn!(
                    "Could not queue notification '{}', sending it without the queue: {}",
                    event.kind,
                    e
                );
                service.deliver(&event).await;
            }
        });
    }
}

#[cfg(test)]
mod tests;
