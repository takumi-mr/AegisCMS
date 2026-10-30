//! タグの API の結合テスト（実際のルーターを通す）

use std::sync::Arc;

use aegis_runtime::PluginSupervisor;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use serde_json::{json, Value};
use tower::ServiceExt;

use crate::config::AegisConfig;
use crate::state::AppState;
use aegis_db::ContentRecord;
use aegis_tags::TagService;

struct Site {
    state: Arc<AppState>,
    editor: String,
    viewer: String,
    admin: String,
}

async fn site(configure: impl FnOnce(&mut AegisConfig)) -> Site {
    let mut config = AegisConfig::default();
    config.audit.enabled = true;
    configure(&mut config);
    let state =
        crate::test_support::test_app_state(config, PluginSupervisor::new(), Vec::new()).await;
    let state = if state.config.tags.enabled {
        let tags = TagService::new(state.db.clone(), state.config.tags.clone());
        state.with_tags(tags)
    } else {
        state
    };
    let state = Arc::new(state);

    for (content_type, id, slug) in [("article", "a1", "hello"), ("page", "p1", "about")] {
        let schema = state.schemas.read().await[content_type].clone();
        state
            .db
            .insert_content(
                &schema,
                &ContentRecord {
                    id: id.to_string(),
                    content_type: content_type.to_string(),
                    title: slug.to_uppercase(),
                    slug: slug.to_string(),
                    status: "published".to_string(),
                    blocks_json: "[]".to_string(),
                    author_id: None,
                    published_at: Some("2026-10-01T00:00:00Z".to_string()),
                    unpublish_at: None,
                    created_at: "2026-10-01T00:00:00Z".to_string(),
                    updated_at: "2026-10-01T00:00:00Z".to_string(),
                    custom_fields: Default::default(),
                },
            )
            .await
            .unwrap();
    }
    let session = |user: &'static str, role: &'static str| {
        let state = state.clone();
        async move {
            state
                .sessions
                .create_session(user, role, Default::default(), None)
                .await
                .unwrap()
                .id
        }
    };
    let editor = session("editor1", "editor").await;
    let viewer = session("viewer1", "viewer").await;
    let admin = session("admin", "admin").await;
    Site {
        state,
        editor,
        viewer,
        admin,
    }
}

impl Site {
    async fn call(
        &self,
        method: &str,
        path: &str,
        body: Option<Value>,
        token: Option<&str>,
    ) -> (StatusCode, Value) {
        let mut request = Request::builder().method(method).uri(path);
        if let Some(token) = token {
            request = request.header("authorization", format!("Bearer {}", token));
        }
        let request = match body {
            Some(body) => request
                .header("content-type", "application/json")
                .body(Body::from(body.to_string())),
            None => request.body(Body::empty()),
        }
        .unwrap();
        let response = crate::app::router(&self.state.config, self.state.clone())
            .oneshot(request)
            .await
            .unwrap();
        let status = response.status();
        let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        (
            status,
            serde_json::from_slice(&bytes).unwrap_or(Value::Null),
        )
    }

    async fn editor(&self, method: &str, path: &str, body: Option<Value>) -> (StatusCode, Value) {
        self.call(method, path, body, Some(&self.editor.clone()))
            .await
    }

    async fn create(&self, name: &str) -> Value {
        let (status, tag) = self
            .editor("POST", "/api/v1/tags", Some(json!({ "name": name })))
            .await;
        assert_eq!(status, StatusCode::CREATED, "{}", tag);
        tag
    }
}

