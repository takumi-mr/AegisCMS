//! プラグインのホスト API の実装（`aegis_runtime::HostServices`）。
//! KV は `[cache]` のバックエンド（`redis` なら再起動後も残り、全サーバーで共有される）、
//! HTTP は `allowed_domains` を確認済みのリクエストだけを送り、設定値は `[plugins.settings.<name>]`

use std::collections::BTreeMap;
use std::net::IpAddr;
use std::sync::Arc;
use std::time::Duration;

use aegis_core::{AegisError, CacheBackend, Result};
use aegis_runtime::PluginsConfig;
use aegis_runtime::{HostServices, HttpRequest, HttpResponse};
use async_trait::async_trait;

use crate::ssrf::{is_public_ip, PublicOnlyResolver};

/// KV のキーの接頭辞（セッションなど他の用途のキーと分ける）
const KV_PREFIX: &str = "plugin-kv:";
/// 送れる本文・受け取る本文の上限
const MAX_HTTP_BODY: usize = 1 << 20;
/// 1 回の HTTP リクエストの時間の上限（プラグインの `execution_timeout_ms` が短ければそちら）
const HTTP_TIMEOUT: Duration = Duration::from_secs(10);
/// プラグインが設定できないヘッダー（HTTP クライアントが決める）
const RESERVED_HEADERS: [&str; 5] = [
    "host",
    "content-length",
    "transfer-encoding",
    "connection",
    "upgrade",
];

/// 送ってよい URL か: `http` / `https` で、ポートは `https` が 443、`http` が 80 か 8080 だけ、
/// 宛先が IP アドレスで書かれていれば公開されたアドレスであること（ホスト名のほうは、
/// 接続のたびに [`PublicOnlyResolver`] が名前解決の結果を確かめる）
fn check_destination(url: &str) -> Result<()> {
    let deny = |reason: &str| AegisError::PermissionDenied(format!("{}: {}", reason, url));
    let parsed = url::Url::parse(url).map_err(|_| deny("Invalid URL"))?;
    let port_ok = match (parsed.scheme(), parsed.port()) {
        // `url` はスキームの既定のポート（80・443）を `None` として返す
        ("https" | "http", None) => true,
        ("http", Some(8080)) => true,
        ("http" | "https", Some(_)) => false,
        _ => return Err(deny("Only http and https URLs are allowed")),
    };
    if !port_ok {
        return Err(deny(
            "Only ports 80 (http), 443 (https) and 8080 (http) are allowed",
        ));
    }
    match parsed.host() {
        Some(url::Host::Ipv4(ip)) if !is_public_ip(IpAddr::V4(ip)) => {
            Err(deny("Requests to internal IP addresses are not allowed"))
        }
        Some(url::Host::Ipv6(ip)) if !is_public_ip(IpAddr::V6(ip)) => {
            Err(deny("Requests to internal IP addresses are not allowed"))
        }
        Some(_) => Ok(()),
        None => Err(deny("The URL has no host")),
    }
}

pub struct ServerHostServices {
    cache: Arc<dyn CacheBackend>,
    plugins: PluginsConfig,
    client: reqwest::Client,
    /// ポートと宛先の IP アドレスの制限（`false` はテストで手元のサーバーに送るときだけ）
    restrict_network: bool,
    dynamic_settings: Arc<
        std::sync::RwLock<
            std::collections::HashMap<String, std::collections::HashMap<String, String>>,
        >,
    >,
}

impl ServerHostServices {
    pub fn new(cache: Arc<dyn CacheBackend>, plugins: PluginsConfig) -> Result<Self> {
        Self::build(cache, plugins, true)
    }

    /// ポートと宛先の IP アドレスを制限しない（テストで 127.0.0.1 の手元のサーバーに送るときだけ）
    #[cfg(test)]
    pub fn new_unrestricted(cache: Arc<dyn CacheBackend>, plugins: PluginsConfig) -> Result<Self> {
        Self::build(cache, plugins, false)
    }

