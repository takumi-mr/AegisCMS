//! サイト内検索の結合テスト（実際のルーターとイベントバスを通す）

use std::sync::Arc;

use aegis_db::ContentRecord;
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

async fn site_with_tags(configure: impl FnOnce(&mut AegisConfig)) -> Site {
    let mut config = AegisConfig::default();
    config.search.index_dir = String::new();
    config.tags.enabled = true;
    configure(&mut config);
    let mut state =
        crate::test_support::test_app_state(config, PluginSupervisor::new(), Vec::new()).await;
    if state.config.tags.enabled {
        let tag_service = aegis_tags::TagService::new(state.db.clone(), state.config.tags.clone());
        state = state.with_tags(tag_service);
    }
    if state.config.search.enabled {
        state.search = Some(Arc::new(
            super::SiteSearch::from_config(&state.config.search).unwrap(),
        ));
    }
    let state = Arc::new(state);
    crate::cluster::spawn_listener(state.clone(), state.bus.subscribe());
    let editor = state
        .sessions
        .create_session("editor1", "editor", Default::default(), None)
        .await
        .unwrap()
        .id;
    Site { state, editor }
}
async fn site(configure: impl FnOnce(&mut AegisConfig)) -> Site {
    site_with_schemas(configure, Vec::new()).await
}

async fn site_with_schemas(
    configure: impl FnOnce(&mut AegisConfig),
    extra_schemas: Vec<aegis_schema::SchemaDefinition>,
) -> Site {
    let mut config = AegisConfig::default();
    config.search.index_dir = String::new();
    configure(&mut config);
    let mut state =
        crate::test_support::test_app_state(config, PluginSupervisor::new(), extra_schemas).await;
    if state.config.search.enabled {
        state.search = Some(Arc::new(
            super::SiteSearch::from_config(&state.config.search).unwrap(),
        ));
    }
    let state = Arc::new(state);
    crate::cluster::spawn_listener(state.clone(), state.bus.subscribe());
    let editor = state
        .sessions
        .create_session("editor1", "editor", Default::default(), None)
        .await
        .unwrap()
        .id;
    Site { state, editor }
}

fn record(
    content_type: &str,
    id: &str,
    slug: &str,
    title: &str,
    status: &str,
    text: &str,
) -> ContentRecord {
    ContentRecord {
        id: id.to_string(),
        content_type: content_type.to_string(),
        title: title.to_string(),
        slug: slug.to_string(),
        status: status.to_string(),
        blocks_json: json!([{ "name": "core/paragraph", "text": text }]).to_string(),
        author_id: None,
        published_at: Some("2026-10-01T00:00:00Z".to_string()),
        unpublish_at: None,
        created_at: "2026-10-01T00:00:00Z".to_string(),
        updated_at: "2026-10-01T00:00:00Z".to_string(),
        custom_fields: Default::default(),
    }
}

impl Site {
    async fn send(&self, request: Request<Body>) -> (StatusCode, axum::http::HeaderMap, String) {
        let response = crate::app::router(&self.state.config, self.state.clone())
            .oneshot(request)
            .await
            .unwrap();
        let status = response.status();
        let headers = response.headers().clone();
        let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        (status, headers, String::from_utf8_lossy(&bytes).to_string())
    }

    async fn get(&self, path: &str) -> (StatusCode, axum::http::HeaderMap, String) {
        self.send(Request::get(path).body(Body::empty()).unwrap())
            .await
    }

    async fn api(&self, q: &str) -> Value {
        let path = format!(
            "/api/v1/search?q={}",
            url::form_urlencoded::byte_serialize(q.as_bytes()).collect::<String>()
        );
        let (status, _, body) = self.get(&path).await;
        assert_eq!(status, StatusCode::OK, "{}", body);
        serde_json::from_str(&body).unwrap()
    }

    async fn write(&self, method: &str, path: &str, body: Value) -> StatusCode {
        let request = Request::builder()
            .method(method)
            .uri(path)
            .header("authorization", format!("Bearer {}", self.editor))
            .header("content-type", "application/json")
            .body(Body::from(body.to_string()))
            .unwrap();
        self.send(request).await.0
    }

