use std::sync::Mutex;

use aegis_core::{AuditEvent, AuditSink};
use async_trait::async_trait;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpListener;

use super::*;

/// 送った通知を覚えておくチャネル。`failures` 回だけ失敗してから成功する
struct RecordingChannel {
    staff: bool,
    failures: Mutex<u32>,
    permanent: bool,
    sent: Mutex<Vec<NotificationMessage>>,
    calls: Mutex<u32>,
}

impl RecordingChannel {
    fn new(staff: bool) -> Arc<Self> {
        Self::failing(staff, 0, false)
    }

    fn failing(staff: bool, failures: u32, permanent: bool) -> Arc<Self> {
        Arc::new(Self {
            staff,
            failures: Mutex::new(failures),
            permanent,
            sent: Mutex::new(Vec::new()),
            calls: Mutex::new(0),
        })
    }

    fn sent(&self) -> Vec<NotificationMessage> {
        self.sent.lock().unwrap().clone()
    }
}

#[async_trait]
impl NotificationChannel for RecordingChannel {
    fn name(&self) -> &str {
        "recording"
    }

    fn accepts(&self, recipient: &Recipient) -> bool {
        self.staff || matches!(recipient, Recipient::Person { .. })
    }

    async fn send(
        &self,
        message: &NotificationMessage,
    ) -> std::result::Result<Delivered, SendError> {
        *self.calls.lock().unwrap() += 1;
        let mut failures = self.failures.lock().unwrap();
        if *failures > 0 {
            *failures -= 1;
            return Err(if self.permanent {
                SendError::permanent("550 mailbox unavailable")
            } else {
                SendError::temporary("421 try again later")
            });
        }
        self.sent.lock().unwrap().push(message.clone());
        Ok(Delivered::Sent)
    }
}

/// 運営者向けの通知だけを送り、ほかは `Skipped` を返すチャネル（プラグインのチャネルと同じ振る舞い）
#[derive(Default)]
struct StaffOnlyChannel {
    sent: Mutex<Vec<NotificationMessage>>,
}

#[async_trait]
impl NotificationChannel for StaffOnlyChannel {
    fn name(&self) -> &str {
        "staff-only"
    }

    fn accepts(&self, _recipient: &Recipient) -> bool {
        true
    }

    async fn send(
        &self,
        message: &NotificationMessage,
    ) -> std::result::Result<Delivered, SendError> {
        if message.recipient != Recipient::Staff {
            return Ok(Delivered::Skipped);
        }
        self.sent.lock().unwrap().push(message.clone());
        Ok(Delivered::Sent)
    }
}

#[derive(Default)]
struct MemoryAuditSink {
    events: Mutex<Vec<AuditEvent>>,
}

#[async_trait]
impl AuditSink for MemoryAuditSink {
    fn name(&self) -> &str {
        "memory"
    }

    async fn record(&self, event: &AuditEvent) -> Result<()> {
        self.events.lock().unwrap().push(event.clone());
        Ok(())
    }
}

fn config() -> NotificationsConfig {
    NotificationsConfig {
        enabled: true,
        channels: vec![ChannelKind::Log],
        ..Default::default()
    }
}

fn site() -> SiteInfo {
    SiteInfo {
        name: "Aegis Coffee".to_string(),
        url: "https://shop.example.com".to_string(),
        admin_url: "https://shop.example.com/admin/".to_string(),
    }
}

fn placed_event() -> NotificationEvent {
    NotificationEvent::new(
        "order.placed",
        vec![
            Recipient::person("hanako@example.com", "Hanako"),
            Recipient::Staff,
        ],
        json!({
            "order": {
                "number": "261023-ABC234",
                "status": "pending_payment",
                "currency": "jpy",
                "lines": [{ "title": "Mug", "quantity": 1, "unit_price": 1200, "line_total": 1200 }],
                "adjustments": [],
                "subtotal": 1200,
                "total": 1200,
                "customer_name": "Hanako",
                "customer_email": "hanako@example.com"
            }
        }),
    )
}

