//! 公開サイトのショップページ（テーマのテンプレート `products.html` / `product.html` /
//! `cart.html` / `order.html`。テーマに無ければ組み込みのテンプレート）とカートのスクリプト

use axum::{
    extract::{Path, Query, State},
    http::{header, StatusCode},
    response::{IntoResponse, Response},
};
use std::collections::HashMap;
use std::sync::Arc;

use super::public::{extract_excerpt, render_not_found, themed_response, IndexParams};
use crate::render;
use crate::state::AppState;
use aegis_ast::BlockNode;
use aegis_core::{ContentSummary, PageRenderContext, Pagination};
use aegis_db::{resolve_relations_for_records, ContentListQuery};

/// カート・注文確認ページのスクリプト（`/shop/cart.js`）
const CART_SCRIPT: &str = include_str!("../../assets/cart.js");

/// テンプレートの `theme_vars` に渡すショップの情報
fn shop_vars(state: &AppState) -> HashMap<String, String> {
    let base_path = state
        .config
        .server
        .normalized_base_path()
        .unwrap_or_default();
    let commerce = &state.config.commerce;
    HashMap::from([
        (
            "shop_enabled".to_string(),
            state.commerce.is_some().to_string(),
        ),
        ("currency".to_string(), commerce.currency.clone()),
        (
            "products_url".to_string(),
            format!("{}/products", base_path),
        ),
        ("cart_url".to_string(), format!("{}/cart", base_path)),
        ("shop_api".to_string(), format!("{}/api/v1/shop", base_path)),
        (
            "cart_script".to_string(),
            format!("{}/shop/cart.js", base_path),
        ),
    ])
}

/// ショップのページを描画する（`on-render-page` フックを適用）
async fn render_shop_page(state: &AppState, mut ctx: PageRenderContext) -> Response {
    ctx.site_title = state.config.theme.site_title.clone();
    ctx.site_description = state.config.theme.site_description.clone();
    ctx.extra_css = state.config.theme.custom_css.clone().unwrap_or_default();
    ctx.theme_vars.extend(shop_vars(state));
    {
        let supervisor = state.supervisor.read().await;
        if let Ok(hooked) = supervisor.apply_render_page(ctx.clone()).await {
            ctx = hooked;
        }
    }
    themed_response(
        state
            .theme
            .render(&ctx.template, &ctx)
            .map_err(|e| e.to_string()),
    )
}

async fn product_schema(state: &AppState) -> Option<aegis_schema::SchemaDefinition> {
    state
        .schemas
        .read()
        .await
        .get(&state.config.commerce.product_type)
        .cloned()
}

/// `GET /products` — 公開中の商品一覧（`?page=N`、`[theme].posts_per_page` 件ずつ）
pub async fn render_products(
    State(state): State<Arc<AppState>>,
    Query(params): Query<IndexParams>,
) -> Response {
    let Some(schema) = product_schema(&state).await else {
        return render_not_found(&state, "This site has no products.").await;
    };
    let per_page = state.config.theme.posts_per_page.max(1);
    let page = params.page.unwrap_or(1).max(1);
    let query = ContentListQuery {
        status: Some("published".to_string()),
        limit: Some(per_page),
        offset: (page - 1).saturating_mul(per_page),
    };
    let content_page = match state.db.list_contents_page(&schema, &query).await {
        Ok(content_page) => content_page,
        Err(e) => {
            tracing::error!("Failed to list products: {}", e);
            return render_not_found(&state, "Products are not available right now.").await;
        }
    };
    let total_pages = content_page.total.div_ceil(per_page).max(1);
    if page > total_pages {
        return render_not_found(&state, "This page does not exist.").await;
    }
    let relations = {
        let schemas = state.schemas.read().await;
        let fields: Vec<_> = content_page
            .records
            .iter()
            .map(|r| &r.custom_fields)
            .collect();
        resolve_relations_for_records(&*state.db, &schemas, &schema, &fields).await
    };
    let items = content_page
        .records
        .into_iter()
        .zip(relations)
        .map(|(r, relations)| {
            let blocks: Vec<BlockNode> = serde_json::from_str(&r.blocks_json).unwrap_or_default();
            ContentSummary {
                id: r.id,
                content_type: r.content_type,
                title: r.title,
                slug: r.slug,
                excerpt: extract_excerpt(&blocks, 120),
                author_id: r.author_id,
                published_at: r.published_at,
                created_at: r.created_at,
                fields: r.custom_fields,
                relations: relations
                    .iter()
                    .map(|(name, rel)| (name.clone(), render::relation_summary(rel)))
                    .collect(),
            }
        })
        .collect();
    let base_path = state
        .config
        .server
        .normalized_base_path()
        .unwrap_or_default();
    let page_url = |n: u64| {
        if n <= 1 {
            format!("{}/products", base_path)
        } else {
            format!("{}/products?page={}", base_path, n)
        }
    };
    let ctx = PageRenderContext {
        template: "products.html".to_string(),
        content_type: schema.content_type.clone(),
        title: "Products".to_string(),
        items,
        pagination: Some(Pagination {
            page,
            per_page,
            total_items: content_page.total,
            total_pages,
            prev_url: (page > 1).then(|| page_url(page - 1)),
            next_url: (page < total_pages).then(|| page_url(page + 1)),
        }),
        ..Default::default()
    };
    render_shop_page(&state, ctx).await
}

