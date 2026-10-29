use std::sync::{Arc, Mutex};

use aegis_cache::MemoryCacheBackend;
use aegis_core::{AuditEvent, AuditSink, ModerationStatus, NotificationEvent, Notifier};
use aegis_db::SqliteAdapter;

use super::*;

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

#[derive(Default)]
struct RecordingNotifier(Mutex<Vec<NotificationEvent>>);

impl Notifier for RecordingNotifier {
    fn notify(&self, event: NotificationEvent) {
        self.0.lock().unwrap().push(event);
    }
}

struct Fixture {
    service: ForumService,
    #[allow(dead_code)]
    db: Arc<dyn DatabaseAdapter>,
    #[allow(dead_code)]
    sink: Arc<RecordingSink>,
    notifier: Arc<RecordingNotifier>,
}

async fn fixture(mut config: ForumConfig) -> Fixture {
    config.enabled = true;
    let db: Arc<dyn DatabaseAdapter> =
        Arc::new(SqliteAdapter::connect("sqlite::memory:", 1).await.unwrap());
    db.init().await.unwrap();

    let cache: Arc<dyn CacheBackend> = Arc::new(MemoryCacheBackend::new());
    let sink = Arc::new(RecordingSink::default());
    let audit = Arc::new(AuditService::with_sinks(vec![sink.clone()]));
    let notifier = Arc::new(RecordingNotifier::default());

    let deps = ForumDeps {
        db: db.clone(),
        cache,
        audit,
        notifier: Some(notifier.clone()),
        site_url: "https://example.com".to_string(),
        admin_url: "https://example.com/admin/".to_string(),
    };

    let service = ForumService::new(config, deps).unwrap();
    Fixture {
        service,
        db,
        sink,
        notifier,
    }
}

#[tokio::test]
async fn test_category_lifecycle() {
    let f = fixture(ForumConfig::default()).await;

    // 作成
    let cat = f
        .service
        .create_category(
            CreateCategoryRequest {
                slug: "general".to_string(),
                name: "General Discussion".to_string(),
                description: "Talk about anything".to_string(),
                position: 1,
                is_private: false,
            },
            Some("admin"),
        )
        .await
        .unwrap();

    assert_eq!(cat.slug, "general");
    assert_eq!(cat.name, "General Discussion");

    // 重複 slug の拒否
    let err = f
        .service
        .create_category(
            CreateCategoryRequest {
                slug: "general".to_string(),
                name: "Another".to_string(),
                description: "".to_string(),
                position: 2,
                is_private: false,
            },
            None,
        )
        .await
        .unwrap_err();
    assert!(matches!(err, ForumError::Invalid(_)));

    // 取得
    let fetched = f.service.get_category_by_slug("general").await.unwrap();
    assert_eq!(fetched.id, cat.id);

    // 更新
    let updated = f
        .service
        .update_category(
            &cat.id,
            UpdateCategoryRequest {
                slug: "general-chat".to_string(),
                name: "General Chat".to_string(),
                description: "Updated desc".to_string(),
                position: 10,
                is_private: true,
            },
            Some("admin"),
        )
        .await
        .unwrap();
    assert_eq!(updated.slug, "general-chat");
    assert!(updated.is_private);

    // 一覧
    let public_list = f.service.list_categories(false).await.unwrap();
    assert!(public_list.is_empty());
    let all_list = f.service.list_categories(true).await.unwrap();
    assert_eq!(all_list.len(), 1);

    // 削除
    f.service
        .delete_category(&cat.id, Some("admin"))
        .await
        .unwrap();
    assert!(f.service.get_category(&cat.id).await.is_err());
}

