use std::collections::HashMap;
use std::sync::Mutex;

use aegis_cache::MemoryCacheBackend;
use aegis_core::{AuditActor, AuditEvent, AuditSink};
use aegis_db::{NewsletterStorage, SqliteAdapter};

use super::*;

#[derive(Default)]
struct RecordingNotifier(Mutex<Vec<NotificationEvent>>);

impl Notifier for RecordingNotifier {
    fn notify(&self, event: NotificationEvent) {
        self.0.lock().unwrap().push(event);
    }
}

impl RecordingNotifier {
    fn take(&self) -> Vec<NotificationEvent> {
        std::mem::take(&mut *self.0.lock().unwrap())
    }
}

#[derive(Default)]
struct RecordingSink(Mutex<Vec<AuditEvent>>);

#[async_trait::async_trait]
impl AuditSink for RecordingSink {
    fn name(&self) -> &str {
        "recording"
    }

    async fn record(&self, event: &AuditEvent) -> aegis_core::Result<()> {
        self.0.lock().unwrap().push(event.clone());
        Ok(())
    }
}

struct Fixture {
    service: NewsletterService,
    db: Arc<SqliteAdapter>,
    notifier: Arc<RecordingNotifier>,
    audit: Arc<RecordingSink>,
}

async fn fixture(configure: impl FnOnce(&mut NewsletterConfig)) -> Fixture {
    let mut config = NewsletterConfig {
        enabled: true,
        ..NewsletterConfig::default()
    };
    configure(&mut config);
    let db = SqliteAdapter::connect("sqlite::memory:", 1).await.unwrap();
    db.init().await.unwrap();
    let db = Arc::new(db);
    let notifier = Arc::new(RecordingNotifier::default());
    let audit = Arc::new(RecordingSink::default());
    let service = NewsletterService::new(
        config,
        NewsletterDeps {
            db: db.clone(),
            cache: Arc::new(MemoryCacheBackend::new()),
            audit: Arc::new(AuditService::with_sinks(vec![audit.clone()])),
            notifier: Some(notifier.clone()),
            site_url: "https://example.com".to_string(),
        },
    )
    .unwrap();
    Fixture {
        service,
        db,
        notifier,
        audit,
    }
}

fn client(ip: &str) -> ClientInfo {
    ClientInfo {
        ip: Some(ip.to_string()),
    }
}

fn admin() -> AuditOrigin {
    AuditOrigin {
        actor: Some(AuditActor {
            user_id: "admin".to_string(),
            role: "admin".to_string(),
        }),
        ..Default::default()
    }
}

fn content() -> IssueContent {
    IssueContent {
        content_type: "article".to_string(),
        content_id: "a1".to_string(),
        title: "Autumn blend".to_string(),
        summary: "Beans have arrived.".to_string(),
        url: "https://example.com/articles/autumn-blend".to_string(),
    }
}

/// 申し込み → 確認メールのトークン
async fn sign_up(f: &Fixture, email: &str) -> String {
    f.service
        .subscribe(email, None, None, &client("203.0.113.1"))
        .await
        .unwrap();
    f.db.get_newsletter_subscriber_by_email(&email.to_lowercase())
        .await
        .unwrap()
        .unwrap()
        .token
}

async fn status_of(f: &Fixture, email: &str) -> String {
    f.db.get_newsletter_subscriber_by_email(email)
        .await
        .unwrap()
        .unwrap()
        .status
}

#[test]
fn test_normalize_email() {
    assert_eq!(
        normalize_email("  Reader@Example.COM ").as_deref(),
        Some("reader@example.com")
    );
    assert_eq!(
        normalize_email("a+tag@sub.example.co.jp").as_deref(),
        Some("a+tag@sub.example.co.jp")
    );
    for bad in [
        "",
        "plain",
        "@example.com",
        "a@",
        "a@b",
        "a@@example.com",
        "a b@example.com",
        "<a@example.com>",
        "a@example.com, b@example.com",
        "a@.example.com",
        "a@example..com",
        "a@example.com.",
        "a\n@example.com",
        "=cmd@example.com",
        "+1@example.com",
        "-x@example.com",
        "@x@example.com",
    ] {
        assert_eq!(normalize_email(bad), None, "{:?}", bad);
    }
    assert_eq!(
        normalize_email(&format!("{}@example.com", "a".repeat(65))),
        None
    );
}

