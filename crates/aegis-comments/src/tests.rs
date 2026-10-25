use std::sync::Mutex;

use aegis_cache::MemoryCacheBackend;
use aegis_core::{AuditActor, AuditEvent, AuditSink, NotificationEvent, PluginManifest};
use aegis_db::{ContentRecord, SqliteAdapter, UserRecord};
use aegis_runtime::AegisPlugin;

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

impl RecordingSink {
    fn actions(&self) -> Vec<(String, String)> {
        self.0
            .lock()
            .unwrap()
            .iter()
            .map(|e| {
                (
                    e.action.as_str().to_string(),
                    e.outcome.as_str().to_string(),
                )
            })
            .collect()
    }
}

/// `check-comment` プラグインの代わり（本文に応じた判定を返す）
struct Checker {
    manifest: PluginManifest,
}

impl Checker {
    fn new() -> Self {
        Self {
            manifest: serde_json::from_value(json!({
                "name": "checker",
                "version": "1.0.0",
                "entrypoint": "checker.wasm",
                "capabilities": { "hooks": ["check-comment"] },
            }))
            .unwrap(),
        }
    }
}

#[async_trait::async_trait]
impl AegisPlugin for Checker {
    fn manifest(&self) -> &PluginManifest {
        &self.manifest
    }

    async fn check_comment(
        &self,
        comment: CommentSubmission,
    ) -> aegis_core::Result<CommentVerdict> {
        Ok(match comment.body.as_str() {
            b if b.contains("[spam]") => CommentVerdict::Spam {
                reason: Some("listed".to_string()),
            },
            b if b.contains("[hold]") => CommentVerdict::Hold { reason: None },
            b if b.contains("[reject]") => {
                return Err(AegisError::PluginRejected("No, thanks".to_string()))
            }
            b if b.contains("[crash]") => {
                return Err(AegisError::PluginRuntime("trap".to_string()))
            }
            _ => CommentVerdict::Allow,
        })
    }
}

struct Fixture {
    service: CommentService,
    db: Arc<dyn DatabaseAdapter>,
    notifier: Arc<RecordingNotifier>,
    audit: Arc<RecordingSink>,
    target: CommentTarget,
}

async fn fixture(config: CommentsConfig) -> Fixture {
    let db: Arc<dyn DatabaseAdapter> =
        Arc::new(SqliteAdapter::connect("sqlite::memory:", 1).await.unwrap());
    db.init().await.unwrap();
    let article: SchemaDefinition = serde_json::from_value(json!({
        "content_type": "article",
        "label": "Articles",
        "fields": [{ "name": "title", "type": "string", "required": true }]
    }))
    .unwrap();
    let schemas = HashMap::from([("article".to_string(), article.clone())]);
    db.run_migrations(&schemas).await.unwrap();
    db.insert_user(&UserRecord {
        id: "u1".to_string(),
        username: "editor1".to_string(),
        email: Some("editor@example.com".to_string()),
        display_name: Some("Editor".to_string()),
        role: "editor".to_string(),
        password_hash: "x".to_string(),
        disabled: false,
        avatar_key: None,
        created_at: "2026-01-01T00:00:00Z".to_string(),
        updated_at: "2026-01-01T00:00:00Z".to_string(),
        pending_approval: false,
    })
    .await
    .unwrap();
    db.insert_content(
        &article,
        &ContentRecord {
            id: "a1".to_string(),
            content_type: "article".to_string(),
            title: "Hello".to_string(),
            slug: "hello".to_string(),
            status: "published".to_string(),
            blocks_json: "[]".to_string(),
            author_id: Some("editor1".to_string()),
            published_at: Some("2026-10-01T00:00:00Z".to_string()),
            unpublish_at: None,
            created_at: "2026-10-01T00:00:00Z".to_string(),
            updated_at: "2026-10-01T00:00:00Z".to_string(),
            custom_fields: HashMap::new(),
        },
    )
    .await
    .unwrap();

    let mut supervisor = PluginSupervisor::new();
    supervisor.register(Arc::new(Checker::new()));
    let audit = Arc::new(RecordingSink::default());
    let notifier = Arc::new(RecordingNotifier::default());
    let service = CommentService::new(
        config,
        CommentDeps {
            db: db.clone(),
            cache: Arc::new(MemoryCacheBackend::new()),
            schemas: Arc::new(RwLock::new(schemas)),
            supervisor: Arc::new(RwLock::new(supervisor)),
            audit: Arc::new(AuditService::with_sinks(vec![audit.clone()])),
            notifier: Some(notifier.clone()),
            links: Links {
                content_url: Arc::new(|_, slug| format!("https://example.com/{}", slug)),
                site_url: "https://example.com".to_string(),
                admin_url: "https://example.com/admin/".to_string(),
            },
        },
    )
    .unwrap();
    let target = service.target("article", "hello").await.unwrap();
    Fixture {
        service,
        db,
        notifier,
        audit,
        target,
    }
}

