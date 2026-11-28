//! パッケージストア（`[store]`。docs/store.md）: 署名したカタログの取得と検証。
//!
//! カタログ（`catalog.json`）と署名（`catalog.json.sig`）を取得し、設定した公開鍵での署名・
//! `schema_version`・`sequence`（巻き戻しの拒否）・`expires_at` を確かめてから使う。
//! ストアからのインストールでは、ZIP を取得してカタログの `size`・`sha256` と照合し、展開した中身が
//! カタログのエントリ（ID・バージョン・署名の鍵・`capabilities`）と一致することを確かめる

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use aegis_core::{AegisError, Result};
use aegis_db::DatabaseAdapter;
use ring::signature;
use serde::{Deserialize, Serialize};
use sha2::Digest;

use crate::ssrf::{is_private_or_loopback_host, PublicOnlyResolver};

/// 公式のカタログ（AegisCMS-Store の GitHub Pages）。`[[store.catalogs]]` を書かなければこれを読む
pub const OFFICIAL_CATALOG_URL: &str = "https://takumi-mr.github.io/AegisCMS-Store/catalog.json";

/// 公式のカタログの署名を確かめる Ed25519 公開鍵。替えるとこの値を持つ版のサーバーだけが読めるので、
/// 秘密鍵は失くさないよう控えておく（docs/store.md の 5）
pub const OFFICIAL_CATALOG_PUBLIC_KEY: &str =
    "f0addc0ea8e0d47a0fa8706ccb06e8352e3042174228cb567a92ec6c30ec15e9";

/// 読めるカタログの形式のバージョン
pub const CATALOG_SCHEMA_VERSION: u32 = 1;

/// カタログごとに最後に受け入れた `sequence`（`_aegis_settings`。JSON の `{"<name>": <sequence>}`）
const SEQUENCE_SETTING: &str = "store.catalog_sequences";

/// カタログの取得で許すリダイレクトの回数
const MAX_REDIRECTS: usize = 3;

/// 署名ファイル（`catalog.json.sig`）の大きさの上限。中身は 64 バイトの hex と改行だけ
const MAX_SIGNATURE_BYTES: u64 = 1024;

/// `[store]`
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct StoreConfig {
    /// ストアを使うか（既定: `true`。カタログを取得するのは、管理者が一覧・インストールの API を呼んだときだけ）
    pub enabled: bool,
    /// 読むカタログ（`[[store.catalogs]]`）。書かなければ公式のカタログ。書くと置き換わる
    pub catalogs: Vec<StoreCatalogConfig>,
    /// `catalog.json` の大きさの上限（バイト。既定: 1 MiB）
    pub max_catalog_bytes: u64,
    /// 1 回の取得のタイムアウト（秒。既定: 15）
    pub timeout_secs: u64,
}

impl Default for StoreConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            catalogs: vec![StoreCatalogConfig::official()],
            max_catalog_bytes: 1024 * 1024,
            timeout_secs: 15,
        }
    }
}

/// `[[store.catalogs]]`
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoreCatalogConfig {
    /// 識別名（英小文字・数字・`-`・`_`。`sequence` の記録と一覧の表示に使う）
    pub name: String,
    /// `catalog.json` の URL（`https://` のみ）。署名は同じ URL に `.sig` を付けた場所から取る
    pub url: String,
    /// カタログの署名を確かめる Ed25519 公開鍵（32 バイトの hex）
    pub public_key: String,
}

impl StoreCatalogConfig {
    /// 公式のカタログ
    pub fn official() -> Self {
        Self {
            name: "official".to_string(),
            url: OFFICIAL_CATALOG_URL.to_string(),
            public_key: OFFICIAL_CATALOG_PUBLIC_KEY.to_string(),
        }
    }
}

