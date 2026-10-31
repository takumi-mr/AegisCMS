//! アクセス解析の結合テスト（実際のルーターを通す）

use std::sync::Arc;

use aegis_runtime::PluginSupervisor;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use serde_json::{json, Value};
use tower::ServiceExt;

use crate::config::AegisConfig;
use crate::state::AppState;

const DESKTOP: &str =
    "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 Chrome/120 Safari/537.36";

struct Site {
    state: Arc<AppState>,
    viewer: String,
}

async fn site(configure: impl FnOnce(&mut AegisConfig)) -> Site {
    let mut config = AegisConfig::default();
    config.analytics.enabled = true;
    configure(&mut config);
    let mut state =
        crate::test_support::test_app_state(config, PluginSupervisor::new(), Vec::new()).await;
    if state.config.analytics.enabled {
        state.analytics = Some(Arc::new(aegis_analytics::AnalyticsService::new(
            state.config.analytics.clone(),
            state.db.clone(),
            state.cache.clone(),
        )));
    }
    let state = Arc::new(state);
    let viewer = state
        .sessions
        .create_session("viewer1", "viewer", Default::default(), None)
        .await
        .unwrap()
        .id;
    Site { state, viewer }
}

impl Site {
    async fn send(&self, request: Request<Body>) -> (StatusCode, String) {
        let response = crate::app::router(&self.state.config, self.state.clone())
            .oneshot(request)
            .await
            .unwrap();
        let status = response.status();
        let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        (status, String::from_utf8_lossy(&bytes).to_string())
    }

    async fn beacon(&self, body: &str, ip: [u8; 4], extra: Option<(&str, &str)>) -> StatusCode {
        let mut request = Request::post("/api/v1/analytics/event")
            .header("host", "example.com")
            .header("user-agent", DESKTOP)
            .header("content-type", "text/plain;charset=UTF-8")
            .extension(axum::extract::ConnectInfo(std::net::SocketAddr::from((
                ip, 50000,
            ))));
        if let Some((name, value)) = extra {
            request = request.header(name, value);
        }
        self.send(request.body(Body::from(body.to_string())).unwrap())
            .await
            .0
    }

    async fn summary(&self, query: &str, token: Option<&str>) -> (StatusCode, Value) {
        let mut request = Request::get(format!("/api/v1/analytics/summary{}", query));
        if let Some(token) = token {
            request = request.header("authorization", format!("Bearer {}", token));
        }
        let (status, body) = self.send(request.body(Body::empty()).unwrap()).await;
        (status, serde_json::from_str(&body).unwrap_or(Value::Null))
    }

    async fn export_csv(&self, query: &str, token: Option<&str>) -> (StatusCode, String) {
        let mut request = Request::get(format!("/api/v1/analytics/export{}", query));
        if let Some(token) = token {
            request = request.header("authorization", format!("Bearer {}", token));
        }
        self.send(request.body(Body::empty()).unwrap()).await
    }
}

