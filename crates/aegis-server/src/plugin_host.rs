//! プラグインのホスト API の実装（`aegis_runtime::HostServices`）。
//! KV は `[cache]` のバックエンド（`redis` なら再起動後も残り、全サーバーで共有される）、
//! HTTP は `allowed_domains` を確認済みのリクエストだけを送り、設定値は `[plugins.settings.<name>]`

use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Duration;

use aegis_core::{AegisError, CacheBackend, Result};
use aegis_runtime::PluginsConfig;
use aegis_runtime::{HostServices, HttpRequest, HttpResponse};
use async_trait::async_trait;

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

pub struct ServerHostServices {
    cache: Arc<dyn CacheBackend>,
    plugins: PluginsConfig,
    client: reqwest::Client,
    dynamic_settings: Arc<
        std::sync::RwLock<
            std::collections::HashMap<String, std::collections::HashMap<String, String>>,
        >,
    >,
}

impl ServerHostServices {
    pub fn new(cache: Arc<dyn CacheBackend>, plugins: PluginsConfig) -> Result<Self> {
        let client = reqwest::Client::builder()
            .timeout(HTTP_TIMEOUT)
            // リダイレクト先は allowed_domains の確認を受けていないため追わない（3xx をそのまま返す）
            .redirect(reqwest::redirect::Policy::none())
            .user_agent(concat!("AegisCMS-Plugin/", env!("CARGO_PKG_VERSION")))
            .build()
            .map_err(|e| AegisError::Config(format!("Failed to build the HTTP client: {}", e)))?;
        Ok(Self {
            cache,
            plugins,
            client,
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

        let services = ServerHostServices::new(
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
