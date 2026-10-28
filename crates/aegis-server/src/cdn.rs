//! 公開ページのレスポンスへの `Cache-Tag` / `Surrogate-Key` ヘッダー付与と、
//! コンテンツ・設定の更新時における CDN キャッシュの無効化ヘルパー。

use std::collections::HashMap;

use axum::{
    http::{header, HeaderValue, StatusCode},
    response::Response,
};

pub use aegis_cache::CacheTags;
use aegis_db::ResolvedRelation;

use crate::state::AppState;

/// 公開ページの `200 OK` レスポンスに `[cdn]` 設定に基づいて `Cache-Tag`、`Surrogate-Key`、
/// および `Cache-Control` ヘッダーを付与する。`[cdn].enabled = false` のときは何もしない。
pub fn apply_cache_tags(state: &AppState, mut response: Response, tags: &CacheTags) -> Response {
    if !state.config.cdn.enabled || response.status() != StatusCode::OK {
        return response;
    }

    let headers = response.headers_mut();
    if state.config.cdn.emit_headers && !tags.is_empty() {
        tags.write_headers(headers);
    }

    let cc = state.config.cdn.cache_control.trim();
    if !cc.is_empty() && !headers.contains_key(header::CACHE_CONTROL) {
        if let Ok(val) = HeaderValue::from_str(cc) {
            headers.insert(header::CACHE_CONTROL, val);
        }
    }

    response
}

/// 解決済みリレーションから参照先コンテンツの依存タグ（`{content_type}-{id}` と `{content_type}-{slug}`）を追加する
pub fn add_relation_tags(tags: &mut CacheTags, relations: &HashMap<String, ResolvedRelation>) {
    for rel in relations.values() {
        tags.add_dependency(&rel.content_type, &rel.id);
        tags.add_dependency(&rel.content_type, &rel.slug);
    }
}

/// スキーマの `relation` フィールド値から参照先コンテンツの依存タグ（`{target_type}-{id_or_slug}`）を追加する
pub fn add_field_relation_tags(
    tags: &mut CacheTags,
    schema: &aegis_schema::SchemaDefinition,
    fields: &HashMap<String, serde_json::Value>,
) {
    for field in &schema.fields {
        if field.field_type == aegis_schema::FieldType::Relation {
            let target_type = field
                .target
                .as_deref()
                .unwrap_or(&field.name)
                .split('.')
                .next()
                .unwrap_or(&field.name);
            if let Some(val) = fields
                .get(&field.name)
                .and_then(|v| v.as_str())
                .map(str::trim)
                .filter(|s| !s.is_empty())
            {
                tags.add_dependency(target_type, val);
            }
        }
    }
}

/// コンテンツ（記事・固定ページ・商品など）の公開・更新・非公開・削除時に無効化すべきキャッシュタグ一式を組み立てる
pub fn content_invalidation_tags(
    state: &AppState,
    content_type: &str,
    id: &str,
    slug: &str,
    old_slug: Option<&str>,
) -> CacheTags {
    let mut tags = CacheTags::new();
    tags.add_dependency(content_type, id);
    tags.add_dependency(content_type, slug);
    tags.add_dependency("collection", content_type);
    tags.add_dependency("seo", "sitemap");
    tags.add_url("/sitemap.xml");
    tags.add_url(&format!("/{}", slug));

    if content_type == "article" {
        tags.add_url("/");
        tags.add_url("/articles");
        tags.add_url(&format!("/articles/{}", slug));
    }
    if content_type == state.config.commerce.product_type {
        tags.add_url("/products");
        tags.add_url(&format!("/products/{}", slug));
    }

    if let Some(old) = old_slug.filter(|s| !s.is_empty() && *s != slug) {
        tags.add_dependency(content_type, old);
        tags.add_url(&format!("/{}", old));
        if content_type == "article" {
            tags.add_url(&format!("/articles/{}", old));
        }
        if content_type == state.config.commerce.product_type {
            tags.add_url(&format!("/products/{}", old));
        }
    }

    tags
}

/// 指定したタグを CDN からパージする（`[cdn].enabled = false` または `provider = "none"` なら何もしない）。
/// 外部 CDN API の一時的なエラーは警告ログに記録し、呼び出し元のリクエストは失敗させない。
pub async fn purge_tags(state: &AppState, tags: &CacheTags) {
    if !state.config.cdn.enabled || !state.cdn.is_enabled() || tags.is_empty() {
        return;
    }
    if let Err(e) = state.cdn.purge_tags(tags.tags()).await {
        tracing::warn!(
            "CDN cache purge failed (provider: {}, tags: {:?}): {}",
            state.cdn.name(),
            tags.tags(),
            e
        );
    }
}