#[tokio::test]
async fn test_thread_and_post_flow() {
    let cfg = ForumConfig {
        rate_limit_secs: 0, // テスト用に連投制限オフ
        ..Default::default()
    };
    let f = fixture(cfg).await;

    let cat = f
        .service
        .create_category(
            CreateCategoryRequest {
                slug: "tech".to_string(),
                name: "Technology".to_string(),
                description: "Tech discussions".to_string(),
                position: 0,
                is_private: false,
            },
            None,
        )
        .await
        .unwrap();

    // スレッド作成（最初の投稿が自動的に作成される）
    let (th, post1) = f
        .service
        .create_thread(CreateThreadRequest {
            category_id: cat.id.clone(),
            title: "Rust 2026 Edition Discussion".to_string(),
            body: "What features are you most excited about?".to_string(),
            author_name: "Ferris".to_string(),
            author_id: Some("user1".to_string()),
            ip: Some("127.0.0.1".to_string()),
            user_agent: None,
        })
        .await
        .unwrap();

    assert_eq!(th.title, "Rust 2026 Edition Discussion");
    assert_eq!(th.post_count, 1);
    assert_eq!(th.status, "approved");
    assert_eq!(post1.post_number, 1);
    assert_eq!(post1.body, "What features are you most excited about?");

    // レス投稿 (post 2)
    let post2 = f
        .service
        .create_post(CreatePostRequest {
            thread_id: th.id.clone(),
            body: "Pattern types and faster compile times!".to_string(),
            reply_to_post_id: Some(post1.id.clone()),
            reply_to_number: Some(1),
            author_name: "CrabFan".to_string(),
            author_id: Some("user2".to_string()),
            ip: Some("127.0.0.2".to_string()),
            user_agent: None,
        })
        .await
        .unwrap();

    assert_eq!(post2.post_number, 2);
    assert_eq!(post2.reply_to_number, Some(1));

    // スレッド情報の更新確認 (post_count が 2 になっていること)
    let updated_th = f.service.get_thread(&th.id).await.unwrap();
    assert_eq!(updated_th.post_count, 2);
    assert!(updated_th.last_post_at >= th.last_post_at);

    // レス一覧のキーセット取得
    let posts = f
        .service
        .list_posts(&PostListQuery {
            thread_id: th.id.clone(),
            after_post_number: 0,
            status: None,
            limit: 10,
        })
        .await
        .unwrap();
    assert_eq!(posts.len(), 2);
    assert_eq!(posts[0].post_number, 1);
    assert_eq!(posts[1].post_number, 2);

    // after_post_number = 1 で 2番目だけ取得
    let posts_after = f
        .service
        .list_posts(&PostListQuery {
            thread_id: th.id.clone(),
            after_post_number: 1,
            status: None,
            limit: 10,
        })
        .await
        .unwrap();
    assert_eq!(posts_after.len(), 1);
    assert_eq!(posts_after[0].post_number, 2);

    // レスのソフトデリート
    f.service
        .delete_post(&post2.id, Some("moderator"))
        .await
        .unwrap();
    let deleted_post = f.service.get_post(&post2.id).await.unwrap();
    assert!(deleted_post.is_deleted);

    // 新着通知が 2 件送られていること
    let notifications = f.notifier.0.lock().unwrap();
    assert_eq!(notifications.len(), 2);
    assert_eq!(notifications[0].kind, "forum.posted");
    assert_eq!(notifications[1].kind, "forum.posted");
}

#[tokio::test]
async fn test_thread_pin_and_lock() {
    let cfg = ForumConfig {
        rate_limit_secs: 0,
        ..Default::default()
    };
    let f = fixture(cfg).await;

    let cat = f
        .service
        .create_category(
            CreateCategoryRequest {
                slug: "announcements".to_string(),
                name: "Announcements".to_string(),
                description: "".to_string(),
                position: 0,
                is_private: false,
            },
            None,
        )
        .await
        .unwrap();

    let (th, _) = f
        .service
        .create_thread(CreateThreadRequest {
            category_id: cat.id.clone(),
            title: "Rules and Guidelines".to_string(),
            body: "Please be nice.".to_string(),
            author_name: "Admin".to_string(),
            author_id: Some("admin1".to_string()),
            ip: None,
            user_agent: None,
        })
        .await
        .unwrap();

    // ピン留め
    f.service
        .set_thread_pinned(&th.id, true, Some("admin"))
        .await
        .unwrap();
    let pinned_th = f.service.get_thread(&th.id).await.unwrap();
    assert!(pinned_th.is_pinned);

    // ロック
    f.service
        .set_thread_locked(&th.id, true, Some("admin"))
        .await
        .unwrap();
    let locked_th = f.service.get_thread(&th.id).await.unwrap();
    assert!(locked_th.is_locked);

    // ロックされたスレッドへの投稿拒否
    let err = f
        .service
        .create_post(CreatePostRequest {
            thread_id: th.id.clone(),
            body: "Trying to reply to locked thread".to_string(),
            reply_to_post_id: None,
            reply_to_number: None,
            author_name: "User".to_string(),
            author_id: None,
            ip: None,
            user_agent: None,
        })
        .await
        .unwrap_err();

    assert!(matches!(err, ForumError::Locked));
}

