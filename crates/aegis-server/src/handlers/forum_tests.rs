//! フォーラムの分離DB、公開テーマUI、API の結合テスト

use std::sync::Arc;

use axum::body::Body;
use axum::http::{header, Request, StatusCode};
use serde_json::json;
use tower::ServiceExt;

use crate::app;
use crate::config::AegisConfig;
use crate::state::AppState;
use aegis_forum::{CreateCategoryRequest, CreateThreadRequest, ForumService};
use aegis_runtime::PluginSupervisor;

async fn setup_forum_app(
    dedicated_db_url: Option<String>,
) -> (Arc<AppState>, axum::Router, String) {
    let mut config = AegisConfig::default();
    config.forum.enabled = true;
    config.forum.database_url = dedicated_db_url.clone();
    config.forum.rate_limit_secs = 0; // 結合テストでは連続実行を許可
    config.audit.enabled = true;

    let state =
        crate::test_support::test_app_state(config.clone(), PluginSupervisor::new(), Vec::new())
            .await;

    // 専用DBが指定されている場合は独立アダプターを作成、なければメインDB
    let forum_db = if let Some(url) = dedicated_db_url {
        let forum_db_cfg = aegis_db::DatabaseConfig {
            url,
            ..config.database.clone()
        };
        let dedicated = aegis_db::create_database_adapter(&forum_db_cfg)
            .await
            .unwrap();
        dedicated.init().await.unwrap();
        dedicated
    } else {
        state.db.clone()
    };

    let forum_deps = state.forum_deps(forum_db.clone());
    let forum_service = ForumService::new(config.forum.clone(), forum_deps).unwrap();

    // デフォルトカテゴリ "general" を作成
    let cat = forum_service
        .create_category(
            CreateCategoryRequest {
                slug: "general".to_string(),
                name: "General Discussion".to_string(),
                description: "Talk about anything".to_string(),
                position: 0,
                is_private: false,
            },
            None,
        )
        .await
        .unwrap();

    let state = Arc::new(state.with_forum(forum_service));
    let router = app::router(&config, state.clone());

    (state, router, cat.id)
}

#[tokio::test]
async fn test_forum_dedicated_database_isolation() {
    // 独立したインメモリSQLite接続先を指定
    let dedicated_url = "sqlite::memory:?mode=memory&cache=shared".to_string();
    let (state, _, cat_id) = setup_forum_app(Some(dedicated_url)).await;

    let forum = state.forum.as_ref().unwrap();

    // 1. フォーラムサービス経由でスレッドと投稿を作成（専用DBに保存される）
    let (thread, post) = forum
        .create_thread(CreateThreadRequest {
            category_id: cat_id.clone(),
            title: "First Isolated Thread".to_string(),
            body: "This is stored in dedicated database".to_string(),
            author_name: "Explorer".to_string(),
            author_id: None,
            ip: Some("127.0.0.1".to_string()),
            user_agent: None,
        })
        .await
        .unwrap();

    assert_eq!(thread.title, "First Isolated Thread");
    assert_eq!(post.body, "This is stored in dedicated database");

    // 2. 専用DB側で取得できることを確認
    let fetched_thread = forum.get_thread(&thread.id).await.unwrap();
    assert_eq!(fetched_thread.id, thread.id);

    // 3. メインDB側にはフォーラムカテゴリが作成されていない（完全に隔離されている）ことを検証
    let main_cats = state.db.list_categories().await.unwrap();
    assert!(
        main_cats.is_empty(),
        "Main CMS database should not contain forum categories when isolated"
    );
}

