//! 商品ページのレビュー欄（`/shop/reviews.js`、`reviews.html`、JSON-LD）の結合テスト（ルーター経由）

use std::sync::Arc;

use aegis_db::{ContentRecord, ReviewRecord};
use aegis_runtime::PluginSupervisor;
use aegis_schema::SchemaDefinition;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use serde_json::{json, Value};
use tower::ServiceExt;

use crate::app::router;
use crate::config::AegisConfig;
use crate::state::AppState;
use crate::test_support::test_app_state;

fn product_schema() -> SchemaDefinition {
    serde_json::from_value(json!({
        "content_type": "product",
        "label": "Products",
        "fields": [
            { "name": "price", "type": "integer", "required": true },
            { "name": "stock", "type": "integer", "nullable": true }
        ]
    }))
    .unwrap()
}

struct Site {
    app: axum::Router,
    state: Arc<AppState>,
}

async fn site_with(configure: impl FnOnce(&mut AegisConfig)) -> Site {
    let mut config = AegisConfig::default();
    config.commerce.enabled = true;
    config.members.enabled = true;
    config.commerce.product_type = "product".to_string();
    configure(&mut config);
    let state = test_app_state(
        config.clone(),
        PluginSupervisor::new(),
        vec![product_schema()],
    )
    .await;
    let commerce =
        aegis_commerce::CommerceService::new(config.commerce.clone(), state.commerce_deps())
            .unwrap();
    let members = aegis_commerce::MemberService::from_config(
        &config.members,
        state.db.clone(),
        state.cache.clone(),
        state.audit.clone(),
    );
    let reviews = aegis_commerce::ReviewService::new(
        state.db.clone(),
        state.audit.clone(),
        state.notifier.clone(),
        config.comments.moderation.clone(),
    );
    let state = state.with_commerce(commerce).with_reviews(reviews);
    let state: Arc<AppState> = Arc::new(match members {
        Some(members) => state.with_members(members),
        None => state,
    });
    Site {
        app: router(&config, state.clone()),
        state,
    }
}

impl Site {
    async fn get(&self, path: &str) -> (StatusCode, String) {
        let response = self
            .app
            .clone()
            .oneshot(Request::get(path).body(Body::empty()).unwrap())
            .await
            .unwrap();
        let status = response.status();
        let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        (status, String::from_utf8_lossy(&bytes).to_string())
    }

    async fn add_product(&self, id: &str, title: &str) {
        let schemas = self.state.schemas.read().await;
        let record = ContentRecord {
            id: id.to_string(),
            content_type: "product".to_string(),
            title: title.to_string(),
            slug: id.to_string(),
            status: "published".to_string(),
            blocks_json: "[]".to_string(),
            author_id: None,
            published_at: Some("2026-10-01T00:00:00Z".to_string()),
            unpublish_at: None,
            created_at: "2026-10-01T00:00:00Z".to_string(),
            updated_at: "2026-10-01T00:00:00Z".to_string(),
            custom_fields: [("price".to_string(), json!(1200))].into(),
        };
        self.state
            .db
            .insert_content(&schemas["product"], &record)
            .await
            .unwrap();
    }

    async fn add_review(&self, id: &str, product_id: &str, rating: i64, status: &str) {
        let review = ReviewRecord {
            id: id.to_string(),
            product_id: product_id.to_string(),
            order_id: format!("order-{id}"),
            member_id: None,
            author_name: "Hanako".to_string(),
            author_email: "hanako@example.com".to_string(),
            rating,
            title: "Great".to_string(),
            body: "Really good beans".to_string(),
            status: status.to_string(),
            verified_purchase: 1,
            reply: None,
            replied_at: None,
            rejection_reason: None,
            ip: None,
            user_agent: None,
            created_at: "2026-10-02T00:00:00Z".to_string(),
            updated_at: "2026-10-02T00:00:00Z".to_string(),
        };
        self.state.db.insert_review(&review).await.unwrap();
    }
}

