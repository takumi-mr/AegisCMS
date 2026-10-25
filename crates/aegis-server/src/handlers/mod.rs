pub mod audit;
pub mod auth;
pub mod comments;
pub mod content;
pub mod customers;
pub mod forum;
pub mod health;
pub mod invitation;
pub mod notifications;
pub mod oidc;
pub mod openapi;
pub mod password_reset;
pub mod plugin;
pub mod preview;
pub mod public;
pub mod registration;
pub mod reviews;
pub mod saml;
pub mod schemas;
pub mod seo;
pub mod shop;
pub mod site;
pub mod sso;
pub mod storage;
pub mod storefront;
pub mod theme;
pub mod users;

pub use audit::{get_metrics, list_audit_logs};
pub use auth::{create_session, delete_session, get_current_session};
pub use comments::{
    bulk_comments, delete_comment, get_comment_settings, list_comments, list_public_comments,
    reply_comment, submit_comment, unsubscribe_page, update_comment, update_comment_settings,
};
pub use content::{
    create_content, delete_content, delete_variant, get_content, list_content, list_schedule,
    list_variants, update_content, upsert_variant,
};
pub use customers::{
    admin_get_customer, admin_list_customers, admin_set_customer_status, get_me, get_my_order,
    list_my_orders, list_my_reviews, login as customer_login, logout as customer_logout,
    register as customer_register, update_me,
};
pub use health::health_check;
pub use invitation::{accept_invitation, create_invitation, get_invitation_info};
pub use notifications::{
    delete_queued_notification, get_notification_status, get_notification_template,
    list_notification_queue, list_notification_templates, preview_notification,
    reset_notification_template, retry_queued_notification, save_notification_template,
    send_test_notification,
};
pub use oidc::{oidc_callback, oidc_login};
pub use openapi::get_openapi_spec;
pub use password_reset::{confirm_password_reset, request_password_reset};
pub use plugin::{
    get_plugin_order, get_plugin_settings, install_plugin, list_plugins, reset_plugin_order,
    set_plugin_enabled, set_plugin_order, update_plugin_settings,
};
pub use preview::preview_content;
pub use public::{render_article, render_index, render_page_or_slug};
pub use registration::{register, verify_registration};
pub use reviews::{
    admin_bulk_reviews, admin_delete_reply, admin_get_review, admin_get_review_settings,
    admin_list_reviews, admin_patch_review, admin_put_review_settings, admin_set_reply,
    list_product_reviews, submit_product_review,
};
pub use saml::{saml_acs, saml_login, saml_metadata};
pub use schemas::{get_schema, list_schemas};
pub use seo::{get_robots_txt, get_seo_settings, get_sitemap_xml, update_seo_settings};
pub use shop::{
    checkout, get_order, get_public_order, get_public_product_variants, list_orders,
    payment_webhook, quote, refund_order, sales_report, shop_config, update_order_status,
};
pub use site::{get_site_settings, update_site_settings};
pub use sso::list_auth_providers;
pub use storage::{delete_object, list_objects, upload_file};
pub use storefront::{cart_script, render_cart, render_order, render_product, render_products};
pub use theme::{
    get_active_theme, get_active_theme_asset, get_specific_theme_asset, get_theme_preview,
    install_theme, list_themes, switch_active_theme,
};
pub use users::{
    create_user, delete_avatar, delete_user, get_user, list_users, update_user, upload_avatar,
};

#[cfg(test)]
mod customers_tests;
#[cfg(test)]
mod forum_tests;
#[cfg(test)]
mod package_install_tests;