    fn build(
        cache: Arc<dyn CacheBackend>,
        plugins: PluginsConfig,
        restrict_network: bool,
    ) -> Result<Self> {
        let mut builder = reqwest::Client::builder()
            .timeout(HTTP_TIMEOUT)
            // リダイレクトはここでは追わず（3xx をそのまま返す）、ランタイムが 1 回ずつ許可ドメインを
            // 確かめながら追う
            .redirect(reqwest::redirect::Policy::none())
            .user_agent(concat!("AegisCMS-Plugin/", env!("CARGO_PKG_VERSION")));
        if restrict_network {
            // 環境変数のプロキシ（`HTTP_PROXY` など）は使わない。プロキシ経由だと名前解決がプロキシ側で
            // 行われ、内部アドレスを除く `PublicOnlyResolver` を通らない。いまのビルドは reqwest の
            // `system-proxy` 機能が無く環境変数を読まないが、機能が有効になっても迂回できないようにする保険
            builder = builder
                .no_proxy()
                .dns_resolver(Arc::new(PublicOnlyResolver));
        }
        let client = builder
            .build()
            .map_err(|e| AegisError::Config(format!("Failed to build the HTTP client: {}", e)))?;
        Ok(Self {
            cache,
            plugins,
            client,
            restrict_network,
            dynamic_settings: Arc::new(std::sync::RwLock::new(std::collections::HashMap::new())),
        })
    }

    pub fn with_dynamic_settings(
        mut self,
        dynamic_settings: Arc<
            std::sync::RwLock<
                std::collections::HashMap<String, std::collections::HashMap<String, String>>,
            >,
        >,
    ) -> Self {
        self.dynamic_settings = dynamic_settings;
        self
    }
}

#[async_trait]
impl HostServices for ServerHostServices {
    async fn kv_get(&self, key: &str) -> Result<Option<String>> {
        self.cache.get(&format!("{}{}", KV_PREFIX, key)).await
    }

    async fn kv_set(&self, key: &str, value: &str) -> Result<()> {
        self.cache
            .set(&format!("{}{}", KV_PREFIX, key), value, None)
            .await
    }

    async fn http_fetch(&self, plugin: &str, request: HttpRequest) -> Result<HttpResponse> {
        let method = match request.method.to_ascii_uppercase().as_str() {
            "GET" => reqwest::Method::GET,
            "POST" => reqwest::Method::POST,
            "PUT" => reqwest::Method::PUT,
            "PATCH" => reqwest::Method::PATCH,
            "DELETE" => reqwest::Method::DELETE,
            other => {
                return Err(AegisError::Validation(format!(
                    "HTTP method {} is not allowed",
                    other
                )))
            }
        };
        // ランタイムは、リダイレクトの移った先を含め、1 回の通信ごとにここを呼ぶ
        if self.restrict_network {
            check_destination(&request.url)?;
        }
        let body = request.body.unwrap_or_default();
        if body.len() > MAX_HTTP_BODY {
            return Err(AegisError::Validation(
                "Request body is too large".to_string(),
            ));
        }
        let mut builder = self.client.request(method, &request.url);
        for (name, value) in &request.headers {
            if !RESERVED_HEADERS.contains(&name.to_ascii_lowercase().as_str()) {
                builder = builder.header(name, value);
            }
        }
        if !body.is_empty() {
            builder = builder.body(body);
        }
        tracing::debug!("[Wasm:{}] HTTP {} {}", plugin, request.method, request.url);
        let mut response = builder
            .send()
            .await
            .map_err(|e| AegisError::Internal(format!("HTTP request failed: {}", e)))?;
        let status = response.status().as_u16();
        let headers: BTreeMap<String, String> = response
            .headers()
            .iter()
            .filter_map(|(name, value)| {
                value
                    .to_str()
                    .ok()
                    .map(|v| (name.as_str().to_string(), v.to_string()))
            })
            .collect();
        let mut bytes = Vec::new();
        while let Some(chunk) = response
            .chunk()
            .await
            .map_err(|e| AegisError::Internal(format!("HTTP response failed: {}", e)))?
        {
            if bytes.len() + chunk.len() > MAX_HTTP_BODY {
                return Err(AegisError::ResourceLimitExceeded(format!(
                    "HTTP response is larger than {} bytes",
                    MAX_HTTP_BODY
                )));
            }
            bytes.extend_from_slice(&chunk);
        }
        Ok(HttpResponse {
            status,
            headers,
            body: String::from_utf8_lossy(&bytes).into_owned(),
        })
    }

