//! リンク切れ検査サービス（`BrokenLinksService`）
//!
//! 公開中の記事・固定ページ等に含まれる内部・外部リンクを走査・検証し、
//! リンク切れレポートを生成・保持します。

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use aegis_ast::extract_links;
use aegis_ast::BlockNode;
use aegis_ast::HtmlRenderer;
use aegis_core::{AegisError, Result};
use aegis_db::ContentListQuery;
use chrono::Utc;
use serde::{Deserialize, Serialize};

use crate::state::AppState;

pub const SETTING_KEY: &str = "broken_links.report";

/// 検出されたリンク切れの 1 件
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BrokenLinkItem {
    pub id: String,
    pub content_type: String,
    pub content_id: String,
    pub content_title: String,
    pub content_slug: String,
    pub target_url: String,
    pub is_external: bool,
    pub status_code: Option<u16>,
    pub error_message: Option<String>,
}

/// リンク切れレポート
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct BrokenLinksReport {
    pub last_scanned_at: Option<String>,
    pub total_links_checked: usize,
    pub broken_count: usize,
    pub items: Vec<BrokenLinkItem>,
}

/// 検査状態レスポンス
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BrokenLinksStatusResponse {
    pub is_scanning: bool,
    pub report: BrokenLinksReport,
}

/// リンク切れ検査サービス
#[derive(Clone, Default)]
pub struct BrokenLinksService {
    is_scanning: Arc<AtomicBool>,
}

impl BrokenLinksService {
    pub fn new() -> Self {
        Self {
            is_scanning: Arc::new(AtomicBool::new(false)),
        }
    }

    /// 現在検査が実行中かどうか
    pub fn is_scanning(&self) -> bool {
        self.is_scanning.load(Ordering::SeqCst)
    }

    /// レポートを DB から取得
    pub async fn get_report(&self, state: &AppState) -> BrokenLinksReport {
        match state.db.list_settings().await {
            Ok(map) => {
                if let Some(raw) = map.get(SETTING_KEY) {
                    serde_json::from_str(raw).unwrap_or_default()
                } else {
                    BrokenLinksReport::default()
                }
            }
            _ => BrokenLinksReport::default(),
        }
    }

    /// レポートを DB へ保存
    pub async fn save_report(&self, state: &AppState, report: &BrokenLinksReport) -> Result<()> {
        let json =
            serde_json::to_string(report).map_err(|e| AegisError::Internal(e.to_string()))?;
        state.db.set_setting(SETTING_KEY, &json).await
    }

    /// バックグラウンド検査タスクを起動
    pub fn start_scan_task(&self, state: Arc<AppState>) -> bool {
        if self
            .is_scanning
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_err()
        {
            return false;
        }

        let is_scanning = self.is_scanning.clone();
        let this = self.clone();
        tokio::spawn(async move {
            let result = this.run_scan(&state).await;
            if let Ok(report) = result {
                let _ = this.save_report(&state, &report).await;
            }
            is_scanning.store(false, Ordering::SeqCst);
        });
        true
    }

    /// 検査処理の実行
    pub async fn run_scan(&self, state: &AppState) -> Result<BrokenLinksReport> {
        let mut target_contents = Vec::new();

        // 走査対象のスキーマ
        let schemas = state.schemas.read().await;
        let mut types_to_scan = vec!["article".to_string(), "page".to_string()];
        if state.commerce.is_some() {
            types_to_scan.push(state.config.commerce.product_type.clone());
        }

        for c_type in types_to_scan {
            if let Some(schema) = schemas.get(&c_type) {
                let query = ContentListQuery {
                    limit: Some(1000),
                    ..Default::default()
                };
                if let Ok(page) = state.db.list_published_contents_page(schema, &query).await {
                    for record in page.records {
                        target_contents.push(record);
                    }
                }
            }
        }
        drop(schemas);

        let mut total_links_checked = 0;
        let mut broken_items = Vec::new();
        // 同一 URL の検査結果キャッシュ: url -> Result<(), (Option<u16>, String)>
        let mut url_cache: HashMap<String, std::result::Result<(), (Option<u16>, String)>> =
            HashMap::new();

        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(5))
            .redirect(reqwest::redirect::Policy::limited(5))
            .user_agent("Mozilla/5.0 (compatible; AegisCMS-LinkChecker/1.0)")
            .build()
            .map_err(|e| AegisError::Internal(e.to_string()))?;

