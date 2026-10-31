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
    /// 記事詳細ページに表示する関連記事の最大件数（既定: 4、0 で非表示）
    #[serde(default = "default_related_posts_limit")]
    pub related_posts_limit: usize,
    /// トップページ（`/`）に出すもの: `"posts"`（既定。記事一覧）、`"template"`（テーマの `front.html`）、
    /// `"page:<slug>"`（固定ページ）。`"posts"` 以外では記事一覧は `/articles` だけになる
    #[serde(default = "default_front_page")]
    pub front_page: String,
    /// `true` でテンプレートをリクエストごとに読み直す（テーマ開発用）。
    /// `false`（既定）ではコンパイル済みのテンプレートを再利用し、テーマの切り替え時に読み直す
    #[serde(default)]
    pub reload_templates: bool,
    /// テーマのフッターに「Powered by AegisCMS」の表記を出すか（既定: true）。
    /// `false` で、使っている CMS を公開サイトから読み取りにくくする
    #[serde(default = "default_show_powered_by")]
    pub show_powered_by: bool,
    /// 組み込みテンプレートとカートのスクリプトの言語（`en` / `ja`、既定 `en`）。
    /// 訪問者に見せる文言だけが対象で、API のエラーメッセージは常に英語
    #[serde(default = "default_locale")]
    pub locale: String,
    /// 公開ページに Admin Studio へのリンクを出すか。未指定なら `[server].admin_path` が既定の `/admin` のときだけ
    /// 出す（パスを変えたのは隠すためなので、リンクで公開しない）。`true` / `false` で固定できる
    #[serde(default)]
    pub show_admin_link: Option<bool>,
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

fn default_related_posts_limit() -> usize {
    4
}

fn default_show_powered_by() -> bool {
    true
}

fn default_locale() -> String {
    "en".to_string()
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
        if !crate::messages::is_supported(&self.locale) {
            return Err(aegis_core::AegisError::Config(format!(
                "[theme].locale must be one of {} (got \"{}\")",
                crate::messages::LOCALES.join(", "),
                self.locale
            )));
        }
        if self.related_posts_limit > 50 {
            return Err(aegis_core::AegisError::Config(format!(
                "[theme].related_posts_limit must be between 0 and 50 (got {})",
                self.related_posts_limit
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
            related_posts_limit: default_related_posts_limit(),
            front_page: default_front_page(),
            reload_templates: false,
            locale: default_locale(),
            show_powered_by: default_show_powered_by(),
            show_admin_link: None,
            require_signatures: false,
            trusted_public_keys: Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_related_posts_limit() {
        let config = ThemeConfig::default();
        assert_eq!(config.related_posts_limit, 4);
        assert!(config.validate().is_ok());

        let invalid = ThemeConfig {
            related_posts_limit: 51,
            ..ThemeConfig::default()
        };
        assert!(invalid.validate().is_err());
    }

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

    #[test]
    fn test_locale_values() {
        let with = |value: &str| ThemeConfig {
            locale: value.to_string(),
            ..ThemeConfig::default()
        };
        assert_eq!(ThemeConfig::default().locale, "en");
        assert!(with("en").validate().is_ok() && with("ja").validate().is_ok());
        for invalid in ["", "EN", "fr", "ja-JP"] {
            assert!(with(invalid).validate().is_err(), "{}", invalid);
        }
    }
}