/// `GET /products/{slug}` — 商品ページ
pub async fn render_product(
    State(state): State<Arc<AppState>>,
    Path(slug): Path<String>,
) -> Response {
    let Some(schema) = product_schema(&state).await else {
        return render_not_found(&state, "This site has no products.").await;
    };
    let record = match state.db.get_content(&schema, &slug).await {
        Ok(Some(record)) if record.status == "published" => record,
        Ok(_) => return render_not_found(&state, "The requested product is not available.").await,
        Err(e) => {
            tracing::error!("Database error fetching product '{}': {}", slug, e);
            return render_not_found(&state, "Error querying database").await;
        }
    };
    let blocks: Vec<BlockNode> = serde_json::from_str(&record.blocks_json).unwrap_or_default();
    let relations = {
        let schemas = state.schemas.read().await;
        resolve_relations_for_records(&*state.db, &schemas, &schema, &[&record.custom_fields])
            .await
            .pop()
            .unwrap_or_default()
            .iter()
            .map(|(name, rel)| (name.clone(), render::relation_summary(rel)))
            .collect()
    };
    let ctx = PageRenderContext {
        template: "product.html".to_string(),
        content_type: schema.content_type.clone(),
        title: record.title,
        slug: record.slug,
        content_html: render::render_blocks_html(&state.config.security, &blocks),
        published_at: record.published_at,
        updated_at: Some(record.updated_at),
        fields: {
            let mut fields = record.custom_fields;
            fields.insert("id".to_string(), serde_json::json!(record.id));
            fields
        },
        relations,
        ..Default::default()
    };
    render_shop_page(&state, ctx).await
}

/// `GET /cart` — カートと購入手続き（内容はブラウザのスクリプトが描画する）
pub async fn render_cart(State(state): State<Arc<AppState>>) -> Response {
    let ctx = PageRenderContext {
        template: "cart.html".to_string(),
        content_type: "cart".to_string(),
        title: "Cart".to_string(),
        slug: "cart".to_string(),
        ..Default::default()
    };
    render_shop_page(&state, ctx).await
}

/// `GET /orders/{id}?token=...` — 注文確認ページ（内容はスクリプトが API から取得する）
pub async fn render_order(State(state): State<Arc<AppState>>, Path(id): Path<String>) -> Response {
    if state.commerce.is_none() {
        return render_not_found(&state, "The shop is not enabled.").await;
    }
    let ctx = PageRenderContext {
        template: "order.html".to_string(),
        content_type: "order".to_string(),
        title: "Your order".to_string(),
        slug: id,
        ..Default::default()
    };
    let mut response = render_shop_page(&state, ctx).await;
    // URL のトークンを外部サイトへ Referer で渡さない
    response.headers_mut().insert(
        header::REFERRER_POLICY,
        header::HeaderValue::from_static("no-referrer"),
    );
    response
}

/// `GET /shop/cart.js` — カート（`localStorage`）・購入手続き・注文確認のスクリプト
pub async fn cart_script() -> impl IntoResponse {
    (
        StatusCode::OK,
        [
            (
                header::CONTENT_TYPE,
                "application/javascript; charset=utf-8",
            ),
            (header::CACHE_CONTROL, "public, max-age=300"),
        ],
        CART_SCRIPT,
    )
}
