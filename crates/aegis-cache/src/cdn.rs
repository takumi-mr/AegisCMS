//! CDN の Cache-Tag / Surrogate-Key ヘッダー付与と、タグベースのキャッシュパージ（On-demand Cache-Tag Purge）。
//!
//! バックエンドプロセス内ではページ HTML キャッシュを持たず、前段の CDN（Cloudflare / Fastly /
//! Varnish / CloudFront / BunnyCDN 等）に対して標準のキャッシュタグヘッダー（`Cache-Tag` および
//! `Surrogate-Key`）を送出し、コンテンツ更新・公開・非公開・削除時にタグ単位で CDN キャッシュを無効化する。

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use aegis_core::{AegisError, Result};
use async_trait::async_trait;
use http::{HeaderMap, HeaderValue};
use serde::{Deserialize, Serialize};

/// Cloudflare の 1 リクエストあたりの最大タグ数
const CLOUDFLARE_MAX_TAGS_PER_REQUEST: usize = 30;

/// Fastly の 1 リクエストあたりの最大 Surrogate-Key 数
const FASTLY_MAX_KEYS_PER_REQUEST: usize = 256;

/// 公開レスポンスに付与するデフォルトの CDN 向け `Cache-Control`
pub const DEFAULT_CDN_CACHE_CONTROL: &str =
    "public, max-age=0, s-maxage=3600, stale-while-revalidate=60";

/// レスポンスに付与するキャッシュタグ（`Cache-Tag` / `Surrogate-Key`）と、
/// コンテンツ更新時にパージする対象タグを管理する構造体。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CacheTags {
    tags: Vec<String>,
}

impl CacheTags {
    pub fn new() -> Self {
        Self { tags: Vec::new() }
    }

    pub fn from_tags<I, S>(iter: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        let mut tags = Self::new();
        for item in iter {
            tags.add_tag(item);
        }
        tags
    }

    /// 任意のタグを追加する（空白・カンマ・制御文字を安全に正規化し、重複は除外する）
    pub fn add_tag(&mut self, tag: impl AsRef<str>) {
        let Some(clean) = sanitize_tag(tag.as_ref()) else {
            return;
        };
        if !self.tags.contains(&clean) {
            self.tags.push(clean);
        }
    }

    /// フルパス（URL等）を意識したタグの追加（例: `/articles/hello` → `url-articles-hello`、`/` → `url-index`）
    pub fn add_url(&mut self, url: &str) {
        let path = url.split('?').next().unwrap_or(url).trim();
        let clean_url = path.trim_matches('/').replace('/', "-");
        if clean_url.is_empty() {
            self.add_tag("url-index");
        } else {
            self.add_tag(format!("url-{}", clean_url));
        }
    }

    /// データ依存（IDやSlug等）のタグの追加（例: `("article", "123")` → `article-123`）
    pub fn add_dependency(&mut self, entity: &str, id: &str) {
        let entity = entity.trim().trim_matches('/');
        let id = id.trim().trim_matches('/');
        if !entity.is_empty() && !id.is_empty() {
            self.add_tag(format!("{}-{}", entity, id));
        }
    }

    /// 別の `CacheTags` のタグをマージする
    pub fn extend(&mut self, other: &CacheTags) {
        for tag in &other.tags {
            self.add_tag(tag);
        }
    }

    pub fn is_empty(&self) -> bool {
        self.tags.is_empty()
    }

    pub fn len(&self) -> usize {
        self.tags.len()
    }

    pub fn tags(&self) -> &[String] {
        &self.tags
    }

    pub fn into_tags(self) -> Vec<String> {
        self.tags
    }

    /// Cloudflare / BunnyCDN / Netlify 等向けの `Cache-Tag` ヘッダー値（カンマ区切り）
    pub fn cache_tag_header_value(&self) -> Option<String> {
        (!self.tags.is_empty()).then(|| self.tags.join(", "))
    }

    /// Fastly / Varnish / Google Cloud CDN 等向けの `Surrogate-Key` ヘッダー値（スペース区切り）
    pub fn surrogate_key_header_value(&self) -> Option<String> {
        (!self.tags.is_empty()).then(|| self.tags.join(" "))
    }

