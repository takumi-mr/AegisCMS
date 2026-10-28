//! HTTP ルーティングと `[server].base_path`（リバースプロキシのサブパス配下での公開）

use std::sync::Arc;

use axum::{
    extract::{DefaultBodyLimit, Request},
    http::{uri::PathAndQuery, Uri},
    response::Redirect,
    routing::{delete, get, patch, post, put},
    Router,
};
use tower::ServiceBuilder;
use tower_http::cors::{Any, CorsLayer};
use tower_http::services::{ServeDir, ServeFile};
use tower_http::trace::TraceLayer;

use crate::config::AegisConfig;
use crate::handlers;
use crate::state::AppState;
use aegis_storage::StorageBackendKind;

/// API・公開サイト・静的ファイルのルーター（パスはすべてルート `/` 起点）
pub fn router(config: &AegisConfig, state: Arc<AppState>) -> Router {
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    let mut app = Router::new()
        .route("/health", get(handlers::health_check))
        .route("/api/v1/health", get(handlers::health_check))
        .route("/api/v1/openapi.json", get(handlers::get_openapi_spec))
        .route("/api/v1/schemas", get(handlers::list_schemas))
        .route("/api/v1/schemas/{content_type}", get(handlers::get_schema))
        .route(
            "/api/v1/contents/{content_type}",
            get(handlers::list_content).post(handlers::create_content),
        )
        .route(
            "/api/v1/contents/{content_type}/{slug}",
            get(handlers::get_content)
                .put(handlers::update_content)
                .delete(handlers::delete_content),
        )
        .route(
            "/api/v1/contents/{content_type}/{slug}/versions",
            get(handlers::list_content_versions),
        )
        .route(
            "/api/v1/contents/{content_type}/{slug}/versions/{revision}/restore",
            post(handlers::restore_content_version),
        )
        .route(
            "/api/v1/contents/{content_type}/{id}/variants",
            get(handlers::list_variants).post(handlers::upsert_variant),
        )
        .route(
            "/api/v1/contents/{content_type}/{id}/variants/{variant_id}",
            delete(handlers::delete_variant),
        )
        // --- コメント（[comments]） ---
        .route(
            "/api/v1/contents/{content_type}/{slug}/comments",
            get(handlers::list_public_comments).post(handlers::submit_comment),
        )
        .route(
            "/api/v1/contents/{content_type}/{id}/comments/state",
            get(handlers::get_content_comments).put(handlers::set_content_comments),
        )
        .route("/api/v1/comments", get(handlers::list_comments))
        .route(
            "/api/v1/comments/settings",
            get(handlers::get_comment_settings).put(handlers::update_comment_settings),
        )
        .route("/api/v1/comments/bulk", post(handlers::bulk_comments))
        .route(
            "/api/v1/comments/{id}",
            patch(handlers::update_comment).delete(handlers::delete_comment),
        )
        .route(
            "/api/v1/comments/{id}/replies",
            post(handlers::reply_comment),
        )
        .route("/comments/unsubscribe", get(handlers::unsubscribe_page))
        // --- 掲示板（[forum]） ---
        .route(
            "/api/v1/forum/settings",
            get(handlers::forum::api_get_forum_settings)
                .put(handlers::forum::api_update_forum_settings),
        )
        .route(
            "/api/v1/forum/reports",
            get(handlers::forum::api_list_reports).post(handlers::forum::api_create_report),
        )
        .route(
            "/api/v1/forum/reports/{id}",
            patch(handlers::forum::api_resolve_report),
        )
        .route(
            "/api/v1/forum/categories",
            get(handlers::forum::api_list_categories).post(handlers::forum::api_create_category),
        )
        .route(
            "/api/v1/forum/categories/{slug}",
            get(handlers::forum::api_get_category)
                .put(handlers::forum::api_update_category)
                .delete(handlers::forum::api_delete_category),
        )
        .route(
            "/api/v1/forum/categories/{slug}/threads",
            get(handlers::forum::api_list_threads),
        )
        .route(
            "/api/v1/forum/threads",
            get(handlers::forum::api_list_all_threads).post(handlers::forum::api_create_thread),
        )
        .route(
            "/api/v1/forum/threads/{id}",
            get(handlers::forum::api_get_thread).delete(handlers::forum::api_delete_thread),
        )
        .route(
            "/api/v1/forum/threads/{id}/status",
            patch(handlers::forum::api_moderate_thread),
        )
        .route(
            "/api/v1/forum/threads/{id}/posts",
            get(handlers::forum::api_list_posts),
        )
        .route(
            "/api/v1/forum/posts",
            post(handlers::forum::api_create_post),
        )
        .route(
            "/api/v1/forum/threads/{id}/pin",
            post(handlers::forum::api_pin_thread),
        )
        .route(
            "/api/v1/forum/threads/{id}/lock",
            post(handlers::forum::api_lock_thread),
        )
        .route(
            "/api/v1/forum/posts/{id}/status",
            patch(handlers::forum::api_moderate_post),
        )
        .route(
            "/api/v1/forum/posts/{id}",
            delete(handlers::forum::api_delete_post),
        )
        .route("/api/v1/schedule", get(handlers::list_schedule))
        .route(
            "/api/v1/storage/upload",
            // axum の既定 (2MB) ではなく設定値をアップロードの上限にする
            post(handlers::upload_file)
                .layer(DefaultBodyLimit::max(config.storage.max_upload_bytes())),
        )
        .route("/api/v1/storage/objects", get(handlers::list_objects))
        .route(
            "/api/v1/storage/objects/{*key}",
            delete(handlers::delete_object),
        )
        .route("/api/v1/preview", post(handlers::preview_content))
        .route(
            "/api/v1/auth/session",
            post(handlers::create_session)
                .get(handlers::get_current_session)
                .delete(handlers::delete_session),
        )
        .route("/api/v1/auth/providers", get(handlers::list_auth_providers))
        .route("/api/v1/auth/registration", post(handlers::register))
        .route(
            "/api/v1/auth/registration/verify",
            post(handlers::verify_registration),
        )
        .route(
            "/api/v1/auth/password-reset",
            post(handlers::request_password_reset),
        )
        .route(
            "/api/v1/auth/password-reset/confirm",
            post(handlers::confirm_password_reset),
        )
        .route(
            "/api/v1/auth/invitations",
            post(handlers::create_invitation),
        )
        .route(
            "/api/v1/auth/invitations/info",
            get(handlers::get_invitation_info),
        )
        .route(
            "/api/v1/auth/invitations/accept",
            post(handlers::accept_invitation),
        )
        .route("/api/v1/auth/oidc/login", get(handlers::oidc_login))
        .route("/api/v1/auth/oidc/callback", get(handlers::oidc_callback))
        .route("/api/v1/auth/saml/login", get(handlers::saml_login))
        .route("/api/v1/auth/saml/acs", post(handlers::saml_acs))
        .route("/api/v1/auth/saml/metadata", get(handlers::saml_metadata))
        .route("/api/v1/audit-logs", get(handlers::list_audit_logs))
        .route(
            "/api/v1/users",
            get(handlers::list_users).post(handlers::create_user),
        )
        .route(
            "/api/v1/users/{username}",
            get(handlers::get_user)
                .patch(handlers::update_user)
                .delete(handlers::delete_user),
        )
        .route(
            "/api/v1/users/{username}/avatar",
            // multipart のヘッダー分の余裕を持たせる（画像そのものの上限はハンドラーで確かめる）
            put(handlers::upload_avatar)
                .delete(handlers::delete_avatar)
                .layer(DefaultBodyLimit::max(handlers::users::MAX_AVATAR_BYTES + 64 * 1024)),
        )
        // --- EC（[commerce]） ---
        .route("/api/v1/shop/config", get(handlers::shop_config))
        .route("/api/v1/shop/quote", post(handlers::quote))
        .route("/api/v1/shop/checkout", post(handlers::checkout))
        .route("/api/v1/shop/orders/{id}", get(handlers::get_public_order))
        .route(
            "/api/v1/shop/products/{id}/variants",
            get(handlers::get_public_product_variants),
        )
        .route(
            "/api/v1/shop/webhooks/{provider}",
            post(handlers::payment_webhook),
        )
        .route("/api/v1/orders", get(handlers::list_orders))
        .route("/api/v1/orders/report", get(handlers::sales_report))
        .route("/api/v1/orders/{id}", get(handlers::get_order))
        .route(
            "/api/v1/orders/{id}/status",
            post(handlers::update_order_status),
        )
        .route(
            "/api/v1/orders/{id}/refund",
            post(handlers::refund_order),
        )
        // --- 顧客アカウント（EC 顧客・AegisCMS スタッフとは別管理） ---
        .route("/api/v1/customers/register", post(handlers::customer_register))
        .route("/api/v1/customers/login", post(handlers::customer_login))
        .route("/api/v1/customers/logout", post(handlers::customer_logout))
        .route(
            "/api/v1/customers/me",
            get(handlers::get_me).put(handlers::update_me),
        )
        .route("/api/v1/customers/orders", get(handlers::list_my_orders))
        .route("/api/v1/customers/orders/{id}", get(handlers::get_my_order))
        .route("/api/v1/customers/reviews", get(handlers::list_my_reviews))
        .route("/api/v1/admin/customers", get(handlers::admin_list_customers))
        .route(
            "/api/v1/admin/customers/{id}",
            get(handlers::admin_get_customer).patch(handlers::admin_set_customer_status),
        )
        // --- 商品レビュー（購入者限定・ステマ規制配慮・モデレーション） ---
        .route(
            "/api/v1/products/{product_id}/reviews",
            get(handlers::list_product_reviews).post(handlers::submit_product_review),
        )
        .route("/api/v1/admin/reviews", get(handlers::admin_list_reviews))
        .route(
            "/api/v1/admin/reviews/settings",
            get(handlers::admin_get_review_settings).put(handlers::admin_put_review_settings),
        )
        .route("/api/v1/admin/reviews/bulk", post(handlers::admin_bulk_reviews))
        .route(
            "/api/v1/admin/reviews/{id}",
            get(handlers::admin_get_review).patch(handlers::admin_patch_review),
        )
        .route(
            "/api/v1/admin/reviews/{id}/reply",
            post(handlers::admin_set_reply).delete(handlers::admin_delete_reply),
        )
        .route("/api/v1/plugins", get(handlers::list_plugins))
        .route("/api/v1/plugins/install", post(handlers::install_plugin))
        .route(
            "/api/v1/plugins/order",
            get(handlers::get_plugin_order)
                .put(handlers::set_plugin_order)
                .delete(handlers::reset_plugin_order),
        )
        .route(
            "/api/v1/plugins/{name}/settings",
            get(handlers::get_plugin_settings).put(handlers::update_plugin_settings),
        )
        .route(
            "/api/v1/plugins/{name}/enabled",
            put(handlers::set_plugin_enabled),
        )
        .route(
            "/api/v1/site",
            get(handlers::get_site_settings).put(handlers::update_site_settings),
        )
        .route(
            "/api/v1/seo",
            get(handlers::get_seo_settings).put(handlers::update_seo_settings),
        )
        .route(
            "/api/v1/headers",
            get(handlers::get_headers_settings).put(handlers::update_headers_settings),
        )
        // --- 通知（[notifications]） ---
        .route(
            "/api/v1/notifications",
            get(handlers::get_notification_status),
        )
        .route(
            "/api/v1/notifications/queue",
            get(handlers::list_notification_queue),
        )
        .route(
            "/api/v1/notifications/queue/{id}",
            delete(handlers::delete_queued_notification),
        )
        .route(
            "/api/v1/notifications/queue/{id}/retry",
            post(handlers::retry_queued_notification),
        )
        .route(
            "/api/v1/notifications/templates",
            get(handlers::list_notification_templates),
        )
        .route(
            "/api/v1/notifications/templates/{name}",
            get(handlers::get_notification_template)
                .put(handlers::save_notification_template)
                .delete(handlers::reset_notification_template),
        )
        .route(
            "/api/v1/notifications/preview",
            post(handlers::preview_notification),
        )
        .route(
            "/api/v1/notifications/test",
            post(handlers::send_test_notification),
        )
        .route("/api/v1/themes", get(handlers::list_themes))
        .route("/api/v1/themes/install", post(handlers::install_theme))
        .route(
            "/api/v1/themes/active",
            get(handlers::get_active_theme).put(handlers::switch_active_theme),
        )
        .route(
            "/themes/{theme_id}/preview",
            get(handlers::get_theme_preview),
        )
        .route(
            "/themes/{theme_id}/assets/{*path}",
            get(handlers::get_specific_theme_asset),
        )
        .route(
            "/theme/assets/{*path}",
            get(handlers::get_active_theme_asset),
        )
        .route("/metrics", get(handlers::get_metrics))
        // --- 公開エンドポイント（robots.txt, sitemap.xml, テンプレートレンダリング） ---
        .route("/robots.txt", get(handlers::get_robots_txt))
        .route("/sitemap.xml", get(handlers::get_sitemap_xml))
        .route("/", get(handlers::render_index))
        .route("/articles", get(handlers::render_index))
        .route("/articles/{slug}", get(handlers::render_article))
        .route("/products", get(handlers::render_products))
        .route("/products/{slug}", get(handlers::render_product))
        .route("/cart", get(handlers::render_cart))
        .route("/orders/{id}", get(handlers::render_order))
        .route("/shop/cart.js", get(handlers::cart_script))
        .route("/forum", get(handlers::forum::render_forum_index))
        .route(
            "/forum/{category_slug}",
            get(handlers::forum::render_forum_category),
        )
        .route(
            "/forum/{category_slug}/threads",
            post(handlers::forum::handle_create_thread_form),
        )
        .route(
            "/forum/{category_slug}/{thread_id}",
            get(handlers::forum::render_forum_thread),
        )
        .route(
            "/forum/{category_slug}/{thread_id}/posts",
            post(handlers::forum::handle_create_post_form),
        )
        .route(
            "/forum/{category_slug}/{thread_id}/report",
            post(handlers::forum::handle_create_report_form),
        )
        .route("/{slug}", get(handlers::render_page_or_slug));

    // ローカルストレージの場合、/uploads で静的ファイル配信
    if config.storage.backend == StorageBackendKind::Local {
        let _ = std::fs::create_dir_all(&config.storage.local.upload_dir);
        let prefix = config.storage.local.public_url_prefix.clone();
        app = app.nest_service(&prefix, ServeDir::new(&config.storage.local.upload_dir));
    }

    // Admin Studio SPA 配信 (/admin/)。ビルドはアセットを相対パス（./assets/）で参照するため、
    // ベースパスに関係なく /admin/assets/* から配信される（/admin はスラッシュ付きへリダイレクト）
    if config.paths.admin_dist_dir.exists() {
        let index_file = config.paths.admin_dist_dir.join("index.html");
        let admin_serve = ServeDir::new(&config.paths.admin_dist_dir)
            .not_found_service(ServeFile::new(index_file));
        app = app.nest_service("/admin", admin_serve);
    }

    app.layer(cors)
        .layer(axum::middleware::from_fn_with_state(
            state.clone(),
            security_headers_middleware,
        ))
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}