impl StoreConfig {
    /// 起動時の検査。有効なのにカタログが無い・名前の重複・`https://` でない URL・鍵の形の誤りを断る
    pub fn validate(&self) -> Result<()> {
        let err = |m: String| Err(AegisError::Config(format!("[store] {m}")));
        if !self.enabled {
            return Ok(());
        }
        if self.catalogs.is_empty() {
            return err("enabled = true needs at least one [[store.catalogs]]".to_string());
        }
        if self.max_catalog_bytes == 0 || self.timeout_secs == 0 {
            return err("max_catalog_bytes and timeout_secs must be greater than 0".to_string());
        }
        let mut seen = std::collections::HashSet::new();
        for catalog in &self.catalogs {
            if !aegis_core::is_valid_plugin_name(&catalog.name) {
                return err(format!(
                    "catalogs: name {:?} must be 1-64 lowercase letters, digits, '-' or '_'",
                    catalog.name
                ));
            }
            if !seen.insert(catalog.name.as_str()) {
                return err(format!("catalogs: duplicate name {:?}", catalog.name));
            }
            if let Err(m) = check_https_url(&catalog.url) {
                return err(format!("catalogs.{}: url {m}", catalog.name));
            }
            if decode_hex_len(&catalog.public_key, 32).is_none() {
                return err(format!(
                    "catalogs.{}: public_key must be 32 bytes (64 hex characters)",
                    catalog.name
                ));
            }
        }
        Ok(())
    }
}

/// `https://` で、ユーザー情報を含まない URL か。理由を英語で返す
fn check_https_url(raw: &str) -> std::result::Result<(), &'static str> {
    let url = url::Url::parse(raw).map_err(|_| "is not a valid URL")?;
    if url.scheme() != "https" {
        return Err("must start with https://");
    }
    if !url.username().is_empty() || url.password().is_some() {
        return Err("must not contain user information");
    }
    if url.host_str().is_none() {
        return Err("must have a host");
    }
    Ok(())
}

fn decode_hex_len(value: &str, len: usize) -> Option<Vec<u8>> {
    hex::decode(value.trim())
        .ok()
        .filter(|bytes| bytes.len() == len)
}

/// カタログ（`catalog.json`）。知らない項目は無視する
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Catalog {
    pub schema_version: u32,
    /// 公開のたびに増える番号。最後に受け入れた値より小さければ拒否する
    pub sequence: u64,
    pub generated_at: jiff::Timestamp,
    /// これを過ぎたカタログは拒否する
    pub expires_at: jiff::Timestamp,
    pub packages: Vec<CatalogPackage>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "lowercase")]
pub enum PackageKind {
    Plugin,
    Theme,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CatalogPackage {
    /// プラグイン名・テーマの ID（インストール先のディレクトリ名）
    pub id: String,
    pub kind: PackageKind,
    pub name: String,
    #[serde(default)]
    pub description: String,
    pub publisher: String,
    /// SPDX の識別子か `proprietary`
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub license: Option<String>,
    pub versions: Vec<CatalogVersion>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CatalogVersion {
    pub version: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min_aegis_version: Option<String>,
    /// マニフェストの `capabilities` の写し（インストール前の確認に出す）
    #[serde(default)]
    pub capabilities: serde_json::Value,
    /// ZIP の URL（`https://` のみ）
    pub url: String,
    /// ZIP の SHA-256（64 文字の hex）
    pub sha256: String,
    /// ZIP の大きさ（バイト）
    pub size: u64,
    /// ZIP の `signature.json` の Ed25519 公開鍵（32 バイトの hex）
    pub public_key: String,
}

impl CatalogVersion {
    /// このサーバーで動くか（`min_aegis_version`）
    pub fn is_compatible(&self) -> bool {
        self.min_aegis_version
            .as_deref()
            .is_none_or(|v| aegis_core::check_min_aegis_version(v).is_ok())
    }

    fn semver(&self) -> Option<semver::Version> {
        semver::Version::parse(&self.version).ok()
    }
}

impl CatalogPackage {
    /// このサーバーで動く最も新しいバージョン
    pub fn latest_compatible(&self) -> Option<&CatalogVersion> {
        self.versions
            .iter()
            .filter(|v| v.is_compatible())
            .max_by_key(|v| v.semver())
    }
}

/// カタログを使えない理由。`code` は API の応答に出す機械可読な値
#[derive(Debug, thiserror::Error)]
pub enum CatalogError {
    #[error("Failed to fetch the catalog: {0}")]
    Fetch(String),
    #[error("The catalog signature is invalid")]
    BadSignature,
    #[error("Unsupported catalog schema_version {0} (this server reads {CATALOG_SCHEMA_VERSION})")]
    UnsupportedSchema(u32),
    #[error("The catalog is malformed: {0}")]
    Malformed(String),
    #[error("The catalog sequence {got} is older than the last accepted {last}")]
    Rollback { got: u64, last: u64 },
    #[error("The catalog expired at {0}")]
    Expired(jiff::Timestamp),
}

impl CatalogError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::Fetch(_) => "catalog_fetch_failed",
            Self::BadSignature => "catalog_bad_signature",
            Self::UnsupportedSchema(_) => "catalog_unsupported_schema",
            Self::Malformed(_) => "catalog_malformed",
            Self::Rollback { .. } => "catalog_rollback",
            Self::Expired(_) => "catalog_expired",
        }
    }
}

