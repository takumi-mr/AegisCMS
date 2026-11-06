//! 掲示板と会員アカウント: 会員の投稿、公開範囲（`visibility`）、他のサイトからのフォーム、退会

use std::sync::Arc;

use axum::body::Body;
use axum::http::{header, Request, StatusCode};
use serde_json::{json, Value};
use tower::ServiceExt;

use crate::app;
use crate::config::AegisConfig;
use crate::state::AppState;
use aegis_forum::{CategoryVisibility, CreateCategoryRequest, ForumPosting, ForumService};
use aegis_members::{
    MemberLoginRequest, MemberRegisterRequest, MemberService, MemberUpdateProfileRequest,
};
use aegis_runtime::PluginSupervisor;

struct Site {
    state: Arc<AppState>,
    router: axum::Router,
    members: Arc<MemberService>,
    public_cat: String,
    members_cat: String,
    staff_cat: String,
}

async fn site(posting: ForumPosting) -> Site {
    let mut config = AegisConfig::default();
    config.forum.enabled = true;
    config.forum.rate_limit_secs = 0;
    config.forum.posting = posting;
    config.members.enabled = true;
    config.audit.enabled = true;

    let state =
        crate::test_support::test_app_state(config.clone(), PluginSupervisor::new(), Vec::new())
            .await;

    let members = MemberService::from_config(
        &config.members,
        state.db.clone(),
        state.cache.clone(),
        state.audit.clone(),
    )
    .expect("members enabled");
    let forum =
        ForumService::new(config.forum.clone(), state.forum_deps(state.db.clone())).unwrap();

    let mut ids = Vec::new();
    for (slug, visibility) in [
        ("general", CategoryVisibility::Public),
        ("lounge", CategoryVisibility::Members),
        ("staff-room", CategoryVisibility::Staff),
    ] {
        let cat = forum
            .create_category(
                CreateCategoryRequest {
                    slug: slug.to_string(),
                    name: slug.to_string(),
                    description: String::new(),
                    position: 0,
                    visibility,
                },
                None,
            )
            .await
            .unwrap();
        ids.push(cat.id);
    }

    let state = Arc::new(state.with_forum(forum).with_members(members));
    let router = app::router(&config, state.clone());
    Site {
        members: state.members.clone().unwrap(),
        state,
        router,
        public_cat: ids[0].clone(),
        members_cat: ids[1].clone(),
        staff_cat: ids[2].clone(),
    }
}

impl Site {
    /// 会員を作ってログインし、トークンを返す。`nickname` を渡すと公開名も決める
    async fn member(&self, email: &str, nickname: Option<&str>) -> (String, String) {
        let registered = self
            .members
            .register(
                MemberRegisterRequest {
                    email: email.to_string(),
                    password: "password123".to_string(),
                    name: "Hanako Yamada".to_string(),
                    phone: None,
                    shipping_address: None,
                },
                None,
                None,
            )
            .await
            .unwrap();
        if let Some(nickname) = nickname {
            self.members
                .update_profile(
                    &registered.id,
                    MemberUpdateProfileRequest {
                        nickname: Some(nickname.to_string()),
                        ..Default::default()
                    },
                )
                .await
                .unwrap();
        }
        let (session, _) = self
            .members
            .login(
                MemberLoginRequest {
                    email: email.to_string(),
                    password: "password123".to_string(),
                },
                None,
                None,
            )
            .await
            .unwrap();
        (registered.id, session.token)
    }

    async fn send(&self, request: Request<Body>) -> (StatusCode, header::HeaderMap, String) {
        let res = self.router.clone().oneshot(request).await.unwrap();
        let status = res.status();
        let headers = res.headers().clone();
        let body = axum::body::to_bytes(res.into_body(), 1_000_000)
            .await
            .unwrap();
        (status, headers, String::from_utf8_lossy(&body).to_string())
    }

    async fn api(
        &self,
        method: &str,
        path: &str,
        bearer: Option<&str>,
        body: Option<Value>,
    ) -> (StatusCode, Value) {
        let mut builder = Request::builder().method(method).uri(path);
        if let Some(token) = bearer {
            builder = builder.header(header::AUTHORIZATION, format!("Bearer {token}"));
        }
        let request = match body {
            Some(body) => builder
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(body.to_string()))
                .unwrap(),
            None => builder.body(Body::empty()).unwrap(),
        };
        let (status, _, text) = self.send(request).await;
        (status, serde_json::from_str(&text).unwrap_or(Value::Null))
    }
}

