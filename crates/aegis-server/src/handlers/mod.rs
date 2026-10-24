pub mod audit;
pub mod auth;
pub mod content;
pub mod health;
pub mod invitation;
pub mod oidc;
pub mod openapi;
pub mod password_reset;
pub mod plugin;
pub mod preview;
pub mod public;
pub mod registration;
pub mod saml;
pub mod schemas;
pub mod shop;
pub mod site;
pub mod sso;
pub mod storage;
pub mod storefront;
pub mod theme;
pub mod users;

pub use audit::{get_metrics, list_audit_logs};
pub use auth::{create_session, delete_session, get_current_session};
pub use content::{
    create_content, delete_content, get_content, list_content, list_schedule, update_content,
};
pub use health::health_check;
pub use invitation::{accept_invitation, create_invitation, get_invitation_info};
pub use oidc::{oidc_callback, oidc_login};
pub use openapi::get_openapi_spec;
pub use password_reset::{confirm_password_reset, request_password_reset};
pub use plugin::{get_plugin_settings, list_plugins, update_plugin_settings};
pub use preview::preview_content;
pub use public::{render_article, render_index, render_page_or_slug};
pub use registration::{register, verify_registration};
pub use saml::{saml_acs, saml_login, saml_metadata};
pub use schemas::{get_schema, list_schemas};
pub use shop::{
    checkout, get_order, get_public_order, list_orders, payment_webhook, quote, shop_config,
    update_order_status,
};
pub use site::{get_site_settings, update_site_settings};
pub use sso::list_auth_providers;
pub use storage::{delete_object, list_objects, upload_file};
pub use storefront::{cart_script, render_cart, render_order, render_product, render_products};
pub use theme::{
    get_active_theme, get_active_theme_asset, get_specific_theme_asset, get_theme_preview,
    list_themes, switch_active_theme,
};
pub use users::{
    create_user, delete_avatar, delete_user, get_user, list_users, update_user, upload_avatar,
};