/// 取得したカタログを確かめる。署名 → `schema_version` → 形 → `sequence` → `expires_at` の順。
/// 署名を確かめる前は中身を解析しない
pub fn verify_catalog(
    body: &[u8],
    signature_file: &[u8],
    public_key_hex: &str,
    last_sequence: Option<u64>,
    now: jiff::Timestamp,
) -> std::result::Result<Catalog, CatalogError> {
    let public_key = decode_hex_len(public_key_hex, 32).ok_or(CatalogError::BadSignature)?;
    let sig = std::str::from_utf8(signature_file)
        .ok()
        .and_then(|s| decode_hex_len(s, 64))
        .ok_or(CatalogError::BadSignature)?;
    signature::UnparsedPublicKey::new(&signature::ED25519, &public_key)
        .verify(body, &sig)
        .map_err(|_| CatalogError::BadSignature)?;

    #[derive(Deserialize)]
    struct Header {
        schema_version: u32,
    }
    let header: Header =
        serde_json::from_slice(body).map_err(|e| CatalogError::Malformed(e.to_string()))?;
    if header.schema_version != CATALOG_SCHEMA_VERSION {
        return Err(CatalogError::UnsupportedSchema(header.schema_version));
    }
    let catalog: Catalog =
        serde_json::from_slice(body).map_err(|e| CatalogError::Malformed(e.to_string()))?;
    check_entries(&catalog).map_err(CatalogError::Malformed)?;

    if let Some(last) = last_sequence {
        if catalog.sequence < last {
            return Err(CatalogError::Rollback {
                got: catalog.sequence,
                last,
            });
        }
    }
    if catalog.expires_at <= now {
        return Err(CatalogError::Expired(catalog.expires_at));
    }
    Ok(catalog)
}

/// 各エントリの形を確かめる。署名済みのカタログに誤りがあれば、一部を黙って捨てずに全体を断る
fn check_entries(catalog: &Catalog) -> std::result::Result<(), String> {
    let mut ids = std::collections::HashSet::new();
    for package in &catalog.packages {
        if !aegis_core::is_valid_plugin_name(&package.id) {
            return Err(format!("invalid package id {:?}", package.id));
        }
        if !ids.insert((package.kind, package.id.as_str())) {
            return Err(format!("duplicate package {:?}", package.id));
        }
        let mut versions = std::collections::HashSet::new();
        for v in &package.versions {
            let at = format!("{} {}", package.id, v.version);
            let parsed = semver::Version::parse(&v.version)
                .map_err(|_| format!("{at}: version is not semver"))?;
            if !versions.insert(parsed) {
                return Err(format!("{at}: duplicate version"));
            }
            if let Some(min) = &v.min_aegis_version {
                semver::Version::parse(min.trim())
                    .map_err(|_| format!("{at}: min_aegis_version is not semver"))?;
            }
            check_https_url(&v.url).map_err(|m| format!("{at}: url {m}"))?;
            if decode_hex_len(&v.sha256, 32).is_none() {
                return Err(format!("{at}: sha256 must be 64 hex characters"));
            }
            if decode_hex_len(&v.public_key, 32).is_none() {
                return Err(format!("{at}: public_key must be 64 hex characters"));
            }
            if v.size == 0 {
                return Err(format!("{at}: size must be greater than 0"));
            }
        }
    }
    Ok(())
}

/// カタログの取得に使う HTTP クライアント。公開されたアドレスにだけ接続し（`PublicOnlyResolver`）、
/// リダイレクトは回数を絞って、転送先も `https://` の公開されたホストに限る。環境変数のプロキシは使わない
/// （プロキシ側で名前が解決され、内部のアドレスを除けないため）
pub fn build_client(config: &StoreConfig) -> Result<reqwest::Client> {
    reqwest::Client::builder()
        .timeout(Duration::from_secs(config.timeout_secs))
        .redirect(reqwest::redirect::Policy::custom(|attempt| {
            if attempt.previous().len() >= MAX_REDIRECTS {
                attempt.error("Too many redirects")
            } else if attempt.url().scheme() != "https" {
                attempt.error("Redirects must stay on https://")
            } else if is_private_or_loopback_host(attempt.url().as_str()) {
                attempt.error("Blocked redirect to a private or loopback address")
            } else {
                attempt.follow()
            }
        }))
        .no_proxy()
        .dns_resolver(Arc::new(PublicOnlyResolver))
        .user_agent(concat!("AegisCMS/", env!("CARGO_PKG_VERSION")))
        .build()
        .map_err(|e| AegisError::Internal(e.to_string()))
}