fn thread_body(category_id: &str) -> Value {
    json!({
        "category_id": category_id,
        "title": "A thread by a member",
        "body": "Hello from the hiking club",
        "author_name": "ignored name",
        // クライアントは、投稿者の種類・ID を指定できない（本文から読まない）
        "author": { "kind": "staff", "id": "admin" }
    })
}

/// 会員の投稿: 名前は公開名（指定は無視）、投稿者の種類は `member`。公開名が無いと投稿できない
#[tokio::test]
async fn test_a_member_posts_under_the_public_name() {
    let site = site(ForumPosting::Anyone).await;
    let (member_id, token) = site.member("hana@example.com", Some("hana")).await;

    let (status, body) = site
        .api(
            "POST",
            "/api/v1/forum/threads",
            Some(&token),
            Some(thread_body(&site.public_cat)),
        )
        .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    assert_eq!(body["thread"]["author_name"], "hana");
    assert_eq!(body["thread"]["author_kind"], "member");
    assert_eq!(body["thread"]["author_id"], member_id.as_str());
    assert_eq!(body["initial_post"]["author_kind"], "member");

    // 公開名が無い会員は、本名を出さずに断る（公開名を決めるよう案内する）
    let (_, token_no_nick) = site.member("nonick@example.com", None).await;
    let (status, body) = site
        .api(
            "POST",
            "/api/v1/forum/threads",
            Some(&token_no_nick),
            Some(thread_body(&site.public_cat)),
        )
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["code"], "nickname_required");
}

