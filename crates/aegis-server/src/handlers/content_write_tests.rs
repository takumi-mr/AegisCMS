//! コンテンツの書き込み API の形（カスタムフィールドは `fields` の中）の結合テスト（実際のルーターを通す）

use std::sync::Arc;

use aegis_runtime::PluginSupervisor;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use serde_json::{json, Value};
use tower::ServiceExt;

use crate::config::AegisConfig;
use crate::state::AppState;

struct Site {
    state: Arc<AppState>,
    editor: String,
}

async fn site() -> Site {
    let state = Arc::new(
        crate::test_support::test_app_state(
            AegisConfig::default(),
            PluginSupervisor::new(),
            Vec::new(),
        )
        .await,
    );
    let editor = state
        .sessions
        .create_session("editor1", "editor", Default::default(), None)
        .await
        .unwrap()
        .id;
    Site { state, editor }
}

impl Site {
    async fn new_session(&self, user: &str, role: &str) -> String {
        self.state
            .sessions
            .create_session(user, role, Default::default(), None)
            .await
            .unwrap()
            .id
    }

    async fn send_as(
        &self,
        session_id: &str,
        method: &str,
        path: &str,
        body: Option<Value>,
    ) -> (StatusCode, Value) {
        let mut request = Request::builder()
            .method(method)
            .uri(path)
            .header("authorization", format!("Bearer {}", session_id));
        let body = match body {
            Some(value) => {
                request = request.header("content-type", "application/json");
                Body::from(value.to_string())
            }
            None => Body::empty(),
        };
        let response = crate::app::router(&self.state.config, self.state.clone())
            .oneshot(request.body(body).unwrap())
            .await
            .unwrap();
        let status = response.status();
        let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let value = serde_json::from_slice(&bytes)
            .unwrap_or_else(|_| Value::String(String::from_utf8_lossy(&bytes).to_string()));
        (status, value)
    }

    async fn send(&self, method: &str, path: &str, body: Option<Value>) -> (StatusCode, Value) {
        self.send_as(&self.editor, method, path, body).await
    }
}

