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

const SECRET: &str = "Only paying members read this sentence.";

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
    configure(&mut config);
    let state = test_app_state(
        config.clone(),
        PluginSupervisor::new(),
        vec![schema("article"), schema("page")],
    )
    .await;
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
        let schema = self
            .state
            .schemas
            .read()
            .await
            .get(content_type)
            .cloned()
            .unwrap();
        let mut custom_fields = HashMap::new();
        custom_fields.insert("excerpt".to_string(), json!("A public teaser."));
        if let Some(access) = access {
            custom_fields.insert("access".to_string(), json!(access));
        }
        let record = ContentRecord {
            id: format!("{content_type}-{slug}"),
            content_type: content_type.to_string(),
            title: format!("Title of {slug}"),
            slug: slug.to_string(),
            status: "published".to_string(),
            blocks_json: serde_json::to_string(&[BlockNode::paragraph(SECRET)]).unwrap(),
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