#[test]
fn test_config_is_checked() {
    assert!(NewsletterConfig::default().validate().is_ok());
    for bad in [
        NewsletterConfig {
            locale: "fr".into(),
            ..Default::default()
        },
        NewsletterConfig {
            confirm_ttl_hours: 0,
            ..Default::default()
        },
        NewsletterConfig {
            confirm_ttl_hours: 24 * 15,
            ..Default::default()
        },
    ] {
        assert!(bad.validate().is_err());
    }
    let config: NewsletterConfig = toml::from_str(
        r#"
        enabled = true
        locale = "ja"
        [captcha]
        provider = "turnstile"
        site_key = "k"
        secret = "s"
        "#,
    )
    .unwrap();
    assert!(config.validate().is_ok());
    assert_eq!(config.confirm_ttl_hours, 48);
}

#[tokio::test]
async fn test_the_site_url_is_required_for_the_links() {
    let f = fixture(|_| {}).await;
    let result = NewsletterService::new(
        NewsletterConfig::default(),
        NewsletterDeps {
            site_url: " ".to_string(),
            ..f.service.deps.clone()
        },
    );
    assert!(result.err().unwrap().to_string().contains("public_url"));
}

#[tokio::test]
async fn test_double_opt_in() {
    let f = fixture(|_| {}).await;
    let token = sign_up(&f, "Reader@Example.com").await;

    // A confirmation mail goes out; nothing is sent to the subscriber before the link is opened
    let sent = f.notifier.take();
    assert_eq!(sent.len(), 1);
    assert_eq!(sent[0].kind, "newsletter.confirm");
    assert_eq!(
        sent[0].data["confirm_url"],
        format!("https://example.com/newsletter/confirm?token={}", token)
    );
    assert_eq!(sent[0].data["expires_hours"], 48);
    assert_eq!(status_of(&f, "reader@example.com").await, STATUS_PENDING);
    assert!(f.service.active_emails().await.unwrap().is_empty());

    assert_eq!(
        f.service.confirm(&token).await.unwrap(),
        ConfirmOutcome::Confirmed
    );
    assert_eq!(status_of(&f, "reader@example.com").await, STATUS_ACTIVE);
    assert_eq!(
        f.service.active_emails().await.unwrap(),
        ["reader@example.com"]
    );
    // opening it again is fine
    assert_eq!(
        f.service.confirm(&token).await.unwrap(),
        ConfirmOutcome::AlreadyConfirmed
    );
    let subscriber =
        f.db.get_newsletter_subscriber_by_token(&token)
            .await
            .unwrap()
            .unwrap();
    assert!(subscriber.confirmed_at.is_some());
}

#[tokio::test]
async fn test_signing_up_never_reveals_the_state_of_an_address() {
    let f = fixture(|_| {}).await;
    let token = sign_up(&f, "reader@example.com").await;
    f.service.confirm(&token).await.unwrap();
    f.notifier.take();

    // Already subscribed: same answer, no mail, nothing changes
    f.service
        .subscribe("reader@example.com", None, None, &client("203.0.113.2"))
        .await
        .unwrap();
    assert!(f.notifier.take().is_empty());
    assert_eq!(status_of(&f, "reader@example.com").await, STATUS_ACTIVE);

    // A pending address is not mailed again within a minute
    sign_up(&f, "new@example.com").await;
    f.notifier.take();
    f.service
        .subscribe("new@example.com", None, None, &client("203.0.113.3"))
        .await
        .unwrap();
    assert!(f.notifier.take().is_empty());
}

#[tokio::test]
async fn test_a_pending_address_gets_a_fresh_link_after_a_minute() {
    let f = fixture(|_| {}).await;
    let old_token = sign_up(&f, "reader@example.com").await;
    f.notifier.take();
    let mut subscriber =
        f.db.get_newsletter_subscriber_by_token(&old_token)
            .await
            .unwrap()
            .unwrap();
    subscriber.updated_at =
        (Utc::now() - chrono::Duration::minutes(5)).to_rfc3339_opts(SecondsFormat::Micros, true);
    f.db.update_newsletter_subscriber(&subscriber)
        .await
        .unwrap();

    let new_token = sign_up(&f, "reader@example.com").await;
    assert_ne!(old_token, new_token);
    assert_eq!(f.notifier.take().len(), 1);
    // The link in the first mail no longer works
    assert!(matches!(
        f.service.confirm(&old_token).await,
        Err(NewsletterError::NotFound(_))
    ));
    f.service.confirm(&new_token).await.unwrap();
}

