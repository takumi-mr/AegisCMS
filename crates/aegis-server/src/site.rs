//! 公開サイトの名前と favicon。設定ファイル（`[theme].site_title` / `favicon`）の値を、Admin Studio
//! から変更した値（DB の `_aegis_settings`）で上書きする

use std::collections::HashMap;

use crate::config::AegisConfig;
use aegis_db::DatabaseAdapter;
use aegis_theme::ThemeConfig;
use serde::Serialize;

/// `_aegis_settings` の名前
pub const TITLE_SETTING: &str = "site.title";
pub const FAVICON_SETTING: &str = "site.favicon";

/// サイト名の最大文字数
pub const MAX_TITLE_CHARS: usize = 200;
/// favicon の URL の最大長
pub const MAX_FAVICON_LEN: usize = 2048;

/// Admin Studio から変更した値（`None` は設定ファイルの値を使う）
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SiteOverrides {
    pub title: Option<String>,
    pub favicon: Option<String>,
}

impl SiteOverrides {
    pub fn from_settings(settings: &HashMap<String, String>) -> Self {
        Self {
            title: settings.get(TITLE_SETTING).cloned(),
            favicon: settings.get(FAVICON_SETTING).cloned(),
        }
    }

    /// DB から読み込む。読めなければ警告して設定ファイルの値で動かす
    pub async fn load(db: &dyn DatabaseAdapter) -> Self {
        match db.list_settings().await {
            Ok(settings) => Self::from_settings(&settings),
            Err(e) => {
                tracing::warn!("Could not load the site settings, using [theme]: {}", e);
                Self::default()
            }
        }
    }
}

/// 公開サイトで使う値
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SiteInfo {
    pub title: String,
    pub favicon: Option<String>,
}

impl SiteInfo {
    /// 設定ファイルの値（Admin Studio で戻したときの値）
    pub fn defaults(theme: &ThemeConfig) -> Self {
        Self {
            title: theme.site_title.clone(),
            favicon: theme.favicon.clone().filter(|f| !f.trim().is_empty()),
        }
    }

    pub fn resolve(theme: &ThemeConfig, overrides: &SiteOverrides) -> Self {
        let defaults = Self::defaults(theme);
        Self {
            title: overrides.title.clone().unwrap_or(defaults.title),
            favicon: overrides.favicon.clone().or(defaults.favicon),
        }
    }

    /// テーマの `extra_head` に入れる `<link rel="icon">`（favicon が無ければ空）
    pub fn head_html(&self) -> String {
        self.favicon
            .as_deref()
            .map(|url| {
                format!(
                    r#"<link rel="icon" href="{}">"#,
                    aegis_ast::renderer::escape_html(url)
                )
            })
            .unwrap_or_default()
    }
}

/// 公開サイトでのコンテンツの URL（通知の本文用。`[commerce].public_url` が無い、公開ページの無い種類なら空）。
/// `page` / `article` は `/{slug}`、商品（`[commerce].product_type`）は `/products/{slug}`
pub fn content_url(config: &AegisConfig, content_type: &str, slug: &str) -> String {
    let site = config.commerce.public_url.trim().trim_end_matches('/');
    let path = match content_type {
        "page" | "article" => format!("/{}", slug),
        t if t == config.commerce.product_type => format!("/products/{}", slug),
        _ => return String::new(),
    };
    if site.is_empty() {
        return String::new();
    }
    format!("{}{}", site, path)
}

/// サイト名の検査（前後の空白は除いてから渡す）
pub fn validate_title(title: &str) -> Result<(), &'static str> {
    if title.is_empty() {
        return Err("The site title must not be empty");
    }
    if title.chars().count() > MAX_TITLE_CHARS {
        return Err("The site title is too long (200 characters at most)");
    }
    if title.chars().any(char::is_control) {
        return Err("The site title must not contain control characters");
    }
    Ok(())
}

/// favicon の URL の検査: ルート起点のパス（`/uploads/...`）か http(s) の URL のみ
/// （`javascript:` や `data:`、プロトコル相対の `//host` は受け付けない）
pub fn validate_favicon(url: &str) -> Result<(), &'static str> {
    if url.len() > MAX_FAVICON_LEN {
        return Err("The favicon URL is too long");
    }
    if url.chars().any(|c| c.is_whitespace() || c.is_control()) {
        return Err("The favicon URL must not contain spaces");
    }
    let lower = url.to_ascii_lowercase();
    let root_relative = url.starts_with('/') && !url.starts_with("//");
    let absolute = ["http://", "https://"]
        .iter()
        .any(|scheme| lower.starts_with(scheme) && lower.len() > scheme.len());
    if !(root_relative || absolute) {
        return Err("The favicon must be a path starting with / or an http(s) URL");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn theme() -> ThemeConfig {
        ThemeConfig {
            site_title: "From config".to_string(),
            favicon: Some("/config.ico".to_string()),
            ..Default::default()
        }
    }

    #[test]
    fn test_overrides_take_precedence_over_the_config() {
        let site = SiteInfo::resolve(&theme(), &SiteOverrides::default());
        assert_eq!(site.title, "From config");
        assert_eq!(site.favicon.as_deref(), Some("/config.ico"));

        let mut settings = HashMap::new();
        settings.insert(TITLE_SETTING.to_string(), "From admin".to_string());
        settings.insert(FAVICON_SETTING.to_string(), "/uploads/a.png".to_string());
        let site = SiteInfo::resolve(&theme(), &SiteOverrides::from_settings(&settings));
        assert_eq!(site.title, "From admin");
        assert_eq!(site.favicon.as_deref(), Some("/uploads/a.png"));
    }

    #[test]
    fn test_head_html_escapes_the_url() {
        let site = SiteInfo {
            title: "t".to_string(),
            favicon: Some("/a.png?x=1&y=\"2\"".to_string()),
        };
        assert_eq!(
            site.head_html(),
            r#"<link rel="icon" href="/a.png?x=1&amp;y=&quot;2&quot;">"#
        );
        let none = SiteInfo {
            title: "t".to_string(),
            favicon: None,
        };
        assert_eq!(none.head_html(), "");
    }

    #[test]
    fn test_validate_favicon() {
        assert!(validate_favicon("/uploads/2026/10/icon.png").is_ok());
        assert!(validate_favicon("https://cdn.example.com/icon.svg").is_ok());
        assert!(validate_favicon("HTTP://example.com/i.ico").is_ok());
        for bad in [
            "javascript:alert(1)",
            "data:image/png;base64,AAAA",
            "//evil.example.com/i.png",
            "icon.png",
            "https://",
            "/a b.png",
        ] {
            assert!(validate_favicon(bad).is_err(), "{bad} should be rejected");
        }
        assert!(validate_favicon(&format!("/{}", "a".repeat(MAX_FAVICON_LEN))).is_err());
    }

    #[test]
    fn test_validate_title() {
        assert!(validate_title("My Site").is_ok());
        assert!(validate_title("").is_err());
        assert!(validate_title("a\nb").is_err());
        assert!(validate_title(&"あ".repeat(MAX_TITLE_CHARS)).is_ok());
        assert!(validate_title(&"あ".repeat(MAX_TITLE_CHARS + 1)).is_err());
    }
}
