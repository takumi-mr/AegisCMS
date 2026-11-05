//! ヘッドレス配信 API の結合テスト（実際のルーターを通す）

use std::sync::Arc;

use aegis_db::ContentRecord;
use aegis_runtime::PluginSupervisor;
use axum::body::Body;
use axum::http::{HeaderMap, Request, StatusCode};
use serde_json::{json, Value};
use tower::ServiceExt;

use crate::config::AegisConfig;
use crate::state::AppState;

struct Site {
    state: Arc<AppState>,
}

async fn site(configure: impl FnOnce(&mut AegisConfig)) -> Site {
    let mut config = AegisConfig::default();
    config.headless.enabled = true;
    configure(&mut config);
    let state = Arc::new(
        crate::test_support::test_app_state(config, PluginSupervisor::new(), Vec::new()).await,
    );
    Site { state }
}

fn record(id: &str, day: u32, status: &str, unpublish_at: Option<&str>) -> ContentRecord {
    ContentRecord {
        id: id.to_string(),
        content_type: "article".to_string(),
        title: format!("Title {id}"),
        slug: format!("slug-{id}"),
        status: status.to_string(),
        blocks_json: json!([{ "name": "core/paragraph", "text": format!("Body of {id}") }])
            .to_string(),
        author_id: Some("internal-author".to_string()),
        published_at: Some(format!("2026-10-{day:02}T00:00:00Z")),
        unpublish_at: unpublish_at.map(str::to_string),
        created_at: format!("2026-10-{day:02}T00:00:00Z"),
        updated_at: format!("2026-10-{day:02}T00:00:00Z"),
        custom_fields: Default::default(),
    }
}

impl Site {
    async fn put(&self, record: ContentRecord) {
        let schemas = self.state.schemas.read().await;
        let schema = &schemas[&record.content_type];
        self.state.db.insert_content(schema, &record).await.unwrap();
    }

    async fn send(&self, request: Request<Body>) -> (StatusCode, HeaderMap, String) {
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

    async fn get(&self, path: &str) -> (StatusCode, HeaderMap, String) {
        self.send(Request::get(path).body(Body::empty()).unwrap())
            .await
    }

    async fn get_json(&self, path: &str) -> Value {
        let (status, _, body) = self.get(path).await;
        assert_eq!(status, StatusCode::OK, "{}", body);
        serde_json::from_str(&body).unwrap()
    }
}

#[tokio::test]
async fn test_disabled_by_default() {
    let mut config = AegisConfig::default();
    assert!(!config.headless.enabled, "the API is opt-in");
    config.headless.enabled = false;
    let state = Arc::new(
        crate::test_support::test_app_state(config, PluginSupervisor::new(), Vec::new()).await,
    );
    let site = Site { state };
    let (status, _, body) = site.get("/api/v1/public/contents/article").await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert!(body.contains("not enabled"));
    let (status, _, _) = site.get("/api/v1/public/contents/article/x").await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_serves_published_content_without_a_login() {
    let site = site(|_| {}).await;
    site.put(record("a1", 1, "published", None)).await;
    site.put(record("a2", 2, "draft", None)).await;
    site.put(record("a3", 3, "published", Some("2026-01-01T00:00:00Z")))
        .await;

    let list = site.get_json("/api/v1/public/contents/article").await;
    let ids: Vec<&str> = list["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|item| item["id"].as_str().unwrap())
        .collect();
    assert_eq!(ids, vec!["a1"], "no draft and no expired item");
    assert!(list["next_cursor"].is_null());

    let one = site
        .get_json("/api/v1/public/contents/article/slug-a1")
        .await;
    assert_eq!(one["title"], "Title a1");
    assert_eq!(one["blocks"][0]["text"], "Body of a1");
    let text = one.to_string();
    assert!(!text.contains("internal-author"), "{text}");

    for hidden in ["slug-a2", "slug-a3", "nope"] {
        let (status, _, _) = site
            .get(&format!("/api/v1/public/contents/article/{hidden}"))
            .await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{hidden}");
    }
}

#[tokio::test]
async fn test_types_outside_the_list_and_bad_input() {
    let site = site(|_| {}).await;
    // category は既定の許可リスト（article と page）に無い
    let (status, _, _) = site.get("/api/v1/public/contents/category").await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (status, _, _) = site.get("/api/v1/public/contents/article?limit=0").await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    let (status, _, body) = site
        .get("/api/v1/public/contents/article?cursor=garbage")
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(body.contains("Invalid cursor"));
}

#[tokio::test]
async fn test_include_html_renders_the_body() {
    let site = site(|_| {}).await;
    site.put(record("a1", 1, "published", None)).await;
    let plain = site
        .get_json("/api/v1/public/contents/article/slug-a1")
        .await;
    assert!(plain.get("html").is_none());
    let with_html = site
        .get_json("/api/v1/public/contents/article/slug-a1?include=html")
        .await;
    assert!(with_html["html"].as_str().unwrap().contains("Body of a1"));
}

#[tokio::test]
async fn test_etag_and_cache_headers() {
    let site = site(|config| config.headless.cache_max_age = 120).await;
    site.put(record("a1", 1, "published", None)).await;

    let (status, headers, _) = site.get("/api/v1/public/contents/article/slug-a1").await;
    assert_eq!(status, StatusCode::OK);
    let etag = headers.get("etag").expect("an ETag").to_str().unwrap();
    let cache_control = headers.get("cache-control").unwrap().to_str().unwrap();
    assert!(
        cache_control == "public, max-age=120" || cache_control.contains("no-store"),
        "{cache_control}"
    );

    let (status, headers, body) = site
        .send(
            Request::get("/api/v1/public/contents/article/slug-a1")
                .header("if-none-match", etag)
                .body(Body::empty())
                .unwrap(),
        )
        .await;
    assert_eq!(status, StatusCode::NOT_MODIFIED);
    assert!(body.is_empty());
    assert!(headers.get("etag").is_some());

    // ?include=html は別の ETag
    let (_, html_headers, _) = site
        .get("/api/v1/public/contents/article/slug-a1?include=html")
        .await;
    assert_ne!(html_headers.get("etag").unwrap().to_str().unwrap(), etag);
}

#[tokio::test]
async fn test_rate_limit_per_ip() {
    let site = site(|config| {
        config.headless.rate_limit_per_minute = 2;
        config.audit.trust_forwarded_for = true;
    })
    .await;
    let from = |ip: &str| {
        Request::get("/api/v1/public/contents/article")
            .header("x-forwarded-for", ip)
            .body(Body::empty())
            .unwrap()
    };
    for _ in 0..2 {
        assert_eq!(site.send(from("203.0.113.1")).await.0, StatusCode::OK);
    }
    let (status, headers, _) = site.send(from("203.0.113.1")).await;
    assert_eq!(status, StatusCode::TOO_MANY_REQUESTS);
    assert!(headers.get("retry-after").is_some());
    // ほかのアドレスは影響を受けない
    assert_eq!(site.send(from("203.0.113.2")).await.0, StatusCode::OK);
}