/// `url` を `max_bytes` まで読む。超えたら途中で止める
async fn fetch_limited(
    client: &reqwest::Client,
    url: &str,
    max_bytes: u64,
) -> std::result::Result<Vec<u8>, CatalogError> {
    check_https_url(url).map_err(|m| CatalogError::Fetch(format!("url {m}")))?;
    if is_private_or_loopback_host(url) {
        return Err(CatalogError::Fetch(
            "url points to a private or loopback address".to_string(),
        ));
    }
    let fail = |e: reqwest::Error| CatalogError::Fetch(e.without_url().to_string());
    let mut res = client
        .get(url)
        .send()
        .await
        .and_then(reqwest::Response::error_for_status)
        .map_err(fail)?;
    let too_large = || CatalogError::Fetch(format!("response exceeds {max_bytes} bytes"));
    if res.content_length().is_some_and(|len| len > max_bytes) {
        return Err(too_large());
    }
    let mut body = Vec::new();
    while let Some(chunk) = res.chunk().await.map_err(fail)? {
        if (body.len() + chunk.len()) as u64 > max_bytes {
            return Err(too_large());
        }
        body.extend_from_slice(&chunk);
    }
    Ok(body)
}

/// ストアから入れるパッケージ（カタログのエントリ）。展開した中身との照合と、監査ログに使う
pub struct StoreSource<'a> {
    pub catalog: &'a str,
    pub kind: PackageKind,
    pub id: &'a str,
    pub version: &'a CatalogVersion,
}

impl StoreSource<'_> {
    /// 検証済みのパッケージ（署名は `validate_*_package` が確かめたあと）が、カタログのエントリと一致するか。
    /// ID・バージョン・`capabilities`（テーマは `csp`）が同じで、`signature.json` があり、その鍵がエントリの
    /// `public_key` であること。ストア経由では、署名を必須にしていないサーバーでも署名の無いものは断る
    pub fn check_package(&self, pkg_root: &std::path::Path) -> Result<()> {
        let invalid = |m: String| Err(AegisError::Validation(m));
        let (manifest_file, id_field) = match self.kind {
            PackageKind::Plugin => ("plugin.manifest.json", "name"),
            PackageKind::Theme => ("theme.json", "id"),
        };
        let raw: serde_json::Value = read_json(&pkg_root.join(manifest_file))?;
        if raw.get(id_field).and_then(|v| v.as_str()) != Some(self.id) {
            return invalid(format!(
                "The package {} is not '{}' as the catalog says",
                manifest_file, self.id
            ));
        }
        if raw.get("version").and_then(|v| v.as_str()) != Some(self.version.version.as_str()) {
            return invalid(format!(
                "The package version is not {} as the catalog says",
                self.version.version
            ));
        }
        let empty = || serde_json::json!({});
        let capabilities = match self.kind {
            PackageKind::Plugin => raw.get("capabilities").cloned().unwrap_or_else(empty),
            PackageKind::Theme => raw
                .get("csp")
                .map(|csp| serde_json::json!({ "csp": csp }))
                .unwrap_or_else(empty),
        };
        let listed = if self.version.capabilities.is_null() {
            empty()
        } else {
            self.version.capabilities.clone()
        };
        if capabilities != listed {
            return invalid(
                "The package capabilities differ from the ones listed in the catalog".to_string(),
            );
        }
        let sig_path = pkg_root.join("signature.json");
        if !sig_path.is_file() {
            return invalid("Packages from the store must be signed (signature.json)".to_string());
        }
        let sig: aegis_core::PackageSignature = read_json(&sig_path)?;
        if !sig
            .public_key
            .trim()
            .eq_ignore_ascii_case(self.version.public_key.trim())
        {
            return invalid(
                "The package is signed by a key other than the one in the catalog".to_string(),
            );
        }
        Ok(())
    }

    /// 監査ログの `details.source`
    pub fn audit_details(&self) -> serde_json::Value {
        serde_json::json!({
            "catalog": self.catalog,
            "url": self.version.url,
            "sha256": self.version.sha256,
        })
    }
}

