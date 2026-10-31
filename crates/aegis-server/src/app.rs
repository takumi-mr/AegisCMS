//! HTTP ルーティングと `[server].base_path`（リバースプロキシのサブパス配下での公開）

use std::sync::Arc;

use axum::{
    extract::{DefaultBodyLimit, Request},
    http::{uri::PathAndQuery, StatusCode, Uri},
    response::{IntoResponse, Redirect},
    routing::{any, delete, get, patch, post, put},
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
        // --- タグ（[tags]） ---
        .route(
            "/api/v1/tags",
            get(handlers::list_tags).post(handlers::create_tag),
        )
        .route(
            "/api/v1/tags/{id}",
            put(handlers::update_tag).delete(handlers::delete_tag),
        )
        .route(
            "/api/v1/contents/{content_type}/{slug}/tags",
            get(handlers::get_content_tags).put(handlers::set_content_tag_list),
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
        // --- ニュースレター（[newsletter]） ---
        .route(
            "/newsletter",
            get(handlers::newsletter::render_newsletter_page),
        )
        .route(
            "/newsletter/subscribe",
            post(handlers::newsletter::submit_subscribe_form),
        )
        .route(
            "/newsletter/confirm",
            get(handlers::newsletter::show_confirm).post(handlers::newsletter::do_confirm),
        )
        .route(
            "/newsletter/unsubscribe",
            get(handlers::newsletter::show_unsubscribe).post(handlers::newsletter::do_unsubscribe),
        )
        .route(
            "/api/v1/newsletter/subscribe",
            post(handlers::newsletter::subscribe_api),
        )
        .route(
            "/api/v1/newsletter/subscribers",
            get(handlers::newsletter::list_subscribers),
        )
        .route(
            "/api/v1/newsletter/subscribers/delete",
            post(handlers::newsletter::delete_subscribers),
        )
        .route(
            "/api/v1/newsletter/subscribers/export",
            get(handlers::newsletter::export_subscribers),
        )
        .route(
            "/api/v1/newsletter/issues",
            get(handlers::newsletter::list_issues).post(handlers::newsletter::send_issue),
        )
        // --- 問い合わせフォーム（[contact]） ---
        .route("/contact", get(handlers::contact::render_default_contact))
        .route(
            "/contact/{form}",
            get(handlers::contact::render_contact)
                .post(handlers::contact::submit_contact_form)
                // 添付ファイルのある multipart（大きさそのものの上限は項目ごとに確かめる）
                .layer(DefaultBodyLimit::max(
                    config.contact.max_submission_bytes().max(2 * 1024 * 1024),
                )),
        )
        .route("/api/v1/contact/forms", get(handlers::contact::list_forms))
        .route(
            "/api/v1/contact/forms/{form}",
            post(handlers::contact::submit_contact_api),
        )
        .route(
            "/api/v1/contact/messages",
            get(handlers::contact::list_messages),
        )
        .route(
            "/api/v1/contact/messages/bulk",
            post(handlers::contact::bulk_messages),
        )
        .route(
            "/api/v1/contact/messages/{id}/attachments/{field}",
            get(handlers::contact::download_attachment),
        )
        .route(
            "/api/v1/contact/messages/{id}",
            get(handlers::contact::get_message)
                .patch(handlers::contact::update_message)
                .delete(handlers::contact::delete_message),
        )
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
                .layer(DefaultBodyLimit::max(std::cmp::max(config.storage.max_upload_bytes(), 1024 * 1024 * 1024))),
        )
        .route("/api/v1/storage/objects", get(handlers::list_objects))
        .route(
            "/api/v1/storage/objects/{*key}",
            delete(handlers::delete_object),
        )
        .route("/api/v1/preview", post(handlers::preview_content))
        .route("/api/v1/blocks", get(handlers::list_blocks))
        .route(
            "/api/v1/auth/session",
            post(handlers::create_session)
                .get(handlers::get_current_session)
                .delete(handlers::delete_session),
        )
        .route(
            "/api/v1/auth/session/verify-2fa",
            post(handlers::verify_2fa),
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
        .route("/api/v1/users/me/2fa/setup", post(handlers::setup_2fa))
        .route("/api/v1/users/me/2fa/enable", post(handlers::enable_2fa))
        .route("/api/v1/users/me/2fa/disable", post(handlers::disable_2fa))
        .route(
            "/api/v1/users/{username}/2fa/reset",
            post(handlers::reset_user_2fa),
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
            "/api/v1/system/settings",
            get(handlers::get_system_settings).put(handlers::update_system_settings),
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
        // --- リダイレクト ---
        .route(
            "/api/v1/redirects",
            get(handlers::redirects::list_redirects).post(handlers::redirects::create_redirect),
        )
        .route(
            "/api/v1/redirects/{id}",
            put(handlers::redirects::update_redirect).delete(handlers::redirects::delete_redirect),
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
        // --- 公開エンドポイント（robots.txt, sitemap.xml, フィード, テンプレートレンダリング） ---
        .route("/robots.txt", get(handlers::get_robots_txt))
        .route("/sitemap.xml", get(handlers::get_sitemap_xml))
        .route("/feed.xml", get(handlers::get_feed))
        .route("/", get(handlers::render_index))
        .route("/articles", get(handlers::render_articles))
        .route("/articles/{slug}", get(handlers::render_article))
        .route("/category/{slug}", get(handlers::render_category))
        .route("/category/{slug}/feed.xml", get(handlers::get_category_feed))
        .route("/tags", get(handlers::render_tags_index))
        .route("/tags/{slug}", get(handlers::render_tag))
        .route("/tags/{slug}/feed.xml", get(handlers::get_tag_feed))
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
        // --- WordPress 攻撃・走査ボット用ハニーポット ---
        .route("/wp-login.php", any(handlers::honeypot_trap))
        .route("/xmlrpc.php", any(handlers::honeypot_trap))
        .route("/wp-admin", any(handlers::honeypot_trap))
        .route("/wp-admin/{*path}", any(handlers::honeypot_trap))
        .route("/wp-includes/{*path}", any(handlers::honeypot_trap))
        .route("/wp-content/{*path}", any(handlers::honeypot_trap))
        .route("/wp-json", any(handlers::honeypot_trap))
        .route("/wp-json/{*path}", any(handlers::honeypot_trap))
        .route("/{slug}", get(handlers::render_page_or_slug));

    // サイト内検索（[search]、feature `search`）。無効なら 404 を返す
    #[cfg(feature = "search")]
    {
        app = app
            .route("/search", get(crate::search::render_search_page))
            .route("/api/v1/search", get(crate::search::search_api));
    }

    // アクセス解析（[analytics]、feature `analytics`）。無効なら 404 を返す
    #[cfg(feature = "analytics")]
    {
        app = app
            .route(
                "/api/v1/analytics/event",
                axum::routing::post(crate::analytics::track_event),
            )
            .route(
                "/api/v1/analytics/summary",
                get(crate::analytics::get_summary),
            )
            .route(
                "/api/v1/analytics/export",
                get(crate::analytics::export_csv),
            );
    }

    // ローカルストレージの場合、/uploads で静的ファイル配信
    if config.storage.backend == StorageBackendKind::Local {
        let _ = std::fs::create_dir_all(&config.storage.local.upload_dir);
        let prefix = config.storage.local.public_url_prefix.clone();
        // 問い合わせの添付ファイル（`contact/`）は公開しない。Admin Studio の API からだけ取り出せる
        let private_prefix = format!("{}/contact/", prefix.trim_end_matches('/')).to_lowercase();
        let uploads = Router::new()
            .fallback_service(ServeDir::new(&config.storage.local.upload_dir))
            .layer(axum::middleware::from_fn_with_state(
                private_prefix,
                hide_private_uploads,
            ));
        app = app.nest_service(&prefix, uploads);
    }

    // Admin Studio SPA 配信（`[server].admin_path`、既定 /admin/）。ビルドはアセットを相対パス（./assets/）で
    // 参照するため、ベースパスに関係なく {admin_path}/assets/* から配信される（スラッシュ無しはリダイレクト）
    if config.paths.admin_dist_dir.exists() {
        let index_file = config.paths.admin_dist_dir.join("index.html");
        let admin_serve = ServeDir::new(&config.paths.admin_dist_dir)
            .not_found_service(ServeFile::new(index_file));
        app = app.nest_service(&config.server.admin_path(), admin_serve);
    }

    app.layer(cors)
        .layer(axum::middleware::from_fn_with_state(
            state.clone(),
            redirect_middleware,
        ))
        .layer(axum::middleware::from_fn_with_state(
            state.clone(),
            security_headers_middleware,
        ))
        .layer(axum::middleware::from_fn_with_state(
            state.clone(),
            maintenance_middleware,
        ))
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}

/// `/uploads/contact/...`（問い合わせの添付ファイル）を 404 にする
async fn hide_private_uploads(
    axum::extract::State(private_prefix): axum::extract::State<String>,
    request: Request,
    next: axum::middleware::Next,
) -> axum::response::Response {
    let path = request
        .extensions()
        .get::<axum::extract::OriginalUri>()
        .map(|uri| uri.0.path())
        .unwrap_or_else(|| request.uri().path());
    // `%63ontact` や `Contact`（大文字・小文字を区別しないファイルシステム）でも抜けないように、
    // パーセントエンコードを戻して小文字にしてから確かめる。ベースパス配下でも合うよう部分一致にする
    let decoded = percent_decode(path).to_lowercase();
    if decoded.contains(&private_prefix) {
        return axum::http::StatusCode::NOT_FOUND.into_response();
    }
    next.run(request).await
}

fn percent_decode(path: &str) -> String {
    fn hex(b: u8) -> Option<u8> {
        (b as char).to_digit(16).map(|d| d as u8)
    }
    let bytes = path.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let (Some(h), Some(l)) = (hex(bytes[i + 1]), hex(bytes[i + 2])) {
                out.push(h * 16 + l);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

async fn maintenance_middleware(
    axum::extract::State(state): axum::extract::State<Arc<AppState>>,
    request: Request,
    next: axum::middleware::Next,
) -> axum::response::Response {
    let system = state.system();
    if !system.maintenance_mode {
        return next.run(request).await;
    }

    let path = request.uri().path();
    let admin_path = state.config.server.admin_path();
    if path
        .strip_prefix(admin_path.as_str())
        .is_some_and(|rest| rest.is_empty() || rest.starts_with('/'))
        || path.starts_with("/api/")
        || path.starts_with("/uploads/")
        || path.starts_with("/theme/")
        || path.starts_with("/themes/")
        || path == "/health"
        || path == "/metrics"
        || path == "/robots.txt"
        || path == "/sitemap.xml"
        || path == "/favicon.ico"
    {
        return next.run(request).await;
    }

    if let Some(session_id) = crate::auth::extract_session_id(request.headers()) {
        if let Ok(Some(s)) = state.sessions.get_session(&session_id).await {
            if matches!(s.role.as_str(), "admin" | "editor") {
                return next.run(request).await;
            }
        }
    }

    let message = aegis_ast::renderer::escape_html(&system.maintenance_message);
    let title = state.site().title;
    let title = aegis_ast::renderer::escape_html(&title);
    let locale = state.config.theme.locale.as_str();
    let suffix = aegis_theme::messages::translate(locale, "maintenance.title", &[]);
    let html = format!(
        r#"<!DOCTYPE html>
<html lang="{}">
<head>
    <meta charset="utf-8">
    <meta name="viewport" content="width=device-width, initial-scale=1">
    <title>{} - {}</title>
    <style>
        body {{ font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, sans-serif; display: flex; align-items: center; justify-content: center; min-height: 100vh; margin: 0; background-color: #f8fafc; color: #1e293b; }}
        .box {{ background: white; padding: 2.5rem; border-radius: 0.75rem; box-shadow: 0 4px 6px -1px rgba(0, 0, 0, 0.1); max-width: 480px; width: 90%; text-align: center; }}
        h1 {{ font-size: 1.5rem; font-weight: 600; margin-top: 0; margin-bottom: 1rem; color: #0f172a; }}
        p {{ line-height: 1.6; color: #64748b; margin-bottom: 0; }}
    </style>
</head>
<body>
    <div class="box">
        <h1>{}</h1>
        <p>{}</p>
    </div>
</body>
</html>"#,
        locale, title, suffix, title, message
    );

    let mut response = axum::response::Html(html).into_response();
    *response.status_mut() = StatusCode::SERVICE_UNAVAILABLE;
    response.headers_mut().insert(
        axum::http::header::RETRY_AFTER,
        axum::http::HeaderValue::from_static("300"),
    );
    response
}

/// ページが 404 になったとき、リダイレクトの規則（[`crate::redirects`]）に一致すれば 301 / 302 で転送する。
/// 生きているページの URL は規則があっても奪わない。クエリは、転送先に無ければそのまま引き継ぐ
async fn redirect_middleware(
    axum::extract::State(state): axum::extract::State<Arc<AppState>>,
    request: Request,
    next: axum::middleware::Next,
) -> axum::response::Response {
    let lookup = matches!(
        *request.method(),
        axum::http::Method::GET | axum::http::Method::HEAD
    )
    .then(|| {
        (
            request.uri().path().to_string(),
            request.uri().query().map(str::to_string),
        )
    });
    let response = next.run(request).await;
    if response.status() != StatusCode::NOT_FOUND {
        return response;
    }
    let Some((path, query)) = lookup else {
        return response;
    };
    let rule = state.redirects.read().await.resolve(&path).cloned();
    let Some(rule) = rule else {
        return response;
    };

    let base_path = state
        .config
        .server
        .normalized_base_path()
        .unwrap_or_default();
    let mut target = under_base_path(&base_path, &rule.to);
    if let Some(query) = query.filter(|q| !q.is_empty() && !rule.to.contains('?')) {
        let (before, fragment) = match target.split_once('#') {
            Some((before, fragment)) => (before.to_string(), format!("#{}", fragment)),
            None => (target.clone(), String::new()),
        };
        target = format!("{}?{}{}", before, query, fragment);
    }
    let Ok(location) =
        axum::http::HeaderValue::from_str(&crate::redirects::encode_location(&target))
    else {
        return response;
    };
    let status = if rule.status == 302 {
        StatusCode::FOUND
    } else {
        StatusCode::MOVED_PERMANENTLY
    };
    let mut redirect = axum::response::Response::new(axum::body::Body::empty());
    *redirect.status_mut() = status;
    redirect
        .headers_mut()
        .insert(axum::http::header::LOCATION, location);
    redirect.headers_mut().insert(
        axum::http::header::CACHE_CONTROL,
        axum::http::HeaderValue::from_static(if status == StatusCode::FOUND {
            "no-cache"
        } else {
            "public, max-age=3600"
        }),
    );
    redirect
}

async fn security_headers_middleware(
    axum::extract::State(state): axum::extract::State<Arc<AppState>>,
    request: Request,
    next: axum::middleware::Next,
) -> axum::response::Response {
    let path = request.uri().path().to_string();
    let mut response = next.run(request).await;
    let headers_config = state.headers();
    // 有効なプラグインのカスタムブロックが埋め込む iframe の配信元を CSP の frame-src に足す
    let frame_hosts = state.supervisor.read().await.frame_domains();
    let mut captcha_hosts = state
        .contact
        .as_ref()
        .map(|c| c.captcha_csp_hosts())
        .unwrap_or_default();
    if let Some(newsletter) = &state.newsletter {
        for host in newsletter.captcha_csp_hosts() {
            if !captcha_hosts.contains(&host) {
                captcha_hosts.push(host);
            }
        }
    }
    headers_config.apply_to_headers_with_sources(
        response.headers_mut(),
        &frame_hosts,
        &captcha_hosts,
    );

    // SVG ドキュメントとして直接開かれた場合の XSS 防止: スクリプト実行を完全遮断
    let is_svg = path.ends_with(".svg")
        || response
            .headers()
            .get(axum::http::header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .is_some_and(|ct| ct.starts_with("image/svg+xml"));
    if is_svg {
        response.headers_mut().insert(
            axum::http::header::CONTENT_SECURITY_POLICY,
            axum::http::HeaderValue::from_static("default-src 'none'; style-src 'unsafe-inline'"),
        );
    }

    response
}

/// `router` を `base_path`（`ServerConfig::normalized_base_path` の値）配下で公開する。
///
/// `{base_path}/...` へのリクエストはベースパスを取り除いてから `router` に渡す。ベースパスの無い
/// リクエストもそのまま受け付けるため、プロキシがパスを取り除く設定でも、コンテナのヘルスチェック
/// （`/api/v1/health`）のようにサーバーへ直接アクセスする場合でも動く。
///
/// `admin_path`（`ServerConfig::normalized_admin_path` の値）はスラッシュ付きへリダイレクトする。
pub fn with_base_path(router: Router, base_path: &str, admin_path: &str) -> Router {
    let admin_index = format!("{}{}/", base_path, admin_path);
    let redirect_to_admin = get(move || async move { Redirect::temporary(&admin_index) });
    let mut app = Router::new().route(admin_path, redirect_to_admin.clone());
    if !base_path.is_empty() {
        app = app.route(&format!("{}{}", base_path, admin_path), redirect_to_admin);
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

    #[tokio::test]
    async fn test_maintenance_mode_middleware() {
        let config = AegisConfig::default();
        let state = crate::test_support::test_state_with_config(
            config.clone(),
            aegis_runtime::PluginSupervisor::new(),
        )
        .await;
        let app = router(&config, state.clone());

        // 既定ではメンテナンスモードは無効
        let (status, _, _) = get_text(&app, "/health").await;
        assert_eq!(status, StatusCode::OK);

        // メンテナンスモードを有効化
        state.system.write().unwrap().maintenance_mode = Some(true);
        state.system.write().unwrap().maintenance_message = Some("System is updating".to_string());

        // 公開ルート / は 503 Service Unavailable
        let (status, body, _) = get_text(&app, "/").await;
        assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
        assert!(body.contains("System is updating"));

        assert!(body.contains(r#"<html lang="en">"#) && body.contains("- Maintenance</title>"));

        // 管理・ヘルスチェック・API ルートはアクセス可能
        let (status, _, _) = get_text(&app, "/health").await;
        assert_eq!(status, StatusCode::OK);
        let (status, _, _) = get_text(&app, "/api/v1/health").await;
        assert_eq!(status, StatusCode::OK);

        // 管理者セッション付きリクエストなら公開ルートもアクセス可能
        let session = state
            .sessions
            .create_session("admin1", "admin", std::collections::HashMap::new(), None)
            .await
            .unwrap();
        let res = app
            .clone()
            .oneshot(
                Request::get("/")
                    .header(header::AUTHORIZATION, format!("Bearer {}", session.id))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_ne!(res.status(), StatusCode::SERVICE_UNAVAILABLE);
    }

    #[tokio::test]
    async fn test_maintenance_page_follows_the_theme_locale() {
        let mut config = AegisConfig::default();
        config.theme.locale = "ja".to_string();
        let state = crate::test_support::test_state_with_config(
            config.clone(),
            aegis_runtime::PluginSupervisor::new(),
        )
        .await;
        state.system.write().unwrap().maintenance_mode = Some(true);
        let app = router(&config, state);
        let (status, body, _) = get_text(&app, "/").await;
        assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
        assert!(body.contains(r#"<html lang="ja">"#), "{}", body);
        assert!(body.contains("- メンテナンス中</title>"), "{}", body);
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
        let app = with_base_path(echo_router(), "/sites/cms", "/admin");
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
        let app = with_base_path(echo_router(), "/cms", "/admin");
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
            let app = with_base_path(echo_router(), base, "/admin");
            let (status, _, location) = get_text(&app, path).await;
            assert_eq!(status, StatusCode::TEMPORARY_REDIRECT, "{}", path);
            assert_eq!(location.as_deref(), Some(expected), "{}", path);
        }
    }

    #[tokio::test]
    async fn test_custom_admin_path_serves_the_admin_studio_instead_of_admin() {
        let dist = std::env::temp_dir().join(format!("aegis-admin-dist-{}", std::process::id()));
        std::fs::create_dir_all(&dist).unwrap();
        std::fs::write(dist.join("index.html"), "admin studio").unwrap();
        let mut config = AegisConfig::default();
        config.paths.admin_dist_dir = dist.clone();
        config.server.admin_path = "/backstage".to_string();
        let state = crate::test_support::test_state_with_config(
            config.clone(),
            aegis_runtime::PluginSupervisor::new(),
        )
        .await;
        let app = with_base_path(router(&config, state), "", "/backstage");

        let (status, _, location) = get_text(&app, "/backstage").await;
        assert_eq!(status, StatusCode::TEMPORARY_REDIRECT);
        assert_eq!(location.as_deref(), Some("/backstage/"));
        let (status, body, _) = get_text(&app, "/backstage/").await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body, "admin studio");
        // 既定の /admin は Admin Studio ではなくなる（公開サイトの 404）
        let (status, body, location) = get_text(&app, "/admin/").await;
        assert_eq!(status, StatusCode::NOT_FOUND);
        assert_ne!(body, "admin studio");
        assert_eq!(get_text(&app, "/admin").await.0, StatusCode::NOT_FOUND);
        assert!(location.is_none());
        let _ = std::fs::remove_dir_all(&dist);
    }
}
