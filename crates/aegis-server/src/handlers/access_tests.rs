//! 閲覧の制限（会員レベル）の結合テスト（ルーター経由）。docs/members.md 15

use std::collections::HashMap;
use std::sync::Arc;

use aegis_ast::BlockNode;
use aegis_db::ContentRecord;
use aegis_runtime::PluginSupervisor;
use aegis_schema::SchemaDefinition;
use axum::body::Body;
use axum::http::{HeaderMap, Request, StatusCode};
use serde_json::json;
use tower::ServiceExt;

use crate::app::router;
use crate::config::AegisConfig;
use crate::state::AppState;
use crate::test_support::test_app_state;

const SECRET: &str = "Only paying members read this sentence about zxqunique.";

fn schema(content_type: &str) -> SchemaDefinition {
    serde_json::from_value(json!({
        "content_type": content_type,
        "label": content_type,
        "fields": [
            { "name": "excerpt", "type": "string", "required": false, "nullable": true },
            { "name": "access", "type": "string", "required": false, "nullable": true },
            { "name": "layout", "type": "string", "required": false, "nullable": true }
        ]
    }))
    .unwrap()
}

struct Site {
    app: axum::Router,
    state: Arc<AppState>,
}

struct Reply {
    status: StatusCode,
    headers: HeaderMap,
    body: String,
}

impl Reply {
    fn cache_control(&self) -> &str {
        self.headers
            .get("cache-control")
            .and_then(|v| v.to_str().ok())
            .unwrap_or_default()
    }
}

async fn site_with(configure: impl FnOnce(&mut AegisConfig)) -> Site {
    let mut config = AegisConfig::default();
    config.members.enabled = true;
    config.members.levels = vec!["free".into(), "paid".into()];
    // 本文が出る面を全部有効にして、制限した本文がどこにも出ないことを確かめる
    config.search.enabled = true;
    config.search.index_dir = String::new();
    config.headless.enabled = true;
    config.comments.enabled = true;
    configure(&mut config);
    let mut state = test_app_state(
        config.clone(),
        PluginSupervisor::new(),
        vec![schema("article"), schema("page")],
    )
    .await;
    state.search = Some(Arc::new(
        crate::search::SiteSearch::from_config(&state.config.search).unwrap(),
    ));
    let comments =
        aegis_comments::CommentService::new(state.config.comments.clone(), state.comment_deps())
            .unwrap();
    let state = state.with_comments(comments);
    let members = aegis_members::MemberService::from_config(
        &config.members,
        state.db.clone(),
        state.cache.clone(),
        state.audit.clone(),
    );
    let state = Arc::new(state.with_members(members.unwrap()));
    Site {
        app: router(&config, state.clone()),
        state,
    }
}

impl Site {
    async fn get(&self, path: &str, bearer: Option<&str>) -> Reply {
        let mut request = Request::get(path);
        if let Some(token) = bearer {
            request = request.header("authorization", format!("Bearer {token}"));
        }
        let response = self
            .app
            .clone()
            .oneshot(request.body(Body::empty()).unwrap())
            .await
            .unwrap();
        let status = response.status();
        let headers = response.headers().clone();
        let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        Reply {
            status,
            headers,
            body: String::from_utf8_lossy(&bytes).to_string(),
        }
    }

    async fn publish(&self, content_type: &str, slug: &str, access: Option<&str>) {
        self.publish_blocks(
            content_type,
            slug,
            access,
            Some("A public teaser."),
            vec![BlockNode::paragraph(SECRET)],
        )
        .await;
    }

