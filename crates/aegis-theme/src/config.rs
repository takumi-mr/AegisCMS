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
    /// トップページ（`/`）に出すもの: `"posts"`（既定。記事一覧）、`"template"`（テーマの `front.html`）、
    /// `"page:<slug>"`（固定ページ）。`"posts"` 以外では記事一覧は `/articles` だけになる
    #[serde(default = "default_front_page")]
    pub front_page: String,
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

fn default_front_page() -> String {
    "posts".to_string()
}

/// `[theme].front_page` を解釈した値
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FrontPage {
    /// 記事一覧（`index.html`）
    Posts,
    /// テーマの `front.html`（最新の記事を `items` に渡す）
    Template,
    /// 固定ページ（スラッグ）
    Page(String),
}

impl FrontPage {
    fn parse(value: &str) -> Option<Self> {
        match value.trim() {
            "" | "posts" => Some(Self::Posts),
            "template" => Some(Self::Template),
            other => other
                .strip_prefix("page:")
                .map(str::trim)
                .filter(|slug| !slug.is_empty())
                .map(|slug| Self::Page(slug.to_string())),
        }
    }
}

impl ThemeConfig {
    /// トップページの設定（[`ThemeConfig::validate`] で検証済みの前提。読めない値は記事一覧）
    pub fn front_page(&self) -> FrontPage {
        FrontPage::parse(&self.front_page).unwrap_or(FrontPage::Posts)
    }

    /// `front_page` と署名ポリシー設定（`require_signatures` / `trusted_public_keys`）を検証する
    pub fn validate(&self) -> Result<()> {
        if FrontPage::parse(&self.front_page).is_none() {
            return Err(aegis_core::AegisError::Config(format!(
                "[theme].front_page must be \"posts\", \"template\" or \"page:<slug>\" (got \"{}\")",
                self.front_page
            )));
        }
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
            front_page: default_front_page(),
            reload_templates: false,
            require_signatures: false,
            trusted_public_keys: Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_front_page_values() {
        let with = |value: &str| ThemeConfig {
            front_page: value.to_string(),
            ..ThemeConfig::default()
        };
        assert_eq!(ThemeConfig::default().front_page(), FrontPage::Posts);
        assert_eq!(with("template").front_page(), FrontPage::Template);
        assert_eq!(
            with("page: home ").front_page(),
            FrontPage::Page("home".to_string())
        );
        for invalid in ["page:", "home", "Posts"] {
            assert!(with(invalid).validate().is_err(), "{}", invalid);
        }
        assert!(with("posts").validate().is_ok());
    }
}