async fn security_headers_middleware(
    axum::extract::State(state): axum::extract::State<Arc<AppState>>,
    request: Request,
    next: axum::middleware::Next,
) -> axum::response::Response {
    let mut response = next.run(request).await;
    let headers_config = state.headers();
    headers_config.apply_to_headers(response.headers_mut());
    response
}

/// `router` を `base_path`（`ServerConfig::normalized_base_path` の値）配下で公開する。
///
/// `{base_path}/...` へのリクエストはベースパスを取り除いてから `router` に渡す。ベースパスの無い
/// リクエストもそのまま受け付けるため、プロキシがパスを取り除く設定でも、コンテナのヘルスチェック
/// （`/api/v1/health`）のようにサーバーへ直接アクセスする場合でも動く。
pub fn with_base_path(router: Router, base_path: &str) -> Router {
    let admin_index = format!("{}/admin/", base_path);
    let redirect_to_admin = get(move || async move { Redirect::temporary(&admin_index) });
    let mut app = Router::new().route("/admin", redirect_to_admin.clone());
    if !base_path.is_empty() {
        app = app.route(&format!("{}/admin", base_path), redirect_to_admin);
    }
    let base_path = base_path.to_string();
    app.fallback_service(
        ServiceBuilder::new()
            .map_request(move |req: Request| strip_base_path(&base_path, req))
            .service(router),
    )
}