/// `<script type="application/ld+json">` の中身を取り出して JSON として読む
fn json_ld(page: &str) -> Option<Value> {
    let start = page.find("<script type=\"application/ld+json\">")?;
    let rest = &page[start + "<script type=\"application/ld+json\">".len()..];
    let end = rest.find("</script>")?;
    serde_json::from_str(&rest[..end]).ok()
}

#[tokio::test]
async fn test_product_page_has_the_review_section_and_the_script_is_served() {
    let site = site_with(|config| config.theme.locale = "ja".to_string()).await;
    site.add_product("beans", "Beans").await;

    let (status, page) = site.get("/products/beans").await;
    assert_eq!(status, StatusCode::OK);
    assert!(page.contains("reviews.js"), "the page loads the script");
    assert!(page.contains("data-product-id=\"beans\""));
    assert!(
        page.contains("お客様のレビュー"),
        "the heading is in the site language"
    );

    let (status, script) = site.get("/shop/reviews.js").await;
    assert_eq!(status, StatusCode::OK);
    assert!(script.contains("_CART_I18N = "));
    assert!(script.contains("レビューを投稿する"));
}

#[tokio::test]
async fn test_json_ld_appears_only_with_approved_reviews_and_uses_schema_org_names() {
    let site = site_with(|_| {}).await;
    site.add_product("beans", "Beans").await;

    // レビューが無い・承認待ちだけ: 構造化データは出さない
    let (_, page) = site.get("/products/beans").await;
    assert!(json_ld(&page).is_none());
    site.add_review("r1", "beans", 1, "pending").await;
    let (_, page) = site.get("/products/beans").await;
    assert!(
        json_ld(&page).is_none(),
        "a review nobody approved is not counted"
    );

    site.add_review("r2", "beans", 5, "approved").await;
    site.add_review("r3", "beans", 4, "approved").await;
    let (_, page) = site.get("/products/beans").await;
    let ld = json_ld(&page).expect("structured data");
    assert_eq!(ld["@context"], "https://schema.org");
    assert_eq!(ld["@type"], "Product");
    assert_eq!(ld["name"], "Beans");
    let rating = &ld["aggregateRating"];
    assert_eq!(rating["@type"], "AggregateRating");
    assert_eq!(rating["ratingValue"], 4.5);
    assert_eq!(rating["reviewCount"], 2);
    assert_eq!(rating["bestRating"], 5);
    assert_eq!(rating["worstRating"], 1);
    // snake_case の名前は出さない（schema.org の名前ではない）
    assert!(rating.get("rating_value").is_none());
}

#[tokio::test]
async fn test_json_ld_cannot_close_the_script_tag() {
    let site = site_with(|_| {}).await;
    site.add_product("evil", "A </script><b>x</b> product")
        .await;
    site.add_review("r1", "evil", 5, "approved").await;

    let (_, page) = site.get("/products/evil").await;
    // 商品名の `<` はエスケープされ、`</script>` で閉じられない
    let ld = json_ld(&page).expect("structured data");
    assert_eq!(ld["name"], "A </script><b>x</b> product");
    let start = page.find("<script type=\"application/ld+json\">").unwrap();
    let block_end = page[start..].find("</script>").unwrap();
    assert!(!page[start..start + block_end].contains("<b>"));
}

#[tokio::test]
async fn test_the_review_api_lists_only_approved_reviews_with_schema_org_json_ld() {
    let site = site_with(|_| {}).await;
    site.add_product("beans", "Beans").await;
    site.add_review("r1", "beans", 5, "approved").await;
    site.add_review("r2", "beans", 1, "pending").await;

    let (status, body) = site.get("/api/v1/products/beans/reviews").await;
    assert_eq!(status, StatusCode::OK);
    let data: Value = serde_json::from_str(&body).unwrap();
    assert_eq!(data["reviews"].as_array().unwrap().len(), 1);
    assert_eq!(data["json_ld"]["ratingValue"], 5.0);
    assert_eq!(data["json_ld"]["reviewCount"], 1);
    // 公開の一覧に、投稿者のメールアドレスや IP は出ない
    assert!(!body.contains("hanako@example.com"));
}
