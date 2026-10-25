//! SEO およびクローラー（Bot）制御の設定管理。
//! DB の `_aegis_settings` に保存された設定（robots.txt、メタタグ、OGP、noindex 等）を
//! 公開サイトおよび `/robots.txt`, `/sitemap.xml` に反映する。

use std::collections::HashMap;

use aegis_ast::renderer::escape_html;
use aegis_db::DatabaseAdapter;
use serde::{Deserialize, Serialize};

/// `_aegis_settings` のキー名
pub const ROBOTS_TXT_SETTING: &str = "seo.robots_txt";
pub const META_DESCRIPTION_SETTING: &str = "seo.meta_description";
pub const META_KEYWORDS_SETTING: &str = "seo.meta_keywords";
pub const CANONICAL_BASE_URL_SETTING: &str = "seo.canonical_base_url";
pub const OG_IMAGE_SETTING: &str = "seo.og_image";
pub const NOINDEX_ALL_SETTING: &str = "seo.noindex_all";
pub const TWITTER_CARD_SETTING: &str = "seo.twitter_card";

/// robots.txt の最大バイト数（64 KiB）
pub const MAX_ROBOTS_TXT_BYTES: usize = 65_536;
/// meta description の最大文字数
pub const MAX_DESCRIPTION_CHARS: usize = 500;
/// meta keywords の最大文字数
pub const MAX_KEYWORDS_CHARS: usize = 500;
/// URL の最大長
pub const MAX_URL_LEN: usize = 2048;

/// Admin Studio から変更した SEO & Bot 設定
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SeoOverrides {
    /// カスタム robots.txt（`None` の場合はデフォルト生成）
    pub robots_txt: Option<String>,
    /// サイト全体のデフォルト meta description（`None` の場合はテーマ設定を使用）
    pub meta_description: Option<String>,
    /// meta keywords（カンマ区切り）
    pub meta_keywords: Option<String>,
    /// サイトの公開基底 URL（例: `https://example.com`、末尾スラッシュなし）
    pub canonical_base_url: Option<String>,
    /// デフォルトの OGP 画像（`/uploads/...` または `https://...`）
    pub og_image: Option<String>,
    /// サイト全体に `noindex, nofollow` を適用するかどうか
    pub noindex_all: bool,
    /// Twitter Card 種別（`summary` または `summary_large_image`、既定 `summary_large_image`）
    pub twitter_card: Option<String>,
}

impl SeoOverrides {
    pub fn from_settings(settings: &HashMap<String, String>) -> Self {
        let noindex_all = settings
            .get(NOINDEX_ALL_SETTING)
            .map(|v| v.trim() == "true" || v.trim() == "1")
            .unwrap_or(false);

        Self {
            robots_txt: settings.get(ROBOTS_TXT_SETTING).cloned(),
            meta_description: settings.get(META_DESCRIPTION_SETTING).cloned(),
            meta_keywords: settings.get(META_KEYWORDS_SETTING).cloned(),
            canonical_base_url: settings.get(CANONICAL_BASE_URL_SETTING).cloned(),
            og_image: settings.get(OG_IMAGE_SETTING).cloned(),
            noindex_all,
            twitter_card: settings.get(TWITTER_CARD_SETTING).cloned(),
        }
    }

    /// DB から読み込む。読めなければ警告してデフォルト値で動かす
    pub async fn load(db: &dyn DatabaseAdapter) -> Self {
        match db.list_settings().await {
            Ok(settings) => Self::from_settings(&settings),
            Err(e) => {
                tracing::warn!("Could not load SEO settings: {}", e);
                Self::default()
            }
        }
    }

    /// 有効な robots.txt の内容を返す。カスタム設定があればそれ、無ければデフォルト
    pub fn effective_robots_txt(&self) -> String {
        if let Some(ref custom) = self.robots_txt {
            if !custom.trim().is_empty() {
                return custom.clone();
            }
        }
        default_robots_txt(self.canonical_base_url.as_deref())
    }