/// ルート起点のパス（`/admin/`、`/uploads` など）にベースパスを付ける。絶対 URL（`https://...`、
/// `//cdn...`）はそのまま返す
pub fn under_base_path(base_path: &str, url: &str) -> String {
    if url.starts_with('/') && !url.starts_with("//") {
        format!("{}{}", base_path, url)
    } else {
        url.to_string()
    }
}

/// リクエストの URI から先頭の `base_path` を取り除く（`/cms` → `/`、`/cms/a?x=1` → `/a?x=1`）
fn strip_base_path(base_path: &str, mut req: Request) -> Request {
    if base_path.is_empty() {
        return req;
    }
    let Some(rest) = req.uri().path().strip_prefix(base_path) else {
        return req;
    };
    if !rest.is_empty() && !rest.starts_with('/') {
        // `/cms` に対する `/cmsx` のような別のパス
        return req;
    }
    let path = if rest.is_empty() { "/" } else { rest };
    let path_and_query = match req.uri().query() {
        Some(query) => format!("{}?{}", path, query),
        None => path.to_string(),
    };
    let mut parts = req.uri().clone().into_parts();
    let Ok(path_and_query) = PathAndQuery::try_from(path_and_query) else {
        return req;
    };
    parts.path_and_query = Some(path_and_query);
    if let Ok(uri) = Uri::from_parts(parts) {
        *req.uri_mut() = uri;
    }
    req
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::{header, StatusCode};
    use tower::ServiceExt;

    fn echo_router() -> Router {
        Router::new()
            .route("/", get(|| async { "index" }))
            .route("/{slug}", get(|uri: Uri| async move { uri.to_string() }))
            .route(
                "/api/v1/health",
                get(|uri: Uri| async move { uri.to_string() }),
            )
    }

    async fn get_text(app: &Router, path: &str) -> (StatusCode, String, Option<String>) {
        let res = app
            .clone()
            .oneshot(Request::get(path).body(Body::empty()).unwrap())
            .await
            .unwrap();
        let status = res.status();
        let location = res
            .headers()
            .get(header::LOCATION)
            .map(|v| v.to_str().unwrap().to_string());
        let body = axum::body::to_bytes(res.into_body(), 1 << 16)
            .await
            .unwrap();
        (status, String::from_utf8(body.to_vec()).unwrap(), location)
    }

    #[test]
    fn test_under_base_path() {
        assert_eq!(under_base_path("", "/admin/"), "/admin/");
        assert_eq!(under_base_path("/cms", "/admin/"), "/cms/admin/");
        assert_eq!(under_base_path("/cms", "/uploads"), "/cms/uploads");
        assert_eq!(
            under_base_path("/cms", "https://admin.example.com/admin/"),
            "https://admin.example.com/admin/"
        );
        assert_eq!(
            under_base_path("/cms", "//cdn.example.com/u"),
            "//cdn.example.com/u"
        );
    }

    #[tokio::test]
    async fn test_base_path_is_stripped_before_routing() {
        let app = with_base_path(echo_router(), "/sites/cms");
        assert_eq!(get_text(&app, "/sites/cms").await.1, "index");
        assert_eq!(get_text(&app, "/sites/cms/").await.1, "index");
        assert_eq!(
            get_text(&app, "/sites/cms/api/v1/health?verbose=1").await.1,
            "/api/v1/health?verbose=1"
        );
        assert_eq!(get_text(&app, "/sites/cms/about").await.1, "/about");
        // ベースパスの無いリクエスト（パスを取り除くプロキシ・直接のヘルスチェック）
        assert_eq!(get_text(&app, "/api/v1/health").await.1, "/api/v1/health");
        // 前方一致するだけの別のパスは取り除かない
        let app = with_base_path(echo_router(), "/cms");
        assert_eq!(get_text(&app, "/cmsx").await.1, "/cmsx");
        assert_eq!(get_text(&app, "/cms/cmsx").await.1, "/cmsx");
    }

    #[tokio::test]
    async fn test_admin_redirects_to_trailing_slash_under_base_path() {
        for (base, path, expected) in [
            ("", "/admin", "/admin/"),
            ("/cms", "/cms/admin", "/cms/admin/"),
            ("/cms", "/admin", "/cms/admin/"),
        ] {
            let app = with_base_path(echo_router(), base);
            let (status, _, location) = get_text(&app, path).await;
            assert_eq!(status, StatusCode::TEMPORARY_REDIRECT, "{}", path);
            assert_eq!(location.as_deref(), Some(expected), "{}", path);
        }
    }
}
