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
    site_with_cdn(configure, None).await
}

async fn site_with_cdn(
    configure: impl FnOnce(&mut AegisConfig),
    purger: Option<Arc<aegis_cache::RecordingCdnPurger>>,
) -> Site {
    let mut config = AegisConfig::default();
    config.audit.enabled = true;
    configure(&mut config);
    let state =
        crate::test_support::test_app_state(config, PluginSupervisor::new(), Vec::new()).await;
    let state = match purger {
        Some(purger) => state.with_cdn(purger),
        None => state,
    };
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

    /// 公開ページ・フィードの本文（ログインなし）
    async fn text(&self, path: &str) -> (StatusCode, String) {
        let response = crate::app::router(&self.state.config, self.state.clone())
            .oneshot(Request::get(path).body(Body::empty()).unwrap())
            .await
            .unwrap();
        let status = response.status();
        let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        // テンプレートの属性値では `/` が `&#x2f;` にエスケープされる
        (
            status,
            String::from_utf8_lossy(&bytes).replace("&#x2f;", "/"),
        )
    }

    async fn editor(&self, method: &str, path: &str, body: Option<Value>) -> (StatusCode, Value) {
        self.call(method, path, body, Some(&self.editor.clone()))
            .await
    }

    async fn viewer(&self, method: &str, path: &str, body: Option<Value>) -> (StatusCode, Value) {
        self.call(method, path, body, Some(&self.viewer.clone()))
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

/// 公開記事・公開固定ページ・下書き記事を加える（`site()` の `hello` と `about` に足す）
async fn add_content(site: &Site) {
    let article = site.state.schemas.read().await["article"].clone();
    for (id, slug, status, published_at) in [
        ("a2", "second", "published", "2026-10-02T00:00:00Z"),
        ("a3", "secret-draft", "draft", "2026-10-03T00:00:00Z"),
    ] {
        site.state
            .db
            .insert_content(
                &article,
                &ContentRecord {
                    id: id.to_string(),
                    content_type: "article".to_string(),
                    title: slug.to_uppercase(),
                    slug: slug.to_string(),
                    status: status.to_string(),
                    blocks_json:
                        json!([{ "name": "core/paragraph", "text": format!("About {}", slug) }])
                            .to_string(),
                    author_id: None,
                    published_at: Some(published_at.to_string()),
                    unpublish_at: None,
                    created_at: published_at.to_string(),
                    updated_at: published_at.to_string(),
                    custom_fields: Default::default(),
                },
            )
            .await
            .unwrap();
    }
}

#[tokio::test]
async fn test_public_tag_pages_list_published_content_of_every_type() {
    let site = site(|config| config.theme.posts_per_page = 2).await;
    add_content(&site).await;
    let rust = site.create("Rust Lang").await;
    let docker = site.create("Docker").await;
    let rust_id = rust["id"].as_str().unwrap();
    let docker_id = docker["id"].as_str().unwrap();
    for (path, tag_ids) in [
        (
            "/api/v1/contents/article/hello/tags",
            vec![rust_id, docker_id],
        ),
        ("/api/v1/contents/article/second/tags", vec![rust_id]),
        ("/api/v1/contents/article/secret-draft/tags", vec![rust_id]),
        ("/api/v1/contents/page/about/tags", vec![rust_id]),
    ] {
        let (status, _) = site
            .editor("PUT", path, Some(json!({ "tag_ids": tag_ids })))
            .await;
        assert_eq!(status, StatusCode::OK, "{}", path);
    }

    // タグのページ: 公開中の記事と固定ページが新しい順（下書きは出ない）。1 ページ 2 件で、
    // 公開日時が同じなら ID の降順（`p1`＝about が先、`a1`＝hello が後）
    let (status, html) = site.text("/tags/rust-lang").await;
    assert_eq!(status, StatusCode::OK, "{}", html);
    assert!(
        html.contains("<h1 style=\"margin: 0 0 0.75rem;\">Rust Lang</h1>"),
        "{}",
        html
    );
    assert!(html.contains(r#"href="/articles/second""#), "{}", html);
    assert!(html.contains(r#"href="/about""#), "{}", html);
    assert!(!html.contains(r#"href="/articles/hello""#), "{}", html);
    assert!(!html.contains("SECRET-DRAFT"), "{}", html);
    assert!(html.contains("/tags/rust-lang?page=2"), "{}", html);
    assert!(html.contains("/tags/rust-lang/feed.xml"), "{}", html);
    let (status, second) = site.text("/tags/rust-lang?page=2").await;
    assert_eq!(status, StatusCode::OK);
    assert!(second.contains(r#"href="/articles/hello""#), "{}", second);
    assert_eq!(
        site.text("/tags/rust-lang?page=3").await.0,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        site.text("/tags/no-such-tag").await.0,
        StatusCode::NOT_FOUND
    );

    // 記事のページに、付いているタグのリンク
    let (status, article) = site.text("/articles/hello").await;
    assert_eq!(status, StatusCode::OK, "{}", article);
    assert!(article.contains("rel=\"tag\""), "{}", article);
    assert!(article.contains("/tags/rust-lang"), "{}", article);
    assert!(article.contains("/tags/docker"), "{}", article);
    // 固定ページにも
    let (_, page) = site.text("/about").await;
    assert!(page.contains("/tags/rust-lang"), "{}", page);

    // 一覧: 公開中のコンテンツが付いたタグと、その数（下書きは数えない）
    let (status, index) = site.text("/tags").await;
    assert_eq!(status, StatusCode::OK);
    assert!(
        index.contains("/tags/rust-lang") && index.contains("3 items"),
        "{}",
        index
    );
    assert!(
        index.contains("/tags/docker") && index.contains("1 item"),
        "{}",
        index
    );

    // 公開中の記事が付いていないタグは、一覧とサイトマップに出ない
    let empty = site.create("Unused").await;
    let (_, index) = site.text("/tags").await;
    assert!(
        !index.contains(empty["slug"].as_str().unwrap()),
        "{}",
        index
    );
    let (_, sitemap) = site.text("/sitemap.xml").await;
    assert!(sitemap.contains("/tags/rust-lang</loc>"), "{}", sitemap);
    assert!(sitemap.contains("/tags</loc>"), "{}", sitemap);
    assert!(!sitemap.contains("/tags/unused"), "{}", sitemap);

    // フィードは記事だけ（固定ページは載せない）。下書きも載せない
    let (status, feed) = site.text("/tags/rust-lang/feed.xml").await;
    assert_eq!(status, StatusCode::OK);
    assert!(
        feed.contains("/articles/second") && feed.contains("/articles/hello"),
        "{}",
        feed
    );
    assert!(
        !feed.contains("secret-draft") && !feed.contains("/about"),
        "{}",
        feed
    );
    assert_eq!(
        site.text("/tags/no-such-tag/feed.xml").await.0,
        StatusCode::NOT_FOUND
    );

    // タグを外すとページから消え、タグを消すと 404
    let (status, _) = site
        .editor(
            "PUT",
            "/api/v1/contents/article/second/tags",
            Some(json!({ "tag_ids": [] })),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    let (_, html) = site.text("/tags/rust-lang").await;
    assert!(!html.contains("/articles/second"), "{}", html);
    let (status, _) = site
        .editor("DELETE", &format!("/api/v1/tags/{}", rust_id), None)
        .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    assert_eq!(site.text("/tags/rust-lang").await.0, StatusCode::NOT_FOUND);
    let (_, article) = site.text("/articles/hello").await;
    assert!(!article.contains("/tags/rust-lang"), "{}", article);
}

#[tokio::test]
async fn test_public_tag_pages_are_off_when_tags_are_disabled() {
    let off = site(|config| config.tags.enabled = false).await;
    for path in ["/tags", "/tags/anything", "/tags/anything/feed.xml"] {
        assert_eq!(off.text(path).await.0, StatusCode::NOT_FOUND, "{}", path);
    }
    // 記事のページにタグは出ない
    let (status, article) = off.text("/articles/hello").await;
    assert_eq!(status, StatusCode::OK);
    assert!(!article.contains("rel=\"tag\""), "{}", article);
}

#[tokio::test]
async fn test_tag_changes_purge_the_cached_pages() {
    let purger = Arc::new(aegis_cache::RecordingCdnPurger::new());
    let site = site_with_cdn(
        |config| {
            config.cdn.enabled = true;
            config.cdn.provider = aegis_cache::CdnProviderKind::Log;
        },
        Some(purger.clone()),
    )
    .await;
    let take = |purger: &aegis_cache::RecordingCdnPurger| -> Vec<String> {
        purger.take_batches().into_iter().flatten().collect()
    };
    let tag = site.create("Rust").await;
    let id = tag["id"].as_str().unwrap();
    take(&purger);

    // 付ける: 記事のページと、タグのページ・一覧・サイトマップ
    let (status, _) = site
        .editor(
            "PUT",
            "/api/v1/contents/article/hello/tags",
            Some(json!({ "tag_ids": [id] })),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    let purged = take(&purger);
    for expected in [
        "article-a1".to_string(),
        "url-articles-hello".to_string(),
        format!("tag-{}", id),
        "tag-rust".to_string(),
        "url-tags-rust".to_string(),
        "url-tags".to_string(),
        "url-sitemap.xml".to_string(),
    ] {
        assert!(
            purged.contains(&expected),
            "{:?} is missing in {:?}",
            expected,
            purged
        );
    }

    // 名前の変更では、前の slug のページも
    let (status, _) = site
        .editor(
            "PUT",
            &format!("/api/v1/tags/{}", id),
            Some(json!({ "name": "Rust Lang", "slug": "rust-lang" })),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    let purged = take(&purger);
    for expected in [
        "url-tags-rust",
        "url-tags-rust-lang",
        &format!("tag-{}", id),
    ] {
        assert!(
            purged.iter().any(|t| t == expected),
            "{:?} is missing in {:?}",
            expected,
            purged
        );
    }

    // 削除
    let (status, _) = site
        .editor("DELETE", &format!("/api/v1/tags/{}", id), None)
        .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let purged = take(&purger);
    assert!(
        purged.contains(&"url-tags-rust-lang".to_string()),
        "{:?}",
        purged
    );
}

#[tokio::test]
async fn test_list_contents_filter_by_tag() {
    let site = site(|_| {}).await;

    // タグを作成
    let (_, body) = site
        .editor(
            "POST",
            "/api/v1/tags",
            Some(json!({ "name": "Rust", "slug": "rust" })),
        )
        .await;
    let rust_id = body["id"].as_str().unwrap().to_string();

    let (_, body) = site
        .editor(
            "POST",
            "/api/v1/tags",
            Some(json!({ "name": "Go", "slug": "go" })),
        )
        .await;
    let go_id = body["id"].as_str().unwrap().to_string();

    // 追加の記事 a2, a3 を DB に入れる
    let schema = site.state.schemas.read().await["article"].clone();
    for (id, slug, title) in [
        ("a2", "go-lang", "Go Title"),
        ("a3", "plain", "Plain Title"),
    ] {
        site.state
            .db
            .insert_content(
                &schema,
                &ContentRecord {
                    id: id.to_string(),
                    content_type: "article".to_string(),
                    title: title.to_string(),
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

    // タグ付け: a1 に rust, a2 に go, a3 はタグなし
    let (status, _) = site
        .editor(
            "PUT",
            "/api/v1/contents/article/a1/tags",
            Some(json!({ "tag_ids": [rust_id] })),
        )
        .await;
    assert_eq!(status, StatusCode::OK);

    let (status, _) = site
        .editor(
            "PUT",
            "/api/v1/contents/article/a2/tags",
            Some(json!({ "tag_ids": [go_id] })),
        )
        .await;
    assert_eq!(status, StatusCode::OK);

    // 1. 全件取得 (a1, a2, a3 の 3 件)
    let (status, body) = site.editor("GET", "/api/v1/contents/article", None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["total"], 3);

    // 2. tag slug で絞り込み ("rust")
    let (status, body) = site
        .viewer("GET", "/api/v1/contents/article?tag=rust", None)
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["total"], 1);
    assert_eq!(body["contents"][0]["id"], "a1");

    // 3. tag id で絞り込み
    let (status, body) = site
        .viewer(
            "GET",
            &format!("/api/v1/contents/article?tag={}", go_id),
            None,
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["total"], 1);
    assert_eq!(body["contents"][0]["id"], "a2");

    // 4. 存在しないタグ
    let (status, body) = site
        .viewer("GET", "/api/v1/contents/article?tag=no-such", None)
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["total"], 0);
    assert!(body["contents"].as_array().unwrap().is_empty());

    // 5. タグ機能が無効のときは 404
    let disabled = self::site(|config| config.tags.enabled = false).await;
    let (status, _) = disabled
        .viewer("GET", "/api/v1/contents/article?tag=rust", None)
        .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}