    /// ページ用 HTML `<head>` 挿入タグ群を生成する
    pub fn generate_head_tags(&self, params: &PageHeadParams<'_>) -> String {
        let mut tags = Vec::new();

        // 1. noindex / nofollow
        if self.noindex_all {
            tags.push(r#"<meta name="robots" content="noindex, nofollow">"#.to_string());
        }

        // 2. description
        let effective_desc = params
            .description
            .filter(|d| !d.trim().is_empty())
            .or(self.meta_description.as_deref())
            .filter(|d| !d.trim().is_empty())
            .unwrap_or(params.theme_description);

        if !effective_desc.is_empty() {
            tags.push(format!(
                r#"<meta name="description" content="{}">"#,
                escape_html(effective_desc)
            ));
        }

        // 3. keywords
        if let Some(ref kw) = self.meta_keywords {
            if !kw.trim().is_empty() {
                tags.push(format!(
                    r#"<meta name="keywords" content="{}">"#,
                    escape_html(kw)
                ));
            }
        }

        // 4. Canonical URL
        let full_url = self.resolve_full_url(params.path);
        if let Some(ref url) = full_url {
            tags.push(format!(
                r#"<link rel="canonical" href="{}">"#,
                escape_html(url)
            ));
        }

        // 5. OGP タグ
        let page_title = if let Some(t) = params.title.filter(|t| !t.trim().is_empty()) {
            format!("{} • {}", t, params.site_title)
        } else {
            params.site_title.to_string()
        };

        tags.push(format!(
            r#"<meta property="og:site_name" content="{}">"#,
            escape_html(params.site_title)
        ));
        tags.push(format!(
            r#"<meta property="og:title" content="{}">"#,
            escape_html(&page_title)
        ));
        if !effective_desc.is_empty() {
            tags.push(format!(
                r#"<meta property="og:description" content="{}">"#,
                escape_html(effective_desc)
            ));
        }

        let og_type = if params.is_article {
            "article"
        } else {
            "website"
        };
        tags.push(format!(
            r#"<meta property="og:type" content="{}">"#,
            og_type
        ));

        if let Some(ref url) = full_url {
            tags.push(format!(
                r#"<meta property="og:url" content="{}">"#,
                escape_html(url)
            ));
        }

        let effective_image = params
            .image
            .filter(|img| !img.trim().is_empty())
            .or(self.og_image.as_deref())
            .filter(|img| !img.trim().is_empty());

        if let Some(img) = effective_image {
            let full_img_url = self.resolve_full_asset_url(img);
            tags.push(format!(
                r#"<meta property="og:image" content="{}">"#,
                escape_html(&full_img_url)
            ));
        }

        // 6. Twitter Card
        let card_type = self
            .twitter_card
            .as_deref()
            .unwrap_or("summary_large_image");
        tags.push(format!(
            r#"<meta name="twitter:card" content="{}">"#,
            escape_html(card_type)
        ));
        tags.push(format!(
            r#"<meta name="twitter:title" content="{}">"#,
            escape_html(&page_title)
        ));
        if !effective_desc.is_empty() {
            tags.push(format!(
                r#"<meta name="twitter:description" content="{}">"#,
                escape_html(effective_desc)
            ));
        }
        if let Some(img) = effective_image {
            let full_img_url = self.resolve_full_asset_url(img);
            tags.push(format!(
                r#"<meta name="twitter:image" content="{}">"#,
                escape_html(&full_img_url)
            ));
        }

        tags.join("\n  ")
    }

    /// 相対パスまたは絶対パスを canonical_base_url と結合して絶対 URL にする
    pub fn resolve_full_url(&self, path: Option<&str>) -> Option<String> {
        let base = self.canonical_base_url.as_deref()?.trim_end_matches('/');
        let path = path.unwrap_or("/").trim();
        let path = if path.starts_with('/') {
            path
        } else {
            &format!("/{}", path)
        };
        Some(format!("{}{}", base, path))
    }

    /// 画像パス（`/uploads/...` または `https://...`）を完全な URL に解決する
    pub fn resolve_full_asset_url(&self, img_url: &str) -> String {
        let trimmed = img_url.trim();
        if trimmed.starts_with("http://") || trimmed.starts_with("https://") {
            trimmed.to_string()
        } else if let Some(base) = self.canonical_base_url.as_deref() {
            let base = base.trim_end_matches('/');
            let path = if trimmed.starts_with('/') {
                trimmed
            } else {
                &format!("/{}", trimmed)
            };
            format!("{}{}", base, path)
        } else {
            trimmed.to_string()
        }
    }
}

/// ページ個別メタタグ生成パラメータ
pub struct PageHeadParams<'a> {
    pub site_title: &'a str,
    pub theme_description: &'a str,
    pub title: Option<&'a str>,
    pub description: Option<&'a str>,
    pub image: Option<&'a str>,
    pub path: Option<&'a str>,
    pub is_article: bool,
}