    async fn publish_blocks(
        &self,
        content_type: &str,
        slug: &str,
        access: Option<&str>,
        excerpt: Option<&str>,
        blocks: Vec<BlockNode>,
    ) {
        let schema = self
            .state
            .schemas
            .read()
            .await
            .get(content_type)
            .cloned()
            .unwrap();
        let mut custom_fields = HashMap::new();
        if let Some(excerpt) = excerpt {
            custom_fields.insert("excerpt".to_string(), json!(excerpt));
        }
        if let Some(access) = access {
            custom_fields.insert("access".to_string(), json!(access));
        }
        let record = ContentRecord {
            id: format!("{content_type}-{slug}"),
            content_type: content_type.to_string(),
            title: format!("Title of {slug}"),
            slug: slug.to_string(),
            status: "published".to_string(),
            blocks_json: serde_json::to_string(&blocks).unwrap(),
            author_id: None,
            published_at: Some("2026-10-22T00:00:00Z".to_string()),
            unpublish_at: None,
            created_at: "2026-10-22T00:00:00Z".to_string(),
            updated_at: "2026-10-22T00:00:00Z".to_string(),
            custom_fields,
        };
        self.state
            .db
            .insert_content(&schema, &record)
            .await
            .unwrap();
    }

    /// 会員を作ってログインし、トークンを返す。`level` が `free` 以外なら付与する
    async fn member(&self, email: &str, level: &str) -> String {
        let service = self.state.members.as_ref().unwrap();
        let member = service
            .register(
                aegis_members::MemberRegisterRequest {
                    email: email.to_string(),
                    password: "password123".to_string(),
                    name: "Member".to_string(),
                    phone: None,
                    shipping_address: None,
                },
                None,
                None,
            )
            .await
            .unwrap();
        if level != "free" {
            service
                .admin_set_level(&member.id, level, "admin")
                .await
                .unwrap();
        }
        service
            .login(
                aegis_members::MemberLoginRequest {
                    email: email.to_string(),
                    password: "password123".to_string(),
                },
                None,
                None,
            )
            .await
            .unwrap()
            .0
            .token
    }
}

