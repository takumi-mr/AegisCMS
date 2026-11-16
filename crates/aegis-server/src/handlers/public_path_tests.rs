//! スキーマの `public_path`（公開ページの URL の先頭）の結合テスト（実際のルーターを通す）

use std::sync::Arc;

use aegis_db::ContentRecord;
use aegis_runtime::PluginSupervisor;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use tower::ServiceExt;

use crate::config::AegisConfig;
use crate::state::AppState;

fn schema(yaml: serde_json::Value) -> aegis_schema::SchemaDefinition {
    serde_json::from_value(yaml).unwrap()
}

async fn site() -> Arc<AppState> {
    let article = schema(serde_json::json!({
        "content_type": "article",
        "label": "Articles",
        "public_path": "/news",
        "fields": []
    }));
    let works = schema(serde_json::json!({
        "content_type": "works",
        "label": "Works",
        "public_path": "/works",
        "fields": []
    }));
    let state = Arc::new(
        crate::test_support::test_app_state(
            AegisConfig::default(),
            PluginSupervisor::new(),
            vec![article, works],
        )
        .await,
    );
    for (content_type, slug) in [("article", "hello"), ("works", "case-one")] {
        let schema = state.schemas.read().await[content_type].clone();
        state
            .db
            .insert_content(
                &schema,
                &ContentRecord {
                    id: format!("{content_type}-{slug}"),
                    content_type: content_type.to_string(),
                    title: format!("Title of {slug}"),
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
    state
}

async fn get(state: &Arc<AppState>, path: &str) -> (StatusCode, Option<String>, String) {
    let response = crate::app::router(&state.config, state.clone())
        .oneshot(Request::get(path).body(Body::empty()).unwrap())
        .await
        .unwrap();
    let status = response.status();
    let location = response
        .headers()
        .get("location")
        .map(|v| v.to_str().unwrap().to_string());
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    (
        status,
        location,
        String::from_utf8_lossy(&body).replace("&#x2f;", "/"),
    )
}

#[tokio::test]
async fn articles_are_served_under_the_public_path() {
    let state = site().await;

    let (status, _, html) = get(&state, "/news/hello").await;
    assert_eq!(status, StatusCode::OK);
    assert!(html.contains("Title of hello"), "{}", html);
    // 一覧は /news。記事へのリンクも /news/{slug}
    let (status, _, list) = get(&state, "/news").await;
    assert_eq!(status, StatusCode::OK);
    assert!(list.contains(r#"href="/news/hello""#), "{}", list);
    assert!(!list.contains("/articles/hello"), "{}", list);
}

#[tokio::test]
async fn legacy_article_urls_redirect_to_the_public_path() {
    let state = site().await;

    let (status, location, _) = get(&state, "/articles/hello").await;
    assert_eq!(status, StatusCode::MOVED_PERMANENTLY);
    assert_eq!(location.as_deref(), Some("/news/hello"));
    let (status, location, _) = get(&state, "/articles").await;
    assert_eq!(status, StatusCode::MOVED_PERMANENTLY);
    assert_eq!(location.as_deref(), Some("/news"));
}

#[tokio::test]
async fn custom_types_get_a_detail_page_under_their_public_path() {
    let state = site().await;

    let (status, _, html) = get(&state, "/works/case-one").await;
    assert_eq!(status, StatusCode::OK);
    assert!(html.contains("Title of case-one"), "{}", html);
    let (status, _, _) = get(&state, "/works/missing").await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    // 別の型の slug は引かない
    let (status, _, _) = get(&state, "/works/hello").await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn public_content_path_follows_the_schema() {
    let state = site().await;

    assert_eq!(
        super::public::public_content_path(&state, "article", "a"),
        "/news/a"
    );
    assert_eq!(
        super::public::public_content_path(&state, "works", "w"),
        "/works/w"
    );
    // 公開ページを持たない型は、従来どおり固定ページ風のパス
    assert_eq!(
        super::public::public_content_path(&state, "other", "x"),
        "/x"
    );
}

#[tokio::test]
async fn sitemap_and_feed_follow_the_public_path() {
    let state = site().await;

    let (status, _, xml) = get(&state, "/sitemap.xml").await;
    assert_eq!(status, StatusCode::OK);
    assert!(xml.contains("/news</loc>"), "{}", xml);
    assert!(xml.contains("/news/hello</loc>"), "{}", xml);
    assert!(xml.contains("/works/case-one</loc>"), "{}", xml);
    assert!(!xml.contains("/articles"), "{}", xml);

    let (status, _, feed) = get(&state, "/feed.xml").await;
    assert_eq!(status, StatusCode::OK);
    assert!(feed.contains("/news/hello\""), "{}", feed);
    assert!(!feed.contains("/articles/"), "{}", feed);
}

/// お知らせ（`news`）とブログ（`blog`）を別の型にして、それぞれの URL に一覧・フィード・詳細を持たせる
async fn split_site() -> Arc<AppState> {
    let make = |ct: &str, label: &str, path: &str| {
        schema(serde_json::json!({
            "content_type": ct, "label": label, "public_path": path, "fields": []
        }))
    };
    let state = Arc::new(
        crate::test_support::test_app_state(
            AegisConfig::default(),
            PluginSupervisor::new(),
            vec![make("news", "News", "/news"), make("blog", "Blog", "/blog")],
        )
        .await,
    );
    for (content_type, slug, n) in [
        ("news", "n1", 1),
        ("news", "n2", 2),
        ("news", "n3", 3),
        ("blog", "b1", 4),
    ] {
        let schema = state.schemas.read().await[content_type].clone();
        let at = format!("2026-10-01T00:00:{n:02}Z");
        state
            .db
            .insert_content(
                &schema,
                &ContentRecord {
                    id: format!("{content_type}-{slug}"),
                    content_type: content_type.to_string(),
                    title: format!("Title of {slug}"),
                    slug: slug.to_string(),
                    status: "published".to_string(),
                    blocks_json: "[]".to_string(),
                    author_id: None,
                    published_at: Some(at.clone()),
                    unpublish_at: None,
                    created_at: at.clone(),
                    updated_at: at,
                    custom_fields: Default::default(),
                },
            )
            .await
            .unwrap();
    }
    state
}

#[tokio::test]
async fn each_custom_type_has_its_own_list_and_feed() {
    let state = split_site().await;

    // 一覧はその型だけ。リンクは fields.url
    let (status, _, news) = get(&state, "/news").await;
    assert_eq!(status, StatusCode::OK);
    assert!(news.contains(r#"href="/news/n1""#), "{}", news);
    assert!(!news.contains("/blog/b1"), "{}", news);
    let (_, _, blog) = get(&state, "/blog").await;
    assert!(blog.contains(r#"href="/blog/b1""#), "{}", blog);
    assert!(!blog.contains("/news/n1"), "{}", blog);
    // 自動検出用のフィードのリンク
    assert!(news.contains("/news/feed.xml"), "{}", news);

    // フィードもその型だけ
    let (status, _, feed) = get(&state, "/news/feed.xml").await;
    assert_eq!(status, StatusCode::OK);
    assert!(feed.contains("/news/n1"), "{}", feed);
    assert!(!feed.contains("/blog/"), "{}", feed);
    let (_, _, feed) = get(&state, "/blog/feed.xml").await;
    assert!(
        feed.contains("/blog/b1") && !feed.contains("/news/"),
        "{}",
        feed
    );

    // 範囲外のページは 404
    let (status, _, _) = get(&state, "/news?page=9").await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn custom_detail_has_adjacent_and_related_of_its_own_type() {
    let state = split_site().await;

    let (status, _, html) = get(&state, "/news/n2").await;
    assert_eq!(status, StatusCode::OK);
    // 前後・関連に出るのは同じ型だけ（ブログは混ざらない）
    assert!(!html.contains("/blog/b1"), "{}", html);
    // 関連記事（既定で表示）に同じ型の別のお知らせが出て、リンクは /news/{slug}
    assert!(html.contains(r#"href="/news/n1""#), "{}", html);
    assert!(html.contains(r#"href="/news/n3""#), "{}", html);
}