#[tokio::test]
async fn test_an_expired_confirmation_link_does_not_work() {
    let f = fixture(|c| c.confirm_ttl_hours = 1).await;
    let token = sign_up(&f, "reader@example.com").await;
    let mut subscriber =
        f.db.get_newsletter_subscriber_by_token(&token)
            .await
            .unwrap()
            .unwrap();
    subscriber.updated_at =
        (Utc::now() - chrono::Duration::hours(2)).to_rfc3339_opts(SecondsFormat::Micros, true);
    f.db.update_newsletter_subscriber(&subscriber)
        .await
        .unwrap();

    assert!(matches!(
        f.service.confirm(&token).await,
        Err(NewsletterError::NotFound(_))
    ));
    assert_eq!(status_of(&f, "reader@example.com").await, STATUS_PENDING);
    // Unknown and malformed tokens are the same error
    for bad in ["", "nope", "../x", &"a".repeat(200)] {
        assert!(matches!(
            f.service.confirm(bad).await,
            Err(NewsletterError::NotFound(_))
        ));
    }
}

#[tokio::test]
async fn test_unsubscribe_and_signing_up_again() {
    let f = fixture(|_| {}).await;
    let token = sign_up(&f, "reader@example.com").await;
    f.service.confirm(&token).await.unwrap();

    assert_eq!(
        f.service.unsubscribe(&token).await.unwrap(),
        UnsubscribeOutcome::Unsubscribed
    );
    assert_eq!(
        status_of(&f, "reader@example.com").await,
        STATUS_UNSUBSCRIBED
    );
    assert!(f.service.active_emails().await.unwrap().is_empty());
    assert_eq!(
        f.service.unsubscribe(&token).await.unwrap(),
        UnsubscribeOutcome::AlreadyUnsubscribed
    );
    // An unsubscribed link cannot bring the address back
    assert!(matches!(
        f.service.confirm(&token).await,
        Err(NewsletterError::NotFound(_))
    ));

    // Signing up again starts over with a new link
    f.notifier.take();
    let new_token = sign_up(&f, "reader@example.com").await;
    assert_ne!(token, new_token);
    assert_eq!(f.notifier.take()[0].kind, "newsletter.confirm");
    assert_eq!(status_of(&f, "reader@example.com").await, STATUS_PENDING);
    f.service.confirm(&new_token).await.unwrap();
    assert_eq!(status_of(&f, "reader@example.com").await, STATUS_ACTIVE);
    // An unknown link is refused
    assert!(matches!(
        f.service.unsubscribe("0123456789abcdef").await,
        Err(NewsletterError::NotFound(_))
    ));
}

#[tokio::test]
async fn test_bots_bad_addresses_and_the_rate_limit() {
    let f = fixture(|c| c.max_per_ip_per_hour = 2).await;
    // The hidden field: looks accepted, stores nothing
    f.service
        .subscribe(
            "bot@example.com",
            Some("http://spam.example"),
            None,
            &client("203.0.113.9"),
        )
        .await
        .unwrap();
    assert!(f
        .db
        .get_newsletter_subscriber_by_email("bot@example.com")
        .await
        .unwrap()
        .is_none());
    assert!(f.notifier.take().is_empty());

    // A bad address is refused with a message for the sign-up page, and costs no quota
    let err = f
        .service
        .subscribe("nope", None, None, &client("203.0.113.1"))
        .await
        .unwrap_err();
    assert!(
        matches!(&err, NewsletterError::Invalid(m) if m == "Please enter a valid email address.")
    );

    for email in ["a@example.com", "b@example.com"] {
        f.service
            .subscribe(email, None, None, &client("203.0.113.1"))
            .await
            .unwrap();
    }
    assert!(matches!(
        f.service
            .subscribe("c@example.com", None, None, &client("203.0.113.1"))
            .await,
        Err(NewsletterError::RateLimited)
    ));
    // another address has its own quota
    f.service
        .subscribe("c@example.com", None, None, &client("203.0.113.2"))
        .await
        .unwrap();
}