    /// Axum / `http` の `HeaderMap` に `Cache-Tag` および `Surrogate-Key` ヘッダーを注入する
    pub fn write_headers(&self, headers: &mut HeaderMap) {
        if let Some(cache_tag) = self.cache_tag_header_value() {
            if let Ok(value) = HeaderValue::from_str(&cache_tag) {
                headers.insert("Cache-Tag", value);
            }
        }
        if let Some(surrogate_key) = self.surrogate_key_header_value() {
            if let Ok(value) = HeaderValue::from_str(&surrogate_key) {
                headers.insert("Surrogate-Key", value);
            }
        }
    }
}

/// タグ文字列を HTTP ヘッダー（`Cache-Tag` のカンマ区切りおよび `Surrogate-Key` のスペース区切り）で
/// 安全に扱える ASCII トークンへ正規化する。
fn sanitize_tag(raw: &str) -> Option<String> {
    let trimmed = raw.trim().trim_matches('/');
    if trimmed.is_empty() {
        return None;
    }
    let mut out = String::with_capacity(trimmed.len());
    let mut last_dash = false;
    for b in trimmed.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'_' | b'.' | b':') {
            out.push(b as char);
            last_dash = false;
        } else if matches!(b, b'-' | b'/' | b' ' | b',' | b'\t') {
            if !out.is_empty() && !last_dash {
                out.push('-');
                last_dash = true;
            }
        } else if b >= 0x20 && b != 0x7f && b != b';' && b != b'"' && b.is_ascii() {
            out.push(b as char);
            last_dash = false;
        } else {
            // 非 ASCII（日本語スラッグ等）や制御文字は 2 桁 hex にエンコードして一意性とヘッダー安全性を両立する
            use std::fmt::Write;
            let _ = write!(&mut out, "{:02x}", b);
            last_dash = false;
        }
    }
    let cleaned = out.trim_matches('-').to_string();
    if cleaned.is_empty() {
        None
    } else {
        Some(cleaned)
    }
}

/// CDN プロバイダ種別（`[cdn].provider`）
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CdnProviderKind {
    /// 外部 CDN パージ API を呼ばない（ヘッダー送出のみ、または CDN 未使用時）
    #[default]
    None,
    /// パージ対象タグを構造化ログ（`aegis::cdn`）に出力する（開発・検証用）
    Log,
    /// Cloudflare Cache Purge by Tag API
    Cloudflare,
    /// Fastly Instant Purge by Surrogate-Key API
    Fastly,
    /// 汎用 HTTP POST Webhook（Varnish, CloudFront, BunnyCDN, Nginx, 独自エッジプロキシ向け）
    Webhook,
}

impl std::str::FromStr for CdnProviderKind {
    type Err = String;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s.to_ascii_lowercase().as_str() {
            "none" | "noop" | "" => Ok(Self::None),
            "log" => Ok(Self::Log),
            "cloudflare" | "cf" => Ok(Self::Cloudflare),
            "fastly" => Ok(Self::Fastly),
            "webhook" | "http" => Ok(Self::Webhook),
            other => Err(format!("Unknown CDN provider: {}", other)),
        }
    }
}

/// `[cdn]` セクションの設定
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct CdnConfig {
    /// CDN 連携（`Cache-Tag` / `Surrogate-Key` ヘッダー送出および自動パージ）の有効・無効
    pub enabled: bool,
    /// パージ API の送信先プロバイダ
    pub provider: CdnProviderKind,
    /// `enabled = true` のときに公開レスポンスへ `Cache-Tag` / `Surrogate-Key` ヘッダーを付与するか
    pub emit_headers: bool,
    /// `enabled = true` のときに公開ページのレスポンスへ付与する `Cache-Control`（空なら付与しない）
    pub cache_control: String,
    /// パージ API 呼び出しの HTTP タイムアウト（ミリ秒）
    pub timeout_ms: u64,
    #[serde(default)]
    pub cloudflare: CloudflareCdnConfig,
    #[serde(default)]
    pub fastly: FastlyCdnConfig,
    #[serde(default)]
    pub webhook: WebhookCdnConfig,
}

impl Default for CdnConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            provider: CdnProviderKind::None,
            emit_headers: true,
            cache_control: DEFAULT_CDN_CACHE_CONTROL.to_string(),
            timeout_ms: 5000,
            cloudflare: CloudflareCdnConfig::default(),
            fastly: FastlyCdnConfig::default(),
            webhook: WebhookCdnConfig::default(),
        }
    }
}