fn enabled() -> CommentsConfig {
    CommentsConfig {
        enabled: true,
        ..Default::default()
    }
}

fn request(name: &str, body: &str) -> CommentRequest {
    CommentRequest {
        name: name.to_string(),
        email: format!("{}@example.com", name.to_lowercase()),
        body: body.to_string(),
        ..Default::default()
    }
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

async fn all_comments(db: &Arc<dyn DatabaseAdapter>) -> Vec<CommentRecord> {
    db.list_comments(&CommentListQuery {
        oldest_first: true,
        ..Default::default()
    })
    .await
    .unwrap()
    .comments
}

#[tokio::test]
async fn test_first_comment_waits_and_known_authors_are_approved_like_wordpress() {
    let f = fixture(enabled()).await;
    let submitted = f
        .service
        .submit(
            &f.target,
            request("Hanako", "First!"),
            &client("203.0.113.1"),
        )
        .await
        .unwrap();
    assert_eq!(submitted.status, ModerationStatus::Pending);
    assert!(submitted.comment.is_none());

    // The author and the staff hear about the comment waiting for approval
    let events = f.notifier.take();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].kind, "comment.pending");
    assert_eq!(
        events[0].recipients,
        vec![
            Recipient::person("editor@example.com", "Editor"),
            Recipient::Staff
        ]
    );
    let data = &events[0].data;
    assert_eq!(data["comment"]["author_name"], "Hanako");
    assert_eq!(data["comment"]["author_email"], "hanako@example.com");
    assert_eq!(data["content"]["title"], "Hello");
    assert_eq!(data["display_name"], "Editor");
    assert_eq!(
        data["moderation_url"],
        "https://example.com/admin/#comments"
    );

    let first = all_comments(&f.db).await.remove(0);
    assert_eq!(first.status, "pending");
    assert_eq!(first.ip.as_deref(), Some("203.0.113.1"));

    // Not on the public page until approved
    let section = f
        .service
        .section(&f.target, "/post".to_string())
        .await
        .unwrap();
    assert_eq!(section.count, 0);
    assert!(section.open);

    assert_eq!(
        f.service
            .moderate(&admin(), &[first.id.clone()], ModerationStatus::Approved)
            .await
            .unwrap(),
        1
    );
    assert!(f
        .audit
        .actions()
        .contains(&("comment.moderate".to_string(), "success".to_string())));

    // Same name and email: approved without waiting, and the author is told
    let submitted = f
        .service
        .submit(
            &f.target,
            request("Hanako", "Again"),
            &client("203.0.113.1"),
        )
        .await
        .unwrap();
    assert_eq!(submitted.status, ModerationStatus::Approved);
    assert_eq!(submitted.comment.unwrap().body_html, "<p>Again</p>");
    let events = f.notifier.take();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].kind, "comment.posted");
    assert_eq!(
        events[0].recipients,
        vec![Recipient::person("editor@example.com", "Editor")]
    );
    assert!(events[0].data["content_url"]
        .as_str()
        .unwrap()
        .starts_with("https://example.com/hello#comment-"));

    // A different name with the same email still waits
    let submitted = f
        .service
        .submit(
            &f.target,
            CommentRequest {
                email: "hanako@example.com".to_string(),
                ..request("Someone", "Hi")
            },
            &client("203.0.113.1"),
        )
        .await
        .unwrap();
    assert_eq!(submitted.status, ModerationStatus::Pending);

    let section = f
        .service
        .section(&f.target, "/post".to_string())
        .await
        .unwrap();
    assert_eq!(section.count, 2);
    assert_eq!(section.post_url, "/post");
}