#[tokio::test]
async fn test_japanese_texts() {
    let f = fixture(|c| c.locale = "ja".into()).await;
    let err = f
        .service
        .subscribe("nope", None, None, &client("203.0.113.1"))
        .await
        .unwrap_err();
    assert!(matches!(&err, NewsletterError::Invalid(m) if m.contains("メールアドレス")));
    assert_eq!(f.service.text("submit"), "購読する");
}

#[tokio::test]
async fn test_forms_are_escaped_and_carry_the_trap() {
    let f = fixture(|_| {}).await;
    let html = f.service.form_html(
        "/newsletter/subscribe",
        "\"><script>x</script>",
        Some("<b>bad</b>"),
    );
    assert!(html.contains("action=\"/newsletter/subscribe\""));
    assert!(html.contains("name=\"website\" tabindex=\"-1\""));
    assert!(html.contains("value=\"&quot;&gt;&lt;script&gt;x&lt;/script&gt;\""));
    assert!(html.contains("&lt;b&gt;bad&lt;/b&gt;"));
    assert!(!html.contains("<script>"));
    assert!(!html.contains("aegis-newsletter-captcha"));

    let confirm = f
        .service
        .token_form_html("confirm", "/newsletter/confirm", "ab\"cd");
    assert!(confirm.contains("method=\"post\""));
    assert!(confirm.contains("name=\"token\" value=\"ab&quot;cd\""));
    assert!(confirm.contains("Confirm subscription"));
}

#[tokio::test]
async fn test_listing_counts_and_deleting() {
    let f = fixture(|_| {}).await;
    let a = sign_up(&f, "a@example.com").await;
    sign_up(&f, "b@example.com").await;
    let c = sign_up(&f, "c@example.com").await;
    f.service.confirm(&a).await.unwrap();
    f.service.confirm(&c).await.unwrap();
    f.service.unsubscribe(&c).await.unwrap();

    let (page, counts) = f
        .service
        .list(&NewsletterListQuery::default())
        .await
        .unwrap();
    assert_eq!(page.total, 3);
    assert_eq!(
        counts,
        Counts {
            pending: 1,
            active: 1,
            unsubscribed: 1
        }
    );
    let (found, _) = f
        .service
        .list(&NewsletterListQuery {
            status: Some(STATUS_ACTIVE.to_string()),
            ..Default::default()
        })
        .await
        .unwrap();
    assert_eq!(found.subscribers.len(), 1);
    assert_eq!(found.subscribers[0].email, "a@example.com");
    let (found, _) = f
        .service
        .list(&NewsletterListQuery {
            search: Some("B@EX".to_string()),
            ..Default::default()
        })
        .await
        .unwrap();
    assert_eq!(found.total, 1);
    // `%` and `_` in a search are not wildcards
    let (found, _) = f
        .service
        .list(&NewsletterListQuery {
            search: Some("%".to_string()),
            ..Default::default()
        })
        .await
        .unwrap();
    assert_eq!(found.total, 0);

    let ids: Vec<String> = page.subscribers.iter().map(|s| s.id.clone()).collect();
    assert_eq!(f.service.delete(&admin(), &ids[..2]).await.unwrap(), 2);
    assert_eq!(
        f.service
            .list(&NewsletterListQuery::default())
            .await
            .unwrap()
            .0
            .total,
        1
    );
    assert!(f.service.delete(&admin(), &[]).await.is_err());
    let events = f.audit.0.lock().unwrap();
    let delete = events
        .iter()
        .find(|e| e.action == AuditAction::NewsletterDelete)
        .unwrap();
    assert_eq!(delete.details["count"], 2);
    assert_eq!(delete.actor.as_ref().unwrap().user_id, "admin");
}