fn read_json<T: serde::de::DeserializeOwned>(path: &std::path::Path) -> Result<T> {
    let text = std::fs::read_to_string(path).map_err(AegisError::Io)?;
    serde_json::from_str(&text).map_err(|e| {
        AegisError::Validation(format!(
            "Invalid {}: {}",
            path.file_name().unwrap_or_default().to_string_lossy(),
            e
        ))
    })
}

/// パッケージのインストールに失敗した理由（アップロードとストアで共通）
#[derive(Debug)]
pub enum InstallError {
    /// パッケージの中身が不正（展開・検証・カタログとの照合）
    Invalid(String),
    /// サーバー側の失敗（一時ディレクトリ・ファイルの配置）
    Internal(String),
}

/// ZIP の取得に失敗した理由
#[derive(Debug, thiserror::Error)]
pub enum PackageFetchError {
    #[error("Failed to fetch the package: {0}")]
    Fetch(String),
    #[error("The package does not match the catalog: {0}")]
    Mismatch(String),
}

/// カタログのエントリの ZIP を取得し、`size`・`sha256` と照合する（`size` を超える応答は途中で止める）
pub async fn fetch_package(
    client: &reqwest::Client,
    version: &CatalogVersion,
) -> std::result::Result<Vec<u8>, PackageFetchError> {
    let bytes = fetch_limited(client, &version.url, version.size)
        .await
        .map_err(|e| match e {
            CatalogError::Fetch(m) => PackageFetchError::Fetch(m),
            other => PackageFetchError::Fetch(other.to_string()),
        })?;
    if bytes.len() as u64 != version.size {
        return Err(PackageFetchError::Mismatch(format!(
            "size is {} bytes, the catalog says {}",
            bytes.len(),
            version.size
        )));
    }
    let digest = hex::encode(sha2::Sha256::digest(&bytes));
    if !digest.eq_ignore_ascii_case(version.sha256.trim()) {
        return Err(PackageFetchError::Mismatch(
            "sha256 differs from the catalog".to_string(),
        ));
    }
    Ok(bytes)
}

/// カタログ 1 つを取得して確かめる。受け入れたら `sequence` を記録する
pub async fn load_catalog(
    client: &reqwest::Client,
    db: &dyn DatabaseAdapter,
    config: &StoreConfig,
    catalog: &StoreCatalogConfig,
) -> std::result::Result<Catalog, CatalogError> {
    let sig_url = format!("{}.sig", catalog.url);
    let (body, sig) = tokio::try_join!(
        fetch_limited(client, &catalog.url, config.max_catalog_bytes),
        fetch_limited(client, &sig_url, MAX_SIGNATURE_BYTES),
    )?;
    let last = load_sequences(db).await.get(&catalog.name).copied();
    let verified = verify_catalog(
        &body,
        &sig,
        &catalog.public_key,
        last,
        jiff::Timestamp::now(),
    )?;
    if last.is_none_or(|last| verified.sequence > last) {
        if let Err(e) = record_sequence(db, &catalog.name, verified.sequence).await {
            tracing::warn!("Failed to record the store catalog sequence: {}", e);
        }
    }
    Ok(verified)
}

/// 記録した `sequence` を読む。読めなければ空（巻き戻しの検査は、次に受け入れたときから効く）
async fn load_sequences(db: &dyn DatabaseAdapter) -> HashMap<String, u64> {
    match db.list_settings().await {
        Ok(settings) => settings
            .get(SEQUENCE_SETTING)
            .and_then(|json| serde_json::from_str(json).ok())
            .unwrap_or_default(),
        Err(e) => {
            tracing::warn!("Failed to read the store catalog sequences: {}", e);
            HashMap::new()
        }
    }
}

/// 同じプロセスでの読み書きの競合で、ほかのカタログの記録を上書きしないよう直列にする
static SEQUENCE_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