#[tokio::test]
async fn test_approval_can_be_switched_off_and_back_from_admin_studio() {
    let f = fixture(enabled()).await;
    let settings = f.service.settings().await.unwrap();
    assert!(settings.policy.require_approval);
    assert_eq!(settings.overrides, PolicyOverrides::default());

    let settings = f
        .service
        .update_settings(
            &admin(),
            PolicyOverrides {
                require_approval: Some(false),
                auto_approve_known: None,
            },
        )
        .await
        .unwrap();
    assert!(!settings.policy.require_approval);
    assert!(settings.defaults.require_approval);
    assert!(f
        .audit
        .actions()
        .contains(&("comment.settings_update".to_string(), "success".to_string())));

    let submitted = f
        .service
        .submit(&f.target, request("Taro", "Open"), &client("203.0.113.2"))
        .await
        .unwrap();
    assert_eq!(submitted.status, ModerationStatus::Approved);

    // Back to the configuration file's value
    f.service
        .update_settings(&admin(), PolicyOverrides::default())
        .await
        .unwrap();
    let submitted = f
        .service
        .submit(
            &f.target,
            request("Jiro", "Closed again"),
            &client("203.0.113.2"),
        )
        .await
        .unwrap();
    assert_eq!(submitted.status, ModerationStatus::Pending);
}

#[tokio::test]
async fn test_spam_words_links_honeypot_and_rate_limit() {
    let mut config = enabled();
    config.moderation.require_approval = false;
    config.moderation.spam_words = vec!["casino".to_string()];
    config.moderation.max_per_ip_per_hour = 3;
    let f = fixture(config).await;

    // Spam: stored as spam, nobody notified, and the sender is told it waits
    let submitted = f
        .service
        .submit(
            &f.target,
            request("Bot", "Best Casino"),
            &client("198.51.100.1"),
        )
        .await
        .unwrap();
    assert_eq!(submitted.status, ModerationStatus::Pending);
    assert!(f.notifier.take().is_empty());

    // Too many links: held
    let submitted = f
        .service
        .submit(
            &f.target,
            request(
                "Linker",
                "http://a.example http://b.example http://c.example",
            ),
            &client("198.51.100.1"),
        )
        .await
        .unwrap();
    assert_eq!(submitted.status, ModerationStatus::Pending);

    // Honeypot: not stored and not counted
    let submitted = f
        .service
        .submit(
            &f.target,
            CommentRequest {
                website: "http://spam.example".to_string(),
                ..request("Bot", "hello")
            },
            &client("198.51.100.1"),
        )
        .await
        .unwrap();
    assert_eq!(submitted.status, ModerationStatus::Pending);

    let statuses: Vec<_> = all_comments(&f.db)
        .await
        .into_iter()
        .map(|c| c.status)
        .collect();
    assert_eq!(statuses, ["spam", "pending"]);

    // The third comment from the address is allowed, the fourth is not
    f.service
        .submit(&f.target, request("Hanako", "ok"), &client("198.51.100.1"))
        .await
        .unwrap();
    assert!(matches!(
        f.service
            .submit(
                &f.target,
                request("Hanako", "again"),
                &client("198.51.100.1")
            )
            .await,
        Err(CommentError::RateLimited)
    ));

    // Input errors
    for bad in [
        request("", "body"),
        request("Hanako", "  "),
        CommentRequest {
            email: "not-an-email".to_string(),
            ..request("Hanako", "x")
        },
    ] {
        assert!(matches!(
            f.service
                .submit(&f.target, bad, &client("198.51.100.2"))
                .await,
            Err(CommentError::Invalid(_))
        ));
    }
}

#[tokio::test]
async fn test_check_comment_plugins_can_hold_mark_spam_or_reject() {
    let mut config = enabled();
    config.moderation.require_approval = false;
    let f = fixture(config).await;
    let submit = |body: &'static str| {
        let f = &f;
        async move {
            f.service
                .submit(&f.target, request("Hanako", body), &client("192.0.2.1"))
                .await
        }
    };

    assert_eq!(
        submit("fine").await.unwrap().status,
        ModerationStatus::Approved
    );
    assert_eq!(
        submit("[hold]").await.unwrap().status,
        ModerationStatus::Pending
    );
    assert_eq!(
        submit("[spam]").await.unwrap().status,
        ModerationStatus::Pending
    );
    match submit("[reject]").await {
        Err(CommentError::Rejected(message)) => assert_eq!(message, "No, thanks"),
        other => panic!("expected a rejection, got {:?}", other),
    }
    // A failing plugin holds the comment and is audited
    assert_eq!(
        submit("[crash]").await.unwrap().status,
        ModerationStatus::Pending
    );
    assert!(f
        .audit
        .actions()
        .contains(&("plugin.error".to_string(), "failure".to_string())));

    let statuses: Vec<_> = all_comments(&f.db)
        .await
        .into_iter()
        .map(|c| c.status)
        .collect();
    assert_eq!(statuses, ["approved", "pending", "spam", "pending"]);
}