#[tokio::test]
async fn delivers_a_message_per_recipient_with_site_values() {
    let channel = RecordingChannel::new(true);
    let audit_sink = Arc::new(MemoryAuditSink::default());
    let audit = Arc::new(AuditService::with_sinks(vec![audit_sink.clone()]));
    let service =
        NotificationService::with_channels(&config(), site(), Some(audit), vec![channel.clone()]);

    let deliveries = service.deliver(&placed_event()).await;
    assert_eq!(deliveries.len(), 2);
    assert!(deliveries
        .iter()
        .all(|d| d.result.is_ok() && d.attempts == 1));

    let sent = channel.sent();
    assert_eq!(
        sent[0].recipient,
        Recipient::person("hanako@example.com", "Hanako")
    );
    assert!(sent[0].subject.contains("Aegis Coffee"));
    assert!(sent[0].body.contains("https://shop.example.com"));
    assert_eq!(sent[1].recipient, Recipient::Staff);
    assert!(sent[1].subject.starts_with("New order 261023-ABC234"));
    assert!(sent[1].body.contains("https://shop.example.com/admin/"));

    let events = audit_sink.events.lock().unwrap();
    assert_eq!(events.len(), 2);
    assert_eq!(events[0].action, AuditAction::NotificationSend);
    assert_eq!(events[0].outcome, AuditOutcome::Success);
    assert_eq!(events[0].resource_id.as_deref(), Some("order.placed"));
    let details = &events[0].details;
    assert_eq!(details["recipient"], "person");
    assert_eq!(details["order_number"], "261023-ABC234");
    // 宛先のアドレスは残さない
    assert!(!details.to_string().contains("hanako@example.com"));
}

#[tokio::test]
async fn skips_disabled_events_and_recipients_no_channel_accepts() {
    let channel = RecordingChannel::new(false);
    let mut config = config();
    let service = NotificationService::with_channels(&config, site(), None, vec![channel.clone()]);
    // 運営者向けを受け付けるチャネルが無い
    let deliveries = service.deliver(&placed_event()).await;
    assert_eq!(deliveries.len(), 1);
    assert_eq!(
        deliveries[0].recipient,
        Recipient::person("hanako@example.com", "Hanako")
    );

    config.disabled_events = vec!["order.placed".to_string()];
    let service = NotificationService::with_channels(&config, site(), None, vec![channel]);
    assert!(service.deliver(&placed_event()).await.is_empty());
}

#[tokio::test]
async fn retries_temporary_failures_but_not_permanent_ones() {
    let mut config = config();
    config.max_attempts = 3;

    let flaky = RecordingChannel::failing(false, 2, false);
    let service = NotificationService::with_channels(&config, site(), None, vec![flaky.clone()])
        .with_retry_delay(Duration::from_millis(1));
    let deliveries = service.deliver(&placed_event()).await;
    assert!(deliveries[0].result.is_ok());
    assert_eq!(deliveries[0].attempts, 3);
    assert_eq!(flaky.sent().len(), 1);

    let down = RecordingChannel::failing(false, 5, false);
    let service = NotificationService::with_channels(&config, site(), None, vec![down.clone()])
        .with_retry_delay(Duration::from_millis(1));
    let deliveries = service.deliver(&placed_event()).await;
    assert_eq!(deliveries[0].attempts, 3);
    assert_eq!(
        deliveries[0].result.as_ref().unwrap_err().message,
        "421 try again later"
    );

    let rejected = RecordingChannel::failing(false, 5, true);
    let service = NotificationService::with_channels(&config, site(), None, vec![rejected.clone()])
        .with_retry_delay(Duration::from_millis(1));
    let deliveries = service.deliver(&placed_event()).await;
    assert_eq!(deliveries[0].attempts, 1);
    assert_eq!(*rejected.calls.lock().unwrap(), 1);
}

