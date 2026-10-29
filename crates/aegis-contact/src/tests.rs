use std::sync::Mutex;

use aegis_cache::MemoryCacheBackend;
use aegis_core::{AuditActor, AuditEvent, AuditSink, NotificationEvent};
use aegis_db::SqliteAdapter;

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
    service: ContactService,
    notifier: Arc<RecordingNotifier>,
    audit: Arc<RecordingSink>,
}

async fn fixture(configure: impl FnOnce(&mut ContactConfig)) -> Fixture {
    let mut config = ContactConfig {
        enabled: true,
        ..ContactConfig::default()
    };
    configure(&mut config);
    let db = SqliteAdapter::connect("sqlite::memory:", 1).await.unwrap();
    db.init().await.unwrap();
    let notifier = Arc::new(RecordingNotifier::default());
    let audit = Arc::new(RecordingSink::default());
    let service = ContactService::new(
        config,
        ContactDeps {
            db: Arc::new(db),
            cache: Arc::new(MemoryCacheBackend::new()),
            audit: Arc::new(AuditService::with_sinks(vec![audit.clone()])),
            notifier: Some(notifier.clone()),
            admin_url: "https://example.com/admin/".to_string(),
        },
    )
    .unwrap();
    Fixture {
        service,
        notifier,
        audit,
    }
}

fn values(pairs: &[(&str, &str)]) -> HashMap<String, String> {
    pairs
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect()
}

fn valid() -> HashMap<String, String> {
    values(&[
        ("name", "山田 花子"),
        ("email", "Hanako@Example.com"),
        ("subject", "導入の相談"),
        ("message", "見積もりをお願いします。"),
    ])
}

