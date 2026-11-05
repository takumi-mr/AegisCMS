//! oEmbed プロバイダー: 公開中の記事・固定ページ・商品の URL に、メタデータ（タイトル・説明・
//! サムネイル）を JSON で返す（`GET /oembed?url=…`）。他のサイトの埋め込み（リンクカード）や、
//! AegisCMS のコンシューマープラグインが使う。ページの `<head>` には発見用の
//! `<link rel="alternate" type="application/json+oembed">` を出す（[`discovery_link`]）。
//!
//! 返す `type` は常に `link`（HTML は返さない）。`description` は oEmbed の拡張項目。
//! このサイトの URL 以外・非公開・存在しないコンテンツは 404。

use axum::{
    extract::{Query, State},
    http::{header, HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use serde::Deserialize;
use serde_json::{json, Value};
use std::sync::Arc;

use super::content::is_currently_published;
use super::feed::public_base_url;
use crate::state::AppState;
use aegis_ast::renderer::escape_html;

#[derive(Debug, Deserialize)]
pub struct OembedQuery {
    pub url: Option<String>,
    pub format: Option<String>,
}

fn error(status: StatusCode, message: &str) -> Response {
    (status, Json(json!({ "error": message }))).into_response()
}

/// `url` のサイト内のパス（`/articles/hello`）。このサイトの URL でなければ `None`
fn site_path(url: &str, base: &str) -> Option<String> {
    let url = url::Url::parse(url).ok()?;
    let base = url::Url::parse(base).ok()?;
    if !matches!(url.scheme(), "http" | "https")
        || url.host_str() != base.host_str()
        || url.port_or_known_default() != base.port_or_known_default()
    {
        return None;
    }
    let base_path = base.path().trim_end_matches('/');
    let rest = url.path().strip_prefix(base_path)?;
    // `/blog` と `/blogger` を取り違えないよう、境界は `/` か末尾
    if !rest.is_empty() && !rest.starts_with('/') {
        return None;
    }
    Some(rest.trim_end_matches('/').to_string())
}

/// パスが指すコンテンツの（種類, スラッグ）の候補。先に見つかったものを使う
fn candidates(path: &str) -> Vec<(&'static str, String)> {
    let segments: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
    match segments.as_slice() {
        ["articles", slug] => vec![("article", slug.to_string())],
        ["products", slug] => vec![("product", slug.to_string())],
        [slug] => vec![("page", slug.to_string()), ("article", slug.to_string())],
        _ => Vec::new(),
    }
}

/// `GET /oembed?url=<ページの URL>&format=json`
pub async fn get_oembed(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Query(query): Query<OembedQuery>,
) -> Response {
    // 返せるのは JSON だけ（oEmbed の仕様では、対応しない形式は 501）
    if query
        .format
        .as_deref()
        .is_some_and(|f| !f.eq_ignore_ascii_case("json"))
    {
        return error(
            StatusCode::NOT_IMPLEMENTED,
            "Only the json format is supported",
        );
    }
    let Some(url) = query
        .url
        .as_deref()
        .map(str::trim)
        .filter(|u| !u.is_empty())
    else {
        return error(StatusCode::BAD_REQUEST, "The url parameter is required");
    };
    let base = public_base_url(&state, &headers);
    let Some(path) = site_path(url, &base) else {
        return error(StatusCode::NOT_FOUND, "Not a URL of this site");
    };

    let now = chrono::Utc::now();
    for (content_type, slug) in candidates(&path) {
        let schema = state.schemas.read().await.get(content_type).cloned();
        let Some(schema) = schema else { continue };
        let Ok(Some(record)) = state.db.get_published_content(&schema, &slug).await else {
            continue;
        };
        if !is_currently_published(&record.status, record.unpublish_at.as_deref(), now) {
            continue;
        }
        let field = |name: &str| {
            record
                .custom_fields
                .get(name)
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|v| !v.is_empty())
        };
        let seo = state.seo();
        let mut body = json!({
            "version": "1.0",
            "type": "link",
            "title": record.title,
            "provider_name": state.site().title,
            "provider_url": base,
            "url": url,
        });
        if let Some(description) = field("excerpt") {
            body["description"] = json!(description);
        }
        if let Some(image) = field("featured_image").or_else(|| field("image")) {
            // `/uploads/…` は、サイトの URL を付けて絶対 URL にする
            let thumbnail = if image.starts_with('/') && seo.canonical_base_url.is_none() {
                format!("{}{}", base, image)
            } else {
                seo.resolve_full_asset_url(image)
            };
            body["thumbnail_url"] = json!(thumbnail);
        }
        return (
            StatusCode::OK,
            [
                (header::CONTENT_TYPE, "application/json; charset=utf-8"),
                (header::CACHE_CONTROL, "public, max-age=300"),
                (header::ACCESS_CONTROL_ALLOW_ORIGIN, "*"),
            ],
            body.to_string(),
        )
            .into_response();
    }
    error(StatusCode::NOT_FOUND, "Not found")
}

/// ページの `<head>` に入れる、oEmbed の発見用 `<link>`。`full_url` はそのページの絶対 URL
/// （`canonical_base_url` が無ければ作れないので呼び出し側で出さない）
pub fn discovery_link(title: &str, base_path: &str, full_url: &str) -> String {
    let query: String = url::form_urlencoded::Serializer::new(String::new())
        .append_pair("url", full_url)
        .append_pair("format", "json")
        .finish();
    format!(
        r#"<link rel="alternate" type="application/json+oembed" href="{}/oembed?{}" title="{}">"#,
        escape_html(base_path),
        escape_html(&query),
        escape_html(title)
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn site_path_accepts_only_this_sites_urls() {
        let base = "https://example.com";
        assert_eq!(
            site_path("https://example.com/articles/hello", base).as_deref(),
            Some("/articles/hello")
        );
        assert_eq!(site_path("https://example.com/", base).as_deref(), Some(""));
        assert_eq!(
            site_path("https://example.com/about/?x=1#y", base).as_deref(),
            Some("/about")
        );
        assert_eq!(site_path("https://evil.example/articles/a", base), None);
        assert_eq!(site_path("https://example.com.evil.example/a", base), None);
        assert_eq!(site_path("ftp://example.com/a", base), None);
        assert_eq!(site_path("https://example.com:8443/a", base), None);
        assert_eq!(site_path("not a url", base), None);
        // サブディレクトリに置いたサイト
        let sub = "https://example.com/blog";
        assert_eq!(
            site_path("https://example.com/blog/about", sub).as_deref(),
            Some("/about")
        );
        assert_eq!(site_path("https://example.com/blogger/about", sub), None);
        assert_eq!(site_path("https://example.com/about", sub), None);
    }

    #[test]
    fn candidates_follow_the_public_routes() {
        assert_eq!(
            candidates("/articles/a"),
            vec![("article", "a".to_string())]
        );
        assert_eq!(
            candidates("/products/p"),
            vec![("product", "p".to_string())]
        );
        assert_eq!(
            candidates("/about"),
            vec![
                ("page", "about".to_string()),
                ("article", "about".to_string())
            ]
        );
        assert!(candidates("").is_empty());
        assert!(candidates("/a/b/c").is_empty());
    }

    #[test]
    fn discovery_link_is_escaped() {
        let link = discovery_link("A \"B\"", "/blog", "https://example.com/blog/a?b=1&c=2");
        assert_eq!(
            link,
            "<link rel=\"alternate\" type=\"application/json+oembed\" href=\"/blog/oembed?url=https%3A%2F%2Fexample.com%2Fblog%2Fa%3Fb%3D1%26c%3D2&amp;format=json\" title=\"A &quot;B&quot;\">"
        );
    }
}