#[tokio::test]
async fn test_manage_tags() {
    let site = site(|_| {}).await;

    let rust = site.create("Rust Lang").await;
    assert_eq!(rust["slug"], "rust-lang");
    assert_eq!(rust["usage"], 0);
    let (status, go) = site
        .editor(
            "POST",
            "/api/v1/tags",
            Some(json!({ "name": "Go", "slug": "golang", "description": "The Go language" })),
        )
        .await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(go["slug"], "golang");

    // 重複は 409、入力の不備は 400
    for (body, expected) in [
        (json!({ "name": "rust lang" }), StatusCode::CONFLICT),
        (
            json!({ "name": "Other", "slug": "golang" }),
            StatusCode::CONFLICT,
        ),
        (json!({ "name": "" }), StatusCode::BAD_REQUEST),
        (
            json!({ "name": "X", "slug": "Bad Slug" }),
            StatusCode::BAD_REQUEST,
        ),
    ] {
        let (status, error) = site.editor("POST", "/api/v1/tags", Some(body)).await;
        assert_eq!(status, expected, "{}", error);
        assert!(error["error"].is_string());
    }

    // 一覧（viewer でも読める）: 名前の昇順、検索、ページング
    let (status, list) = site
        .call("GET", "/api/v1/tags", None, Some(&site.viewer))
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(list["total"], 2);
    assert_eq!(list["tags"][0]["name"], "Go");
    assert_eq!(list["tags"][1]["name"], "Rust Lang");
    let (_, found) = site
        .call("GET", "/api/v1/tags?q=RUST", None, Some(&site.viewer))
        .await;
    assert_eq!(found["total"], 1);
    let (_, page) = site
        .call(
            "GET",
            "/api/v1/tags?limit=1&offset=1",
            None,
            Some(&site.viewer),
        )
        .await;
    assert_eq!(page["tags"][0]["name"], "Rust Lang");

    // 更新: slug を省略したら今のまま
    let id = rust["id"].as_str().unwrap();
    let (status, updated) = site
        .editor(
            "PUT",
            &format!("/api/v1/tags/{}", id),
            Some(json!({ "name": "Rust" })),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{}", updated);
    assert_eq!(updated["name"], "Rust");
    assert_eq!(updated["slug"], "rust-lang");
    let (status, _) = site
        .editor(
            "PUT",
            "/api/v1/tags/missing",
            Some(json!({ "name": "Nope" })),
        )
        .await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    // 認可: 作成は editor 以上、一覧はログインが必要
    let (status, _) = site
        .call(
            "POST",
            "/api/v1/tags",
            Some(json!({ "name": "Denied" })),
            Some(&site.viewer),
        )
        .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    let (status, _) = site.call("GET", "/api/v1/tags", None, None).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);

    // 削除
    let (status, _) = site
        .editor("DELETE", &format!("/api/v1/tags/{}", id), None)
        .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let (status, _) = site
        .editor("DELETE", &format!("/api/v1/tags/{}", id), None)
        .await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    // 監査ログに残る
    let (_, logs) = site
        .call(
            "GET",
            "/api/v1/audit-logs?action=tag.create",
            None,
            Some(&site.admin),
        )
        .await;
    // 作成 2 件と、重複・不備で断られた 3 件（Failure）
    assert!(
        logs["items"].as_array().map_or(0, Vec::len) >= 2,
        "{}",
        logs
    );
}

#[tokio::test]
async fn test_tag_any_content_type_and_clean_up_on_delete() {
    let site = site(|_| {}).await;
    let a = site.create("Alpha").await;
    let b = site.create("Beta").await;
    let ids = |tags: &[&Value]| -> Vec<String> {
        tags.iter()
            .map(|t| t["id"].as_str().unwrap().to_string())
            .collect()
    };

    // 記事（スラッグ指定）にも、固定ページ（ID 指定）にも付けられる
    let (status, set) = site
        .editor(
            "PUT",
            "/api/v1/contents/article/hello/tags",
            Some(json!({ "tag_ids": ids(&[&b, &a, &b]) })),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{}", set);
    assert_eq!(set["content_id"], "a1");
    assert_eq!(set["tags"][0]["name"], "Alpha");
    assert_eq!(set["tags"].as_array().unwrap().len(), 2);
    let (status, _) = site
        .editor(
            "PUT",
            "/api/v1/contents/page/p1/tags",
            Some(json!({ "tag_ids": ids(&[&a]) })),
        )
        .await;
    assert_eq!(status, StatusCode::OK);

    let (_, got) = site
        .call(
            "GET",
            "/api/v1/contents/article/a1/tags",
            None,
            Some(&site.viewer),
        )
        .await;
    assert_eq!(got["tags"].as_array().unwrap().len(), 2);

    // 付いている数
    let (_, list) = site.editor("GET", "/api/v1/tags", None).await;
    assert_eq!(list["tags"][0]["usage"], 2);
    assert_eq!(list["tags"][1]["usage"], 1);

    // 存在しないタグ・コンテンツ・種類
    for (path, body, expected) in [
        (
            "/api/v1/contents/article/hello/tags",
            json!({ "tag_ids": ["missing"] }),
            StatusCode::BAD_REQUEST,
        ),
        (
            "/api/v1/contents/article/nothing/tags",
            json!({ "tag_ids": [] }),
            StatusCode::NOT_FOUND,
        ),
        (
            "/api/v1/contents/unknown/hello/tags",
            json!({ "tag_ids": [] }),
            StatusCode::NOT_FOUND,
        ),
    ] {
        let (status, error) = site.editor("PUT", path, Some(body)).await;
        assert_eq!(status, expected, "{} {}", path, error);
    }

    // コンテンツを消すと、結びつきも消える
    let (status, _) = site
        .editor("DELETE", "/api/v1/contents/article/hello", None)
        .await;
    assert!(status.is_success(), "{}", status);
    assert!(site
        .state
        .db
        .list_tagged_contents(a["id"].as_str().unwrap(), Some("article"))
        .await
        .unwrap()
        .is_empty());
    let (_, list) = site.editor("GET", "/api/v1/tags", None).await;
    assert_eq!(list["tags"][0]["usage"], 1);
}

#[tokio::test]
async fn test_limit_per_content_and_disabled() {
    let limited = site(|config| config.tags.max_per_content = 1).await;
    let a = limited.create("Alpha").await;
    let b = limited.create("Beta").await;
    let (status, error) = limited
        .editor(
            "PUT",
            "/api/v1/contents/article/hello/tags",
            Some(json!({ "tag_ids": [a["id"], b["id"]] })),
        )
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(error["error"].as_str().unwrap().contains("Too many tags"));

    // `[tags].enabled = false` なら 404
    let off = site(|config| config.tags.enabled = false).await;
    let (status, _) = off.editor("GET", "/api/v1/tags", None).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}