fn assert_notice_only(reply: &Reply) {
    assert_eq!(reply.status, StatusCode::OK);
    assert!(
        !reply.body.contains(SECRET),
        "the restricted body must not be in the HTML"
    );
    assert!(
        reply.body.contains(r#"<div class="cms-access-notice""#),
        "{}",
        reply.body
    );
    assert_eq!(reply.cache_control(), "private, no-store");
    assert!(!reply.headers.contains_key("cache-tag"));
}

#[tokio::test]
async fn test_public_content_is_unchanged_and_cacheable() {
    let site = site_with(|_| {}).await;
    site.publish("article", "open", None).await;
    site.publish("article", "explicit", Some("public")).await;
    for slug in ["open", "explicit"] {
        let reply = site.get(&format!("/articles/{slug}"), None).await;
        assert_eq!(reply.status, StatusCode::OK);
        assert!(reply.body.contains(SECRET));
        assert!(!reply.body.contains(r#"<div class="cms-access-notice""#));
        assert_ne!(reply.cache_control(), "private, no-store");
    }
}

#[tokio::test]
async fn test_guest_sees_a_notice_with_login_links_instead_of_the_body() {
    let site = site_with(|_| {}).await;
    site.publish("article", "secret", Some("paid")).await;
    let reply = site.get("/articles/secret", None).await;
    assert_notice_only(&reply);
    // タイトルと、書き手が公開してよい抜粋は出る
    assert!(reply.body.contains("Title of secret"));
    // ログインのあとに、この記事へ戻る
    assert!(reply
        .body
        .contains("/account/login?next=%2Farticles%2Fsecret"));
    assert!(reply
        .body
        .contains("/account/register?next=%2Farticles%2Fsecret"));
    // 本文のない案内にコメント欄などを出さない
    assert!(!reply.body.contains("aegis-comments"));
}

#[tokio::test]
async fn test_member_level_decides_who_reads_it() {
    let site = site_with(|c| c.members.upgrade_url = "/plans".to_string()).await;
    site.publish("article", "secret", Some("paid")).await;
    site.publish("article", "members-only", Some("free")).await;

    let free = site.member("free@example.com", "free").await;
    let paid = site.member("paid@example.com", "paid").await;

    // free の会員は、paid の記事は読めず、アップグレードの案内が出る
    let reply = site.get("/articles/secret", Some(&free)).await;
    assert_notice_only(&reply);
    assert!(reply.body.contains("paid"));
    assert!(reply.body.contains(r#"href="/plans""#));
    assert!(!reply.body.contains("/account/login"));
    // free の会員は、会員だけの記事（free）は読める。応答は会員ごとに変わるのでキャッシュさせない
    let reply = site.get("/articles/members-only", Some(&free)).await;
    assert!(reply.body.contains(SECRET));
    assert_eq!(reply.cache_control(), "private, no-store");
    // paid の会員は読める
    let reply = site.get("/articles/secret", Some(&paid)).await;
    assert_eq!(reply.status, StatusCode::OK);
    assert!(reply.body.contains(SECRET));
    assert_eq!(reply.cache_control(), "private, no-store");
    // 会員だけの記事を、ゲストは読めない
    assert_notice_only(&site.get("/articles/members-only", None).await);
}

#[tokio::test]
async fn test_a_bad_token_or_a_removed_level_is_a_guest() {
    let site = site_with(|_| {}).await;
    site.publish("article", "secret", Some("paid")).await;
    assert_notice_only(&site.get("/articles/secret", Some("mses_nope")).await);
    // 設定に無いレベルを求める記事は、だれにも見せない
    site.publish("article", "typo", Some("gold")).await;
    let paid = site.member("paid@example.com", "paid").await;
    assert_notice_only(&site.get("/articles/typo", Some(&paid)).await);
}

#[tokio::test]
async fn test_pages_and_the_slug_route_are_gated_too() {
    let site = site_with(|_| {}).await;
    site.publish("page", "club", Some("paid")).await;
    site.publish("article", "secret", Some("paid")).await;
    // 固定ページ（/{slug}）
    assert_notice_only(&site.get("/club", None).await);
    // article への /{slug} のフォールバック
    assert_notice_only(&site.get("/secret", None).await);
    let paid = site.member("paid@example.com", "paid").await;
    assert!(site.get("/club", Some(&paid)).await.body.contains(SECRET));
    assert!(site.get("/secret", Some(&paid)).await.body.contains(SECRET));
}

#[tokio::test]
async fn test_a_suspended_member_loses_access() {
    let site = site_with(|_| {}).await;
    site.publish("article", "secret", Some("paid")).await;
    let paid = site.member("paid@example.com", "paid").await;
    assert!(site
        .get("/articles/secret", Some(&paid))
        .await
        .body
        .contains(SECRET));
    let service = site.state.members.as_ref().unwrap();
    let member = site
        .state
        .db
        .get_member_by_email("paid@example.com")
        .await
        .unwrap()
        .unwrap();
    service
        .admin_set_status(&member.id, "suspended")
        .await
        .unwrap();
    assert_notice_only(&site.get("/articles/secret", Some(&paid)).await);
}

#[tokio::test]
async fn test_without_member_accounts_restricted_content_stays_hidden() {
    let mut config = AegisConfig::default();
    config.members.enabled = false;
    let state = Arc::new(
        test_app_state(
            config.clone(),
            PluginSupervisor::new(),
            vec![schema("article"), schema("page")],
        )
        .await,
    );
    let site = Site {
        app: router(&config, state.clone()),
        state,
    };
    site.publish("article", "secret", Some("paid")).await;
    let reply = site.get("/articles/secret", None).await;
    assert_notice_only(&reply);
    // 会員アカウントが無いので、ログインのリンクは出さない
    assert!(!reply.body.contains("/account/login"));
}

/// 制限した本文（`SECRET`）が、本文を出す面のどこにも出ない（案内・要約・フィード・検索・API）
#[tokio::test]
async fn test_restricted_body_does_not_leak_through_any_surface() {
    let site = site_with(|_| {}).await;
    site.publish("article", "secret", Some("paid")).await;
    site.publish("article", "open", None).await;
    crate::search::rebuild(&site.state).await;

    // 一覧・フィード・タグ・関連記事: 要約は書き手の `excerpt`。本文の先頭は出ない
    for path in ["/", "/articles", "/feed.xml"] {
        let reply = site.get(path, None).await;
        assert_eq!(reply.status, StatusCode::OK, "{path}");
        assert!(
            reply.body.contains("Title of secret"),
            "{path}: the title is listed"
        );
        assert!(
            reply.body.contains("A public teaser."),
            "{path}: the written excerpt is shown"
        );
        // 制限していない記事の本文は、従来どおり出る（フィードは全文）
        if path == "/feed.xml" {
            assert!(reply.body.contains("Title of open"));
        }
    }
    let feed = site.get("/feed.xml", None).await;
    // フィードに、制限した記事の本文は 1 件分（open）だけ。secret の本文は出ない
    assert_eq!(
        feed.body.matches("zxqunique").count(),
        1,
        "only the open article's body is in the feed"
    );
    let index = site.get("/articles", None).await;
    assert_eq!(
        index.body.matches("zxqunique").count(),
        1,
        "only the open article's excerpt is listed"
    );

    // 検索: 隠した本文の語では当たらない。タイトルは当たる
    let search = site.get("/api/v1/search?q=zxqunique", None).await;
    assert_eq!(search.status, StatusCode::OK);
    let found: serde_json::Value = serde_json::from_str(&search.body).unwrap();
    let slugs: Vec<&str> = found["items"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|r| r["slug"].as_str())
        .collect();
    assert_eq!(slugs, vec!["open"], "{found}");
    let by_title = site.get("/api/v1/search?q=secret", None).await;
    assert!(by_title.body.contains("Title of secret"));

    // ヘッドレス API: `blocks` も `html` も出ない。`access` は出る（鍵の印用）
    let item = site
        .get("/api/v1/public/contents/article/secret?include=html", None)
        .await;
    assert_eq!(item.status, StatusCode::OK, "{}", item.body);
    assert!(!item.body.contains("zxqunique"), "{}", item.body);
    let item: serde_json::Value = serde_json::from_str(&item.body).unwrap();
    assert_eq!(item["blocks"], json!([]));
    assert_eq!(item["fields"]["access"], "paid");
    assert_eq!(item["excerpt"], "A public teaser.");
    let open = site.get("/api/v1/public/contents/article/open", None).await;
    assert!(open.body.contains("zxqunique"));
    let list = site.get("/api/v1/public/contents/article", None).await;
    assert!(!list.body.contains("Only paying members"));
}

#[tokio::test]
async fn test_a_restricted_page_marks_itself_as_not_free_for_search_engines() {
    let site = site_with(|_| {}).await;
    site.publish("article", "secret", Some("paid")).await;
    site.publish("article", "open", None).await;
    let denied = site.get("/articles/secret", None).await;
    assert!(denied.body.contains(r#""isAccessibleForFree":false"#));
    assert!(denied.body.contains(r#""@type":"Article""#));
    // 読める会員のページにも印は付く（同じ記事。クローラーには会員向けと伝わる）
    let paid = site.member("paid@example.com", "paid").await;
    assert!(site
        .get("/articles/secret", Some(&paid))
        .await
        .body
        .contains("isAccessibleForFree"));
    assert!(!site
        .get("/articles/open", None)
        .await
        .body
        .contains("isAccessibleForFree"));
}

#[tokio::test]
async fn test_comments_of_a_restricted_article_need_the_same_access() {
    let site = site_with(|_| {}).await;
    site.publish("article", "secret", Some("paid")).await;
    site.publish("article", "open", None).await;
    let guest = site
        .get("/api/v1/contents/article/secret/comments", None)
        .await;
    assert_eq!(guest.status, StatusCode::FORBIDDEN, "{}", guest.body);
    assert!(guest.body.contains("access_restricted"));
    let paid = site.member("paid@example.com", "paid").await;
    let allowed = site
        .get("/api/v1/contents/article/secret/comments", Some(&paid))
        .await;
    assert_eq!(allowed.status, StatusCode::OK, "{}", allowed.body);
    assert_eq!(
        site.get("/api/v1/contents/article/open/comments", None)
            .await
            .status,
        StatusCode::OK
    );
}

/// 「ここから先」の区切り: 区切りより前はだれにでも見え、後ろは読める会員だけ。どの面でも同じ
#[tokio::test]
async fn test_the_paywall_shows_the_text_before_it_and_hides_the_rest_everywhere() {
    let site = site_with(|_| {}).await;
    site.publish_blocks(
        "article",
        "teased",
        Some("paid"),
        None,
        vec![
            BlockNode::paragraph("Teaser zxqteaser is free to read."),
            BlockNode::new("core/paywall"),
            BlockNode::paragraph("Tail zxqtail is for paying members."),
        ],
    )
    .await;
    crate::search::rebuild(&site.state).await;

    // ゲスト: 区切りの前と案内（その後ろに続く）。後ろの本文は HTML にない
    let guest = site.get("/articles/teased", None).await;
    assert_eq!(guest.status, StatusCode::OK);
    assert!(guest.body.contains("zxqteaser"));
    assert!(!guest.body.contains("zxqtail"));
    let teaser_at = guest.body.find("zxqteaser").unwrap();
    let notice_at = guest
        .body
        .find(r#"<div class="cms-access-notice""#)
        .unwrap();
    assert!(teaser_at < notice_at, "the notice follows the teaser");
    assert_eq!(guest.cache_control(), "private, no-store");

    // 足りないレベルの会員も同じ
    let free = site.member("free@example.com", "free").await;
    let denied = site.get("/articles/teased", Some(&free)).await;
    assert!(denied.body.contains("zxqteaser") && !denied.body.contains("zxqtail"));

    // 読める会員: 全文。区切りは何も出さない
    let paid = site.member("paid@example.com", "paid").await;
    let full = site.get("/articles/teased", Some(&paid)).await;
    assert!(full.body.contains("zxqteaser") && full.body.contains("zxqtail"));
    assert!(!full.body.contains("paywall"));
    assert!(!full.body.contains(r#"<div class="cms-access-notice""#));

    // 一覧・フィードの要約は、区切りの前から作る（`excerpt` を書かなくてよい）
    let index = site.get("/articles", None).await;
    assert!(index.body.contains("zxqteaser"));
    assert!(!index.body.contains("zxqtail"));
    let feed = site.get("/feed.xml", None).await;
    assert!(feed.body.contains("zxqteaser"));
    assert!(!feed.body.contains("zxqtail"));

    // 検索: 区切りの前の語では当たり、後ろの語では当たらない
    let hits = |body: &str| {
        let found: serde_json::Value = serde_json::from_str(body).unwrap();
        found["total"].as_u64().unwrap()
    };
    assert_eq!(
        hits(&site.get("/api/v1/search?q=zxqteaser", None).await.body),
        1
    );
    assert_eq!(
        hits(&site.get("/api/v1/search?q=zxqtail", None).await.body),
        0
    );

    // ヘッドレス API: 区切りの前のブロックだけ
    let item = site
        .get("/api/v1/public/contents/article/teased?include=html", None)
        .await;
    assert!(item.body.contains("zxqteaser") && !item.body.contains("zxqtail"));
    let item: serde_json::Value = serde_json::from_str(&item.body).unwrap();
    assert_eq!(item["blocks"].as_array().unwrap().len(), 1);
}

/// 区切りだけでは制限にならない（`access` が無ければ、区切りは何も出さず全文が見える）
#[tokio::test]
async fn test_a_paywall_without_access_changes_nothing() {
    let site = site_with(|_| {}).await;
    site.publish_blocks(
        "article",
        "open",
        None,
        None,
        vec![
            BlockNode::paragraph("Head zxqhead."),
            BlockNode::new("core/paywall"),
            BlockNode::paragraph("Tail zxqtail."),
        ],
    )
    .await;
    let reply = site.get("/articles/open", None).await;
    assert!(reply.body.contains("zxqhead") && reply.body.contains("zxqtail"));
    assert!(!reply.body.contains(r#"<div class="cms-access-notice""#));
    assert_ne!(reply.cache_control(), "private, no-store");
}