        for record in &target_contents {
            let blocks: Vec<BlockNode> =
                serde_json::from_str(&record.blocks_json).unwrap_or_default();
            let html = HtmlRenderer::render_nodes(&blocks);
            let links = extract_links(&html);

            for link in links {
                total_links_checked += 1;

                let check_res = if let Some(cached) = url_cache.get(&link) {
                    cached.clone()
                } else {
                    let res = check_single_link(state, &client, &link).await;
                    url_cache.insert(link.clone(), res.clone());
                    res
                };

                if let Err((status_code, error_message)) = check_res {
                    broken_items.push(BrokenLinkItem {
                        id: format!("{}-{}", record.id, broken_items.len()),
                        content_type: record.content_type.clone(),
                        content_id: record.id.clone(),
                        content_title: record.title.clone(),
                        content_slug: record.slug.clone(),
                        target_url: link.clone(),
                        is_external: is_external_link(&link),
                        status_code,
                        error_message: Some(error_message),
                    });
                }
            }
        }

        let broken_count = broken_items.len();
        Ok(BrokenLinksReport {
            last_scanned_at: Some(Utc::now().to_rfc3339()),
            total_links_checked,
            broken_count,
            items: broken_items,
        })
    }
}

/// 外部リンクかどうかを判定
fn is_external_link(url: &str) -> bool {
    let lower = url.to_ascii_lowercase();
    lower.starts_with("http://") || lower.starts_with("https://")
}

/// 単一リンクの検査
async fn check_single_link(
    state: &AppState,
    client: &reqwest::Client,
    link: &str,
) -> std::result::Result<(), (Option<u16>, String)> {
    if !is_external_link(link) {
        // 内部リンク
        check_internal_link(state, client, link).await
    } else {
        // 外部リンク
        check_external_link(client, link).await
    }
}

/// 内部リンクの検査
async fn check_internal_link(
    state: &AppState,
    client: &reqwest::Client,
    path: &str,
) -> std::result::Result<(), (Option<u16>, String)> {
    let trimmed = path.trim();
    let clean_path = if let Some(idx) = trimmed.find(['?', '#']) {
        &trimmed[..idx]
    } else {
        trimmed
    };

    // 1. まずローカルの HTTP サーバーにリクエストを試みる
    let host = &state.config.server.host;
    let port = state.config.server.port;
    let local_url = format!("http://{}:{}{}", host, port, clean_path);

    if let Ok(resp) = client.head(&local_url).send().await {
        let status = resp.status();
        if status.is_success() || status.is_redirection() {
            return Ok(());
        } else {
            return Err((Some(status.as_u16()), status.to_string()));
        }
    }

    // 2. HTTP 接続できない場合（テスト環境や未起動時）、DB ルーティングでフォールバック検証
    let norm = clean_path.trim_start_matches('/');
    if norm.is_empty() || norm == "tags" || norm == "feed.xml" || norm == "search" {
        return Ok(());
    }

    let schemas = state.schemas.read().await;

    // /articles/{slug}
    if let Some(slug) = norm.strip_prefix("articles/") {
        if let Some(schema) = schemas.get("article") {
            if let Ok(Some(_)) = state.db.get_published_content(schema, slug).await {
                return Ok(());
            }
        }
        return Err((Some(404), "Article not found".to_string()));
    }

    // /category/{slug}
    if let Some(slug) = norm.strip_prefix("category/") {
        if let Some(schema) = schemas.get("category") {
            if let Ok(Some(_)) = state.db.get_published_content(schema, slug).await {
                return Ok(());
            }
        }
        return Err((Some(404), "Category not found".to_string()));
    }

    // /tags/{slug}
    if let Some(slug) = norm.strip_prefix("tags/") {
        if let Ok(Some(_)) = state.db.get_tag_by_slug(slug).await {
            return Ok(());
        }
        return Err((Some(404), "Tag not found".to_string()));
    }

    // /{slug} (page または article)
    if let Some(schema) = schemas.get("page") {
        if let Ok(Some(_)) = state.db.get_published_content(schema, norm).await {
            return Ok(());
        }
    }
    if let Some(schema) = schemas.get("article") {
        if let Ok(Some(_)) = state.db.get_published_content(schema, norm).await {
            return Ok(());
        }
    }

    Err((Some(404), "Page not found".to_string()))
}

/// 外部リンクの検査
async fn check_external_link(
    client: &reqwest::Client,
    url: &str,
) -> std::result::Result<(), (Option<u16>, String)> {
    // SSRF 防止: プライベート IP や localhost へのアクセスを遮断
    if is_private_or_loopback_host(url) {
        return Err((None, "Blocked private or loopback IP address".to_string()));
    }

    // 1. HEAD リクエスト
    match client.head(url).send().await {
        Ok(resp) => {
            let status = resp.status();
            if status.is_success() || status.is_redirection() {
                Ok(())
            } else if status.as_u16() == 405 || status.as_u16() == 403 {
                // HEAD が拒否された場合は GET で再試行
                match client.get(url).send().await {
                    Ok(get_resp) => {
                        let get_status = get_resp.status();
                        if get_status.is_success() || get_status.is_redirection() {
                            Ok(())
                        } else {
                            Err((Some(get_status.as_u16()), get_status.to_string()))
                        }
                    }
                    Err(e) => Err((None, e.to_string())),
                }
            } else {
                Err((Some(status.as_u16()), status.to_string()))
            }
        }
        Err(_) => {
            // HEAD が失敗した場合も GET で一度再試行
            match client.get(url).send().await {
                Ok(get_resp) => {
                    let get_status = get_resp.status();
                    if get_status.is_success() || get_status.is_redirection() {
                        Ok(())
                    } else {
                        Err((Some(get_status.as_u16()), get_status.to_string()))
                    }
                }
                Err(e) => Err((None, e.to_string())),
            }
        }
    }
}