/// デフォルトの robots.txt を生成
pub fn default_robots_txt(canonical_base_url: Option<&str>) -> String {
    let mut s = String::from("User-agent: *\nDisallow: /admin/\nDisallow: /api/\nAllow: /\n");
    if let Some(base) = canonical_base_url.filter(|b| !b.trim().is_empty()) {
        let base = base.trim_end_matches('/');
        s.push_str(&format!("\nSitemap: {}/sitemap.xml\n", base));
    }
    s
}

/// robots.txt の検査
pub fn validate_robots_txt(content: &str) -> Result<(), &'static str> {
    if content.len() > MAX_ROBOTS_TXT_BYTES {
        return Err("robots.txt is too large (maximum 64 KiB)");
    }
    // バイナリ文字（NULバイトなど）を含んでいないか検査
    if content
        .chars()
        .any(|c| c == '\0' || (c.is_control() && c != '\n' && c != '\r' && c != '\t'))
    {
        return Err("robots.txt must not contain invalid control characters");
    }
    Ok(())
}

/// canonical_base_url の検査
pub fn validate_canonical_base_url(url: &str) -> Result<String, &'static str> {
    if url.len() > MAX_URL_LEN {
        return Err("The canonical URL is too long");
    }
    if url.chars().any(|c| c.is_whitespace() || c.is_control()) {
        return Err("The canonical URL must not contain spaces or control characters");
    }
    let lower = url.to_ascii_lowercase();
    if !lower.starts_with("http://") && !lower.starts_with("https://") {
        return Err("The canonical URL must start with http:// or https://");
    }
    // 末尾のスラッシュは取り除いて正規化する
    Ok(url.trim_end_matches('/').to_string())
}

/// og_image の検査
pub fn validate_og_image(url: &str) -> Result<(), &'static str> {
    if url.len() > MAX_URL_LEN {
        return Err("The OGP image URL is too long");
    }
    if url.chars().any(|c| c.is_whitespace() || c.is_control()) {
        return Err("The OGP image URL must not contain spaces");
    }
    let lower = url.to_ascii_lowercase();
    let root_relative = url.starts_with('/') && !url.starts_with("//");
    let absolute = ["http://", "https://"]
        .iter()
        .any(|scheme| lower.starts_with(scheme) && lower.len() > scheme.len());
    if !(root_relative || absolute) {
        return Err("The OGP image must be a path starting with / or an http(s) URL");
    }
    Ok(())
}

/// meta_description の検査
pub fn validate_meta_description(desc: &str) -> Result<(), &'static str> {
    if desc.chars().count() > MAX_DESCRIPTION_CHARS {
        return Err("The meta description is too long (500 characters at most)");
    }
    if desc.chars().any(char::is_control) {
        return Err("The meta description must not contain control characters");
    }
    Ok(())
}

