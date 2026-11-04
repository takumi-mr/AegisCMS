//! oEmbed プロバイダー（`GET /oembed`）と発見用リンクの結合テスト

use std::sync::Arc;

use aegis_runtime::PluginSupervisor;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use serde_json::{json, Value};
use tower::ServiceExt;

use crate::config::AegisConfig;
use crate::seo::SeoOverrides;
use crate::state::AppState;
use aegis_db::ContentRecord;

async fn site(canonical: Option<&str>) -> Arc<AppState> {
    let state = crate::test_support::test_app_state(
        AegisConfig::default(),
        PluginSupervisor::new(),
        Vec::new(),
    )
    .await;
    *state.seo.write().unwrap() = SeoOverrides {
        canonical_base_url: canonical.map(str::to_string),
        ..Default::default()
    };
    let state = Arc::new(state);
    for (content_type, slug, status, fields) in [
        (
            "article",
            "hello",
            "published",
            json!({ "excerpt": "An intro", "featured_image": "/uploads/a.png" }),
        ),
        ("page", "about", "published", json!({})),
        ("article", "secret", "draft", json!({})),
    ] {
        let schema = state.schemas.read().await[content_type].clone();
        state
            .db
            .insert_content(
                &schema,
                &ContentRecord {
                    id: format!("id-{slug}"),
                    content_type: content_type.to_string(),
                    title: format!("Title of {slug}"),
                    slug: slug.to_string(),
                    status: status.to_string(),
                    blocks_json: "[]".to_string(),
                    author_id: None,
                    published_at: Some("2026-10-01T00:00:00Z".to_string()),
                    unpublish_at: None,
                    created_at: "2026-10-01T00:00:00Z".to_string(),
                    updated_at: "2026-10-01T00:00:00Z".to_string(),
                    custom_fields: serde_json::from_value(fields).unwrap(),
                },
            )
            .await
            .unwrap();
    }
    state
}

async fn get(state: &Arc<AppState>, path: &str) -> (StatusCode, String) {
    let response = crate::app::router(&state.config, state.clone())
        .oneshot(Request::get(path).body(Body::empty()).unwrap())
        .await
        .unwrap();
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    (status, String::from_utf8_lossy(&bytes).into_owned())
}

#[tokio::test]
async fn oembed_returns_the_metadata_of_published_content() {
    let state = site(Some("https://example.com")).await;

    let (status, body) = get(
        &state,
        "/oembed?url=https%3A%2F%2Fexample.com%2Farticles%2Fhello&format=json",
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let json: Value = serde_json::from_str(&body).unwrap();
    assert_eq!(json["version"], "1.0");
    assert_eq!(json["type"], "link");
    assert_eq!(json["title"], "Title of hello");
    assert_eq!(json["description"], "An intro");
    assert_eq!(json["thumbnail_url"], "https://example.com/uploads/a.png");
    assert_eq!(json["provider_url"], "https://example.com");

    // 固定ページは `/{slug}`
    let (status, body) = get(&state, "/oembed?url=https%3A%2F%2Fexample.com%2Fabout").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(
        serde_json::from_str::<Value>(&body).unwrap()["title"],
        "Title of about"
    );
}

#[tokio::test]
async fn oembed_refuses_what_is_not_public_or_not_this_site() {
    let state = site(Some("https://example.com")).await;
    for (path, expected) in [
        // 下書き・存在しない
        (
            "/oembed?url=https%3A%2F%2Fexample.com%2Farticles%2Fsecret",
            StatusCode::NOT_FOUND,
        ),
        (
            "/oembed?url=https%3A%2F%2Fexample.com%2Farticles%2Fnope",
            StatusCode::NOT_FOUND,
        ),
        // 他のサイトの URL（このエンドポイントを経由した探りにしない）
        (
            "/oembed?url=https%3A%2F%2Fevil.example%2Farticles%2Fhello",
            StatusCode::NOT_FOUND,
        ),
        ("/oembed", StatusCode::BAD_REQUEST),
        (
            "/oembed?url=https%3A%2F%2Fexample.com%2Farticles%2Fhello&format=xml",
            StatusCode::NOT_IMPLEMENTED,
        ),
    ] {
        assert_eq!(get(&state, path).await.0, expected, "{path}");
    }
}

#[tokio::test]
async fn content_pages_link_to_their_oembed_endpoint() {
    let state = site(Some("https://example.com")).await;
    let (status, html) = get(&state, "/articles/hello").await;
    assert_eq!(status, StatusCode::OK);
    assert!(
        html.contains(r#"type="application/json+oembed""#)
            && html.contains(
                "/oembed?url=https%3A%2F%2Fexample.com%2Farticles%2Fhello&amp;format=json"
            ),
        "{html}"
    );

    // 絶対 URL が分からない（canonical_base_url なし）ときは出さない
    let state = site(None).await;
    let (_, html) = get(&state, "/articles/hello").await;
    assert!(!html.contains("application/json+oembed"));
}