#[tokio::test]
async fn test_forum_theme_rendering_index_and_thread() {
    let (_, router, _) = setup_forum_app(None).await;

    // 1. GET /forum (フォーラムトップ)
    let res = router
        .clone()
        .oneshot(
            Request::builder()
                .uri("/forum")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(res.status(), StatusCode::OK);
    let body = axum::body::to_bytes(res.into_body(), 100_000)
        .await
        .unwrap();
    let html = String::from_utf8(body.to_vec()).unwrap();
    assert!(html.contains("Community Forum"));
    assert!(html.contains("General Discussion"));
    assert!(html.contains("Talk about anything"));

    // 2. GET /forum/general (カテゴリ一覧)
    let res = router
        .clone()
        .oneshot(
            Request::builder()
                .uri("/forum/general")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(res.status(), StatusCode::OK);
    let body = axum::body::to_bytes(res.into_body(), 100_000)
        .await
        .unwrap();
    let html = String::from_utf8(body.to_vec()).unwrap();
    assert!(html.contains("General Discussion"));
    assert!(html.contains("Start a New Thread"));
}

#[tokio::test]
async fn test_forum_form_submission_and_redirects() {
    let (_, router, _) = setup_forum_app(None).await;

    // 1. スレッド作成フォーム送信: POST /forum/general/threads
    let form_body = "title=Hello+Forum&author_name=Alice&body=Excited+to+be+here%21";
    let res = router
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/forum/general/threads")
                .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
                .body(Body::from(form_body))
                .unwrap(),
        )
        .await
        .unwrap();

    // 正常作成時は 303 See Other でリダイレクト
    assert_eq!(res.status(), StatusCode::SEE_OTHER);
    let location = res
        .headers()
        .get(header::LOCATION)
        .unwrap()
        .to_str()
        .unwrap();
    assert!(location.starts_with("/forum/general/"));

    // 2. 作成されたスレッド画面を GET
    let res = router
        .clone()
        .oneshot(
            Request::builder()
                .uri(location)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(res.status(), StatusCode::OK);
    let body = axum::body::to_bytes(res.into_body(), 100_000)
        .await
        .unwrap();
    let html = String::from_utf8(body.to_vec()).unwrap();
    assert!(html.contains("Hello Forum"));
    assert!(html.contains("Alice"));
    assert!(html.contains("Excited to be here!"));

    // 3. レス投稿フォーム送信: POST /forum/general/<thread_id>/posts
    let thread_id = location.strip_prefix("/forum/general/").unwrap();
    let post_form_body = "author_name=Bob&body=Welcome+Alice%21";
    let res = router
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(&format!("/forum/general/{}/posts", thread_id))
                .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
                .body(Body::from(post_form_body))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(res.status(), StatusCode::SEE_OTHER);
    let location = res
        .headers()
        .get(header::LOCATION)
        .unwrap()
        .to_str()
        .unwrap();
    assert!(location.contains("#post-2"));
}

#[tokio::test]
async fn test_forum_json_api() {
    let (_, router, cat_id) = setup_forum_app(None).await;

    // 1. GET /api/v1/forum/categories
    let res = router
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/v1/forum/categories")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(res.status(), StatusCode::OK);
    let body = axum::body::to_bytes(res.into_body(), 100_000)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["categories"][0]["slug"], "general");

    // 2. POST /api/v1/forum/threads
    let thread_payload = json!({
        "category_id": cat_id,
        "title": "API Created Thread",
        "body": "Body via JSON API",
        "author_name": "API Tester"
    });
    let res = router
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/forum/threads")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(serde_json::to_vec(&thread_payload).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(res.status(), StatusCode::CREATED);
    let body = axum::body::to_bytes(res.into_body(), 100_000)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let thread_id = json["thread"]["id"].as_str().unwrap();

    // 3. POST /api/v1/forum/posts
    let post_payload = json!({
        "thread_id": thread_id,
        "body": "Replying via API",
        "author_name": "Replier"
    });
    let res = router
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/forum/posts")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(serde_json::to_vec(&post_payload).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(res.status(), StatusCode::CREATED);
    let body = axum::body::to_bytes(res.into_body(), 100_000)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["post"]["post_number"], 2);
}

#[tokio::test]
async fn test_forum_rate_limiting() {
    let mut config = AegisConfig::default();
    config.forum.enabled = true;
    config.forum.rate_limit_secs = 60; // 60秒のレートリミット

    let state =
        crate::test_support::test_app_state(config.clone(), PluginSupervisor::new(), Vec::new())
            .await;
    let forum_deps = state.forum_deps(state.db.clone());
    let forum_service = ForumService::new(config.forum.clone(), forum_deps).unwrap();

    let cat = forum_service
        .create_category(
            CreateCategoryRequest {
                slug: "ratelimit-test".to_string(),
                name: "RateLimit Test".to_string(),
                description: "Testing rate limits".to_string(),
                position: 0,
                is_private: false,
            },
            None,
        )
        .await
        .unwrap();

    let state = Arc::new(state.with_forum(forum_service));
    let router = app::router(&config, state);

    // 1回目の投稿 -> 成功 (201)
    let payload = json!({
        "category_id": cat.id,
        "title": "Fast Post 1",
        "body": "First post body",
        "author_name": "Speedy"
    });
    let res = router
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/forum/threads")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(serde_json::to_vec(&payload).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::CREATED);

    // 直後に同じ author_name / IP から2回目の投稿 -> レートリミット (429)
    let payload2 = json!({
        "category_id": cat.id,
        "title": "Fast Post 2",
        "body": "Second post too fast",
        "author_name": "Speedy"
    });
    let res2 = router
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/forum/threads")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(serde_json::to_vec(&payload2).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res2.status(), StatusCode::TOO_MANY_REQUESTS);
}