impl CdnConfig {
    pub fn validate(&self) -> Result<()> {
        if !self.enabled {
            return Ok(());
        }
        if self.timeout_ms < 100 {
            return Err(AegisError::Config(
                "[cdn].timeout_ms must be at least 100".to_string(),
            ));
        }
        if self.cache_control.contains('\r') || self.cache_control.contains('\n') {
            return Err(AegisError::Config(
                "[cdn].cache_control cannot contain newline characters".to_string(),
            ));
        }
        match self.provider {
            CdnProviderKind::None | CdnProviderKind::Log => Ok(()),
            CdnProviderKind::Cloudflare => {
                if self.cloudflare.zone_id.trim().is_empty() {
                    return Err(AegisError::Config(
                        "[cdn.cloudflare].zone_id is required when [cdn].provider = \"cloudflare\""
                            .to_string(),
                    ));
                }
                if self.cloudflare.resolved_api_token().is_none() {
                    return Err(AegisError::Config(
                        "[cdn.cloudflare].api_token (or AEGIS_CLOUDFLARE_API_TOKEN) is required when [cdn].provider = \"cloudflare\""
                            .to_string(),
                    ));
                }
                validate_http_url(&self.cloudflare.api_base, "[cdn.cloudflare].api_base")
            }
            CdnProviderKind::Fastly => {
                if self.fastly.service_id.trim().is_empty() {
                    return Err(AegisError::Config(
                        "[cdn.fastly].service_id is required when [cdn].provider = \"fastly\""
                            .to_string(),
                    ));
                }
                if self.fastly.resolved_api_token().is_none() {
                    return Err(AegisError::Config(
                        "[cdn.fastly].api_token (or AEGIS_FASTLY_API_TOKEN) is required when [cdn].provider = \"fastly\""
                            .to_string(),
                    ));
                }
                validate_http_url(&self.fastly.api_base, "[cdn.fastly].api_base")
            }
            CdnProviderKind::Webhook => {
                if self.webhook.endpoint.trim().is_empty() {
                    return Err(AegisError::Config(
                        "[cdn.webhook].endpoint is required when [cdn].provider = \"webhook\""
                            .to_string(),
                    ));
                }
                validate_http_url(&self.webhook.endpoint, "[cdn.webhook].endpoint")
            }
        }
    }
}

fn validate_http_url(url: &str, field: &str) -> Result<()> {
    let trimmed = url.trim();
    if !(trimmed.starts_with("http://") || trimmed.starts_with("https://")) {
        return Err(AegisError::Config(format!(
            "{} must start with http:// or https://",
            field
        )));
    }
    Ok(())
}

/// `"env:VAR_NAME"` 形式または直接指定されたシークレット値を解決する
fn resolve_secret(configured: &str, fallback_env: Option<&str>) -> Option<String> {
    let trimmed = configured.trim();
    if let Some(env_name) = trimmed.strip_prefix("env:") {
        let val = std::env::var(env_name.trim()).ok()?;
        let val = val.trim().to_string();
        return (!val.is_empty()).then_some(val);
    }
    if !trimmed.is_empty() {
        return Some(trimmed.to_string());
    }
    if let Some(env_name) = fallback_env {
        let val = std::env::var(env_name).ok()?;
        let val = val.trim().to_string();
        return (!val.is_empty()).then_some(val);
    }
    None
}

/// `[cdn.cloudflare]` の設定
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct CloudflareCdnConfig {
    pub zone_id: String,
    /// Cloudflare API トークン（空なら環境変数 `AEGIS_CLOUDFLARE_API_TOKEN`、または `"env:..."`）
    pub api_token: String,
    pub api_base: String,
}

impl Default for CloudflareCdnConfig {
    fn default() -> Self {
        Self {
            zone_id: String::new(),
            api_token: String::new(),
            api_base: "https://api.cloudflare.com/client/v4".to_string(),
        }
    }
}

impl CloudflareCdnConfig {
    pub fn resolved_api_token(&self) -> Option<String> {
        resolve_secret(&self.api_token, Some("AEGIS_CLOUDFLARE_API_TOKEN"))
    }
}

/// `[cdn.fastly]` の設定
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct FastlyCdnConfig {
    pub service_id: String,
    /// Fastly API トークン（空なら環境変数 `AEGIS_FASTLY_API_TOKEN`、または `"env:..."`）
    pub api_token: String,
    /// `Fastly-Soft-Purge: 1` を付与するか（即座に削除せず stale 状態にして再検証させる）
    pub soft_purge: bool,
    pub api_base: String,
}