/// meta_keywords の検査
pub fn validate_meta_keywords(kw: &str) -> Result<(), &'static str> {
    if kw.chars().count() > MAX_KEYWORDS_CHARS {
        return Err("The meta keywords are too long (500 characters at most)");
    }
    if kw.chars().any(char::is_control) {
        return Err("The meta keywords must not contain control characters");
    }
    Ok(())
}

/// twitter_card の検査
pub fn validate_twitter_card(card: &str) -> Result<(), &'static str> {
    if card != "summary" && card != "summary_large_image" {
        return Err("The Twitter card must be either 'summary' or 'summary_large_image'");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_robots_txt_generation() {
        let text = default_robots_txt(None);
        assert!(text.contains("User-agent: *"));
        assert!(text.contains("Disallow: /admin/"));
        assert!(!text.contains("Sitemap:"));

        let text_with_sitemap = default_robots_txt(Some("https://example.com/"));
        assert!(text_with_sitemap.contains("Sitemap: https://example.com/sitemap.xml"));
    }

    #[test]
    fn test_seo_head_tags_generation() {
        let seo = SeoOverrides {
            robots_txt: None,
            meta_description: Some("Site description from SEO".to_string()),
            meta_keywords: Some("cms, rust, secure".to_string()),
            canonical_base_url: Some("https://example.com".to_string()),
            og_image: Some("/uploads/default-og.png".to_string()),
            noindex_all: false,
            twitter_card: Some("summary_large_image".to_string()),
        };

        let params = PageHeadParams {
            site_title: "My Site",
            theme_description: "Theme fallback desc",
            title: Some("Article One"),
            description: Some("Custom article excerpt"),
            image: Some("/uploads/art1.png"),
            path: Some("/articles/article-one"),
            is_article: true,
        };

        let tags = seo.generate_head_tags(&params);
        assert!(tags.contains(r#"<meta name="description" content="Custom article excerpt">"#));
        assert!(tags.contains(r#"<meta name="keywords" content="cms, rust, secure">"#));
        assert!(tags
            .contains(r#"<link rel="canonical" href="https://example.com/articles/article-one">"#));
        assert!(tags.contains(r#"<meta property="og:site_name" content="My Site">"#));
        assert!(tags.contains(r#"<meta property="og:title" content="Article One • My Site">"#));
        assert!(tags.contains(r#"<meta property="og:type" content="article">"#));
        assert!(tags.contains(
            r#"<meta property="og:url" content="https://example.com/articles/article-one">"#
        ));
        assert!(tags.contains(
            r#"<meta property="og:image" content="https://example.com/uploads/art1.png">"#
        ));
        assert!(tags.contains(r#"<meta name="twitter:card" content="summary_large_image">"#));
        assert!(!tags.contains("noindex"));
    }

    #[test]
    fn test_noindex_tag_injected_when_enabled() {
        let seo = SeoOverrides {
            noindex_all: true,
            ..Default::default()
        };
        let params = PageHeadParams {
            site_title: "My Site",
            theme_description: "Desc",
            title: None,
            description: None,
            image: None,
            path: None,
            is_article: false,
        };
        let tags = seo.generate_head_tags(&params);
        assert!(tags.contains(r#"<meta name="robots" content="noindex, nofollow">"#));
    }

    #[test]
    fn test_validation() {
        assert!(validate_canonical_base_url("https://example.com/").is_ok());
        assert_eq!(
            validate_canonical_base_url("https://example.com/").unwrap(),
            "https://example.com"
        );
        assert!(validate_canonical_base_url("ftp://example.com").is_err());
        assert!(validate_og_image("/uploads/test.jpg").is_ok());
        assert!(validate_og_image("https://cdn.example.com/test.jpg").is_ok());
        assert!(validate_og_image("javascript:alert(1)").is_err());
        assert!(validate_twitter_card("summary_large_image").is_ok());
        assert!(validate_twitter_card("invalid").is_err());
    }
}