#[tokio::test]
async fn test_auto_lock_on_max_posts() {
    let cfg = ForumConfig {
        rate_limit_secs: 0,
        auto_lock_post_count: 3, // 3レスで自動ロック
        ..Default::default()
    };
    let f = fixture(cfg).await;

    let cat = f
        .service
        .create_category(
            CreateCategoryRequest {
                slug: "fast".to_string(),
                name: "Fast Thread".to_string(),
                description: "".to_string(),
                position: 0,
                is_private: false,
            },
            None,
        )
        .await
        .unwrap();

    let (th, _) = f
        .service
        .create_thread(CreateThreadRequest {
            category_id: cat.id,
            title: "Short Lived".to_string(),
            body: "Post 1".to_string(),
            author_name: "A".to_string(),
            author_id: None,
            ip: None,
            user_agent: None,
        })
        .await
        .unwrap();

    // Post 2
    f.service
        .create_post(CreatePostRequest {
            thread_id: th.id.clone(),
            body: "Post 2".to_string(),
            reply_to_post_id: None,
            reply_to_number: None,
            author_name: "B".to_string(),
            author_id: None,
            ip: None,
            user_agent: None,
        })
        .await
        .unwrap();

    let th_mid = f.service.get_thread(&th.id).await.unwrap();
    assert!(!th_mid.is_locked);

    // Post 3 (ここで自動ロック発動)
    f.service
        .create_post(CreatePostRequest {
            thread_id: th.id.clone(),
            body: "Post 3 - Max reached!".to_string(),
            reply_to_post_id: None,
            reply_to_number: None,
            author_name: "C".to_string(),
            author_id: None,
            ip: None,
            user_agent: None,
        })
        .await
        .unwrap();

    let th_locked = f.service.get_thread(&th.id).await.unwrap();
    assert!(th_locked.is_locked);
    assert_eq!(th_locked.post_count, 3);
}

#[tokio::test]
async fn test_rate_limiting() {
    let cfg = ForumConfig {
        rate_limit_secs: 10,
        ..Default::default()
    };
    let f = fixture(cfg).await;

    let cat = f
        .service
        .create_category(
            CreateCategoryRequest {
                slug: "speed".to_string(),
                name: "Speed".to_string(),
                description: "".to_string(),
                position: 0,
                is_private: false,
            },
            None,
        )
        .await
        .unwrap();

    // 1回目の投稿
    let (th, _) = f
        .service
        .create_thread(CreateThreadRequest {
            category_id: cat.id,
            title: "First Thread".to_string(),
            body: "Hello".to_string(),
            author_name: "Spammer".to_string(),
            author_id: None,
            ip: Some("192.168.1.100".to_string()),
            user_agent: None,
        })
        .await
        .unwrap();

    // 直後の2回目（同じIP）→ RateLimited で拒否される
    let err = f
        .service
        .create_post(CreatePostRequest {
            thread_id: th.id,
            body: "Too fast reply".to_string(),
            reply_to_post_id: None,
            reply_to_number: None,
            author_name: "Spammer".to_string(),
            author_id: None,
            ip: Some("192.168.1.100".to_string()),
            user_agent: None,
        })
        .await
        .unwrap_err();

    assert!(matches!(err, ForumError::RateLimited));
}

#[tokio::test]
async fn test_anonymous_disallowed() {
    let cfg = ForumConfig {
        allow_anonymous: false, // 匿名禁止
        ..Default::default()
    };
    let f = fixture(cfg).await;

    let cat = f
        .service
        .create_category(
            CreateCategoryRequest {
                slug: "members".to_string(),
                name: "Members Only".to_string(),
                description: "".to_string(),
                position: 0,
                is_private: false,
            },
            None,
        )
        .await
        .unwrap();

    let err = f
        .service
        .create_thread(CreateThreadRequest {
            category_id: cat.id,
            title: "Guest Post".to_string(),
            body: "I have no account".to_string(),
            author_name: "Guest".to_string(),
            author_id: None, // 未ログイン
            ip: None,
            user_agent: None,
        })
        .await
        .unwrap_err();

    assert!(matches!(err, ForumError::Forbidden(_)));
}

