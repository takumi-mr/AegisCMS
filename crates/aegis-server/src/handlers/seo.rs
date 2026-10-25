use axum::{
    extract::State,
    http::{header, HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::sync::Arc;

use crate::audit::AuditContext;
use crate::auth::authorize::RequireRole;
use crate::auth::AdminSession;
use crate::seo::{
    self, default_robots_txt, SeoOverrides, CANONICAL_BASE_URL_SETTING, META_DESCRIPTION_SETTING,
    META_KEYWORDS_SETTING, NOINDEX_ALL_SETTING, OG_IMAGE_SETTING, ROBOTS_TXT_SETTING,
    TWITTER_CARD_SETTING,
};
use crate::state::AppState;
use aegis_ast::renderer::escape_html;
use aegis_core::{AuditAction, AuditActor, AuditOutcome};
use aegis_db::ContentListQuery;

type ApiError = (StatusCode, Json<serde_json::Value>);

/// デフォルト値情報
#[derive(Debug, Serialize)]
pub struct SeoDefaults {
    pub robots_txt: String,
    pub meta_description: String,
}

/// `GET` / `PUT /api/v1/seo` の応答
#[derive(Debug, Serialize)]
pub struct SeoSettingsResponse {
    pub robots_txt: Option<String>,
    pub meta_description: Option<String>,
    pub meta_keywords: Option<String>,
    pub canonical_base_url: Option<String>,
    pub og_image: Option<String>,
    pub noindex_all: bool,
    pub twitter_card: Option<String>,
    pub defaults: SeoDefaults,
}

/// `PUT /api/v1/seo` のリクエスト本文
#[derive(Debug, Default, Deserialize)]
pub struct UpdateSeoSettings {
    #[serde(default)]
    pub robots_txt: Option<String>,
    #[serde(default)]
    pub meta_description: Option<String>,
    #[serde(default)]
    pub meta_keywords: Option<String>,
    #[serde(default)]
    pub canonical_base_url: Option<String>,
    #[serde(default)]
    pub og_image: Option<String>,
    #[serde(default)]
    pub noindex_all: Option<bool>,
    #[serde(default)]
    pub twitter_card: Option<String>,
}

fn response(state: &AppState, overrides: SeoOverrides) -> SeoSettingsResponse {
    let defaults = SeoDefaults {
        robots_txt: default_robots_txt(overrides.canonical_base_url.as_deref()),
        meta_description: state.config.theme.site_description.clone(),
    };
    SeoSettingsResponse {
        robots_txt: overrides.robots_txt,
        meta_description: overrides.meta_description,
        meta_keywords: overrides.meta_keywords,
        canonical_base_url: overrides.canonical_base_url,
        og_image: overrides.og_image,
        noindex_all: overrides.noindex_all,
        twitter_card: overrides.twitter_card,
        defaults,
    }
}

fn bad_request(message: &str) -> ApiError {
    (StatusCode::BAD_REQUEST, Json(json!({ "error": message })))
}

/// 空白のみの文字列は `None`（未設定）として扱う
fn normalize_opt(val: Option<String>) -> Option<String> {
    val.map(|v| v.trim().to_string()).filter(|v| !v.is_empty())
}

/// `GET /api/v1/seo` — SEO & Bot 設定の取得（admin 必須）
pub async fn get_seo_settings(
    State(state): State<Arc<AppState>>,
    RequireRole(_session): AdminSession,
) -> Json<SeoSettingsResponse> {
    let overrides = state.seo();
    Json(response(&state, overrides))
}

/// `PUT /api/v1/seo` — SEO & Bot 設定の更新（admin 必須）
pub async fn update_seo_settings(
    State(state): State<Arc<AppState>>,
    RequireRole(session): AdminSession,
    audit_ctx: AuditContext,
    Json(payload): Json<UpdateSeoSettings>,
) -> Result<Json<SeoSettingsResponse>, ApiError> {
    let mut next = SeoOverrides {
        robots_txt: payload.robots_txt.filter(|s| !s.trim().is_empty()),
        meta_description: normalize_opt(payload.meta_description),
        meta_keywords: normalize_opt(payload.meta_keywords),
        canonical_base_url: normalize_opt(payload.canonical_base_url),
        og_image: normalize_opt(payload.og_image),
        noindex_all: payload.noindex_all.unwrap_or(false),
        twitter_card: normalize_opt(payload.twitter_card),
    };

    // バリデーション
    if let Some(ref robots) = next.robots_txt {
        seo::validate_robots_txt(robots).map_err(bad_request)?;
    }
    if let Some(ref base) = next.canonical_base_url {
        let normalized = seo::validate_canonical_base_url(base).map_err(bad_request)?;
        next.canonical_base_url = Some(normalized);
    }
    if let Some(ref img) = next.og_image {
        seo::validate_og_image(img).map_err(bad_request)?;
    }
    if let Some(ref desc) = next.meta_description {
        seo::validate_meta_description(desc).map_err(bad_request)?;
    }
    if let Some(ref kw) = next.meta_keywords {
        seo::validate_meta_keywords(kw).map_err(bad_request)?;
    }
    if let Some(ref card) = next.twitter_card {
        seo::validate_twitter_card(card).map_err(bad_request)?;
    }

    let previous = state.seo();
    let actor = Some(AuditActor {
        user_id: session.user_id,
        role: session.role,
    });

    let noindex_str = if next.noindex_all {
        Some("true".to_string())
    } else {
        None
    };

    // DB 保存
    let saved = async {
        let settings_to_save: [(&str, &Option<String>); 7] = [
            (ROBOTS_TXT_SETTING, &next.robots_txt),
            (META_DESCRIPTION_SETTING, &next.meta_description),
            (META_KEYWORDS_SETTING, &next.meta_keywords),
            (CANONICAL_BASE_URL_SETTING, &next.canonical_base_url),
            (OG_IMAGE_SETTING, &next.og_image),
            (NOINDEX_ALL_SETTING, &noindex_str),
            (TWITTER_CARD_SETTING, &next.twitter_card),
        ];

        for (name, val) in settings_to_save {
            match val {
                Some(v) => state.db.set_setting(name, v).await?,
                None => state.db.delete_setting(name).await?,
            }
        }
        Ok::<_, aegis_core::AegisError>(())
    }
    .await;

    if let Err(e) = saved {
        tracing::error!("Failed to save SEO settings: {}", e);
        state
            .audit
            .record(
                audit_ctx
                    .event(AuditAction::SeoUpdate, AuditOutcome::Failure)
                    .with_actor(actor)
                    .with_details(json!({ "error": e.to_string() })),
            )
            .await;
        return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": "Failed to save SEO settings" })),
        ));
    }

    // メモリ上のキャッシュ更新
    *state
        .seo
        .write()
        .unwrap_or_else(std::sync::PoisonError::into_inner) = next.clone();

    // 監査ログ記録
    state
        .audit
        .record(
            audit_ctx
                .event(AuditAction::SeoUpdate, AuditOutcome::Success)
                .with_actor(actor)
                .with_details(json!({
                    "robots_txt": { "changed": previous.robots_txt != next.robots_txt },
                    "meta_description": { "from": previous.meta_description, "to": next.meta_description },
                    "meta_keywords": { "from": previous.meta_keywords, "to": next.meta_keywords },
                    "canonical_base_url": { "from": previous.canonical_base_url, "to": next.canonical_base_url },
                    "og_image": { "from": previous.og_image, "to": next.og_image },
                    "noindex_all": { "from": previous.noindex_all, "to": next.noindex_all },
                    "twitter_card": { "from": previous.twitter_card, "to": next.twitter_card },
                })),
        )
        .await;

    Ok(Json(response(&state, next)))
}