impl Default for FastlyCdnConfig {
    fn default() -> Self {
        Self {
            service_id: String::new(),
            api_token: String::new(),
            soft_purge: true,
            api_base: "https://api.fastly.com".to_string(),
        }
    }
}

impl FastlyCdnConfig {
    pub fn resolved_api_token(&self) -> Option<String> {
        resolve_secret(&self.api_token, Some("AEGIS_FASTLY_API_TOKEN"))
    }
}

/// `[cdn.webhook]` の設定
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct WebhookCdnConfig {
    /// パージリクエストを送る POST エンドポイント URL
    pub endpoint: String,
    /// `Authorization: Bearer <token>` に設定するトークン（空なら `AEGIS_CDN_WEBHOOK_TOKEN`、または `"env:..."`）
    pub bearer_token: String,
    /// 追加で付与する任意の HTTP ヘッダー（値は `"env:..."` 対応）
    pub headers: HashMap<String, String>,
}

impl WebhookCdnConfig {
    pub fn resolved_bearer_token(&self) -> Option<String> {
        resolve_secret(&self.bearer_token, Some("AEGIS_CDN_WEBHOOK_TOKEN"))
    }
}

/// CDN のキャッシュパージを抽象化するトレイト
#[async_trait]
pub trait CdnPurger: Send + Sync {
    /// プロバイダ名（`"none"`, `"log"`, `"cloudflare"`, `"fastly"`, `"webhook"` 等）
    fn name(&self) -> &str;

    /// パージ処理が有効かどうか（`NoopCdnPurger` は `false`）
    fn is_enabled(&self) -> bool {
        true
    }

    /// 指定されたキャッシュタグに一致するエッジキャッシュを無効化する
    async fn purge_tags(&self, tags: &[String]) -> Result<()>;

    /// CDN 上の全キャッシュを無効化する
    async fn purge_all(&self) -> Result<()>;
}

/// 何もしないパージャー（CDN 無効時または `provider = "none"` 時）
#[derive(Debug, Default, Clone, Copy)]
pub struct NoopCdnPurger;

#[async_trait]
impl CdnPurger for NoopCdnPurger {
    fn name(&self) -> &str {
        "none"
    }

    fn is_enabled(&self) -> bool {
        false
    }

    async fn purge_tags(&self, _tags: &[String]) -> Result<()> {
        Ok(())
    }

    async fn purge_all(&self) -> Result<()> {
        Ok(())
    }
}

/// パージ対象タグをサーバーログに出力するパージャー（`provider = "log"`）
#[derive(Debug, Default, Clone, Copy)]
pub struct LogCdnPurger;

#[async_trait]
impl CdnPurger for LogCdnPurger {
    fn name(&self) -> &str {
        "log"
    }

    async fn purge_tags(&self, tags: &[String]) -> Result<()> {
        if !tags.is_empty() {
            tracing::info!(
                target: "aegis::cdn",
                tags = ?tags,
                "Purging CDN cache tags"
            );
        }
        Ok(())
    }

    async fn purge_all(&self) -> Result<()> {
        tracing::info!(target: "aegis::cdn", "Purging all CDN cache");
        Ok(())
    }
}

/// テスト・検証用のメモリ記録型パージャー
#[derive(Debug, Default)]
pub struct RecordingCdnPurger {
    purged_batches: Mutex<Vec<Vec<String>>>,
    purge_all_count: Mutex<usize>,
}

impl RecordingCdnPurger {
    pub fn new() -> Self {
        Self::default()
    }

    /// これまでにパージされた全タグをフラットな一覧で取得する
    pub fn purged_tags(&self) -> Vec<String> {
        self.purged_batches
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .iter()
            .flatten()
            .cloned()
            .collect()
    }

    /// パージ呼び出し単位のバッチ履歴を取得してクリアする
    pub fn take_batches(&self) -> Vec<Vec<String>> {
        let mut guard = self
            .purged_batches
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        std::mem::take(&mut *guard)
    }

    pub fn purge_all_count(&self) -> usize {
        *self
            .purge_all_count
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}

#[async_trait]
impl CdnPurger for RecordingCdnPurger {
    fn name(&self) -> &str {
        "recording"
    }

    async fn purge_tags(&self, tags: &[String]) -> Result<()> {
        if !tags.is_empty() {
            self.purged_batches
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .push(tags.to_vec());
        }
        Ok(())
    }

