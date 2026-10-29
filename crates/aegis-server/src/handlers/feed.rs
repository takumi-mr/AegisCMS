//! Atom フィード: 新しい記事（`/feed.xml`）と、カテゴリごとの記事（`/category/{slug}/feed.xml`）

use axum::{
    extract::{Path, State},
    http::{header, HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use std::sync::Arc;

use crate::handlers::content::is_currently_published;
use crate::handlers::public::{extract_excerpt, find_category, list_published_articles};
use crate::render;
use crate::state::AppState;
use aegis_ast::renderer::escape_html;
use aegis_ast::BlockNode;
use aegis_cache::CacheTags;

/// フィードに載せる記事の数（新しい順）
const FEED_ENTRIES: u64 = 20;

/// フィードの自動検出用の `<link>`（`<head>` に入れる）
pub fn feed_link_tag(title: &str, href: &str) -> String {
    format!(
        r#"<link rel="alternate" type="application/atom+xml" title="{}" href="{}">"#,
        escape_html(title),
        escape_html(href)
    )
}

/// サイトの URL（末尾の `/` なし）: SEO 設定の canonical_base_url、無ければリクエストの Host と `[server].base_path`
fn public_base_url(state: &AppState, headers: &HeaderMap) -> String {
    if let Some(base) = state.seo().canonical_base_url {
        return base.trim_end_matches('/').to_string();
    }
    let host = headers
        .get(header::HOST)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("localhost:8080");
    let scheme = headers
        .get("x-forwarded-proto")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("http");
    let base_path = state
        .config
        .server
        .normalized_base_path()
        .unwrap_or_default();
    format!("{}://{}{}", scheme, host.trim(), base_path)
}

/// 保存されている日時を Atom の日時（RFC 3339、UTC）にする。読めなければ `None`
fn atom_date(value: Option<&str>) -> Option<String> {
    let value = value?.trim();
    let parsed = chrono::DateTime::parse_from_rfc3339(value)
        .map(|d| d.with_timezone(&chrono::Utc))
        .or_else(|_| {
            chrono::NaiveDateTime::parse_from_str(value, "%Y-%m-%d %H:%M:%S").map(|d| d.and_utc())
        })
        .ok()?;
    Some(parsed.to_rfc3339_opts(chrono::SecondsFormat::Secs, true))
}

/// `GET /feed.xml` — 新しい記事の Atom フィード
pub async fn get_feed(State(state): State<Arc<AppState>>, headers: HeaderMap) -> Response {
    let mut tags = CacheTags::new();
    tags.add_url("/feed.xml");
    let title = state.site().title;
    build_feed(&state, &headers, &title, "/feed.xml", "/", None, tags).await
}

/// `GET /category/{slug}/feed.xml` — カテゴリの記事の Atom フィード
pub async fn get_category_feed(
    State(state): State<Arc<AppState>>,
    Path(slug): Path<String>,
    headers: HeaderMap,
) -> Response {
    let Some((category, filter)) = find_category(&state, &slug).await else {
        return (StatusCode::NOT_FOUND, "Category not found").into_response();
    };
    let mut tags = CacheTags::new();
    tags.add_url(&format!("/category/{}/feed.xml", category.slug));
    tags.add_dependency("category", &category.id);
    tags.add_dependency("category", &category.slug);
    let title = format!("{}: {}", state.site().title, category.title);
    build_feed(
        &state,
        &headers,
        &title,
        &format!("/category/{}/feed.xml", category.slug),
        &format!("/category/{}", category.slug),
        Some(&filter),
        tags,
    )
    .await
}

/// フィードの XML を組み立てる。`self_path` はフィード、`alternate_path` は対応する HTML のページのパス
async fn build_feed(
    state: &AppState,
    headers: &HeaderMap,
    title: &str,
    self_path: &str,
    alternate_path: &str,
    filter: Option<&(String, String)>,
    mut tags: CacheTags,
) -> Response {
    tags.add_dependency("site", "global");
    tags.add_dependency("collection", "article");

    let base_url = public_base_url(state, headers);
    let article_schema = {
        let schemas = state.schemas.read().await;
        schemas.get("article").cloned()
    };
    let records = match article_schema {
        Some(schema) => {
            match list_published_articles(state, &schema, filter, FEED_ENTRIES, 0).await {
                Ok(page) => page.records,
                Err(e) => {
                    tracing::error!("Failed to list articles for {}: {}", self_path, e);
                    Vec::new()
                }
            }
        }
        None => Vec::new(),
    };
    let now = chrono::Utc::now();
    let records: Vec<_> = records
        .into_iter()
        .filter(|r| is_currently_published(&r.status, r.unpublish_at.as_deref(), now))
        .collect();

    let mut entries = String::new();
    let mut feed_updated: Option<String> = None;
    for record in &records {
        tags.add_dependency("article", &record.id);
        tags.add_dependency("article", &record.slug);

        let updated = atom_date(Some(&record.updated_at))
            .or_else(|| atom_date(record.published_at.as_deref()))
            .unwrap_or_else(|| now.to_rfc3339_opts(chrono::SecondsFormat::Secs, true));
        let published = atom_date(record.published_at.as_deref())
            .or_else(|| atom_date(Some(&record.created_at)));
        // RFC 3339 の UTC 表記どうしなので、文字列の比較で新しい方がわかる
        if feed_updated.as_deref().is_none_or(|u| updated.as_str() > u) {
            feed_updated = Some(updated.clone());
        }

        let blocks: Vec<BlockNode> = serde_json::from_str(&record.blocks_json).unwrap_or_default();
        let summary = record
            .custom_fields
            .get("excerpt")
            .and_then(|v| v.as_str())
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_string)
            .unwrap_or_else(|| extract_excerpt(&blocks, 200));
        let content_html = render::render_content_html(state, &blocks).await;

        entries.push_str("  <entry>\n");
        entries.push_str(&format!(
            "    <title>{}</title>\n",
            escape_html(&record.title)
        ));
        entries.push_str(&format!(
            "    <link rel=\"alternate\" type=\"text/html\" href=\"{}/articles/{}\"/>\n",
            escape_html(&base_url),
            escape_html(&record.slug)
        ));
        // スラッグを変えても同じ記事とわかるよう、ID は記事の ID の URL（/articles/{id} でも開ける）
        entries.push_str(&format!(
            "    <id>{}/articles/{}</id>\n",
            escape_html(&base_url),
            escape_html(&record.id)
        ));
        if let Some(published) = published {
            entries.push_str(&format!("    <published>{}</published>\n", published));
        }
        entries.push_str(&format!("    <updated>{}</updated>\n", updated));
        if !summary.is_empty() {
            entries.push_str(&format!(
                "    <summary>{}</summary>\n",
                escape_html(&summary)
            ));
        }
        entries.push_str(&format!(
            "    <content type=\"html\">{}</content>\n",
            escape_html(&content_html)
        ));
        entries.push_str("  </entry>\n");
    }

    let updated =
        feed_updated.unwrap_or_else(|| now.to_rfc3339_opts(chrono::SecondsFormat::Secs, true));
    let alternate_url = format!("{}{}", base_url, alternate_path);
    let mut xml = String::from("<?xml version=\"1.0\" encoding=\"utf-8\"?>\n");
    // 本文の相対 URL（/uploads/... など）はサイトの URL を基準に解決させる
    xml.push_str(&format!(
        "<feed xmlns=\"http://www.w3.org/2005/Atom\" xml:base=\"{}/\">\n",
        escape_html(&base_url)
    ));
    xml.push_str(&format!("  <title>{}</title>\n", escape_html(title)));
    let description = &state.config.theme.site_description;
    if !description.trim().is_empty() {
        xml.push_str(&format!(
            "  <subtitle>{}</subtitle>\n",
            escape_html(description)
        ));
    }
    xml.push_str(&format!(
        "  <link rel=\"self\" type=\"application/atom+xml\" href=\"{}{}\"/>\n",
        escape_html(&base_url),
        escape_html(self_path)
    ));
    xml.push_str(&format!(
        "  <link rel=\"alternate\" type=\"text/html\" href=\"{}\"/>\n",
        escape_html(&alternate_url)
    ));
    xml.push_str(&format!("  <id>{}</id>\n", escape_html(&alternate_url)));
    xml.push_str(&format!("  <updated>{}</updated>\n", updated));
    xml.push_str(&format!(
        "  <author><name>{}</name></author>\n",
        escape_html(&state.site().title)
    ));
    xml.push_str(
        "  <generator uri=\"https://github.com/takumi-mr/aegiscms\">AegisCMS</generator>\n",
    );
    xml.push_str(&entries);
    xml.push_str("</feed>\n");

    let resp = (
        StatusCode::OK,
        [
            (header::CONTENT_TYPE, "application/atom+xml; charset=utf-8"),
            (header::CACHE_CONTROL, "public, max-age=600"),
        ],
        xml,
    )
        .into_response();
    crate::cdn::apply_cache_tags(state, resp, &tags)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_atom_date() {
        assert_eq!(
            atom_date(Some("2026-10-22T09:30:00+09:00")).as_deref(),
            Some("2026-10-22T00:30:00Z")
        );
        assert_eq!(
            atom_date(Some("2026-10-22 00:30:00")).as_deref(),
            Some("2026-10-22T00:30:00Z")
        );
        assert_eq!(atom_date(Some("not a date")), None);
        assert_eq!(atom_date(None), None);
    }

    #[test]
    fn test_feed_link_tag_escapes() {
        assert_eq!(
            feed_link_tag("A & \"B\"", "/feed.xml"),
            r#"<link rel="alternate" type="application/atom+xml" title="A &amp; &quot;B&quot;" href="/feed.xml">"#
        );
    }
}