    fn plugin_setting(&self, plugin: &str, key: &str) -> Option<String> {
        if let Ok(guard) = self.dynamic_settings.read() {
            if let Some(map) = guard.get(plugin) {
                if let Some(val) = map.get(key) {
                    return Some(val.clone());
                }
            }
        }
        self.plugins.setting(plugin, key)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use aegis_cache::MemoryCacheBackend;

    #[test]
    fn destinations_are_limited_by_scheme_port_and_ip() {
        for ok in [
            "https://example.com/a",
            "https://example.com:443/a",
            "http://example.com/a",
            "http://example.com:80/a",
            "http://example.com:8080/a",
            "https://8.8.8.8/",
        ] {
            assert!(check_destination(ok).is_ok(), "{}", ok);
        }
        for ng in [
            "https://example.com:8443/a",
            "https://example.com:8080/a",
            "http://example.com:443/a",
            "http://example.com:22/a",
            "ftp://example.com/a",
            "file:///etc/passwd",
            "http://127.0.0.1/",
            "http://127.0.0.1:8080/",
            "http://[::1]/",
            "http://169.254.169.254/latest/meta-data/",
            "http://10.1.2.3/",
            // 10 進数・16 進数の書き方でも、URL パーサーが IP アドレスに直す
            "http://2130706433/",
            "http://0x7f.0.0.1/",
            "http://[::ffff:7f00:1]/",
            "not a url",
        ] {
            assert!(check_destination(ng).is_err(), "{}", ng);
        }
    }

    #[tokio::test]
    async fn hostnames_that_resolve_to_internal_addresses_are_refused() {
        let services = ServerHostServices::new(
            Arc::new(MemoryCacheBackend::new()),
            PluginsConfig::default(),
        )
        .unwrap();
        // `localhost` は許可ドメインに入っていても、127.0.0.1 に解決されるので接続しない
        let result = services
            .http_fetch(
                "p",
                HttpRequest {
                    method: "GET".to_string(),
                    url: "http://localhost:8080/".to_string(),
                    ..Default::default()
                },
            )
            .await;
        assert!(result.is_err(), "{:?}", result);
        let result = services
            .http_fetch(
                "p",
                HttpRequest {
                    url: "http://127.0.0.1:8080/".to_string(),
                    ..Default::default()
                },
            )
            .await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_http_fetch_and_kv() {
        // A local server that echoes the request and redirects /moved
        let app = axum::Router::new()
            .route(
                "/echo",
                axum::routing::post(|headers: axum::http::HeaderMap, body: String| async move {
                    format!(
                        "{} {}",
                        headers
                            .get("x-token")
                            .and_then(|v| v.to_str().ok())
                            .unwrap_or("-"),
                        body
                    )
                }),
            )
            .route(
                "/moved",
                axum::routing::get(|| async {
                    axum::response::Redirect::temporary("https://evil.example/")
                }),
            )
            .route(
                "/big",
                axum::routing::get(|| async { "x".repeat(MAX_HTTP_BODY + 1) }),
            );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });

        let services = ServerHostServices::new_unrestricted(
            Arc::new(MemoryCacheBackend::new()),
            PluginsConfig::default(),
        )
        .unwrap();
        let response = services
            .http_fetch(
                "notifier",
                HttpRequest {
                    method: "post".to_string(),
                    url: format!("{}/echo", base),
                    headers: BTreeMap::from([("X-Token".to_string(), "abc".to_string())]),
                    body: Some("hello".to_string()),
                },
            )
            .await
            .unwrap();
        assert_eq!(response.status, 200);
        assert_eq!(response.body, "abc hello");

        // Redirects are returned, not followed
        let moved = services
            .http_fetch(
                "notifier",
                HttpRequest {
                    method: "GET".to_string(),
                    url: format!("{}/moved", base),
                    ..Default::default()
                },
            )
            .await
            .unwrap();
        assert_eq!(moved.status, 307);
        assert_eq!(moved.headers["location"], "https://evil.example/");

        let too_big = services
            .http_fetch(
                "notifier",
                HttpRequest {
                    method: "GET".to_string(),
                    url: format!("{}/big", base),
                    ..Default::default()
                },
            )
            .await;
        assert!(too_big.is_err());
        assert!(services
            .http_fetch(
                "notifier",
                HttpRequest {
                    method: "TRACE".to_string(),
                    url: format!("{}/echo", base),
                    ..Default::default()
                },
            )
            .await
            .is_err());

        assert_eq!(services.kv_get("ns:count").await.unwrap(), None);
        services.kv_set("ns:count", "3").await.unwrap();
        assert_eq!(
            services.kv_get("ns:count").await.unwrap().as_deref(),
            Some("3")
        );
    }
}
