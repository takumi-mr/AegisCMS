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

async fn site(configure: impl FnOnce(&mut AegisConfig)) -> Site {
    let mut config = AegisConfig::default();
    config.search.index_dir = String::new();
    configure(&mut config);
    let mut state =
        crate::test_support::test_app_state(config, PluginSupervisor::new(), Vec::new()).await;
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