#[tokio::test]
async fn skipped_messages_are_neither_delivered_nor_audited() {
    let channel = Arc::new(StaffOnlyChannel::default());
    let audit_sink = Arc::new(MemoryAuditSink::default());
    let audit = Arc::new(AuditService::with_sinks(vec![audit_sink.clone()]));
    let service =
        NotificationService::with_channels(&config(), site(), Some(audit), vec![channel.clone()]);
    let deliveries = service.deliver(&placed_event()).await;
    assert_eq!(deliveries.len(), 1);
    assert_eq!(deliveries[0].recipient, Recipient::Staff);
    assert_eq!(audit_sink.events.lock().unwrap().len(), 1);
    // チャネルにはテンプレートに渡した値も届く
    let sent = channel.sent.lock().unwrap();
    assert_eq!(sent[0].data["order"]["number"], "261023-ABC234");
    assert_eq!(sent[0].data["site_name"], "Aegis Coffee");
    assert_eq!(sent[0].data["kind"], "order.placed");
}

#[test]
fn parses_channel_names() {
    let config: NotificationsConfig = serde_json::from_value(json!({
        "channels": ["email", "log", "plugin:official-order-notifier"]
    }))
    .unwrap();
    assert_eq!(
        config.channels,
        vec![
            ChannelKind::Email,
            ChannelKind::Log,
            ChannelKind::Plugin("official-order-notifier".to_string()),
        ]
    );
    assert_eq!(config.channels[2].name(), "plugin:official-order-notifier");
    for bad in ["sms", "plugin:", "plugin:../evil"] {
        let error = serde_json::from_value::<NotificationsConfig>(json!({ "channels": [bad] }))
            .unwrap_err()
            .to_string();
        assert!(error.contains("unknown notification channel"), "{}", error);
    }

    // プラグインのチャネルは作り方を渡したときだけ使える
    let config = NotificationsConfig {
        enabled: true,
        channels: vec![ChannelKind::Plugin("slack".to_string())],
        ..Default::default()
    };
    assert!(NotificationService::from_config(&config, site(), None).is_err());
    let service = NotificationService::from_config_with_plugins(&config, site(), None, |name| {
        assert_eq!(name, "slack");
        Ok(Arc::new(StaffOnlyChannel::default()) as Arc<dyn NotificationChannel>)
    })
    .unwrap();
    assert_eq!(service.channel_names(), vec!["staff-only"]);
}

