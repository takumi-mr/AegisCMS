//! `[theme]` の設定

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThemeConfig {
    pub active: String,
    pub site_title: String,
    pub site_description: String,
    /// 公開サイトの favicon の URL（`/favicon.png` のようなルート起点のパスか http(s) の URL）。
    /// Admin Studio の「サイト」設定で変更すると、そちらが優先される
    #[serde(default)]
    pub favicon: Option<String>,
    #[serde(default)]
    pub custom_css: Option<String>,
    /// トップページ（記事一覧）の 1 ページあたりの件数（`/?page=N` でページ送り）
    #[serde(default = "default_posts_per_page")]
    pub posts_per_page: u64,
    /// `true` でテンプレートをリクエストごとに読み直す（テーマ開発用）。
    /// `false`（既定）ではコンパイル済みのテンプレートを再利用し、テーマの切り替え時に読み直す
    #[serde(default)]
    pub reload_templates: bool,
}

fn default_posts_per_page() -> u64 {
    10
}

impl Default for ThemeConfig {
    fn default() -> Self {
        Self {
            active: "default".to_string(),
            site_title: "AegisCMS".to_string(),
            site_description: "Next-Generation Secure CMS with Sandboxed Extensibility".to_string(),
            favicon: None,
            custom_css: None,
            posts_per_page: default_posts_per_page(),
            reload_templates: false,
        }
    }
}
