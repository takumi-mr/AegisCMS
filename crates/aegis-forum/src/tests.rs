use std::sync::{Arc, Mutex};

use aegis_cache::MemoryCacheBackend;
use aegis_core::{AuditEvent, AuditSink};
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

struct Fixture {
    service: ForumService,
    #[allow(dead_code)]
    db: Arc<dyn DatabaseAdapter>,
    #[allow(dead_code)]
    sink: Arc<RecordingSink>,
}

async fn fixture(mut config: ForumConfig) -> Fixture {
    config.enabled = true;
    let db: Arc<dyn DatabaseAdapter> =
        Arc::new(SqliteAdapter::connect("sqlite::memory:", 1).await.unwrap());
    db.init().await.unwrap();

    let cache: Arc<dyn CacheBackend> = Arc::new(MemoryCacheBackend::new());
    let sink = Arc::new(RecordingSink::default());
    let audit = Arc::new(AuditService::with_sinks(vec![sink.clone()]));

    let deps = ForumDeps {
        db: db.clone(),
        cache,
        audit,
    };

    let service = ForumService::new(config, deps).unwrap();
    Fixture { service, db, sink }
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