/// `GET /robots.txt` — クローラー向け robots.txt 配信
pub async fn get_robots_txt(State(state): State<Arc<AppState>>) -> Response {
    let seo = state.seo();
    let content = seo.effective_robots_txt();

    (
        StatusCode::OK,
        [
            (header::CONTENT_TYPE, "text/plain; charset=utf-8"),
            (header::CACHE_CONTROL, "public, max-age=3600"),
        ],
        content,
    )
        .into_response()
}

/// 日付文字列（RFC 3339 等）から `YYYY-MM-DD` を安全に抽出
fn format_lastmod(date_str: Option<&str>) -> Option<String> {
    let s = date_str?.trim();
    if s.len() >= 10 {
        // YYYY-MM-DD
        let part = &s[..10];
        if part.chars().all(|c| c.is_ascii_digit() || c == '-') {
            return Some(part.to_string());
        }
    }
    None
}

/// `GET /sitemap.xml` — 検索エンジン向けサイトマップ XML 配信
pub async fn get_sitemap_xml(State(state): State<Arc<AppState>>, headers: HeaderMap) -> Response {
    let seo = state.seo();

    // noindex_all が有効な場合は空のサイトマップを返す
    if seo.noindex_all {
        let empty_xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<urlset xmlns="http://www.sitemaps.org/schemas/sitemap/0.9">
</urlset>"#;
        return (
            StatusCode::OK,
            [
                (header::CONTENT_TYPE, "application/xml; charset=utf-8"),
                (header::CACHE_CONTROL, "no-cache"),
            ],
            empty_xml,
        )
            .into_response();
    }

    // 基底 URL の解決: 設定の canonical_base_url が最優先。無ければリクエストの Host から推測
    let base_url = if let Some(ref base) = seo.canonical_base_url {
        base.trim_end_matches('/').to_string()
    } else {
        let host = headers
            .get(header::HOST)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("localhost:8080");
        let scheme = headers
            .get("x-forwarded-proto")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("http");
        format!("{}://{}", scheme, host.trim())
    };

    let mut xml = String::from("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
    xml.push_str("<urlset xmlns=\"http://www.sitemaps.org/schemas/sitemap/0.9\">\n");

    // 1. トップページ
    xml.push_str("  <url>\n");
    xml.push_str(&format!("    <loc>{}/</loc>\n", escape_html(&base_url)));
    xml.push_str("    <changefreq>daily</changefreq>\n");
    xml.push_str("    <priority>1.0</priority>\n");
    xml.push_str("  </url>\n");

    // 2. 記事一覧・商品一覧
    let schemas = state.schemas.read().await.clone();

    // 記事 (article)
    if let Some(schema) = schemas.get("article") {
        xml.push_str("  <url>\n");
        xml.push_str(&format!(
            "    <loc>{}/articles</loc>\n",
            escape_html(&base_url)
        ));
        xml.push_str("    <changefreq>daily</changefreq>\n");
        xml.push_str("    <priority>0.8</priority>\n");
        xml.push_str("  </url>\n");

        let query = ContentListQuery {
            status: Some("published".to_string()),
            limit: Some(1000),
            offset: 0,
        };
        if let Ok(page) = state.db.list_contents_page(schema, &query).await {
            let now = chrono::Utc::now();
            for item in page.records {
                if !super::content::is_currently_published(
                    &item.status,
                    item.unpublish_at.as_deref(),
                    now,
                ) {
                    continue;
                }
                xml.push_str("  <url>\n");
                xml.push_str(&format!(
                    "    <loc>{}/articles/{}</loc>\n",
                    escape_html(&base_url),
                    escape_html(&item.slug)
                ));
                let lastmod = format_lastmod(Some(&item.updated_at))
                    .or_else(|| format_lastmod(item.published_at.as_deref()));
                if let Some(lm) = lastmod {
                    xml.push_str(&format!("    <lastmod>{}</lastmod>\n", lm));
                }
                xml.push_str("    <changefreq>weekly</changefreq>\n");
                xml.push_str("    <priority>0.7</priority>\n");
                xml.push_str("  </url>\n");
            }
        }
    }

    // 商品 (product)
    if state.commerce.is_some() {
        let product_type = &state.config.commerce.product_type;
        if let Some(schema) = schemas.get(product_type) {
            xml.push_str("  <url>\n");
            xml.push_str(&format!(
                "    <loc>{}/products</loc>\n",
                escape_html(&base_url)
            ));
            xml.push_str("    <changefreq>daily</changefreq>\n");
            xml.push_str("    <priority>0.8</priority>\n");
            xml.push_str("  </url>\n");

            let query = ContentListQuery {
                status: Some("published".to_string()),
                limit: Some(1000),
                offset: 0,
            };
            if let Ok(page) = state.db.list_contents_page(schema, &query).await {
                let now = chrono::Utc::now();
                for item in page.records {
                    if !super::content::is_currently_published(
                        &item.status,
                        item.unpublish_at.as_deref(),
                        now,
                    ) {
                        continue;
                    }
                    xml.push_str("  <url>\n");
                    xml.push_str(&format!(
                        "    <loc>{}/products/{}</loc>\n",
                        escape_html(&base_url),
                        escape_html(&item.slug)
                    ));
                    let lastmod = format_lastmod(Some(&item.updated_at))
                        .or_else(|| format_lastmod(item.published_at.as_deref()));
                    if let Some(lm) = lastmod {
                        xml.push_str(&format!("    <lastmod>{}</lastmod>\n", lm));
                    }
                    xml.push_str("    <changefreq>weekly</changefreq>\n");
                    xml.push_str("    <priority>0.7</priority>\n");
                    xml.push_str("  </url>\n");
                }
            }
        }
    }

    // 固定ページ (page)
    if let Some(schema) = schemas.get("page") {
        let query = ContentListQuery {
            status: Some("published".to_string()),
            limit: Some(500),
            offset: 0,
        };
        if let Ok(page) = state.db.list_contents_page(schema, &query).await {
            let now = chrono::Utc::now();
            for item in page.records {
                if !super::content::is_currently_published(
                    &item.status,
                    item.unpublish_at.as_deref(),
                    now,
                ) {
                    continue;
                }
                xml.push_str("  <url>\n");
                xml.push_str(&format!(
                    "    <loc>{}/{}</loc>\n",
                    escape_html(&base_url),
                    escape_html(&item.slug)
                ));
                let lastmod = format_lastmod(Some(&item.updated_at))
                    .or_else(|| format_lastmod(item.published_at.as_deref()));
                if let Some(lm) = lastmod {
                    xml.push_str(&format!("    <lastmod>{}</lastmod>\n", lm));
                }
                xml.push_str("    <changefreq>monthly</changefreq>\n");
                xml.push_str("    <priority>0.6</priority>\n");
                xml.push_str("  </url>\n");
            }
        }
    }

    xml.push_str("</urlset>\n");

    (
        StatusCode::OK,
        [
            (header::CONTENT_TYPE, "application/xml; charset=utf-8"),
            (header::CACHE_CONTROL, "public, max-age=3600"),
        ],
        xml,
    )
        .into_response()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::AegisConfig;
    use crate::test_support::test_state_with_config;
    use aegis_auth::SessionData;
    use aegis_runtime::PluginSupervisor;

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

    #[test]
    fn test_format_lastmod() {
        assert_eq!(
            format_lastmod(Some("2026-10-25T10:00:00Z")),
            Some("2026-10-25".to_string())
        );
        assert_eq!(
            format_lastmod(Some("2026-10-25")),
            Some("2026-10-25".to_string())
        );
        assert_eq!(format_lastmod(Some("invalid")), None);
        assert_eq!(format_lastmod(None), None);
    }

    #[tokio::test]
    async fn test_seo_settings_api_and_endpoints() {
        let mut config = AegisConfig::default();
        config.theme.site_description = "Default Theme Desc".to_string();
        let state = test_state_with_config(config, PluginSupervisor::new()).await;

        // 1. 初期取得 (GET)
        let Json(initial) = get_seo_settings(State(state.clone()), admin()).await;
        assert_eq!(initial.robots_txt, None);
        assert_eq!(initial.meta_description, None);
        assert!(!initial.noindex_all);
        assert_eq!(initial.defaults.meta_description, "Default Theme Desc");
        assert!(initial.defaults.robots_txt.contains("User-agent: *"));

        // デフォルト robots.txt エンドポイント
        let res = get_robots_txt(State(state.clone())).await;
        assert_eq!(res.status(), StatusCode::OK);

        // 2. 更新 (PUT)
        let update_payload = UpdateSeoSettings {
            robots_txt: Some("User-agent: *\nDisallow: /private/\n".to_string()),
            meta_description: Some("Custom SEO description".to_string()),
            meta_keywords: Some("cms, security, seo".to_string()),
            canonical_base_url: Some("https://example.com/".to_string()),
            og_image: Some("/uploads/og.png".to_string()),
            noindex_all: Some(false),
            twitter_card: Some("summary_large_image".to_string()),
        };

        let Json(updated) = update_seo_settings(
            State(state.clone()),
            admin(),
            AuditContext::default(),
            Json(update_payload),
        )
        .await
        .unwrap();

        assert_eq!(
            updated.robots_txt.as_deref(),
            Some("User-agent: *\nDisallow: /private/\n")
        );
        assert_eq!(
            updated.meta_description.as_deref(),
            Some("Custom SEO description")
        );
        assert_eq!(
            updated.canonical_base_url.as_deref(),
            Some("https://example.com")
        );

        // state.seo() に即時反映されていること
        assert_eq!(
            state.seo().robots_txt.as_deref(),
            Some("User-agent: *\nDisallow: /private/\n")
        );
        assert_eq!(
            state.seo().canonical_base_url.as_deref(),
            Some("https://example.com")
        );

        // robots.txt エンドポイントが更新された内容を返すこと
        let res = get_robots_txt(State(state.clone())).await;
        let body_bytes = axum::body::to_bytes(res.into_body(), usize::MAX)
            .await
            .unwrap();
        let body_str = String::from_utf8(body_bytes.to_vec()).unwrap();
        assert!(body_str.contains("Disallow: /private/"));

        // sitemap.xml エンドポイントのテスト
        let headers = HeaderMap::new();
        let sitemap_res = get_sitemap_xml(State(state.clone()), headers).await;
        let sitemap_bytes = axum::body::to_bytes(sitemap_res.into_body(), usize::MAX)
            .await
            .unwrap();
        let sitemap_str = String::from_utf8(sitemap_bytes.to_vec()).unwrap();
        assert!(sitemap_str.contains("<urlset"));
        assert!(sitemap_str.contains("<loc>https://example.com/</loc>"));

        // noindex_all = true にしたとき、sitemap が空になること
        let noindex_update = UpdateSeoSettings {
            noindex_all: Some(true),
            ..Default::default()
        };
        let _ = update_seo_settings(
            State(state.clone()),
            admin(),
            AuditContext::default(),
            Json(noindex_update),
        )
        .await
        .unwrap();

        let sitemap_res = get_sitemap_xml(State(state.clone()), HeaderMap::new()).await;
        let sitemap_bytes = axum::body::to_bytes(sitemap_res.into_body(), usize::MAX)
            .await
            .unwrap();
        let sitemap_str = String::from_utf8(sitemap_bytes.to_vec()).unwrap();
        assert!(sitemap_str.contains("<urlset"));
        assert!(!sitemap_str.contains("<loc>"));
    }
}
