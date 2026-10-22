pub mod audit;
pub mod auth;
pub mod content;
pub mod health;
pub mod oidc;
pub mod openapi;
pub mod plugin;
pub mod preview;
pub mod public;
pub mod schemas;
pub mod storage;
pub mod theme;
pub mod users;

pub use audit::{get_metrics, list_audit_logs};
pub use auth::{create_session, delete_session, get_current_session};
pub use content::{create_content, get_content, list_content};
pub use health::health_check;
pub use oidc::{list_auth_providers, oidc_callback, oidc_login};
pub use openapi::get_openapi_spec;
pub use plugin::list_plugins;
pub use preview::preview_content;
pub use public::{render_article, render_index, render_page_or_slug};
pub use schemas::{get_schema, list_schemas};
pub use storage::{delete_object, list_objects, upload_file};
pub use theme::{
    get_active_theme, get_active_theme_asset, get_specific_theme_asset, get_theme_preview,
    list_themes, switch_active_theme,
};
pub use users::{
    create_user, delete_avatar, delete_user, get_user, list_users, update_user, upload_avatar,
};