#[tokio::test]
async fn test_custom_fields_are_written_and_read_under_fields() {
    let site = site().await;
    let (status, created) = site
        .send(
            "POST",
            "/api/v1/contents/page",
            Some(json!({
                "title": "Nested",
                "slug": "nested",
                "status": "draft",
                "blocks": [],
                "fields": {
                    "layout": "blank",
                    "custom_meta": { "seo": { "k": 1 } },
                    // スキーマに無いフィールドは保存しない
                    "not_in_the_schema": "dropped"
                }
            })),
        )
        .await;
    assert_eq!(status, StatusCode::CREATED, "{created}");
    assert_eq!(created["fields"]["layout"], "blank");
    assert_eq!(
        created["fields"]["custom_meta"],
        json!({ "seo": { "k": 1 } })
    );
    assert!(created["fields"].get("not_in_the_schema").is_none());
    // 書き込んだ形と読み取った形が同じ（応答にトップレベルのカスタムフィールドは無い）
    assert!(created.get("layout").is_none());

    let (status, read) = site
        .send(
            "GET",
            &format!("/api/v1/contents/page/{}", created["id"].as_str().unwrap()),
            None,
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(read["fields"], created["fields"]);

    // PUT も同じ形。送らなかったフィールドは空になる（全体の置き換え）
    let (status, updated) = site
        .send(
            "PUT",
            &format!("/api/v1/contents/page/{}", created["id"].as_str().unwrap()),
            Some(json!({
                "title": "Nested",
                "slug": "nested",
                "blocks": [],
                "fields": { "layout": "default" }
            })),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{updated}");
    assert_eq!(updated["fields"]["layout"], "default");
    assert!(updated["fields"].get("custom_meta").is_none());
}

#[tokio::test]
async fn test_the_old_flat_form_is_refused_instead_of_dropped() {
    let site = site().await;
    // 以前の形（カスタムフィールドをトップレベルに書く）は、黙って捨てずに拒否する
    let (status, body) = site
        .send(
            "POST",
            "/api/v1/contents/article",
            Some(json!({
                "title": "Flat",
                "slug": "flat",
                "status": "draft",
                "blocks": [],
                "layout": "blank"
            })),
        )
        .await;
    assert!(status.is_client_error(), "{status} {body}");
    assert!(
        body.to_string().contains("unknown field `layout`"),
        "the error names the field: {body}"
    );
    let (_, listed) = site
        .send("GET", "/api/v1/contents/article?status=draft", None)
        .await;
    assert!(
        !listed.to_string().contains("\"slug\":\"flat\""),
        "nothing was created: {listed}"
    );
}

/// `access` は、公開（`public`・空・`null`）か `[members].levels` のレベルだけ。設定にない値は、
/// だれも読めなくなるので、保存時に 400 で断る（作成も更新も）
#[tokio::test]
async fn test_access_must_be_public_or_a_configured_member_level() {
    let site = site().await;
    let create = |slug: &str, access: Value| {
        json!({
            "title": "T", "slug": slug, "status": "draft", "blocks": [],
            "fields": { "access": access }
        })
    };
    for (i, ok) in [
        json!("free"),
        json!("paid"),
        json!("public"),
        json!(""),
        Value::Null,
    ]
    .into_iter()
    .enumerate()
    {
        let (status, body) = site
            .send(
                "POST",
                "/api/v1/contents/article",
                Some(create(&format!("ok-{i}"), ok.clone())),
            )
            .await;
        assert_eq!(status, StatusCode::CREATED, "{ok}: {body}");
    }
    for (i, bad) in [json!("gold"), json!("Paid"), json!(5)]
        .into_iter()
        .enumerate()
    {
        let (status, body) = site
            .send(
                "POST",
                "/api/v1/contents/article",
                Some(create(&format!("bad-{i}"), bad.clone())),
            )
            .await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{bad}: {body}");
    }
    // 理由が分かる（選べるレベルを伝える）
    let (_, body) = site
        .send(
            "POST",
            "/api/v1/contents/article",
            Some(create("typo", json!("gold"))),
        )
        .await;
    assert_eq!(body["code"], "unknown_access_level");
    assert!(
        body["error"].as_str().unwrap().contains("free, paid"),
        "{body}"
    );

    // 更新も同じ
    let (status, _) = site
        .send(
            "PUT",
            "/api/v1/contents/article/ok-0",
            Some(create("ok-0", json!("gold"))),
        )
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    let (status, _) = site
        .send(
            "PUT",
            "/api/v1/contents/article/ok-0",
            Some(create("ok-0", json!("paid"))),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
}

/// `purchase_price`（単独購入の価格）は、`null` か 0 以上の整数で、Stripe の上限以下。範囲外は保存時に 400
#[tokio::test]
async fn test_purchase_price_must_be_a_non_negative_integer_within_the_limit() {
    let site = site().await;
    let create = |slug: &str, price: Value| {
        json!({
            "title": "T", "slug": slug, "status": "draft", "blocks": [],
            "fields": { "purchase_price": price }
        })
    };
    for (i, ok) in [json!(500), json!(0), json!(99_999_999), Value::Null]
        .into_iter()
        .enumerate()
    {
        let (status, body) = site
            .send(
                "POST",
                "/api/v1/contents/article",
                Some(create(&format!("price-ok-{i}"), ok.clone())),
            )
            .await;
        assert_eq!(status, StatusCode::CREATED, "{ok}: {body}");
    }
    for (i, bad) in [json!(-1), json!(100_000_000), json!(1.5), json!("500")]
        .into_iter()
        .enumerate()
    {
        let (status, body) = site
            .send(
                "POST",
                "/api/v1/contents/article",
                Some(create(&format!("price-bad-{i}"), bad.clone())),
            )
            .await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{bad}: {body}");
        assert_eq!(body["code"], "invalid_purchase_price");
    }
}

/// `reviewer` ロールの認可テスト:
/// - 新規作成（POST）は 403 Forbidden
/// - `pending` 記事の校正・承認（published）・差し戻し（draft）は許可
/// - `pending` 以外の記事更新は 403 Forbidden
/// - 削除（DELETE）は 403 Forbidden
#[tokio::test]
async fn test_reviewer_role_permissions() {
    let site = site().await;
    let contributor = site.new_session("contributor1", "contributor").await;
    let reviewer = site.new_session("reviewer1", "reviewer").await;

    // 1. reviewer による新規作成は 403 Forbidden
    let (status, body) = site
        .send_as(
            &reviewer,
            "POST",
            "/api/v1/contents/article",
            Some(json!({
                "title": "Reviewer Article",
                "slug": "reviewer-article",
                "status": "pending",
                "blocks": []
            })),
        )
        .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{body}");
    assert_eq!(body["error"], "Reviewers cannot create new content");

    // 2. contributor が下書きを作成し、pending でレビュー依頼
    let (status, created) = site
        .send_as(
            &contributor,
            "POST",
            "/api/v1/contents/article",
            Some(json!({
                "title": "Contributor Draft",
                "slug": "contributor-draft",
                "status": "pending",
                "blocks": []
            })),
        )
        .await;
    assert_eq!(status, StatusCode::CREATED, "{created}");
    let content_id = created["id"].as_str().unwrap();

    // 3. reviewer による校正（タイトル修正）と承認（published）
    let (status, updated) = site
        .send_as(
            &reviewer,
            "PUT",
            &format!("/api/v1/contents/article/{}", content_id),
            Some(json!({
                "title": "Contributor Article (Proofread)",
                "slug": "contributor-draft",
                "status": "published",
                "blocks": []
            })),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{updated}");
    assert_eq!(updated["title"], "Contributor Article (Proofread)");
    assert_eq!(updated["status"], "published");

    // 4. published になった記事を reviewer が直接更新しようとすると 403 Forbidden
    let (status, body) = site
        .send_as(
            &reviewer,
            "PUT",
            &format!("/api/v1/contents/article/{}", content_id),
            Some(json!({
                "title": "Tampered",
                "slug": "contributor-draft",
                "status": "published",
                "blocks": []
            })),
        )
        .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{body}");
    assert_eq!(
        body["error"],
        "Reviewers can only change content in 'pending' status"
    );

    // 5. reviewer による削除は 403 Forbidden
    let (status, body) = site
        .send_as(
            &reviewer,
            "DELETE",
            &format!("/api/v1/contents/article/{}", content_id),
            None,
        )
        .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{body}");
    assert_eq!(body["error"], "Reviewers cannot delete content");
}

fn page(title: &str, slug: &str) -> Value {
    json!({ "title": title, "slug": slug, "blocks": [] })
}

fn with(mut body: Value, key: &str, value: Value) -> Value {
    body[key] = value;
    body
}

#[tokio::test]
async fn test_an_unknown_status_is_refused() {
    let site = site().await;
    let (status, body) = site
        .send(
            "POST",
            "/api/v1/contents/page",
            Some(with(page("Live", "live"), "status", json!("live"))),
        )
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    assert!(body["error"].as_str().unwrap().contains("Invalid status"));

    // Nothing was stored
    let (status, _) = site.send("GET", "/api/v1/contents/page/live", None).await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    // The same on update, and the item is left as it was
    let (status, _) = site
        .send("POST", "/api/v1/contents/page", Some(page("Real", "real")))
        .await;
    assert_eq!(status, StatusCode::CREATED);
    let (status, body) = site
        .send(
            "PUT",
            "/api/v1/contents/page/real",
            Some(with(page("Real", "real"), "status", json!("live"))),
        )
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    let (_, read) = site.send("GET", "/api/v1/contents/page/real", None).await;
    assert_eq!(read["status"], "draft");

    // The five documented states are accepted
    for state in ["draft", "pending", "published", "archived"] {
        let (status, body) = site
            .send(
                "POST",
                "/api/v1/contents/page",
                Some(with(
                    page(state, &format!("is-{state}")),
                    "status",
                    json!(state),
                )),
            )
            .await;
        assert_eq!(status, StatusCode::CREATED, "{state}: {body}");
    }
}

#[tokio::test]
async fn test_a_duplicate_slug_is_a_conflict() {
    let site = site().await;
    let (status, _) = site
        .send("POST", "/api/v1/contents/page", Some(page("One", "same")))
        .await;
    assert_eq!(status, StatusCode::CREATED);

    let (status, body) = site
        .send("POST", "/api/v1/contents/page", Some(page("Two", "same")))
        .await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert!(body["error"].as_str().unwrap().contains("'same'"));

    // Renaming another item to a slug that is taken is a conflict too
    let (status, _) = site
        .send(
            "POST",
            "/api/v1/contents/page",
            Some(page("Other", "other")),
        )
        .await;
    assert_eq!(status, StatusCode::CREATED);
    let (status, body) = site
        .send(
            "PUT",
            "/api/v1/contents/page/other",
            Some(page("Other", "same")),
        )
        .await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");

    // Saving an item with its own slug is not a conflict, and a free slug works
    let (status, body) = site
        .send(
            "PUT",
            "/api/v1/contents/page/other",
            Some(page("Other again", "other")),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let (status, body) = site
        .send(
            "PUT",
            "/api/v1/contents/page/other",
            Some(page("Renamed", "renamed")),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
}

#[tokio::test]
async fn test_an_update_without_status_keeps_the_current_status() {
    let site = site().await;

    // Published stays published, and keeps its first publication time
    let (status, created) = site
        .send(
            "POST",
            "/api/v1/contents/page",
            Some(with(
                page("Live", "kept-live"),
                "status",
                json!("published"),
            )),
        )
        .await;
    assert_eq!(status, StatusCode::CREATED, "{created}");
    let (status, updated) = site
        .send(
            "PUT",
            "/api/v1/contents/page/kept-live",
            Some(page("Live, edited", "kept-live")),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{updated}");
    assert_eq!(updated["status"], "published");
    assert_eq!(updated["title"], "Live, edited");
    assert_eq!(updated["published_at"], created["published_at"]);

    // A draft stays a draft, an archived item stays archived
    for state in ["draft", "archived"] {
        let slug = format!("kept-{state}");
        site.send(
            "POST",
            "/api/v1/contents/page",
            Some(with(page(state, &slug), "status", json!(state))),
        )
        .await;
        let (status, updated) = site
            .send(
                "PUT",
                &format!("/api/v1/contents/page/{slug}"),
                Some(page("Edited", &slug)),
            )
            .await;
        assert_eq!(status, StatusCode::OK, "{updated}");
        assert_eq!(updated["status"], state);
    }

    // A scheduled item keeps the time it is scheduled for
    let (status, created) = site
        .send(
            "POST",
            "/api/v1/contents/page",
            Some(with(
                with(
                    page("Later", "kept-scheduled"),
                    "status",
                    json!("scheduled"),
                ),
                "published_at",
                json!("2999-01-01T00:00:00Z"),
            )),
        )
        .await;
    assert_eq!(status, StatusCode::CREATED, "{created}");
    let (status, updated) = site
        .send(
            "PUT",
            "/api/v1/contents/page/kept-scheduled",
            Some(page("Later, edited", "kept-scheduled")),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{updated}");
    assert_eq!(updated["status"], "scheduled");
    assert_eq!(updated["published_at"], created["published_at"]);

    // A new item without a status is still a draft
    let (status, created) = site
        .send("POST", "/api/v1/contents/page", Some(page("New", "fresh")))
        .await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(created["status"], "draft");
}

#[tokio::test]
async fn test_a_contributor_update_without_status_cannot_publish() {
    let site = site().await;
    let contributor = site.new_session("writer", "contributor").await;

    // Their own item waiting for review stays that way
    let (status, _) = site
        .send_as(
            &contributor,
            "POST",
            "/api/v1/contents/page",
            Some(with(page("Mine", "mine"), "status", json!("pending"))),
        )
        .await;
    assert_eq!(status, StatusCode::CREATED);
    let (status, updated) = site
        .send_as(
            &contributor,
            "PUT",
            "/api/v1/contents/page/mine",
            Some(page("Mine, edited", "mine")),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{updated}");
    assert_eq!(updated["status"], "pending");

    // Leaving the status out does not let them touch published content
    site.send(
        "POST",
        "/api/v1/contents/page",
        Some(with(page("Live", "editors"), "status", json!("published"))),
    )
    .await;
    let (status, body) = site
        .send_as(
            &contributor,
            "PUT",
            "/api/v1/contents/page/editors",
            Some(page("Hijacked", "editors")),
        )
        .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{body}");
}