/// コンテンツの変更に伴う関連タグ一式を CDN からパージする
pub async fn purge_content(
    state: &AppState,
    content_type: &str,
    id: &str,
    slug: &str,
    old_slug: Option<&str>,
) {
    let tags = content_invalidation_tags(state, content_type, id, slug, old_slug);
    purge_tags(state, &tags).await;
}

/// サイト全体（サイト名・favicon・テーマ・共通ヘッダー等）の変更時に `site-global` タグをパージする
pub async fn purge_site_global(state: &AppState) {
    let mut tags = CacheTags::new();
    tags.add_dependency("site", "global");
    purge_tags(state, &tags).await;
}

/// CDN 上のキャッシュをすべてパージする
#[allow(dead_code)]
pub async fn purge_all(state: &AppState) {
    if !state.config.cdn.enabled || !state.cdn.is_enabled() {
        return;
    }
    if let Err(e) = state.cdn.purge_all().await {
        tracing::warn!(
            "CDN cache purge_all failed (provider: {}): {}",
            state.cdn.name(),
            e
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audit::AuditContext;
    use crate::auth::authorize::RequireRole;
    use crate::auth::{AdminSession, EditorSession};
    use crate::config::AegisConfig;
    use crate::handlers::content::{
        create_content, delete_content, update_content, CreateContentRequest, STATUS_SCHEDULED,
    };
    use crate::handlers::headers::{update_headers_settings, UpdateHeadersSettings};
    use crate::handlers::public::{render_article, render_index, render_page_or_slug, IndexParams};
    use crate::handlers::seo::{
        get_robots_txt, get_sitemap_xml, update_seo_settings, UpdateSeoSettings,
    };
    use crate::handlers::site::{update_site_settings, UpdateSiteSettings};
    use crate::scheduler::{publish_due_contents, unpublish_due_contents};
    use crate::test_support::test_app_state;
    use aegis_auth::SessionData;
    use aegis_cache::{CdnProviderKind, RecordingCdnPurger};
    use aegis_runtime::PluginSupervisor;
    use axum::extract::{Path, Query, State};
    use axum::http::HeaderMap;
    use axum::response::IntoResponse;
    use axum::Json;
    use std::sync::Arc;

    fn take_tags(purger: &RecordingCdnPurger) -> Vec<String> {
        purger.take_batches().into_iter().flatten().collect()
    }

    fn editor() -> EditorSession {
        RequireRole(SessionData {
            id: "ses_editor".to_string(),
            user_id: "editor1".to_string(),
            role: "editor".to_string(),
            created_at: String::new(),
            expires_at: String::new(),
            metadata: Default::default(),
        })
    }

    fn admin() -> AdminSession {
        RequireRole(SessionData {
            id: "ses_admin".to_string(),
            user_id: "admin1".to_string(),
            role: "admin".to_string(),
            created_at: String::new(),
            expires_at: String::new(),
            metadata: Default::default(),
        })
    }

    #[tokio::test]
    async fn test_cdn_disabled_neither_emits_headers_nor_purges() {
        let mut config = AegisConfig::default();
        config.cdn.enabled = false;
        let purger = Arc::new(RecordingCdnPurger::new());
        let state = Arc::new(
            test_app_state(config, PluginSupervisor::new(), Vec::new())
                .await
                .with_cdn(purger.clone()),
        );

        let _ = create_content(
            State(state.clone()),
            editor(),
            Path("article".to_string()),
            AuditContext::default(),
            Json(CreateContentRequest {
                title: "Hello".to_string(),
                slug: "hello".to_string(),
                status: "published".to_string(),
                blocks: Vec::new(),
                author_id: None,
                published_at: None,
                unpublish_at: None,
                publish: true,
                fields: HashMap::new(),
            }),
        )
        .await
        .unwrap();

        assert!(purger.purged_tags().is_empty());

        let resp = render_index(State(state.clone()), Query(IndexParams { page: None }))
            .await
            .into_response();
        assert_eq!(resp.status(), StatusCode::OK);
        assert!(resp.headers().get("cache-tag").is_none());
        assert!(resp.headers().get("surrogate-key").is_none());
    }

    #[tokio::test]
    async fn test_cdn_enabled_emits_cache_tags_and_purges_on_mutations_and_schedules() {
        let mut config = AegisConfig::default();
        config.cdn.enabled = true;
        config.cdn.provider = CdnProviderKind::Log;
        config.cdn.cache_control =
            "public, max-age=0, s-maxage=86400, stale-while-revalidate=60".to_string();

        let purger = Arc::new(RecordingCdnPurger::new());
        let state = Arc::new(
            test_app_state(config, PluginSupervisor::new(), Vec::new())
                .await
                .with_cdn(purger.clone()),
        );

        // 1. 公開記事の作成 -> タグがパージされる
        let _ = create_content(
            State(state.clone()),
            editor(),
            Path("article".to_string()),
            AuditContext::default(),
            Json(CreateContentRequest {
                title: "First Post".to_string(),
                slug: "first-post".to_string(),
                status: "published".to_string(),
                blocks: Vec::new(),
                author_id: None,
                published_at: None,
                unpublish_at: None,
                publish: true,
                fields: HashMap::new(),
            }),
        )
        .await
        .unwrap();

        let created_tags = take_tags(&purger);
        assert!(created_tags.contains(&"article-first-post".to_string()));
        assert!(created_tags.contains(&"collection-article".to_string()));
        assert!(created_tags.contains(&"url-index".to_string()));
        assert!(created_tags.contains(&"url-articles-first-post".to_string()));
        assert!(created_tags.contains(&"url-sitemap.xml".to_string()));

        // 2. 公開ページへのリクエストで Cache-Tag / Surrogate-Key / Cache-Control が注入される
        let index_resp = render_index(State(state.clone()), Query(IndexParams { page: None }))
            .await
            .into_response();
        assert_eq!(index_resp.status(), StatusCode::OK);
        let index_cache_tag = index_resp
            .headers()
            .get("cache-tag")
            .unwrap()
            .to_str()
            .unwrap();
        let index_surrogate = index_resp
            .headers()
            .get("surrogate-key")
            .unwrap()
            .to_str()
            .unwrap();
        assert!(index_cache_tag.contains("url-index"));
        assert!(index_cache_tag.contains("site-global"));
        assert!(index_cache_tag.contains("collection-article"));
        assert!(index_cache_tag.contains("article-first-post"));
        assert!(index_surrogate.contains("url-index url-articles site-global"));
        assert_eq!(
            index_resp
                .headers()
                .get("cache-control")
                .unwrap()
                .to_str()
                .unwrap(),
            "public, max-age=0, s-maxage=86400, stale-while-revalidate=60"
        );

        let article_resp = render_article(State(state.clone()), Path("first-post".to_string()))
            .await
            .into_response();
        assert_eq!(article_resp.status(), StatusCode::OK);
        let article_cache_tag = article_resp
            .headers()
            .get("cache-tag")
            .unwrap()
            .to_str()
            .unwrap();
        assert!(article_cache_tag.contains("url-articles-first-post"));
        assert!(article_cache_tag.contains("article-first-post"));

        // 404 のレスポンスには Cache-Tag を付与しない
        let missing_resp = render_article(State(state.clone()), Path("does-not-exist".to_string()))
            .await
            .into_response();
        assert_eq!(missing_resp.status(), StatusCode::NOT_FOUND);
        assert!(missing_resp.headers().get("cache-tag").is_none());

        // 3. スラッグ変更を伴う更新 -> 新旧両方のスラッグ・URLタグがパージされる
        let _ = update_content(
            State(state.clone()),
            editor(),
            Path(("article".to_string(), "first-post".to_string())),
            AuditContext::default(),
            Json(CreateContentRequest {
                title: "First Post Renamed".to_string(),
                slug: "renamed-post".to_string(),
                status: "published".to_string(),
                blocks: Vec::new(),
                author_id: None,
                published_at: None,
                unpublish_at: None,
                publish: true,
                fields: HashMap::new(),
            }),
        )
        .await
        .unwrap();

        let updated_tags = take_tags(&purger);
        assert!(updated_tags.contains(&"article-renamed-post".to_string()));
        assert!(updated_tags.contains(&"article-first-post".to_string()));
        assert!(updated_tags.contains(&"url-articles-renamed-post".to_string()));
        assert!(updated_tags.contains(&"url-articles-first-post".to_string()));

        // 4. 固定ページの公開と `GET /{slug}` のタグ確認 + 削除時のパージ
        let _ = create_content(
            State(state.clone()),
            editor(),
            Path("page".to_string()),
            AuditContext::default(),
            Json(CreateContentRequest {
                title: "About Us".to_string(),
                slug: "about".to_string(),
                status: "published".to_string(),
                blocks: Vec::new(),
                author_id: None,
                published_at: None,
                unpublish_at: None,
                publish: true,
                fields: HashMap::new(),
            }),
        )
        .await
        .unwrap();
        let _ = take_tags(&purger);

        let page_resp = render_page_or_slug(State(state.clone()), Path("about".to_string()))
            .await
            .into_response();
        assert_eq!(page_resp.status(), StatusCode::OK);
        let page_cache_tag = page_resp
            .headers()
            .get("cache-tag")
            .unwrap()
            .to_str()
            .unwrap();
        assert!(page_cache_tag.contains("url-about"));
        assert!(page_cache_tag.contains("page-about"));

        let _ = delete_content(
            State(state.clone()),
            editor(),
            Path(("page".to_string(), "about".to_string())),
            AuditContext::default(),
        )
        .await
        .unwrap();
        let deleted_tags = take_tags(&purger);
        assert!(deleted_tags.contains(&"page-about".to_string()));
        assert!(deleted_tags.contains(&"url-about".to_string()));

        // 5. 予約公開と予約非公開によるパージ
        let future_pub = (chrono::Utc::now() + chrono::Duration::minutes(10)).to_rfc3339();
        let future_unpub = (chrono::Utc::now() + chrono::Duration::minutes(20)).to_rfc3339();
        let _ = create_content(
            State(state.clone()),
            editor(),
            Path("article".to_string()),
            AuditContext::default(),
            Json(CreateContentRequest {
                title: "Scheduled Post".to_string(),
                slug: "scheduled-post".to_string(),
                status: STATUS_SCHEDULED.to_string(),
                blocks: Vec::new(),
                author_id: None,
                published_at: Some(future_pub),
                unpublish_at: Some(future_unpub),
                publish: false,
                fields: HashMap::new(),
            }),
        )
        .await
        .unwrap();
        // 予約状態での作成時はまだ公開されていないのでパージされない
        assert!(take_tags(&purger).is_empty());

        // 15分後にスケジューラ実行 -> 公開されてパージされる
        let pub_count =
            publish_due_contents(&state, chrono::Utc::now() + chrono::Duration::minutes(15)).await;
        assert_eq!(pub_count, 1);
        let sched_pub_tags = take_tags(&purger);
        assert!(sched_pub_tags.contains(&"article-scheduled-post".to_string()));
        assert!(sched_pub_tags.contains(&"url-articles-scheduled-post".to_string()));

        // 25分後にスケジューラ実行 -> 非公開化されてパージされる
        let unpub_count =
            unpublish_due_contents(&state, chrono::Utc::now() + chrono::Duration::minutes(25))
                .await;
        assert_eq!(unpub_count, 1);
        let sched_unpub_tags = take_tags(&purger);
        assert!(sched_unpub_tags.contains(&"article-scheduled-post".to_string()));

        // 6. robots.txt / sitemap.xml のタグと、サイト・SEO・ヘッダー設定更新時のパージ
        let robots_resp = get_robots_txt(State(state.clone())).await;
        assert!(robots_resp
            .headers()
            .get("cache-tag")
            .unwrap()
            .to_str()
            .unwrap()
            .contains("seo-robots"));

        let sitemap_resp = get_sitemap_xml(State(state.clone()), HeaderMap::new()).await;
        assert!(sitemap_resp
            .headers()
            .get("cache-tag")
            .unwrap()
            .to_str()
            .unwrap()
            .contains("seo-sitemap"));

        let _ = update_site_settings(
            State(state.clone()),
            admin(),
            AuditContext::default(),
            Json(UpdateSiteSettings {
                title: Some("New Site Title".to_string()),
                favicon: None,
            }),
        )
        .await
        .unwrap();
        assert_eq!(take_tags(&purger), vec!["site-global".to_string()]);

        let _ = update_seo_settings(
            State(state.clone()),
            admin(),
            AuditContext::default(),
            Json(UpdateSeoSettings {
                meta_description: Some("Updated SEO".to_string()),
                ..Default::default()
            }),
        )
        .await
        .unwrap();
        let seo_tags = take_tags(&purger);
        assert!(seo_tags.contains(&"site-global".to_string()));
        assert!(seo_tags.contains(&"seo-robots".to_string()));
        assert!(seo_tags.contains(&"seo-sitemap".to_string()));

        let _ = update_headers_settings(
            State(state.clone()),
            admin(),
            AuditContext::default(),
            Json(UpdateHeadersSettings::default()),
        )
        .await
        .unwrap();
        assert_eq!(take_tags(&purger), vec!["site-global".to_string()]);
    }
}