#[tokio::test]
async fn test_sending_an_issue_to_each_confirmed_subscriber() {
    let f = fixture(|_| {}).await;
    let mut tokens = HashMap::new();
    for email in [
        "a@example.com",
        "b@example.com",
        "c@example.com",
        "d@example.com",
    ] {
        tokens.insert(email, sign_up(&f, email).await);
    }
    f.service.confirm(&tokens["a@example.com"]).await.unwrap();
    f.service.confirm(&tokens["b@example.com"]).await.unwrap();
    f.service.confirm(&tokens["c@example.com"]).await.unwrap();
    f.service
        .unsubscribe(&tokens["c@example.com"])
        .await
        .unwrap();
    // d stays pending
    f.notifier.take();

    let issue = f
        .service
        .send_issue(&admin(), &content(), Some("admin".into()), false)
        .await
        .unwrap();
    assert_eq!(issue.recipients, 2);
    assert_eq!(issue.created_by.as_deref(), Some("admin"));

    // One notification per confirmed subscriber, each with its own unsubscribe link
    let sent = f.notifier.take();
    assert_eq!(sent.len(), 2);
    for event in &sent {
        assert_eq!(event.kind, "newsletter.issue");
        assert_eq!(event.data["title"], "Autumn blend");
        assert_eq!(event.data["summary"], "Beans have arrived.");
        assert_eq!(
            event.data["url"],
            "https://example.com/articles/autumn-blend"
        );
        let Recipient::Person { email, .. } = &event.recipients[0] else {
            panic!("a person");
        };
        assert_eq!(
            event.data["unsubscribe_url"],
            format!(
                "https://example.com/newsletter/unsubscribe?token={}",
                tokens[email.as_str()]
            )
        );
    }
    let mut to: Vec<String> = sent
        .iter()
        .map(|e| match &e.recipients[0] {
            Recipient::Person { email, .. } => email.clone(),
            Recipient::Staff => String::new(),
        })
        .collect();
    to.sort();
    assert_eq!(to, ["a@example.com", "b@example.com"]);

    // The history and the audit log
    let (issues, total) = f.service.issues(10, 0).await.unwrap();
    assert_eq!((total, issues[0].id.as_str()), (1, issue.id.as_str()));
    let events = f.audit.0.lock().unwrap();
    let send = events
        .iter()
        .find(|e| e.action == AuditAction::NewsletterSend)
        .unwrap();
    assert_eq!(send.outcome, AuditOutcome::Success);
    assert_eq!(send.details["recipients"], 2);
}

#[tokio::test]
async fn test_the_same_content_is_not_sent_twice_unless_forced() {
    let f = fixture(|_| {}).await;
    let token = sign_up(&f, "a@example.com").await;
    f.service.confirm(&token).await.unwrap();
    f.notifier.take();

    f.service
        .send_issue(&admin(), &content(), None, false)
        .await
        .unwrap();
    f.notifier.take();
    let again = f
        .service
        .send_issue(&admin(), &content(), None, false)
        .await
        .unwrap_err();
    assert!(matches!(again, NewsletterError::AlreadySent(_)));
    assert!(f.notifier.take().is_empty());

    f.service
        .send_issue(&admin(), &content(), None, true)
        .await
        .unwrap();
    assert_eq!(f.notifier.take().len(), 1);
    assert_eq!(f.service.issues(10, 0).await.unwrap().1, 2);
    // A different content is a different issue
    let other = IssueContent {
        content_id: "a2".into(),
        ..content()
    };
    f.service
        .send_issue(&admin(), &other, None, false)
        .await
        .unwrap();
}

#[tokio::test]
async fn test_nothing_is_recorded_without_subscribers_and_failures_are_audited() {
    let f = fixture(|_| {}).await;
    let err = f
        .service
        .send_issue(&admin(), &content(), None, false)
        .await
        .unwrap_err();
    assert!(matches!(err, NewsletterError::NoSubscribers));
    assert_eq!(f.service.issues(10, 0).await.unwrap().1, 0);
    // A failed attempt does not block the next one
    let token = sign_up(&f, "a@example.com").await;
    f.service.confirm(&token).await.unwrap();
    f.service
        .send_issue(&admin(), &content(), None, false)
        .await
        .unwrap();
    let events = f.audit.0.lock().unwrap();
    let outcomes: Vec<AuditOutcome> = events
        .iter()
        .filter(|e| e.action == AuditAction::NewsletterSend)
        .map(|e| e.outcome)
        .collect();
    assert_eq!(outcomes, [AuditOutcome::Failure, AuditOutcome::Success]);
}