    /// イベントバス経由の反映は非同期なので、`total` が期待どおりになるまで待つ
    async fn wait_for_query_total(&self, path: &str, total: u64) -> Value {
        for _ in 0..100 {
            let (status, _, body) = self.get(path).await;
            assert_eq!(status, StatusCode::OK);
            let result: Value = serde_json::from_str(&body).unwrap();
            if result["total"] == total {
                return result;
            }
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
        panic!("search for {:?} never reached {} results", path, total);
    }

    async fn wait_for_total(&self, q: &str, total: u64) -> Value {
        for _ in 0..100 {
            let result = self.api(q).await;
            if result["total"] == total {
                return result;
            }
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
        panic!("search for {:?} never reached {} results", q, total);
    }
}

#[tokio::test]
async fn test_rebuild_indexes_only_published_content_of_configured_types() {
    let site = site(|_| {}).await;
    let schemas = site.state.schemas.read().await.clone();
    for r in [
        record(
            "article",
            "a1",
            "tokyo",
            "東京都の天気",
            "published",
            "今日は晴れ。",
        ),
        record(
            "article",
            "a2",
            "kyoto",
            "京都の寺",
            "draft",
            "下書きの東京都",
        ),
        record(
            "page",
            "p1",
            "about",
            "About",
            "published",
            "東京都にある会社です。",
        ),
    ] {
        site.state
            .db
            .insert_content(&schemas[&r.content_type], &r)
            .await
            .unwrap();
    }
    super::rebuild(&site.state).await;

    let result = site.api("東京都").await;
    assert_eq!(result["total"], 2);
    // タイトルの一致が先
    assert_eq!(result["items"][0]["url"], "/articles/tokyo");
    assert_eq!(result["items"][1]["url"], "/about");
    assert_eq!(
        result["items"][1]["snippet_html"],
        "<mark>東京都</mark>にある会社です。"
    );

    let (_, _, body) = site
        .get("/api/v1/search?q=%E6%9D%B1%E4%BA%AC%E9%83%BD&type=page")
        .await;
    let only_pages: Value = serde_json::from_str(&body).unwrap();
    assert_eq!(only_pages["total"], 1);
    let (_, _, body) = site
        .get("/api/v1/search?q=%E6%9D%B1%E4%BA%AC%E9%83%BD&type=product")
        .await;
    let unknown: Value = serde_json::from_str(&body).unwrap();
    assert_eq!(unknown["total"], 0);
}

#[tokio::test]
async fn test_category_filter_and_newest_sort() {
    // 記事に `category_id` のあるスキーマ
    let article = serde_json::from_value(json!({
        "content_type": "article",
        "label": "Articles",
        "fields": [
            { "name": "title", "type": "string", "required": true },
            { "name": "category_id", "type": "string", "required": false, "nullable": true }
        ]
    }))
    .unwrap();
    let site = site_with_schemas(|_| {}, vec![article]).await;
    let schemas = site.state.schemas.read().await.clone();
    let article = |id: &str, title: &str, published_at: &str, category: Option<&str>| {
        let mut r = record("article", id, id, title, "published", "Docker guide.");
        r.published_at = Some(published_at.to_string());
        if let Some(category) = category {
            r.custom_fields
                .insert("category_id".to_string(), json!(category));
        }
        r
    };
    for r in [
        // タイトルに語があるので、一致の良い順では先頭
        article("old", "Docker", "2026-01-01T00:00:00Z", Some("news")),
        article("new", "News", "2026-10-30T00:00:00Z", Some("news")),
        article("guide", "Guide", "2026-06-01T00:00:00Z", Some("guide")),
        article("plain", "Plain", "2026-05-01T00:00:00Z", None),
    ] {
        site.state
            .db
            .insert_content(&schemas["article"], &r)
            .await
            .unwrap();
    }
    super::rebuild(&site.state).await;

    let slugs = |result: &Value| -> Vec<String> {
        result["items"]
            .as_array()
            .unwrap()
            .iter()
            .map(|item| item["slug"].as_str().unwrap().to_string())
            .collect()
    };
    let get = |query: &str| {
        let path = format!("/api/v1/search?q=docker{}", query);
        let site = &site;
        async move {
            let (status, _, body) = site.get(&path).await;
            (status, serde_json::from_str::<Value>(&body).unwrap())
        }
    };

    // 既定は一致の良い順
    let (_, result) = get("").await;
    assert_eq!(result["total"], 4);
    assert_eq!(slugs(&result)[0], "old");

    let (_, result) = get("&sort=newest").await;
    assert_eq!(slugs(&result), vec!["new", "guide", "plain", "old"]);

    let (_, result) = get("&category=news").await;
    assert_eq!(result["total"], 2);
    let (_, result) = get("&category=news&sort=newest").await;
    assert_eq!(slugs(&result), vec!["new", "old"]);
    let (_, result) = get("&category=nothing").await;
    assert_eq!(result["total"], 0);
    // 空の `category` と `sort` は指定なし
    let (_, result) = get("&category=&sort=").await;
    assert_eq!(result["total"], 4);

    let (status, result) = get("&sort=oldest").await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(result["error"].as_str().unwrap().contains("Unknown sort"));

    // 検索ページ: 絞り込みと並びをページ送りのリンクに引き継ぐ
    let (status, _, html) = site.get("/search?q=docker&category=news&sort=newest").await;
    assert_eq!(status, StatusCode::OK);
    assert!(html.contains("New") || html.contains("News"), "{}", html);
    assert!(!html.contains("Guide"), "{}", html);
    // 並びの選択とカテゴリは、検索フォームで引き継ぐ
    assert!(
        html.contains(r#"<option value="newest" selected>"#),
        "{}",
        html
    );
    assert!(html.contains(r#"name="category" value="news""#), "{}", html);
}

#[tokio::test]
async fn test_content_changes_reach_the_index_through_the_event_bus() {
    let site = site(|_| {}).await;
    let status = site
        .write(
            "POST",
            "/api/v1/contents/article",
            json!({
                "title": "全文検索を追加しました",
                "slug": "search-release",
                "status": "published",
                "blocks": [{ "name": "core/paragraph", "text": "**日本語**でも探せます。" }]
            }),
        )
        .await;
    assert!(status.is_success(), "{}", status);
    let result = site.wait_for_total("日本語", 1).await;
    assert_eq!(result["items"][0]["slug"], "search-release");
    assert_eq!(
        result["items"][0]["snippet_html"],
        "<mark>日本語</mark>でも探せます。"
    );

    let status = site
        .write(
            "PUT",
            "/api/v1/contents/article/search-release",
            json!({
                "title": "サイト内検索",
                "slug": "search-release",
                "status": "published",
                // 公開中の版を差し替える（無ければ下書きの保存で、公開中の版と索引はそのまま）
                "publish": true,
                "blocks": [{ "name": "core/paragraph", "text": "英語も探せます。" }]
            }),
        )
        .await;
    assert!(status.is_success(), "{}", status);
    site.wait_for_total("日本語", 0).await;
    site.wait_for_total("英語", 1).await;

    let status = site
        .write(
            "DELETE",
            "/api/v1/contents/article/search-release",
            json!({}),
        )
        .await;
    assert!(status.is_success(), "{}", status);
    // 削除したコンテンツは公開ページにも出ない
    assert_eq!(
        site.get("/articles/search-release").await.0,
        StatusCode::NOT_FOUND
    );
    site.wait_for_total("英語", 0).await;
}

#[tokio::test]
async fn test_search_page_renders_results_with_marks() {
    let site = site(|config| config.server.base_path = "/cms".to_string()).await;
    let schemas = site.state.schemas.read().await.clone();
    let r = record(
        "article",
        "a1",
        "hello",
        "Hello <World>",
        "published",
        "Install AegisCMS with Docker.",
    );
    site.state
        .db
        .insert_content(&schemas["article"], &r)
        .await
        .unwrap();
    super::rebuild(&site.state).await;

    let (status, headers, html) = site.get("/search?q=docker").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(headers["x-robots-tag"], "noindex");
    assert!(
        html.contains(r#"href="&#x2f;cms&#x2f;articles&#x2f;hello""#),
        "{}",
        html
    );
    assert!(html.contains("Hello &lt;World&gt;"));
    assert!(html.contains("with <mark>Docker</mark>."));
    assert!(html.contains(r#"value="docker""#));
    assert!(html.contains("1 result"));

    // 検索語の HTML はエスケープされる
    let (_, _, html) = site.get("/search?q=%3Cscript%3E").await;
    assert!(!html.contains("<script>"));
    assert!(html.contains("No pages matched"));

    // 検索語が無ければフォームだけ
    let (status, _, html) = site.get("/search").await;
    assert_eq!(status, StatusCode::OK);
    assert!(html.contains(r#"role="search""#));
    assert!(!html.contains("No pages matched"));
}

#[tokio::test]
async fn test_limits_and_disabled_search() {
    let site = site(|_| {}).await;
    let long = "あ".repeat(201);
    let path = format!(
        "/api/v1/search?q={}",
        url::form_urlencoded::byte_serialize(long.as_bytes()).collect::<String>()
    );
    assert_eq!(site.get(&path).await.0, StatusCode::BAD_REQUEST);
    assert_eq!(site.api("").await["total"], 0);

    let disabled = self::site(|config| config.search.enabled = false).await;
    assert_eq!(
        disabled.get("/api/v1/search?q=a").await.0,
        StatusCode::NOT_FOUND
    );
    assert_eq!(disabled.get("/search?q=a").await.0, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_tag_filter_and_search_page() {
    let site = site_with_tags(|_| {}).await;
    let schemas = site.state.schemas.read().await.clone();
    let tag_service = site.state.tags.as_ref().unwrap().clone();

    let tag_rust = tag_service
        .create(aegis_tags::TagInput {
            name: "Rust".to_string(),
            slug: Some("rust".to_string()),
            description: None,
        })
        .await
        .unwrap();

    let tag_caddy = tag_service
        .create(aegis_tags::TagInput {
            name: "Caddy".to_string(),
            slug: Some("caddy".to_string()),
            description: None,
        })
        .await
        .unwrap();

    for r in [
        record(
            "article",
            "a1",
            "a1",
            "AegisCMS with Rust",
            "published",
            "Fast engine.",
        ),
        record(
            "article",
            "a2",
            "a2",
            "AegisCMS with Caddy",
            "published",
            "Reverse proxy.",
        ),
        record(
            "article",
            "a3",
            "a3",
            "AegisCMS with Both",
            "published",
            "Complete setup.",
        ),
    ] {
        site.state
            .db
            .insert_content(&schemas[&r.content_type], &r)
            .await
            .unwrap();
    }

    tag_service
        .set_content_tags("article", "a1", &[tag_rust.id.clone()])
        .await
        .unwrap();
    tag_service
        .set_content_tags("article", "a2", &[tag_caddy.id.clone()])
        .await
        .unwrap();
    tag_service
        .set_content_tags(
            "article",
            "a3",
            &[tag_rust.id.clone(), tag_caddy.id.clone()],
        )
        .await
        .unwrap();

    super::rebuild(&site.state).await;

    // 1. API: 全件検索
    let res = site.api("AegisCMS").await;
    assert_eq!(res["total"], 3);

    // 2. API: slug でのタグ絞り込み
    let (status, _, body) = site.get("/api/v1/search?q=AegisCMS&tag=rust").await;
    assert_eq!(status, StatusCode::OK);
    let res: Value = serde_json::from_str(&body).unwrap();
    assert_eq!(res["total"], 2);
    let ids: Vec<_> = res["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|item| item["id"].as_str().unwrap())
        .collect();
    assert!(ids.contains(&"a1"));
    assert!(ids.contains(&"a3"));

    let (status, _, body) = site.get("/api/v1/search?q=AegisCMS&tag=caddy").await;
    assert_eq!(status, StatusCode::OK);
    let res: Value = serde_json::from_str(&body).unwrap();
    assert_eq!(res["total"], 2);

    // 3. API: ID でのタグ絞り込み
    let (status, _, body) = site
        .get(&format!("/api/v1/search?q=AegisCMS&tag={}", tag_rust.id))
        .await;
    assert_eq!(status, StatusCode::OK);
    let res: Value = serde_json::from_str(&body).unwrap();
    assert_eq!(res["total"], 2);

    // 4. API: 存在しないタグ
    let (status, _, body) = site.get("/api/v1/search?q=AegisCMS&tag=no-such").await;
    assert_eq!(status, StatusCode::OK);
    let res: Value = serde_json::from_str(&body).unwrap();
    assert_eq!(res["total"], 0);

    // 5. 検索ページ
    let (status, _, html) = site.get("/search?q=AegisCMS&tag=rust").await;
    assert_eq!(status, StatusCode::OK);
    assert!(html.contains(r#"<input type="hidden" name="tag" value="rust">"#));
    assert!(html.contains("2 results"));

    // 6. イベントバス経由でのタグ更新の反映
    let status = site
        .write(
            "PUT",
            "/api/v1/contents/article/a1/tags",
            json!({ "tag_ids": [] }),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    let res = site
        .wait_for_query_total("/api/v1/search?q=AegisCMS&tag=rust", 1)
        .await;
    assert_eq!(res["items"][0]["id"], "a3");

    // 7. タグの削除で、そのタグが付いていた全コンテンツのインデックスが更新される
    let status = site
        .write(
            "DELETE",
            &format!("/api/v1/tags/{}", tag_caddy.id),
            json!({}),
        )
        .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    site.wait_for_query_total("/api/v1/search?q=AegisCMS&tag=caddy", 0)
        .await;
}