    async fn purge_all(&self) -> Result<()> {
        *self
            .purge_all_count
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) += 1;
        Ok(())
    }
}

/// Cloudflare Cache Purge by Tag 実装
pub struct CloudflareCdnPurger {
    client: reqwest::Client,
    endpoint: String,
    api_token: String,
}

impl CloudflareCdnPurger {
    pub fn new(config: &CdnConfig) -> Result<Self> {
        let zone_id = config.cloudflare.zone_id.trim();
        let api_token = config.cloudflare.resolved_api_token().ok_or_else(|| {
            AegisError::Config(
                "[cdn.cloudflare].api_token (or AEGIS_CLOUDFLARE_API_TOKEN) is not set".to_string(),
            )
        })?;
        let base = config.cloudflare.api_base.trim().trim_end_matches('/');
        let client = reqwest::Client::builder()
            .timeout(Duration::from_millis(config.timeout_ms.max(100)))
            .build()
            .map_err(|e| AegisError::Config(format!("Failed to build CDN HTTP client: {}", e)))?;
        Ok(Self {
            client,
            endpoint: format!("{}/zones/{}/purge_cache", base, zone_id),
            api_token,
        })
    }
}

#[async_trait]
impl CdnPurger for CloudflareCdnPurger {
    fn name(&self) -> &str {
        "cloudflare"
    }

    async fn purge_tags(&self, tags: &[String]) -> Result<()> {
        if tags.is_empty() {
            return Ok(());
        }
        for chunk in tags.chunks(CLOUDFLARE_MAX_TAGS_PER_REQUEST) {
            let body = serde_json::json!({ "tags": chunk });
            let response = self
                .client
                .post(&self.endpoint)
                .bearer_auth(&self.api_token)
                .header(http::header::CONTENT_TYPE, "application/json")
                .body(body.to_string())
                .send()
                .await
                .map_err(|e| {
                    AegisError::Internal(format!("Cloudflare purge request failed: {}", e))
                })?;
            if !response.status().is_success() {
                let status = response.status();
                let text = response.text().await.unwrap_or_default();
                return Err(AegisError::Internal(format!(
                    "Cloudflare purge failed with HTTP {}: {}",
                    status, text
                )));
            }
        }
        Ok(())
    }

    async fn purge_all(&self) -> Result<()> {
        let body = serde_json::json!({ "purge_everything": true });
        let response = self
            .client
            .post(&self.endpoint)
            .bearer_auth(&self.api_token)
            .header(http::header::CONTENT_TYPE, "application/json")
            .body(body.to_string())
            .send()
            .await
            .map_err(|e| {
                AegisError::Internal(format!("Cloudflare purge_all request failed: {}", e))
            })?;
        if !response.status().is_success() {
            let status = response.status();
            let text = response.text().await.unwrap_or_default();
            return Err(AegisError::Internal(format!(
                "Cloudflare purge_all failed with HTTP {}: {}",
                status, text
            )));
        }
        Ok(())
    }
}

/// Fastly Instant Purge by Surrogate-Key 実装
pub struct FastlyCdnPurger {
    client: reqwest::Client,
    purge_endpoint: String,
    purge_all_endpoint: String,
    api_token: String,
    soft_purge: bool,
}

impl FastlyCdnPurger {
    pub fn new(config: &CdnConfig) -> Result<Self> {
        let service_id = config.fastly.service_id.trim();
        let api_token = config.fastly.resolved_api_token().ok_or_else(|| {
            AegisError::Config(
                "[cdn.fastly].api_token (or AEGIS_FASTLY_API_TOKEN) is not set".to_string(),
            )
        })?;
        let base = config.fastly.api_base.trim().trim_end_matches('/');
        let client = reqwest::Client::builder()
            .timeout(Duration::from_millis(config.timeout_ms.max(100)))
            .build()
            .map_err(|e| AegisError::Config(format!("Failed to build CDN HTTP client: {}", e)))?;
        Ok(Self {
            client,
            purge_endpoint: format!("{}/service/{}/purge", base, service_id),
            purge_all_endpoint: format!("{}/service/{}/purge_all", base, service_id),
            api_token,
            soft_purge: config.fastly.soft_purge,
        })
    }
}

#[async_trait]
impl CdnPurger for FastlyCdnPurger {
    fn name(&self) -> &str {
        "fastly"
    }