#[tokio::test]
async fn test_replies_notify_subscribers_and_can_be_unsubscribed() {
    let mut config = enabled();
    config.moderation.require_approval = false;
    config.max_depth = 1;
    let f = fixture(config).await;

    let parent = f
        .service
        .submit(
            &f.target,
            CommentRequest {
                notify_replies: true,
                ..request("Hanako", "Question?")
            },
            &client("192.0.2.1"),
        )
        .await
        .unwrap()
        .comment
        .unwrap();
    f.notifier.take();

    let reply = f
        .service
        .submit(
            &f.target,
            CommentRequest {
                parent_id: Some(parent.id.clone()),
                ..request("Taro", "Answer.")
            },
            &client("192.0.2.2"),
        )
        .await
        .unwrap()
        .comment
        .unwrap();
    assert_eq!(reply.depth, 1);
    let events = f.notifier.take();
    let replied: Vec<_> = events
        .iter()
        .filter(|e| e.kind == "comment.replied")
        .collect();
    assert_eq!(replied.len(), 1);
    assert_eq!(
        replied[0].recipients,
        vec![Recipient::person("hanako@example.com", "Hanako")]
    );
    let data = &replied[0].data;
    assert_eq!(data["reply"]["author_name"], "Taro");
    assert_eq!(data["reply"]["body"], "Answer.");
    // Nothing private about the person replying
    assert!(data["reply"].get("author_email").is_none());
    let unsubscribe = data["unsubscribe_url"].as_str().unwrap().to_string();
    assert!(unsubscribe.starts_with(&format!(
        "https://example.com/comments/unsubscribe?id={}&token=",
        parent.id
    )));

    // Depth 1: no replies to replies
    assert!(matches!(
        f.service
            .submit(
                &f.target,
                CommentRequest {
                    parent_id: Some(reply.id.clone()),
                    ..request("Jiro", "Nested")
                },
                &client("192.0.2.3"),
            )
            .await,
        Err(CommentError::Invalid(_))
    ));

    // Replying to yourself sends nothing
    f.service
        .submit(
            &f.target,
            CommentRequest {
                parent_id: Some(parent.id.clone()),
                ..request("Hanako", "Thanks")
            },
            &client("192.0.2.1"),
        )
        .await
        .unwrap();
    assert!(f
        .notifier
        .take()
        .iter()
        .all(|e| e.kind != "comment.replied"));

    // Unsubscribe: a wrong token is refused, the right one stops further notices
    let token = unsubscribe.rsplit("token=").next().unwrap().to_string();
    assert!(matches!(
        f.service.unsubscribe(&parent.id, "wrong").await,
        Err(CommentError::NotFound(_))
    ));
    f.service.unsubscribe(&parent.id, &token).await.unwrap();
    f.service
        .submit(
            &f.target,
            CommentRequest {
                parent_id: Some(parent.id.clone()),
                ..request("Saburo", "Late answer")
            },
            &client("192.0.2.4"),
        )
        .await
        .unwrap();
    assert!(f
        .notifier
        .take()
        .iter()
        .all(|e| e.kind != "comment.replied"));
}

#[tokio::test]
async fn test_approving_a_held_reply_notifies_the_subscriber() {
    let f = fixture(enabled()).await;
    let parent = f
        .service
        .submit(
            &f.target,
            CommentRequest {
                notify_replies: true,
                ..request("Hanako", "Question?")
            },
            &client("192.0.2.1"),
        )
        .await
        .unwrap();
    assert_eq!(parent.status, ModerationStatus::Pending);
    let parent_id = all_comments(&f.db).await[0].id.clone();

    // Replies go to approved comments only
    assert!(matches!(
        f.service
            .submit(
                &f.target,
                CommentRequest {
                    parent_id: Some(parent_id.clone()),
                    ..request("Taro", "Answer")
                },
                &client("192.0.2.2"),
            )
            .await,
        Err(CommentError::Invalid(_))
    ));

    // A staff reply approves the comment it answers
    let reply = f
        .service
        .reply(
            &admin(),
            &parent_id,
            &StaffAuthor {
                username: "editor1".to_string(),
                display_name: Some("Editor".to_string()),
                email: Some("editor@example.com".to_string()),
            },
            "Thanks for asking",
        )
        .await
        .unwrap();
    assert_eq!(reply.status, "approved");
    assert_eq!(reply.author_user_id.as_deref(), Some("editor1"));
    let parent = f.db.get_comment(&parent_id).await.unwrap().unwrap();
    assert_eq!(parent.status, "approved");
    let events = f.notifier.take();
    let replied = events
        .iter()
        .find(|e| e.kind == "comment.replied")
        .expect("comment.replied");
    assert_eq!(replied.data["reply"]["staff"], true);
    assert!(f
        .audit
        .actions()
        .contains(&("comment.create".to_string(), "success".to_string())));

    let section = f.service.section(&f.target, String::new()).await.unwrap();
    let items: Vec<_> = section.items.iter().map(|c| (c.depth, c.staff)).collect();
    assert_eq!(items, [(0, false), (1, true)]);

    f.service
        .submit(
            &f.target,
            CommentRequest {
                parent_id: Some(parent_id.clone()),
                ..request("Taro", "Me too")
            },
            &client("192.0.2.2"),
        )
        .await
        .unwrap();
    // Taro has no approved comment yet, so the reply waits
    let held = all_comments(&f.db)
        .await
        .into_iter()
        .find(|c| c.author_name == "Taro")
        .unwrap();
    assert_eq!(held.status, "pending");
    f.notifier.take();
    f.service
        .moderate(&admin(), &[held.id.clone()], ModerationStatus::Approved)
        .await
        .unwrap();
    let events = f.notifier.take();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].kind, "comment.replied");
    assert_eq!(events[0].data["reply"]["author_name"], "Taro");
}