fn client(ip: &str) -> ClientInfo {
    ClientInfo {
        ip: Some(ip.to_string()),
        user_agent: Some("test".to_string()),
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

#[tokio::test]
async fn test_submit_saves_and_notifies_staff() {
    let f = fixture(|_| {}).await;
    let saved = f
        .service
        .submit("default", &valid(), &client("203.0.113.1"))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(saved.status, "new");
    assert_eq!(saved.name, "山田 花子");
    assert_eq!(saved.email, "hanako@example.com");
    assert_eq!(saved.subject, "導入の相談");

    let notes = f.notifier.take();
    assert_eq!(notes.len(), 1);
    assert_eq!(notes[0].kind, "contact.received");
    assert!(matches!(notes[0].recipients[0], Recipient::Staff));
    assert_eq!(notes[0].data["form"]["title"], "Contact");
    assert_eq!(notes[0].data["message"]["subject"], "導入の相談");
    assert_eq!(
        notes[0].data["admin_url"],
        "https://example.com/admin/#contact"
    );
    // 空の項目は通知に入れない
    let labels: Vec<&str> = notes[0].data["message"]["fields"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| f["label"].as_str().unwrap())
        .collect();
    assert_eq!(labels, vec!["Name", "Email", "Subject", "Message"]);

    let message = f.service.get(&saved.id).await.unwrap();
    assert_eq!(message.fields.len(), 4);
    assert_eq!(message.fields[3].value, "見積もりをお願いします。");
    assert_eq!(message.form_title, "Contact");
}

#[tokio::test]
async fn test_auto_reply_goes_to_the_sender() {
    let f = fixture(|config| {
        config.forms[0].auto_reply = true;
        config.notify_staff = false;
    })
    .await;
    f.service
        .submit("default", &valid(), &client("203.0.113.1"))
        .await
        .unwrap();
    let notes = f.notifier.take();
    assert_eq!(notes.len(), 1);
    assert_eq!(notes[0].kind, "contact.auto_reply");
    match &notes[0].recipients[0] {
        Recipient::Person { email, name } => {
            assert_eq!(email, "hanako@example.com");
            assert_eq!(name.as_deref(), Some("山田 花子"));
        }
        other => panic!("unexpected recipient {:?}", other),
    }
}

#[tokio::test]
async fn test_invalid_input_unknown_form_and_honeypot() {
    let f = fixture(|_| {}).await;
    let err = f
        .service
        .submit("default", &values(&[("name", "A")]), &client("203.0.113.1"))
        .await
        .unwrap_err();
    match err {
        ContactError::Invalid(errors) => {
            let fields: Vec<&str> = errors.iter().map(|e| e.field.as_str()).collect();
            assert_eq!(fields, vec!["email", "message"]);
        }
        other => panic!("unexpected {:?}", other),
    }
    assert!(matches!(
        f.service
            .submit("missing", &valid(), &client("203.0.113.1"))
            .await,
        Err(ContactError::NotFound(_))
    ));

    let mut bot = valid();
    bot.insert("website".to_string(), "http://spam.example".to_string());
    assert!(f
        .service
        .submit("default", &bot, &client("203.0.113.1"))
        .await
        .unwrap()
        .is_none());
    let page = f
        .service
        .list(&AdminListQuery {
            limit: 10,
            ..Default::default()
        })
        .await
        .unwrap();
    assert_eq!(page.total, 0);
    assert!(f.notifier.take().is_empty());
}

#[tokio::test]
async fn test_spam_is_saved_quietly_and_rate_limit() {
    let f = fixture(|config| {
        config.spam_words = vec!["casino".to_string()];
        config.max_per_ip_per_hour = 2;
    })
    .await;
    let mut spam = valid();
    spam.insert("message".to_string(), "Best CASINO online".to_string());
    let saved = f
        .service
        .submit("default", &spam, &client("203.0.113.1"))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(saved.status, "spam");
    assert!(f.notifier.take().is_empty());

    let mut links = valid();
    links.insert(
        "message".to_string(),
        "http://a.example http://b.example http://c.example www.d.example".to_string(),
    );
    let saved = f
        .service
        .submit("default", &links, &client("198.51.100.2"))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(saved.status, "spam");

    f.service
        .submit("default", &valid(), &client("203.0.113.1"))
        .await
        .unwrap();
    assert!(matches!(
        f.service
            .submit("default", &valid(), &client("203.0.113.1"))
            .await,
        Err(ContactError::RateLimited)
    ));
}

#[tokio::test]
async fn test_inbox_status_delete_and_audit() {
    let f = fixture(|_| {}).await;
    let mut ids = Vec::new();
    for ip in ["203.0.113.1", "203.0.113.2", "203.0.113.3"] {
        let saved = f
            .service
            .submit("default", &valid(), &client(ip))
            .await
            .unwrap()
            .unwrap();
        ids.push(saved.id);
    }
    assert_eq!(
        f.service
            .set_status(&admin(), &ids[..2], "done")
            .await
            .unwrap(),
        2
    );
    let page = f
        .service
        .list(&AdminListQuery {
            status: Some("new".to_string()),
            limit: 10,
            ..Default::default()
        })
        .await
        .unwrap();
    assert_eq!(page.total, 1);
    assert_eq!(page.counts.get("done"), Some(&2));
    assert_eq!(page.counts.get("new"), Some(&1));

    assert!(matches!(
        f.service.set_status(&admin(), &ids, "archived").await,
        Err(ContactError::BadRequest(_))
    ));
    assert!(matches!(
        f.service.delete(&admin(), &[]).await,
        Err(ContactError::BadRequest(_))
    ));
    assert_eq!(
        f.service.delete(&admin(), &[ids[2].clone()]).await.unwrap(),
        1
    );
    assert!(matches!(
        f.service.get(&ids[2]).await,
        Err(ContactError::NotFound(_))
    ));

    let actions: Vec<String> = f
        .audit
        .0
        .lock()
        .unwrap()
        .iter()
        .map(|e| e.action.as_str().to_string())
        .collect();
    assert_eq!(actions, vec!["contact.update", "contact.delete"]);
}

#[tokio::test]
async fn test_retention_deletes_old_messages() {
    let f = fixture(|config| config.retention_days = 30).await;
    f.service
        .submit("default", &valid(), &client("203.0.113.1"))
        .await
        .unwrap();
    assert_eq!(f.service.apply_retention(Utc::now()).await, 0);
    assert_eq!(
        f.service
            .apply_retention(Utc::now() + chrono::Duration::days(31))
            .await,
        1
    );
}

#[test]
fn test_forms_summary_and_default_form() {
    let config = ContactConfig {
        forms: vec![
            FormConfig {
                name: "sales".into(),
                subject_field: "topic".into(),
                fields: vec![FieldConfig {
                    name: "topic".into(),
                    label: "Topic".into(),
                    kind: FieldKind::Text,
                    required: true,
                    placeholder: String::new(),
                    options: vec![],
                    max_length: None,
                }],
                ..FormConfig::default()
            },
            FormConfig::default(),
        ],
        ..ContactConfig::default()
    };
    let rt = tokio::runtime::Runtime::new().unwrap();
    let f = rt.block_on(fixture(|c| {
        *c = ContactConfig {
            enabled: true,
            ..config
        }
    }));
    assert_eq!(f.service.default_form().name, "default");
    let forms = f.service.forms();
    assert_eq!(forms[0].path, "/contact/sales");
    assert_eq!(forms[0].title, "Contact");

    // subject_field の値が件名になる
    let saved = rt
        .block_on(f.service.submit(
            "sales",
            &values(&[("topic", "Pricing")]),
            &client("203.0.113.9"),
        ))
        .unwrap()
        .unwrap();
    assert_eq!(saved.subject, "Pricing");
    assert_eq!(saved.email, "");
}