    async fn purge_tags(&self, tags: &[String]) -> Result<()> {
        if tags.is_empty() {
            return Ok(());
        }
        for chunk in tags.chunks(FASTLY_MAX_KEYS_PER_REQUEST) {
            let body = serde_json::json!({ "surrogate_keys": chunk });
            let mut req = self
                .client
                .post(&self.purge_endpoint)
                .header("Fastly-Key", &self.api_token)
                .header("Surrogate-Key", chunk.join(" "))
                .header(http::header::ACCEPT, "application/json")
                .header(http::header::CONTENT_TYPE, "application/json")
                .body(body.to_string());
            if self.soft_purge {
                req = req.header("Fastly-Soft-Purge", "1");
            }
            let response = req
                .send()
                .await
                .map_err(|e| AegisError::Internal(format!("Fastly purge request failed: {}", e)))?;
            if !response.status().is_success() {
                let status = response.status();
                let text = response.text().await.unwrap_or_default();
                return Err(AegisError::Internal(format!(
                    "Fastly purge failed with HTTP {}: {}",
                    status, text
                )));
            }
        }
        Ok(())
    }

    async fn purge_all(&self) -> Result<()> {
        let mut req = self
            .client
            .post(&self.purge_all_endpoint)
            .header("Fastly-Key", &self.api_token)
            .header(http::header::ACCEPT, "application/json");
        if self.soft_purge {
            req = req.header("Fastly-Soft-Purge", "1");
        }
        let response = req
            .send()
            .await
            .map_err(|e| AegisError::Internal(format!("Fastly purge_all request failed: {}", e)))?;
        if !response.status().is_success() {
            let status = response.status();
            let text = response.text().await.unwrap_or_default();
            return Err(AegisError::Internal(format!(
                "Fastly purge_all failed with HTTP {}: {}",
                status, text
            )));
        }
        Ok(())
    }
}

/// 汎用 Webhook Purge 実装（Varnish / CloudFront / BunnyCDN / Nginx / 独自 CDN アダプター向け）
pub struct WebhookCdnPurger {
    client: reqwest::Client,
    endpoint: String,
    bearer_token: Option<String>,
    headers: Vec<(String, String)>,
}

impl WebhookCdnPurger {
    pub fn new(config: &CdnConfig) -> Result<Self> {
        let endpoint = config.webhook.endpoint.trim().to_string();
        let bearer_token = config.webhook.resolved_bearer_token();
        let mut headers = Vec::new();
        for (k, v) in &config.webhook.headers {
            if let Some(resolved) = resolve_secret(v, None) {
                headers.push((k.trim().to_string(), resolved));
            }
        }
        let client = reqwest::Client::builder()
            .timeout(Duration::from_millis(config.timeout_ms.max(100)))
            .build()
            .map_err(|e| AegisError::Config(format!("Failed to build CDN HTTP client: {}", e)))?;
        Ok(Self {
            client,
            endpoint,
            bearer_token,
            headers,
        })
    }

    async fn send_payload(&self, body: serde_json::Value) -> Result<()> {
        let mut req = self
            .client
            .post(&self.endpoint)
            .header(http::header::CONTENT_TYPE, "application/json")
            .body(body.to_string());
        if let Some(ref token) = self.bearer_token {
            req = req.bearer_auth(token);
        }
        for (k, v) in &self.headers {
            req = req.header(k, v);
        }
        let response = req.send().await.map_err(|e| {
            AegisError::Internal(format!("CDN webhook purge request failed: {}", e))
        })?;
        if !response.status().is_success() {
            let status = response.status();
            let text = response.text().await.unwrap_or_default();
            return Err(AegisError::Internal(format!(
                "CDN webhook purge failed with HTTP {}: {}",
                status, text
            )));
        }
        Ok(())
    }
}

#[async_trait]
impl CdnPurger for WebhookCdnPurger {
    fn name(&self) -> &str {
        "webhook"
    }

    async fn purge_tags(&self, tags: &[String]) -> Result<()> {
        if tags.is_empty() {
            return Ok(());
        }
        self.send_payload(serde_json::json!({
            "tags": tags,
            "purge_all": false,
        }))
        .await
    }

    async fn purge_all(&self) -> Result<()> {
        self.send_payload(serde_json::json!({
            "tags": [],
            "purge_all": true,
        }))
        .await
    }
}