#[tokio::test]
async fn test_forum_moderation_settings_and_reports_api() {
    let (state, router, cat_id) = setup_forum_app(None).await;
    let admin_session = state
        .sessions
        .create_session("admin1", "admin", Default::default(), None)
        .await
        .unwrap()
        .id;
    let auth_header = format!("Bearer {}", admin_session);

    // 1. PUT /api/v1/forum/settings -> require_approval = true に変更
    let res = router
        .clone()
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri("/api/v1/forum/settings")
                .header(header::CONTENT_TYPE, "application/json")
                .header(header::AUTHORIZATION, &auth_header)
                .body(Body::from(
                    serde_json::to_vec(&json!({
                        "require_approval": true,
                        "auto_approve_known": true
                    }))
                    .unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let body = axum::body::to_bytes(res.into_body(), 100_000)
        .await
        .unwrap();
    let settings_json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(settings_json["policy"]["require_approval"], true);

    // 2. 新規投稿 -> pending になる
    let res = router
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/forum/threads")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    serde_json::to_vec(&json!({
                        "category_id": cat_id,
                        "title": "Pending Thread",
                        "body": "Needs moderation",
                        "author_name": "Newbie"
                    }))
                    .unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::CREATED);
    let body = axum::body::to_bytes(res.into_body(), 100_000)
        .await
        .unwrap();
    let created: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let thread_id = created["thread"]["id"].as_str().unwrap().to_string();
    let post_id = created["initial_post"]["id"].as_str().unwrap().to_string();
    assert_eq!(created["thread"]["status"], "pending");

    // 3. 未ログインでの GET /api/v1/forum/threads/:id は 404
    let res = router
        .clone()
        .oneshot(
            Request::builder()
                .uri(&format!("/api/v1/forum/threads/{}", thread_id))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::NOT_FOUND);

    // 4. PATCH /api/v1/forum/threads/:id/status & /posts/:id/status で承認
    let res = router
        .clone()
        .oneshot(
            Request::builder()
                .method("PATCH")
                .uri(&format!("/api/v1/forum/threads/{}/status", thread_id))
                .header(header::CONTENT_TYPE, "application/json")
                .header(header::AUTHORIZATION, &auth_header)
                .body(Body::from(
                    serde_json::to_vec(&json!({ "status": "approved" })).unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    let res = router
        .clone()
        .oneshot(
            Request::builder()
                .method("PATCH")
                .uri(&format!("/api/v1/forum/posts/{}/status", post_id))
                .header(header::CONTENT_TYPE, "application/json")
                .header(header::AUTHORIZATION, &auth_header)
                .body(Body::from(
                    serde_json::to_vec(&json!({ "status": "approved" })).unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    // 5. 通報 POST /api/v1/forum/reports
    let res = router
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/forum/reports")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    serde_json::to_vec(&json!({
                        "target_type": "post",
                        "target_id": post_id,
                        "reason": "Inappropriate content"
                    }))
                    .unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::CREATED);
    let body = axum::body::to_bytes(res.into_body(), 100_000)
        .await
        .unwrap();
    let report_json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let report_id = report_json["report"]["id"].as_str().unwrap().to_string();

    // 6. PATCH /api/v1/forum/reports/:id で解決
    let res = router
        .oneshot(
            Request::builder()
                .method("PATCH")
                .uri(&format!("/api/v1/forum/reports/{}", report_id))
                .header(header::CONTENT_TYPE, "application/json")
                .header(header::AUTHORIZATION, &auth_header)
                .body(Body::from(
                    serde_json::to_vec(&json!({ "status": "resolved" })).unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
}