#[tokio::test]
async fn test_beacon_is_on_public_pages_only_when_enabled() {
    let site = site(|config| config.server.base_path = "/cms".to_string()).await;
    let (_, html) = site
        .send(Request::get("/articles").body(Body::empty()).unwrap())
        .await;
    assert!(html.contains(r#"<script id="aegis-analytics">"#));
    assert!(html.contains(r#"u="/cms/api/v1/analytics/event""#));
    // Cookie は使わない
    assert!(!html.contains("document.cookie"));

    let disabled = self::site(|config| config.analytics.enabled = false).await;
    let (_, html) = disabled
        .send(Request::get("/articles").body(Body::empty()).unwrap())
        .await;
    assert!(!html.contains("aegis-analytics"));
    assert_eq!(
        disabled
            .beacon(r#"{"p":"/"}"#, [203, 0, 113, 1], None)
            .await,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        disabled.summary("", Some(&disabled.viewer)).await.0,
        StatusCode::NOT_FOUND
    );
}

#[tokio::test]
async fn test_events_are_counted_and_summarized() {
    let site = site(|_| {}).await;
    let event = |p: &str, r: &str| json!({ "p": p, "r": r }).to_string();
    assert_eq!(
        site.beacon(
            &event("/", "https://news.example.org/x"),
            [203, 0, 113, 1],
            None
        )
        .await,
        StatusCode::NO_CONTENT
    );
    site.beacon(
        &event("/about", "https://example.com/"),
        [203, 0, 113, 1],
        None,
    )
    .await;
    site.beacon(&event("/about", ""), [198, 51, 100, 2], None)
        .await;
    // Do Not Track・Global Privacy Control は数えない（応答は同じ）
    assert_eq!(
        site.beacon(&event("/", ""), [192, 0, 2, 3], Some(("dnt", "1")))
            .await,
        StatusCode::NO_CONTENT
    );
    site.beacon(&event("/", ""), [192, 0, 2, 4], Some(("sec-gpc", "1")))
        .await;
    // 壊れた本文・大きすぎる本文
    assert_eq!(
        site.beacon("not json", [192, 0, 2, 5], None).await,
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        site.beacon(&"x".repeat(5000), [192, 0, 2, 5], None).await,
        StatusCode::PAYLOAD_TOO_LARGE
    );

    assert_eq!(site.summary("", None).await.0, StatusCode::UNAUTHORIZED);
    let (status, summary) = site.summary("?limit=5", Some(&site.viewer)).await;
    assert_eq!(status, StatusCode::OK, "{}", summary);
    assert_eq!(summary["pageviews"], 3);
    assert_eq!(summary["visitors"], 2);
    assert_eq!(summary["daily"].as_array().unwrap().len(), 30);
    assert_eq!(summary["top_pages"][0]["value"], "/about");
    assert_eq!(summary["top_pages"][0]["pageviews"], 2);
    assert_eq!(
        summary["top_referrers"],
        json!([
            { "value": "", "pageviews": 1, "visitors": 1 },
            { "value": "news.example.org", "pageviews": 1, "visitors": 1 }
        ])
    );

    let (status, error) = site
        .summary("?from=2026-10-10&to=2026-10-01", Some(&site.viewer))
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(error["error"].as_str().unwrap().contains("after"));

    // UTM パラメータ付きビーコン送信
    let utm_event = json!({
        "p": "/campaign",
        "q": "?utm_source=twitter&utm_campaign=winter2026",
        "r": "https://t.co/abc"
    })
    .to_string();
    assert_eq!(
        site.beacon(&utm_event, [198, 51, 100, 10], None).await,
        StatusCode::NO_CONTENT
    );

    let (status, summary) = site.summary("?limit=5", Some(&site.viewer)).await;
    assert_eq!(status, StatusCode::OK);
    assert!(summary["top_utm_sources"]
        .as_array()
        .unwrap()
        .iter()
        .any(|v| v["value"] == "twitter"));
    assert!(summary["top_utm_campaigns"]
        .as_array()
        .unwrap()
        .iter()
        .any(|v| v["value"] == "winter2026"));
    assert!(summary["previous"].is_object());

    // CSV エクスポートのテスト
    assert_eq!(site.export_csv("", None).await.0, StatusCode::UNAUTHORIZED);
    let (status, csv) = site.export_csv("?limit=5", Some(&site.viewer)).await;
    assert_eq!(status, StatusCode::OK);
    assert!(csv.starts_with('\u{FEFF}'));
    assert!(csv.contains("Summary Report"));
    assert!(csv.contains("Daily Trend"));
    assert!(csv.contains("Top Pages"));
}

#[tokio::test]
async fn test_search_page_keyword_tracking() {
    let mut config = AegisConfig::default();
    config.analytics.enabled = true;
    config.search.enabled = true;
    config.search.index_dir = String::new();
    let mut state =
        crate::test_support::test_app_state(config, PluginSupervisor::new(), Vec::new()).await;
    state.analytics = Some(Arc::new(aegis_analytics::AnalyticsService::new(
        state.config.analytics.clone(),
        state.db.clone(),
        state.cache.clone(),
    )));
    state.search = Some(Arc::new(
        crate::search::SiteSearch::from_config(&state.config.search).unwrap(),
    ));
    let state = Arc::new(state);
    let viewer = state
        .sessions
        .create_session("viewer1", "viewer", Default::default(), None)
        .await
        .unwrap()
        .id;
    let site = Site { state, viewer };

    // コンテンツを 1 件投入してインデックスを構築
    let schema = site
        .state
        .schemas
        .read()
        .await
        .get("article")
        .cloned()
        .unwrap();
    let record = aegis_db::ContentRecord {
        id: "a1".to_string(),
        content_type: "article".to_string(),
        slug: "rust-guide".to_string(),
        title: "Rust Guide".to_string(),
        status: "published".to_string(),
        published_at: Some("2026-01-01T00:00:00Z".to_string()),
        created_at: "2026-01-01T00:00:00Z".to_string(),
        updated_at: "2026-01-01T00:00:00Z".to_string(),
        author_id: None,
        unpublish_at: None,
        blocks_json: json!([{ "name": "core/paragraph", "text": "Rust guide" }]).to_string(),
        custom_fields: Default::default(),
    };
    site.state
        .db
        .insert_content(&schema, &record)
        .await
        .unwrap();
    crate::search::rebuild(&site.state).await;

    // 1. 検索ページで "Rust" を検索（1件ヒット）
    let req1 = Request::get("/search?q=Rust")
        .header("host", "example.com")
        .header("user-agent", DESKTOP)
        .header("x-forwarded-for", "203.0.113.1")
        .body(Body::empty())
        .unwrap();
    let (status1, _) = site.send(req1).await;
    assert_eq!(status1, StatusCode::OK);

    // 2. 検索ページで "UnknownKeyword" を検索（0件ヒット）
    let req2 = Request::get("/search?q=UnknownKeyword")
        .header("host", "example.com")
        .header("user-agent", DESKTOP)
        .header("x-forwarded-for", "203.0.113.1")
        .body(Body::empty())
        .unwrap();
    let (status2, _) = site.send(req2).await;
    assert_eq!(status2, StatusCode::OK);

    // 3. アクセス解析の summary を取得して検証
    let (status, summary) = site.summary("?limit=10", Some(&site.viewer)).await;
    assert_eq!(status, StatusCode::OK);

    let queries = summary["top_search_queries"].as_array().unwrap();
    assert!(queries
        .iter()
        .any(|q| q["value"] == "Rust" && q["pageviews"] == 1));
    assert!(queries
        .iter()
        .any(|q| q["value"] == "UnknownKeyword" && q["pageviews"] == 1));

    let empty_queries = summary["top_search_empty"].as_array().unwrap();
    assert!(!empty_queries.iter().any(|q| q["value"] == "Rust"));
    assert!(empty_queries
        .iter()
        .any(|q| q["value"] == "UnknownKeyword" && q["pageviews"] == 1));

    // 4. CSV にも検索キーワードセクションが含まれること
    let (status, csv) = site.export_csv("?limit=10", Some(&site.viewer)).await;
    assert_eq!(status, StatusCode::OK);
    assert!(csv.contains("Top Search Queries"));
    assert!(csv.contains("Rust,1,1"));
    assert!(csv.contains("Top No-Result Search Queries"));
    assert!(csv.contains("UnknownKeyword,1,1"));
}