/// URL のホストがループバックまたはプライベート IP かを判定（SSRF 防御）
fn is_private_or_loopback_host(url_str: &str) -> bool {
    let Ok(parsed) = url::Url::parse(url_str) else {
        return true;
    };
    let Some(host) = parsed.host_str() else {
        return true;
    };
    let host_lower = host.to_ascii_lowercase();

    if host_lower == "localhost" || host_lower.ends_with(".localhost") {
        return true;
    }

    if let Ok(ip) = host_lower.parse::<std::net::IpAddr>() {
        match ip {
            std::net::IpAddr::V4(ipv4) => {
                ipv4.is_loopback()
                    || ipv4.is_private()
                    || ipv4.is_link_local()
                    || ipv4.is_broadcast()
                    || ipv4.is_unspecified()
            }
            std::net::IpAddr::V6(ipv6) => ipv6.is_loopback() || ipv6.is_unspecified(),
        }
    } else {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use aegis_ast::BlockNode;
    use aegis_db::ContentRecord;
    use aegis_runtime::PluginSupervisor;
    use std::collections::HashMap;

    #[tokio::test]
    async fn test_broken_links_report_roundtrip() {
        let config = crate::config::AegisConfig::default();
        let state =
            crate::test_support::test_app_state(config, PluginSupervisor::new(), Vec::new()).await;
        let service = BrokenLinksService::new();

        let report = BrokenLinksReport {
            last_scanned_at: Some("2026-11-01T00:00:00Z".to_string()),
            total_links_checked: 10,
            broken_count: 1,
            items: vec![BrokenLinkItem {
                id: "item-1".to_string(),
                content_type: "article".to_string(),
                content_id: "art-1".to_string(),
                content_title: "Title".to_string(),
                content_slug: "slug".to_string(),
                target_url: "/articles/missing".to_string(),
                is_external: false,
                status_code: Some(404),
                error_message: Some("Not Found".to_string()),
            }],
        };

        service.save_report(&state, &report).await.unwrap();
        let loaded = service.get_report(&state).await;
        assert_eq!(loaded, report);
    }

    #[tokio::test]
    async fn test_scan_detects_internal_broken_link() {
        let config = crate::config::AegisConfig::default();
        let state =
            crate::test_support::test_app_state(config, PluginSupervisor::new(), Vec::new()).await;
        let schema = state.schemas.read().await["article"].clone();

        // 正常な記事とリンク切れを含む記事を挿入
        let record = ContentRecord {
            id: "art-1".to_string(),
            content_type: "article".to_string(),
            title: "Article 1".to_string(),
            slug: "art-1".to_string(),
            status: "published".to_string(),
            blocks_json: serde_json::to_string(&[
                BlockNode::paragraph("Check this valid link: [Home](/)"),
                BlockNode::paragraph("Check this broken link: [Dead](/articles/missing-post)"),
            ])
            .unwrap(),
            author_id: None,
            published_at: Some("2026-11-01T00:00:00Z".to_string()),
            unpublish_at: None,
            created_at: "2026-11-01T00:00:00Z".to_string(),
            updated_at: "2026-11-01T00:00:00Z".to_string(),
            custom_fields: HashMap::new(),
        };
        state.db.insert_content(&schema, &record).await.unwrap();

        let service = BrokenLinksService::new();
        let report = service.run_scan(&state).await.unwrap();

        assert_eq!(report.total_links_checked, 2);
        assert_eq!(report.broken_count, 1);
        assert_eq!(report.items.len(), 1);
        assert_eq!(report.items[0].target_url, "/articles/missing-post");
        assert_eq!(report.items[0].status_code, Some(404));
    }

    #[test]
    fn test_ssrf_protection() {
        assert!(is_private_or_loopback_host("http://localhost/foo"));
        assert!(is_private_or_loopback_host("http://127.0.0.1:8080/foo"));
        assert!(is_private_or_loopback_host("http://192.168.1.1/secret"));
        assert!(is_private_or_loopback_host("http://10.0.0.1/admin"));
        assert!(!is_private_or_loopback_host("https://example.com/page"));
    }
}