#[tokio::test]
async fn notify_delivers_in_the_background() {
    let channel = RecordingChannel::new(true);
    let service =
        NotificationService::with_channels(&config(), site(), None, vec![channel.clone()]);
    service.notify(placed_event());
    for _ in 0..100 {
        if channel.sent().len() == 2 {
            return;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!("notification was not delivered");
}

#[test]
fn validates_the_email_settings() {
    let mut config = NotificationsConfig {
        enabled: true,
        ..Default::default()
    };
    assert!(config
        .validate()
        .unwrap_err()
        .to_string()
        .contains("smtp_host"));
    config.email.smtp_host = "smtp.example.com".to_string();
    assert!(config.validate().unwrap_err().to_string().contains("from"));
    config.email.from = "Shop <shop@example.com>".to_string();
    config.validate().unwrap();
    config.email.staff_recipients = vec!["not an address".to_string()];
    assert!(config
        .validate()
        .unwrap_err()
        .to_string()
        .contains("staff_recipients"));
    config.email.staff_recipients.clear();
    config.locale = "fr".to_string();
    assert!(config
        .validate()
        .unwrap_err()
        .to_string()
        .contains("locale"));
    // 無効なら検証しない
    config.enabled = false;
    config.validate().unwrap();

    assert_eq!(EmailConfig::default().port(), 587);
    let tls = EmailConfig {
        tls: SmtpTls::Tls,
        ..Default::default()
    };
    assert_eq!(tls.port(), 465);
}

/// 最小限の SMTP サーバー。受け取ったメール（DATA の中身）を返す
async fn fake_smtp_server(reject_rcpt: bool) -> (u16, tokio::task::JoinHandle<Vec<String>>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let handle = tokio::spawn(async move {
        let mut mails = Vec::new();
        let (socket, _) = listener.accept().await.unwrap();
        let (read, mut write) = socket.into_split();
        let mut lines = BufReader::new(read).lines();
        write.write_all(b"220 fake ESMTP\r\n").await.unwrap();
        let mut data: Option<String> = None;
        while let Ok(Some(line)) = lines.next_line().await {
            if let Some(body) = data.as_mut() {
                if line == "." {
                    mails.push(data.take().unwrap());
                    write.write_all(b"250 queued\r\n").await.unwrap();
                } else {
                    body.push_str(&line);
                    body.push('\n');
                }
                continue;
            }
            let command = line.to_ascii_uppercase();
            let reply: &[u8] = if command.starts_with("EHLO") || command.starts_with("HELO") {
                b"250-fake\r\n250 8BITMIME\r\n"
            } else if command.starts_with("RCPT") && reject_rcpt {
                b"550 no such user\r\n"
            } else if command.starts_with("DATA") {
                data = Some(String::new());
                b"354 go ahead\r\n"
            } else if command.starts_with("QUIT") {
                write.write_all(b"221 bye\r\n").await.unwrap();
                break;
            } else {
                b"250 ok\r\n"
            };
            write.write_all(reply).await.unwrap();
        }
        mails
    });
    (port, handle)
}

fn email_config(port: u16) -> EmailConfig {
    EmailConfig {
        smtp_host: "127.0.0.1".to_string(),
        smtp_port: port,
        tls: SmtpTls::None,
        from: "Aegis Coffee <shop@example.com>".to_string(),
        reply_to: "support@example.com".to_string(),
        staff_recipients: vec!["owner@example.com".to_string()],
        timeout_secs: 5,
        ..Default::default()
    }
}

#[tokio::test]
async fn email_channel_sends_through_smtp() {
    let (port, server) = fake_smtp_server(false).await;
    let channel = EmailChannel::new(&email_config(port)).unwrap();
    assert!(channel.accepts(&Recipient::Staff));
    channel
        .send(&NotificationMessage {
            kind: "test".to_string(),
            recipient: Recipient::person("hanako@example.com", "Hanako"),
            subject: "Test notification".to_string(),
            body: "Hello from AegisCMS".to_string(),
            data: json!({}),
        })
        .await
        .unwrap();
    drop(channel);
    let mails = tokio::time::timeout(Duration::from_secs(5), server)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(mails.len(), 1);
    let mail = &mails[0];
    assert!(
        mail.contains("From: \"Aegis Coffee\" <shop@example.com>"),
        "{}",
        mail
    );
    assert!(mail.contains("To: Hanako <hanako@example.com>"), "{}", mail);
    assert!(mail.contains("Reply-To: support@example.com"), "{}", mail);
    assert!(mail.contains("Subject: Test notification"), "{}", mail);
    assert!(
        mail.contains("Content-Type: text/plain; charset=utf-8"),
        "{}",
        mail
    );
    assert!(mail.contains("Hello from AegisCMS"), "{}", mail);
}

#[tokio::test]
async fn email_channel_reports_rejected_recipients_as_permanent() {
    let (port, _server) = fake_smtp_server(true).await;
    let channel = EmailChannel::new(&email_config(port)).unwrap();
    let error = channel
        .send(&NotificationMessage {
            kind: "test".to_string(),
            recipient: Recipient::Staff,
            subject: "Test".to_string(),
            body: "Hello".to_string(),
            data: json!({}),
        })
        .await
        .unwrap_err();
    assert!(error.permanent, "{:?}", error);
    assert!(error.message.contains("no such user"), "{:?}", error);

    let invalid = channel
        .send(&NotificationMessage {
            kind: "test".to_string(),
            recipient: Recipient::person("not an address", ""),
            subject: "Test".to_string(),
            body: "Hello".to_string(),
            data: json!({}),
        })
        .await
        .unwrap_err();
    assert!(invalid.permanent);
}