/// `[cdn]` 設定から `CdnPurger` を生成する
pub fn create_cdn_purger(config: &CdnConfig) -> Result<Arc<dyn CdnPurger>> {
    config.validate()?;
    if !config.enabled {
        return Ok(Arc::new(NoopCdnPurger));
    }
    match config.provider {
        CdnProviderKind::None => Ok(Arc::new(NoopCdnPurger)),
        CdnProviderKind::Log => Ok(Arc::new(LogCdnPurger)),
        CdnProviderKind::Cloudflare => Ok(Arc::new(CloudflareCdnPurger::new(config)?)),
        CdnProviderKind::Fastly => Ok(Arc::new(FastlyCdnPurger::new(config)?)),
        CdnProviderKind::Webhook => Ok(Arc::new(WebhookCdnPurger::new(config)?)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{
        body::Bytes,
        extract::State,
        http::{HeaderMap as AxumHeaderMap, StatusCode},
        routing::post,
        Router,
    };

    #[test]
    fn test_cache_tags_add_url_and_dependency_and_write_headers() {
        let mut tags = CacheTags::new();
        assert!(tags.is_empty());

        tags.add_url("/");
        tags.add_url("/articles/hello-world/");
        tags.add_dependency("article", "art-123");
        tags.add_dependency("article", "hello-world");
        // 重複追加は無視される
        tags.add_dependency("article", "art-123");

        assert_eq!(
            tags.tags(),
            &[
                "url-index",
                "url-articles-hello-world",
                "article-art-123",
                "article-hello-world",
            ]
        );

        let mut headers = HeaderMap::new();
        tags.write_headers(&mut headers);
        assert_eq!(
            headers.get("Cache-Tag").unwrap().to_str().unwrap(),
            "url-index, url-articles-hello-world, article-art-123, article-hello-world"
        );
        assert_eq!(
            headers.get("Surrogate-Key").unwrap().to_str().unwrap(),
            "url-index url-articles-hello-world article-art-123 article-hello-world"
        );
    }

    #[test]
    fn test_cache_tags_sanitizes_query_and_non_ascii() {
        let mut tags = CacheTags::new();
        tags.add_url("/articles?page=2");
        tags.add_dependency("article", "日本語スラッグ");

        assert_eq!(tags.tags()[0], "url-articles");
        assert!(tags.tags()[1].starts_with("article-"));

        let mut headers = HeaderMap::new();
        tags.write_headers(&mut headers);
        assert!(headers.contains_key("Cache-Tag"));
        assert!(headers.contains_key("Surrogate-Key"));
    }

    #[test]
    fn test_cdn_config_validation() {
        // enabled = false ならプロバイダ未設定でも通る
        let mut cfg = CdnConfig {
            provider: CdnProviderKind::Cloudflare,
            ..Default::default()
        };
        assert!(cfg.validate().is_ok());

        // enabled = true で Cloudflare 必須項目欠如はエラー
        cfg.enabled = true;
        assert!(cfg.validate().is_err());

        cfg.cloudflare.zone_id = "zone123".to_string();
        cfg.cloudflare.api_token = "tok_secret".to_string();
        assert!(cfg.validate().is_ok());

        // Fastly
        cfg.provider = CdnProviderKind::Fastly;
        assert!(cfg.validate().is_err());
        cfg.fastly.service_id = "svc123".to_string();
        cfg.fastly.api_token = "fastly_tok".to_string();
        assert!(cfg.validate().is_ok());

        // Webhook
        cfg.provider = CdnProviderKind::Webhook;
        assert!(cfg.validate().is_err());
        cfg.webhook.endpoint = "https://edge.example.com/purge".to_string();
        assert!(cfg.validate().is_ok());
    }

    #[derive(Clone, Default)]
    struct MockServerState {
        requests: Arc<Mutex<Vec<(String, AxumHeaderMap, serde_json::Value)>>>,
    }

    async fn spawn_mock_cdn_server() -> (String, MockServerState) {
        let state = MockServerState::default();
        let app = Router::new()
            .route(
                "/{*path}",
                post(
                    |State(state): State<MockServerState>,
                     uri: axum::http::Uri,
                     headers: AxumHeaderMap,
                     body: Bytes| async move {
                        let json: serde_json::Value =
                            serde_json::from_slice(&body).unwrap_or(serde_json::Value::Null);
                        state.requests.lock().unwrap().push((
                            uri.path().to_string(),
                            headers,
                            json,
                        ));
                        (StatusCode::OK, r#"{"success":true}"#)
                    },
                ),
            )
            .with_state(state.clone());

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            let _ = axum::serve(listener, app).await;
        });
        (format!("http://{}", addr), state)
    }

    #[tokio::test]
    async fn test_cloudflare_purger_batches_tags_and_purges_all() {
        let (base_url, mock) = spawn_mock_cdn_server().await;
        let cfg = CdnConfig {
            enabled: true,
            provider: CdnProviderKind::Cloudflare,
            cloudflare: CloudflareCdnConfig {
                zone_id: "zone-abc".to_string(),
                api_token: "cf-token-123".to_string(),
                api_base: base_url,
            },
            ..Default::default()
        };
        let purger = create_cdn_purger(&cfg).unwrap();
        assert_eq!(purger.name(), "cloudflare");
        assert!(purger.is_enabled());

        // 35 件のタグ → 30 件 + 5 件の 2 リクエストに分割されること
        let tags: Vec<String> = (1..=35).map(|i| format!("article-{}", i)).collect();
        purger.purge_tags(&tags).await.unwrap();
        purger.purge_all().await.unwrap();

        let reqs = mock.requests.lock().unwrap().clone();
        assert_eq!(reqs.len(), 3);
        assert_eq!(reqs[0].0, "/zones/zone-abc/purge_cache");
        assert_eq!(
            reqs[0].1.get("authorization").unwrap(),
            "Bearer cf-token-123"
        );
        assert_eq!(reqs[0].2["tags"].as_array().unwrap().len(), 30);
        assert_eq!(reqs[1].2["tags"].as_array().unwrap().len(), 5);
        assert_eq!(reqs[2].2["purge_everything"], serde_json::json!(true));
    }

    #[tokio::test]
    async fn test_fastly_purger_sends_surrogate_key_and_soft_purge() {
        let (base_url, mock) = spawn_mock_cdn_server().await;
        let cfg = CdnConfig {
            enabled: true,
            provider: CdnProviderKind::Fastly,
            fastly: FastlyCdnConfig {
                service_id: "svc-xyz".to_string(),
                api_token: "fastly-secret".to_string(),
                soft_purge: true,
                api_base: base_url,
            },
            ..Default::default()
        };
        let purger = create_cdn_purger(&cfg).unwrap();
        purger
            .purge_tags(&["article-1".to_string(), "url-index".to_string()])
            .await
            .unwrap();
        purger.purge_all().await.unwrap();

        let reqs = mock.requests.lock().unwrap().clone();
        assert_eq!(reqs.len(), 2);
        assert_eq!(reqs[0].0, "/service/svc-xyz/purge");
        assert_eq!(reqs[0].1.get("Fastly-Key").unwrap(), "fastly-secret");
        assert_eq!(
            reqs[0].1.get("Surrogate-Key").unwrap(),
            "article-1 url-index"
        );
        assert_eq!(reqs[0].1.get("Fastly-Soft-Purge").unwrap(), "1");
        assert_eq!(
            reqs[0].2["surrogate_keys"],
            serde_json::json!(["article-1", "url-index"])
        );

        assert_eq!(reqs[1].0, "/service/svc-xyz/purge_all");
        assert_eq!(reqs[1].1.get("Fastly-Key").unwrap(), "fastly-secret");
    }

    #[tokio::test]
    async fn test_webhook_purger_sends_tags_and_custom_headers() {
        let (base_url, mock) = spawn_mock_cdn_server().await;
        let cfg = CdnConfig {
            enabled: true,
            provider: CdnProviderKind::Webhook,
            webhook: WebhookCdnConfig {
                endpoint: format!("{}/hooks/cache-purge", base_url),
                bearer_token: "webhook-secret".to_string(),
                headers: HashMap::from([("X-Edge-Node".to_string(), "tokyo-1".to_string())]),
            },
            ..Default::default()
        };
        let purger = create_cdn_purger(&cfg).unwrap();
        purger
            .purge_tags(&["product-42".to_string()])
            .await
            .unwrap();

        let reqs = mock.requests.lock().unwrap().clone();
        assert_eq!(reqs.len(), 1);
        assert_eq!(reqs[0].0, "/hooks/cache-purge");
        assert_eq!(
            reqs[0].1.get("authorization").unwrap(),
            "Bearer webhook-secret"
        );
        assert_eq!(reqs[0].1.get("x-edge-node").unwrap(), "tokyo-1");
        assert_eq!(
            reqs[0].2,
            serde_json::json!({
                "tags": ["product-42"],
                "purge_all": false,
            })
        );
    }
}