/// 公開の API は、投稿者の IP・User-Agent を返さない
#[tokio::test]
async fn test_the_public_api_hides_ip_and_user_agent() {
    let site = site(ForumPosting::Anyone).await;
    let (_, token) = site.member("hana@example.com", Some("hana")).await;
    let (_, body) = site
        .api(
            "POST",
            "/api/v1/forum/threads",
            Some(&token),
            Some(thread_body(&site.public_cat)),
        )
        .await;
    let thread_id = body["thread"]["id"].as_str().unwrap().to_string();

    let (status, body) = site
        .api(
            "GET",
            &format!("/api/v1/forum/threads/{thread_id}/posts"),
            None,
            None,
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    let post = &body["posts"][0];
    assert!(post["ip"].is_null(), "{post}");
    assert!(post["user_agent"].is_null(), "{post}");
}

/// `posting = "members"`: 匿名は断る。会員は書ける
#[tokio::test]
async fn test_posting_limited_to_members() {
    let site = site(ForumPosting::Members).await;

    let (status, _) = site
        .api(
            "POST",
            "/api/v1/forum/threads",
            None,
            Some(thread_body(&site.public_cat)),
        )
        .await;
    assert_eq!(status, StatusCode::FORBIDDEN);

    let (_, token) = site.member("hana@example.com", Some("hana")).await;
    let (status, _) = site
        .api(
            "POST",
            "/api/v1/forum/threads",
            Some(&token),
            Some(thread_body(&site.public_cat)),
        )
        .await;
    assert_eq!(status, StatusCode::CREATED);
}

/// 公開範囲: 会員限定は匿名には「無い」、スタッフ限定は会員にも「無い」。書き込みも同じ
#[tokio::test]
async fn test_category_visibility_for_each_kind_of_viewer() {
    let site = site(ForumPosting::Anyone).await;
    let (_, token) = site.member("hana@example.com", Some("hana")).await;

    // 一覧: 匿名は公開だけ、会員は会員限定まで
    let slugs = |body: &Value| -> Vec<String> {
        body["categories"]
            .as_array()
            .unwrap()
            .iter()
            .map(|c| c["slug"].as_str().unwrap().to_string())
            .collect()
    };
    let (_, anon) = site
        .api("GET", "/api/v1/forum/categories", None, None)
        .await;
    assert_eq!(slugs(&anon), vec!["general"]);
    let (_, member) = site
        .api("GET", "/api/v1/forum/categories", Some(&token), None)
        .await;
    let mut member_slugs = slugs(&member);
    member_slugs.sort();
    assert_eq!(member_slugs, vec!["general", "lounge"]);

    // カテゴリを直接: 見られなければ 404（あることも教えない）
    for (slug, bearer, expected) in [
        ("lounge", None, StatusCode::NOT_FOUND),
        ("lounge", Some(token.as_str()), StatusCode::OK),
        ("staff-room", None, StatusCode::NOT_FOUND),
        ("staff-room", Some(token.as_str()), StatusCode::NOT_FOUND),
    ] {
        let (status, _) = site
            .api(
                "GET",
                &format!("/api/v1/forum/categories/{slug}"),
                bearer,
                None,
            )
            .await;
        assert_eq!(status, expected, "{slug} {}", bearer.is_some());
    }

    // 書き込み: 会員限定には匿名は書けない、会員は書ける。スタッフ限定には会員も書けない
    let (status, _) = site
        .api(
            "POST",
            "/api/v1/forum/threads",
            None,
            Some(thread_body(&site.members_cat)),
        )
        .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    let (status, created) = site
        .api(
            "POST",
            "/api/v1/forum/threads",
            Some(&token),
            Some(thread_body(&site.members_cat)),
        )
        .await;
    assert_eq!(status, StatusCode::CREATED);
    let (status, _) = site
        .api(
            "POST",
            "/api/v1/forum/threads",
            Some(&token),
            Some(thread_body(&site.staff_cat)),
        )
        .await;
    assert_eq!(status, StatusCode::FORBIDDEN);

    // 会員限定のスレッドとレスは、匿名には 404
    let thread_id = created["thread"]["id"].as_str().unwrap();
    for path in [
        format!("/api/v1/forum/threads/{thread_id}"),
        format!("/api/v1/forum/threads/{thread_id}/posts"),
    ] {
        let (status, _) = site.api("GET", &path, None, None).await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{path}");
        let (status, _) = site.api("GET", &path, Some(&token), None).await;
        assert_eq!(status, StatusCode::OK, "{path}");
    }
}

/// 会員限定のページ: 匿名には 404、会員には出て、共有のキャッシュに載せない。公開のページは今までどおり
#[tokio::test]
async fn test_member_only_pages_are_not_cached_and_not_shown_to_others() {
    let site = site(ForumPosting::Anyone).await;
    let (_, token) = site.member("hana@example.com", Some("hana")).await;

    let get = |path: &str, bearer: Option<&str>| {
        let mut builder = Request::builder().uri(path.to_string());
        if let Some(token) = bearer {
            builder = builder.header(header::AUTHORIZATION, format!("Bearer {token}"));
        }
        builder.body(Body::empty()).unwrap()
    };

    let (status, _, _) = site.send(get("/forum/lounge", None)).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (status, headers, _) = site.send(get("/forum/lounge", Some(&token))).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(headers[header::CACHE_CONTROL], "private, no-store");

    // トップ: 匿名には公開のカテゴリだけ。会員が見るトップは、会員限定が入るので共有のキャッシュに載せない
    let (_, headers, html) = site.send(get("/forum", None)).await;
    assert!(
        html.contains("general") && !html.contains("lounge"),
        "{html}"
    );
    assert_ne!(
        headers.get(header::CACHE_CONTROL).map(|v| v.as_bytes()),
        Some(b"private, no-store".as_slice())
    );
    let (_, headers, html) = site.send(get("/forum", Some(&token))).await;
    assert!(html.contains("lounge"), "{html}");
    assert!(!html.contains("staff-room"), "{html}");
    assert_eq!(headers[header::CACHE_CONTROL], "private, no-store");

    // 公開のカテゴリは、今までどおり
    let (status, headers, _) = site.send(get("/forum/general", Some(&token))).await;
    assert_eq!(status, StatusCode::OK);
    assert_ne!(
        headers.get(header::CACHE_CONTROL).map(|v| v.as_bytes()),
        Some(b"private, no-store".as_slice())
    );
}

/// Cookie で認証する HTML のフォーム: 他のサイトから送られたものは断り、このサイトからのものは通す
#[tokio::test]
async fn test_form_posts_with_the_member_cookie_must_come_from_this_site() {
    let site = site(ForumPosting::Anyone).await;
    let (_, token) = site.member("hana@example.com", Some("hana")).await;
    let cookie = format!(
        "{}={}",
        crate::handlers::members::member_cookie_name(&site.state),
        token
    );

    let form = |origin: Option<&str>, referer: Option<&str>| {
        let mut builder = Request::builder()
            .method("POST")
            .uri("/forum/general/threads")
            .header(header::HOST, "forum.example.com")
            .header(header::COOKIE, cookie.clone())
            .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded");
        if let Some(origin) = origin {
            builder = builder.header(header::ORIGIN, origin);
        }
        if let Some(referer) = referer {
            builder = builder.header(header::REFERER, referer);
        }
        builder
            .body(Body::from("title=From+a+form&body=Posted+with+a+cookie"))
            .unwrap()
    };

    // 他のサイト・Origin が null・どちらのヘッダーも無い: 断る
    for (origin, referer) in [
        (Some("https://evil.example.net"), None),
        (Some("null"), None),
        (None, Some("https://evil.example.net/page")),
        (None, None),
    ] {
        let (status, _, _) = site.send(form(origin, referer)).await;
        assert_eq!(status, StatusCode::FORBIDDEN, "{origin:?} {referer:?}");
    }
    // 何も作られていない
    let (_, body) = site
        .api("GET", "/api/v1/forum/categories/general", None, None)
        .await;
    assert!(body["category"].is_object());
    let threads = site
        .state
        .db
        .list_threads(&aegis_db::ThreadListQuery {
            category_id: Some(site.public_cat.clone()),
            status: None,
            before_last_post_at: None,
            before_id: None,
            limit: 10,
        })
        .await
        .unwrap();
    assert!(threads.is_empty());

    // このサイトから（Origin、または Referer）: 通る。投稿者は公開名の会員
    let (status, _, _) = site
        .send(form(Some("https://forum.example.com"), None))
        .await;
    assert_eq!(status, StatusCode::SEE_OTHER);
    let (status, _, _) = site
        .send(form(None, Some("https://forum.example.com/forum/general")))
        .await;
    assert_eq!(status, StatusCode::SEE_OTHER);
    let threads = site
        .state
        .db
        .list_threads(&aegis_db::ThreadListQuery {
            category_id: Some(site.public_cat.clone()),
            status: None,
            before_last_post_at: None,
            before_id: None,
            limit: 10,
        })
        .await
        .unwrap();
    assert_eq!(threads.len(), 2);
    assert!(threads
        .iter()
        .all(|t| t.author_name == "hana" && t.author_kind.as_deref() == Some("member")));
}

/// 退会: 投稿は残り、会員との紐づき・名前・IP が消える。ほかの会員の投稿はそのまま
#[tokio::test]
async fn test_deleting_the_account_anonymizes_the_forum_posts() {
    let site = site(ForumPosting::Anyone).await;
    let (_, token) = site.member("hana@example.com", Some("hana")).await;
    let (other_id, other_token) = site.member("taro@example.com", Some("taro")).await;

    let (_, mine) = site
        .api(
            "POST",
            "/api/v1/forum/threads",
            Some(&token),
            Some(thread_body(&site.public_cat)),
        )
        .await;
    let (_, theirs) = site
        .api(
            "POST",
            "/api/v1/forum/threads",
            Some(&other_token),
            Some(thread_body(&site.public_cat)),
        )
        .await;
    let my_thread = mine["thread"]["id"].as_str().unwrap().to_string();
    let their_thread = theirs["thread"]["id"].as_str().unwrap().to_string();

    let (status, _) = site
        .api(
            "DELETE",
            "/api/v1/members/me",
            Some(&token),
            Some(json!({ "password": "password123" })),
        )
        .await;
    assert_eq!(status, StatusCode::NO_CONTENT);

    let thread = site.state.db.get_thread(&my_thread).await.unwrap().unwrap();
    assert_eq!(thread.author_id, None);
    assert_eq!(thread.author_kind, None);
    assert_eq!(thread.author_name, "");
    assert_eq!(thread.title, "A thread by a member");
    let posts = site
        .state
        .db
        .list_posts(&aegis_db::PostListQuery {
            thread_id: my_thread.clone(),
            after_post_number: 0,
            status: None,
            limit: 10,
        })
        .await
        .unwrap();
    assert_eq!(posts.len(), 1);
    assert_eq!(posts[0].body, "Hello from the hiking club");
    assert_eq!(posts[0].author_id, None);
    assert_eq!(posts[0].author_name, "");
    assert_eq!(posts[0].ip, None);

    // 画面には、名前の代わりに「退会したユーザー」と出る（投稿は残る）
    let (status, _, html) = site
        .send(
            Request::builder()
                .uri(format!("/forum/general/{my_thread}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert!(html.contains("Former user"), "{html}");
    assert!(html.contains("Hello from the hiking club"));

    let other = site
        .state
        .db
        .get_thread(&their_thread)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(other.author_id.as_deref(), Some(other_id.as_str()));
    assert_eq!(other.author_name, "taro");
}

/// 管理画面の会員の詳細に、掲示板の投稿数が付く（同じ ID のスタッフの分は入らない）
#[tokio::test]
async fn test_the_member_detail_in_the_admin_counts_forum_posts() {
    let site = site(ForumPosting::Anyone).await;
    let (member_id, token) = site.member("hana@example.com", Some("hana")).await;
    let admin = site
        .state
        .sessions
        .create_session("admin", "admin", Default::default(), None)
        .await
        .unwrap();

    let (_, created) = site
        .api(
            "POST",
            "/api/v1/forum/threads",
            Some(&token),
            Some(thread_body(&site.public_cat)),
        )
        .await;
    let thread_id = created["thread"]["id"].as_str().unwrap().to_string();
    let (status, _) = site
        .api(
            "POST",
            "/api/v1/forum/posts",
            Some(&token),
            Some(json!({ "thread_id": thread_id, "body": "A second post by the member", "author_name": "" })),
        )
        .await;
    assert_eq!(status, StatusCode::CREATED);

    let (status, detail) = site
        .api(
            "GET",
            &format!("/api/v1/admin/members/{member_id}"),
            Some(&admin.id),
            None,
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    // スレッド 1 件、レス 2 件（スレッドの 1 つ目の投稿を含む）
    assert_eq!(detail["forum"], json!({ "threads": 1, "posts": 2 }));

    // 掲示板の API が返すスレッドとレスは、会員の ID を持つ（管理画面が会員の詳細へリンクする元）
    let (_, thread) = site
        .api(
            "GET",
            &format!("/api/v1/forum/threads/{thread_id}"),
            Some(&admin.id),
            None,
        )
        .await;
    assert_eq!(thread["thread"]["author_kind"], "member");
    assert_eq!(thread["thread"]["author_id"], member_id);
}

/// 会員のアバターは、表示のたびに今の値を引く: 設定すれば過去の投稿にも出て、変えれば追従し、外せば頭文字に戻る
#[tokio::test]
async fn test_forum_posts_show_the_current_member_avatar() {
    let site = site(ForumPosting::Anyone).await;
    let (member_id, token) = site.member("hana@example.com", Some("hana")).await;
    let (_, body) = site
        .api(
            "POST",
            "/api/v1/forum/threads",
            Some(&token),
            Some(thread_body(&site.public_cat)),
        )
        .await;
    let thread_id = body["thread"]["id"].as_str().unwrap().to_string();
    let page = format!("/forum/general/{thread_id}");
    let get = || {
        Request::builder()
            .uri(page.clone())
            .body(Body::empty())
            .unwrap()
    };

    let (status, _, html) = site.send(get()).await;
    let html = html.replace("&#x2f;", "/");
    assert_eq!(status, StatusCode::OK);
    assert!(html.contains("forum-avatar"), "{html}");
    assert!(!html.contains("/uploads/avatars/"), "{html}");

    site.members
        .set_avatar(
            &member_id,
            Some(format!("avatars/{member_id}-aaaaaaaa.png")),
        )
        .await
        .unwrap();
    let (_, _, html) = site.send(get()).await;
    let html = html.replace("&#x2f;", "/");
    assert!(
        html.contains(&format!("/uploads/avatars/{member_id}-aaaaaaaa.png")),
        "{html}"
    );

    site.members
        .set_avatar(
            &member_id,
            Some(format!("avatars/{member_id}-bbbbbbbb.png")),
        )
        .await
        .unwrap();
    let (_, _, html) = site.send(get()).await;
    let html = html.replace("&#x2f;", "/");
    assert!(
        html.contains("-bbbbbbbb.png") && !html.contains("-aaaaaaaa.png"),
        "{html}"
    );

    site.members.set_avatar(&member_id, None).await.unwrap();
    let (_, _, html) = site.send(get()).await;
    let html = html.replace("&#x2f;", "/");
    assert!(!html.contains("/uploads/avatars/"), "{html}");
}