#[tokio::test]
async fn test_moderation_approval_ng_words_and_reports() {
    let mut cfg = ForumConfig {
        rate_limit_secs: 0,
        ..Default::default()
    };
    cfg.moderation.require_approval = true;
    cfg.moderation.auto_approve_known = true;
    cfg.moderation.hold_words = vec!["reviewme".to_string()];
    cfg.moderation.spam_words = vec!["buy-crypto-now".to_string()];
    let f = fixture(cfg).await;

    let cat = f
        .service
        .create_category(
            CreateCategoryRequest {
                slug: "mod".to_string(),
                name: "Moderated".to_string(),
                description: "".to_string(),
                position: 0,
                is_private: false,
            },
            None,
        )
        .await
        .unwrap();

    // 1. 新規ユーザーの初回スレッド作成 -> require_approval=true なので pending
    let (th1, post1) = f
        .service
        .create_thread(CreateThreadRequest {
            category_id: cat.id.clone(),
            title: "First Thread by Alice".to_string(),
            body: "Hello everyone!".to_string(),
            author_name: "Alice".to_string(),
            author_id: None,
            ip: Some("203.0.113.10".to_string()),
            user_agent: None,
        })
        .await
        .unwrap();
    assert_eq!(th1.status, "pending");
    assert_eq!(post1.status, "pending");

    // 通知に forum.pending が送られていること
    {
        let notes = f.notifier.0.lock().unwrap();
        assert_eq!(notes.last().unwrap().kind, "forum.pending");
    }

    // 公開用フィルタ（status=Some("approved")）ではまだ見えない
    let approved_threads = f
        .service
        .list_threads(&ThreadListQuery {
            category_id: Some(cat.id.clone()),
            status: Some("approved".to_string()),
            before_last_post_at: None,
            before_id: None,
            limit: 10,
        })
        .await
        .unwrap();
    assert!(approved_threads.is_empty());

    // 管理者がスレッドと投稿を承認する
    f.service
        .moderate_thread(&th1.id, ModerationStatus::Approved, Some("admin"))
        .await
        .unwrap();
    f.service
        .moderate_post(&post1.id, ModerationStatus::Approved, Some("admin"))
        .await
        .unwrap();

    let approved_threads = f
        .service
        .list_threads(&ThreadListQuery {
            category_id: Some(cat.id.clone()),
            status: Some("approved".to_string()),
            before_last_post_at: None,
            before_id: None,
            limit: 10,
        })
        .await
        .unwrap();
    assert_eq!(approved_threads.len(), 1);

    // 2. Alice は承認済み投稿があるので、2回目のレスは auto_approve_known で自動承認される
    let post2 = f
        .service
        .create_post(CreatePostRequest {
            thread_id: th1.id.clone(),
            body: "Follow-up from Alice".to_string(),
            reply_to_post_id: None,
            reply_to_number: None,
            author_name: "Alice".to_string(),
            author_id: None,
            ip: Some("203.0.113.10".to_string()),
            user_agent: None,
        })
        .await
        .unwrap();
    assert_eq!(post2.status, "approved");

    // 3. NGワード（hold_words）を含む場合は承認済みユーザーでも pending になる
    let post_hold = f
        .service
        .create_post(CreatePostRequest {
            thread_id: th1.id.clone(),
            body: "Please reviewme before publishing".to_string(),
            reply_to_post_id: None,
            reply_to_number: None,
            author_name: "Alice".to_string(),
            author_id: None,
            ip: Some("203.0.113.10".to_string()),
            user_agent: None,
        })
        .await
        .unwrap();
    assert_eq!(post_hold.status, "pending");

    // 4. NGワード（spam_words）を含む場合は spam になり通知も送られない
    let count_before = f.notifier.0.lock().unwrap().len();
    let post_spam = f
        .service
        .create_post(CreatePostRequest {
            thread_id: th1.id.clone(),
            body: "Click here to buy-crypto-now!".to_string(),
            reply_to_post_id: None,
            reply_to_number: None,
            author_name: "SpamBot".to_string(),
            author_id: None,
            ip: Some("203.0.113.99".to_string()),
            user_agent: None,
        })
        .await
        .unwrap();
    assert_eq!(post_spam.status, "spam");
    assert_eq!(f.notifier.0.lock().unwrap().len(), count_before);

    // 5. 通報（Report）と解決（Resolve）
    let report = f
        .service
        .report(CreateReportRequest {
            target_type: "post".to_string(),
            target_id: post2.id.clone(),
            reason: "Off-topic content".to_string(),
            reporter_id: None,
            reporter_ip: Some("203.0.113.50".to_string()),
        })
        .await
        .unwrap();
    assert_eq!(report.status, "open");
    assert_eq!(report.thread_id, th1.id);

    {
        let notes = f.notifier.0.lock().unwrap();
        assert_eq!(notes.last().unwrap().kind, "forum.reported");
    }

    let open_reports = f.service.list_reports(Some("open"), 10).await.unwrap();
    assert_eq!(open_reports.len(), 1);

    let resolved = f
        .service
        .resolve_report(&report.id, "resolved", Some("admin"))
        .await
        .unwrap();
    assert_eq!(resolved.status, "resolved");
    assert_eq!(resolved.resolved_by.as_deref(), Some("admin"));
}
