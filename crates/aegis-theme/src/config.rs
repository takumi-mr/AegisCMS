//! `[theme]` の設定

use aegis_core::Result;
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
    /// テーマのインストール時に署名を必須とするか（既定: false）
    #[serde(default)]
    pub require_signatures: bool,
    /// 信頼する公開鍵リスト（32バイト hex 形式、`require_signatures = true` のときは 1 件以上必須）
    #[serde(default)]
    pub trusted_public_keys: Vec<String>,
}

fn default_posts_per_page() -> u64 {
    10
}

impl ThemeConfig {
    /// 署名ポリシー設定（`require_signatures` / `trusted_public_keys`）を検証する
    pub fn validate(&self) -> Result<()> {
        aegis_core::validate_signature_policy(
            "theme",
            self.require_signatures,
            &self.trusted_public_keys,
        )
    }
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
            require_signatures: false,
            trusted_public_keys: Vec::new(),
        }
    }
}
