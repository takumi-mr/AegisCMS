//! `[security]` の設定（HTML 出力のリライター）

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct SecurityConfig {
    pub enforce_external_link_rel: bool,
    pub open_external_links_in_new_tab: bool,
    pub internal_domains: Vec<String>,
    pub enforce_image_lazy_loading: bool,
    pub enforce_image_async_decoding: bool,
}

impl Default for SecurityConfig {
    fn default() -> Self {
        Self {
            enforce_external_link_rel: true,
            open_external_links_in_new_tab: false,
            internal_domains: Vec::new(),
            enforce_image_lazy_loading: true,
            enforce_image_async_decoding: true,
        }
    }
}