/// `sequence` を記録する。記録より小さい値では下げない
async fn record_sequence(db: &dyn DatabaseAdapter, name: &str, sequence: u64) -> Result<()> {
    let _guard = SEQUENCE_LOCK.lock().await;
    let mut sequences = load_sequences(db).await;
    let entry = sequences.entry(name.to_string()).or_insert(0);
    if *entry >= sequence {
        return Ok(());
    }
    *entry = sequence;
    let json =
        serde_json::to_string(&sequences).map_err(|e| AegisError::Internal(e.to_string()))?;
    db.set_setting(SEQUENCE_SETTING, &json).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use ring::rand::SystemRandom;
    use ring::signature::{Ed25519KeyPair, KeyPair};
    use serde_json::json;

    fn keypair() -> Ed25519KeyPair {
        let pkcs8 = Ed25519KeyPair::generate_pkcs8(&SystemRandom::new()).unwrap();
        Ed25519KeyPair::from_pkcs8(pkcs8.as_ref()).unwrap()
    }

    fn public_hex(key: &Ed25519KeyPair) -> String {
        hex::encode(key.public_key().as_ref())
    }

    fn sign(key: &Ed25519KeyPair, body: &[u8]) -> Vec<u8> {
        format!("{}\n", hex::encode(key.sign(body).as_ref())).into_bytes()
    }

    fn now() -> jiff::Timestamp {
        "2026-11-28T00:00:00Z".parse().unwrap()
    }

    fn catalog_json(sequence: u64) -> serde_json::Value {
        json!({
            "schema_version": 1,
            "sequence": sequence,
            "generated_at": "2026-11-27T00:00:00Z",
            "expires_at": "2027-02-27T00:00:00Z",
            "future_field": { "ignored": true },
            "packages": [{
                "id": "seo-tools",
                "kind": "plugin",
                "name": "SEO Tools",
                "publisher": "official",
                "license": "MIT",
                "versions": [{
                    "version": "1.2.0",
                    "min_aegis_version": "0.1.0",
                    "capabilities": { "hooks": ["on-render-page"] },
                    "url": "https://example.com/seo-tools-1.2.0.zip",
                    "sha256": "a".repeat(64),
                    "size": 1234,
                    "public_key": "b".repeat(64)
                }]
            }]
        })
    }

    fn verify_value(
        value: &serde_json::Value,
        last: Option<u64>,
    ) -> std::result::Result<Catalog, CatalogError> {
        let key = keypair();
        let body = serde_json::to_vec(value).unwrap();
        verify_catalog(&body, &sign(&key, &body), &public_hex(&key), last, now())
    }

    #[test]
    fn accepts_a_signed_catalog_and_ignores_unknown_fields() {
        let catalog = verify_value(&catalog_json(5), Some(5)).unwrap();
        assert_eq!(catalog.sequence, 5);
        assert_eq!(catalog.packages[0].kind, PackageKind::Plugin);
        assert_eq!(catalog.packages[0].license.as_deref(), Some("MIT"));
        assert!(catalog.packages[0].versions[0].is_compatible());
    }

    #[test]
    fn rejects_a_tampered_body_or_another_key() {
        let key = keypair();
        let body = serde_json::to_vec(&catalog_json(1)).unwrap();
        let sig = sign(&key, &body);
        let mut tampered = body.clone();
        *tampered.last_mut().unwrap() = b' ';
        assert!(matches!(
            verify_catalog(&tampered, &sig, &public_hex(&key), None, now()),
            Err(CatalogError::BadSignature)
        ));
        assert!(matches!(
            verify_catalog(&body, &sig, &public_hex(&keypair()), None, now()),
            Err(CatalogError::BadSignature)
        ));
        assert!(matches!(
            verify_catalog(&body, b"not hex", &public_hex(&key), None, now()),
            Err(CatalogError::BadSignature)
        ));
    }

    #[test]
    fn rejects_rollback_expiry_and_unknown_schema() {
        assert!(matches!(
            verify_value(&catalog_json(4), Some(5)),
            Err(CatalogError::Rollback { got: 4, last: 5 })
        ));
        let mut expired = catalog_json(1);
        expired["expires_at"] = json!("2026-11-28T00:00:00Z");
        assert!(matches!(
            verify_value(&expired, None),
            Err(CatalogError::Expired(_))
        ));
        let mut future = catalog_json(1);
        future["schema_version"] = json!(2);
        future["packages"] = json!("a different shape");
        assert!(matches!(
            verify_value(&future, None),
            Err(CatalogError::UnsupportedSchema(2))
        ));
    }

    #[test]
    fn rejects_malformed_entries() {
        let cases: [(&str, serde_json::Value); 6] = [
            ("id", json!("../evil")),
            ("version", json!("latest")),
            ("url", json!("http://example.com/a.zip")),
            ("sha256", json!("abc")),
            ("public_key", json!("zz")),
            ("size", json!(0)),
        ];
        for (field, bad) in cases {
            let mut value = catalog_json(1);
            if field == "id" {
                value["packages"][0]["id"] = bad;
            } else {
                value["packages"][0]["versions"][0][field] = bad;
            }
            assert!(
                matches!(verify_value(&value, None), Err(CatalogError::Malformed(_))),
                "{field} should be rejected"
            );
        }
        let mut duplicate = catalog_json(1);
        let version = duplicate["packages"][0]["versions"][0].clone();
        duplicate["packages"][0]["versions"]
            .as_array_mut()
            .unwrap()
            .push(version);
        assert!(matches!(
            verify_value(&duplicate, None),
            Err(CatalogError::Malformed(_))
        ));
    }

    fn version_entry(v: &str, min: Option<&str>) -> CatalogVersion {
        CatalogVersion {
            version: v.into(),
            min_aegis_version: min.map(Into::into),
            capabilities: json!({ "hooks": ["on-render-page"] }),
            url: format!("https://example.com/{v}.zip"),
            sha256: "a".repeat(64),
            size: 1,
            public_key: "b".repeat(64),
        }
    }

    #[test]
    fn latest_compatible_skips_versions_this_server_cannot_run() {
        let package = CatalogPackage {
            id: "seo-tools".into(),
            kind: PackageKind::Plugin,
            name: "SEO Tools".into(),
            description: String::new(),
            publisher: "official".into(),
            license: None,
            versions: vec![
                version_entry("1.2.0", None),
                version_entry("1.10.0", None),
                version_entry("9.0.0", Some("999.0.0")),
            ],
        };
        assert_eq!(package.latest_compatible().unwrap().version, "1.10.0");
    }

    /// 署名した `plugin.manifest.json` だけのパッケージを作り、その鍵（hex）を返す
    fn signed_plugin(dir: &std::path::Path, manifest: serde_json::Value) -> String {
        std::fs::write(
            dir.join("plugin.manifest.json"),
            serde_json::to_vec(&manifest).unwrap(),
        )
        .unwrap();
        let (private, public) = aegis_core::generate_ed25519_keypair().unwrap();
        aegis_core::sign_package_dir(dir, &private).unwrap();
        public
    }

    #[test]
    fn store_packages_must_match_their_catalog_entry() {
        let manifest = json!({
            "name": "seo-tools",
            "version": "1.2.0",
            "capabilities": { "hooks": ["on-render-page"] }
        });
        let dir = tempfile::tempdir().unwrap();
        let key = signed_plugin(dir.path(), manifest);
        let mut entry = version_entry("1.2.0", None);
        entry.public_key = key;
        let source = |version: &CatalogVersion, id: &str| -> Result<()> {
            StoreSource {
                catalog: "official",
                kind: PackageKind::Plugin,
                id,
                version,
            }
            .check_package(dir.path())
        };
        source(&entry, "seo-tools").unwrap();

        assert!(source(&entry, "other").is_err(), "another id");
        let mut other_version = entry.clone();
        other_version.version = "1.3.0".into();
        assert!(
            source(&other_version, "seo-tools").is_err(),
            "another version"
        );
        let mut other_caps = entry.clone();
        other_caps.capabilities = json!({ "hooks": [] });
        assert!(
            source(&other_caps, "seo-tools").is_err(),
            "other capabilities"
        );
        let mut other_key = entry.clone();
        other_key.public_key = "c".repeat(64);
        assert!(source(&other_key, "seo-tools").is_err(), "another key");

        std::fs::remove_file(dir.path().join("signature.json")).unwrap();
        assert!(source(&entry, "seo-tools").is_err(), "unsigned");
    }

    #[test]
    fn themes_compare_their_csp() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("theme.json"),
            serde_json::to_vec(&json!({
                "id": "roastery",
                "version": "1.0.0",
                "csp": { "style_domains": ["fonts.googleapis.com"] }
            }))
            .unwrap(),
        )
        .unwrap();
        let (private, public) = aegis_core::generate_ed25519_keypair().unwrap();
        aegis_core::sign_package_dir(dir.path(), &private).unwrap();
        let mut entry = version_entry("1.0.0", None);
        entry.public_key = public;
        entry.capabilities = json!({ "csp": { "style_domains": ["fonts.googleapis.com"] } });
        let check = |entry: &CatalogVersion| {
            StoreSource {
                catalog: "official",
                kind: PackageKind::Theme,
                id: "roastery",
                version: entry,
            }
            .check_package(dir.path())
        };
        check(&entry).unwrap();
        entry.capabilities = serde_json::Value::Null;
        assert!(check(&entry).is_err());
    }

    #[test]
    fn incompatible_versions_are_reported() {
        let version = CatalogVersion {
            version: "1.0.0".into(),
            min_aegis_version: Some("999.0.0".into()),
            capabilities: serde_json::Value::Null,
            url: "https://example.com/a.zip".into(),
            sha256: "a".repeat(64),
            size: 1,
            public_key: "b".repeat(64),
        };
        assert!(!version.is_compatible());
    }

    fn config_with(catalogs: Vec<StoreCatalogConfig>) -> StoreConfig {
        StoreConfig {
            enabled: true,
            catalogs,
            ..Default::default()
        }
    }

    fn catalog_config(name: &str, url: &str) -> StoreCatalogConfig {
        StoreCatalogConfig {
            name: name.into(),
            url: url.into(),
            public_key: "c".repeat(64),
        }
    }

    #[test]
    fn the_official_catalog_is_the_default() {
        let config = StoreConfig::default();
        assert!(config.enabled);
        assert_eq!(config.catalogs.len(), 1);
        assert_eq!(config.catalogs[0].url, OFFICIAL_CATALOG_URL);
        let enabled = StoreConfig {
            enabled: true,
            ..config
        };
        assert!(enabled.validate().is_ok());

        // `[store]` に `enabled` だけ書いても公式のカタログを読み、`[[store.catalogs]]` を書けば置き換わる
        let only_enabled: StoreConfig = toml::from_str("enabled = true").unwrap();
        assert_eq!(
            only_enabled.catalogs[0].public_key,
            OFFICIAL_CATALOG_PUBLIC_KEY
        );
        let replaced: StoreConfig = toml::from_str(&format!(
            "enabled = true
[[catalogs]]
name = \"mine\"
url = \"https://example.com/c.json\"
public_key = \"{}\"
",
            "c".repeat(64)
        ))
        .unwrap();
        assert_eq!(replaced.catalogs.len(), 1);
        assert_eq!(replaced.catalogs[0].name, "mine");
    }

    #[test]
    fn validates_the_config() {
        assert!(StoreConfig::default().validate().is_ok());
        assert!(config_with(vec![]).validate().is_err());
        let ok = catalog_config("official", "https://example.com/catalog.json");
        assert!(config_with(vec![ok.clone()]).validate().is_ok());
        assert!(config_with(vec![ok.clone(), ok.clone()])
            .validate()
            .is_err());
        for bad in [
            catalog_config("Official", "https://example.com/catalog.json"),
            catalog_config("official", "http://example.com/catalog.json"),
            catalog_config("official", "https://user:pw@example.com/catalog.json"),
            StoreCatalogConfig {
                public_key: "short".into(),
                ..ok
            },
        ] {
            assert!(config_with(vec![bad]).validate().is_err());
        }
    }

    #[tokio::test]
    async fn refuses_internal_hosts_before_connecting() {
        let client = build_client(&StoreConfig::default()).unwrap();
        for url in ["https://127.0.0.1/catalog.json", "https://localhost/c.json"] {
            assert!(matches!(
                fetch_limited(&client, url, 1024).await,
                Err(CatalogError::Fetch(_))
            ));
        }
    }

    #[tokio::test]
    async fn records_sequences_without_lowering_them() {
        use aegis_db::DatabaseAdapter;
        let adapter = aegis_db::SqliteAdapter::connect("sqlite::memory:", 1)
            .await
            .unwrap();
        adapter.init().await.unwrap();
        let db: Arc<dyn DatabaseAdapter> = Arc::new(adapter);
        record_sequence(db.as_ref(), "official", 7).await.unwrap();
        record_sequence(db.as_ref(), "other", 2).await.unwrap();
        record_sequence(db.as_ref(), "official", 3).await.unwrap();
        let sequences = load_sequences(db.as_ref()).await;
        assert_eq!(sequences.get("official"), Some(&7));
        assert_eq!(sequences.get("other"), Some(&2));
    }
}