#[tokio::test]
async fn test_admin_list_counts_and_delete() {
    let f = fixture(enabled()).await;
    for name in ["A", "B", "C"] {
        f.service
            .submit(&f.target, request(name, "hi"), &client("192.0.2.9"))
            .await
            .unwrap();
    }
    let ids: Vec<String> = all_comments(&f.db)
        .await
        .into_iter()
        .map(|c| c.id)
        .collect();
    f.service
        .moderate(&admin(), &ids[..1], ModerationStatus::Spam)
        .await
        .unwrap();

    let page = f
        .service
        .list(&AdminListQuery {
            status: Some(ModerationStatus::Pending),
            limit: 10,
            ..Default::default()
        })
        .await
        .unwrap();
    assert_eq!(page.total, 2);
    assert_eq!(page.counts["pending"], 2);
    assert_eq!(page.counts["spam"], 1);
    assert_eq!(page.counts["approved"], 0);
    let first = &page.comments[0];
    assert_eq!(first.author_name, "C");
    let content = first.content.as_ref().unwrap();
    assert_eq!(content.title, "Hello");
    assert_eq!(content.url, "https://example.com/hello");

    assert!(matches!(
        f.service
            .moderate(&admin(), &[], ModerationStatus::Trash)
            .await,
        Err(CommentError::Invalid(_))
    ));
    assert_eq!(f.service.delete(&admin(), &ids).await.unwrap(), 3);
    assert!(f
        .audit
        .actions()
        .contains(&("comment.delete".to_string(), "success".to_string())));
}

#[tokio::test]
async fn test_targets_and_closing() {
    let mut config = enabled();
    config.close_after_days = 30;
    let f = fixture(config).await;

    assert!(matches!(
        f.service.target("article", "missing").await,
        Err(CommentError::NotFound(_))
    ));
    assert!(matches!(
        f.service.target("page", "hello").await,
        Err(CommentError::NotFound(_))
    ));
    assert_eq!(f.target.id, "a1");
    assert_eq!(f.target.label, "Articles");

    let published: DateTime<Utc> = "2026-10-01T00:00:00Z".parse().unwrap();
    assert!(f
        .service
        .is_open(&f.target, published + chrono::Duration::days(29)));
    assert!(!f
        .service
        .is_open(&f.target, published + chrono::Duration::days(30)));
}

fn record(id: &str, parent: Option<&str>) -> CommentRecord {
    CommentRecord {
        id: id.to_string(),
        parent_id: parent.map(str::to_string),
        body: id.to_string(),
        ..Default::default()
    }
}

#[test]
fn test_thread_order_and_depth() {
    let records = vec![
        record("a", None),
        record("b", None),
        record("a1", Some("a")),
        record("a1x", Some("a1")),
        record("b1", Some("b")),
        record("a2", Some("a")),
        record("orphan", Some("gone")),
        record("a1xy", Some("a1x")),
    ];
    let threaded: Vec<_> = thread(records, 2)
        .into_iter()
        .map(|c| (c.id, c.depth))
        .collect();
    assert_eq!(
        threaded,
        [
            ("a".to_string(), 0),
            ("a1".to_string(), 1),
            ("a1x".to_string(), 2),
            ("a1xy".to_string(), 2),
            ("a2".to_string(), 1),
            ("b".to_string(), 0),
            ("b1".to_string(), 1),
            ("orphan".to_string(), 0),
        ]
    );

    // A loop (never written by the service) still shows every comment once
    let looped = thread(vec![record("x", Some("y")), record("y", Some("x"))], 3);
    assert_eq!(looped.len(), 2);
}