#[tokio::test]
async fn test_a_test_mail_goes_to_one_address_and_leaves_no_history() {
    let f = fixture(|_| {}).await;
    f.service
        .send_test(&content(), "Editor@Example.com")
        .await
        .unwrap();
    let sent = f.notifier.take();
    assert_eq!(sent.len(), 1);
    assert!(
        matches!(&sent[0].recipients[0], Recipient::Person { email, .. } if email == "editor@example.com")
    );
    assert_eq!(
        sent[0].data["unsubscribe_url"],
        "https://example.com/newsletter/unsubscribe?token=test"
    );
    assert_eq!(f.service.issues(10, 0).await.unwrap().1, 0);
    assert!(f.service.send_test(&content(), "nope").await.is_err());
    // The sample link matches no subscriber
    assert!(matches!(
        f.service.unsubscribe("test").await,
        Err(NewsletterError::NotFound(_))
    ));
}

#[tokio::test]
async fn test_sending_needs_notifications() {
    let f = fixture(|_| {}).await;
    let silent = NewsletterService::new(
        NewsletterConfig::default(),
        NewsletterDeps {
            notifier: None,
            ..f.service.deps.clone()
        },
    )
    .unwrap();
    assert!(matches!(
        silent.send_issue(&admin(), &content(), None, false).await,
        Err(NewsletterError::Unavailable(_))
    ));
    assert!(matches!(
        silent.send_test(&content(), "a@example.com").await,
        Err(NewsletterError::Unavailable(_))
    ));
    // Signing up still works (the mail is just not sent)
    silent
        .subscribe("a@example.com", None, None, &client("203.0.113.1"))
        .await
        .unwrap();
}

#[tokio::test]
async fn test_old_unconfirmed_sign_ups_are_cleaned_up() {
    let f = fixture(|_| {}).await;
    let token = sign_up(&f, "old@example.com").await;
    let kept = sign_up(&f, "kept@example.com").await;
    let confirmed = sign_up(&f, "confirmed@example.com").await;
    f.service.confirm(&confirmed).await.unwrap();
    for (token, days) in [(&token, 10), (&confirmed, 10)] {
        let mut s =
            f.db.get_newsletter_subscriber_by_token(token)
                .await
                .unwrap()
                .unwrap();
        s.updated_at = (Utc::now() - chrono::Duration::days(days))
            .to_rfc3339_opts(SecondsFormat::Micros, true);
        f.db.update_newsletter_subscriber(&s).await.unwrap();
    }
    assert_eq!(f.service.clean_up(Utc::now()).await, 1);
    assert!(f
        .db
        .get_newsletter_subscriber_by_email("old@example.com")
        .await
        .unwrap()
        .is_none());
    // A recent pending sign-up and a confirmed one stay
    assert!(f
        .db
        .get_newsletter_subscriber_by_token(&kept)
        .await
        .unwrap()
        .is_some());
    assert!(f
        .db
        .get_newsletter_subscriber_by_token(&confirmed)
        .await
        .unwrap()
        .is_some());
}

#[tokio::test]
async fn test_many_subscribers_are_sent_in_batches() {
    let f = fixture(|c| c.max_per_ip_per_hour = 0).await;
    let total = (SEND_BATCH as usize) + 25;
    let at = now();
    for i in 0..total {
        f.db.insert_newsletter_subscriber(&NewsletterSubscriberRecord {
            id: format!("id-{:05}", i),
            email: format!("user{}@example.com", i),
            status: STATUS_ACTIVE.to_string(),
            token: format!("token{:05}", i),
            ip: None,
            created_at: at.clone(),
            confirmed_at: Some(at.clone()),
            unsubscribed_at: None,
            updated_at: at.clone(),
        })
        .await
        .unwrap();
    }
    let issue = f
        .service
        .send_issue(&admin(), &content(), None, false)
        .await
        .unwrap();
    assert_eq!(issue.recipients, total as i64);
    let sent = f.notifier.take();
    assert_eq!(sent.len(), total);
    let unique: std::collections::HashSet<String> = sent
        .iter()
        .map(|e| e.data["unsubscribe_url"].as_str().unwrap().to_string())
        .collect();
    assert_eq!(unique.len(), total);
    assert_eq!(f.service.active_emails().await.unwrap().len(), total);
}
