//! `[security]` の設定（HTML 出力のリライター）

use serde::{Deserialize, Serialize};

/// 公開ページの Strict CSP（`[security].strict_csp`）。インラインのスクリプトを許さず、信頼できる出どころの
/// ものだけをハッシュで許す（docs/csp.md）
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum StrictCsp {
    /// 今までどおり（`script-src 'self' 'unsafe-inline'`）
    #[default]
    Off,
    /// 今の CSP に加えて、Strict の CSP を `Content-Security-Policy-Report-Only` で出す（止まるものを、
    /// ブラウザーのコンソールで確かめられる）
    ReportOnly,
    /// Strict の CSP を強制する（`'unsafe-inline'` を外し、ページのインラインのスクリプトはハッシュで許す）
    On,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct SecurityConfig {
    /// 公開ページの Strict CSP（`off` / `report-only` / `on`）。管理者が CSP を上書きしているときは効かない
    pub strict_csp: StrictCsp,
    pub enforce_external_link_rel: bool,
    pub open_external_links_in_new_tab: bool,
    pub internal_domains: Vec<String>,
    pub enforce_image_lazy_loading: bool,
    pub enforce_image_async_decoding: bool,
}

impl Default for SecurityConfig {
    fn default() -> Self {
        Self {
            strict_csp: StrictCsp::Off,
            enforce_external_link_rel: true,
            open_external_links_in_new_tab: false,
            internal_domains: Vec::new(),
            enforce_image_lazy_loading: true,
            enforce_image_async_decoding: true,
        }
    }
}
