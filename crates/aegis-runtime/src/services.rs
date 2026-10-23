//! プラグインのホスト API のうち、外部とのやりとりを伴うもの（KV ストア・HTTP・設定値）。
//! ランタイムは I/O を持たず、サーバーが `HostServices` を実装して渡す
//! （渡さなければプロセス内メモリの KV のみで、HTTP と設定値は使えない）

use aegis_core::{AegisError, Result};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};
use std::sync::{Arc, Mutex};

/// `host.http_fetch` のリクエスト（プラグインが JSON で渡す）
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct HttpRequest {
    /// `GET` / `POST` / `PUT` / `PATCH` / `DELETE`（既定 `GET`）
    #[serde(default = "default_method")]
    pub method: String,
    pub url: String,
    #[serde(default)]
    pub headers: BTreeMap<String, String>,
    #[serde(default)]
    pub body: Option<String>,
}

fn default_method() -> String {
    "GET".to_string()
}

/// `host.http_fetch` の応答（本文は UTF-8 として読む）
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct HttpResponse {
    pub status: u16,
    #[serde(default)]
    pub headers: BTreeMap<String, String>,
    #[serde(default)]
    pub body: String,
}

#[async_trait]
pub trait HostServices: Send + Sync {
    /// `key` は `{kv_namespace}:{プラグインのキー}`（ネームスペースはホストが付ける）
    async fn kv_get(&self, key: &str) -> Result<Option<String>>;

    async fn kv_set(&self, key: &str, value: &str) -> Result<()>;

    /// 外部への HTTP リクエスト。`allowed_domains` の確認はランタイムが済ませてから呼ぶ
    async fn http_fetch(&self, plugin: &str, request: HttpRequest) -> Result<HttpResponse>;

    /// プラグインの設定値（`[plugins.settings.<plugin>]`）
    fn plugin_setting(&self, plugin: &str, key: &str) -> Option<String>;
}

/// プロセス内メモリの KV だけを持つ既定の実装（テスト・ツール向け）
pub struct LocalHostServices {
    kv: Arc<Mutex<HashMap<String, String>>>,
}

impl LocalHostServices {
    pub fn new(kv: Arc<Mutex<HashMap<String, String>>>) -> Self {
        Self { kv }
    }
}

#[async_trait]
impl HostServices for LocalHostServices {
    async fn kv_get(&self, key: &str) -> Result<Option<String>> {
        let kv = self
            .kv
            .lock()
            .map_err(|_| AegisError::Internal("KV store lock poisoned".to_string()))?;
        Ok(kv.get(key).cloned())
    }

    async fn kv_set(&self, key: &str, value: &str) -> Result<()> {
        let mut kv = self
            .kv
            .lock()
            .map_err(|_| AegisError::Internal("KV store lock poisoned".to_string()))?;
        kv.insert(key.to_string(), value.to_string());
        Ok(())
    }

    async fn http_fetch(&self, _plugin: &str, _request: HttpRequest) -> Result<HttpResponse> {
        Err(AegisError::PermissionDenied(
            "HTTP requests are not available in this host".to_string(),
        ))
    }

    fn plugin_setting(&self, _plugin: &str, _key: &str) -> Option<String> {
        None
    }
}

/// プラグインに渡すホストサービス。従来どおりメモリの KV（`Arc<Mutex<HashMap>>`）も渡せる
#[derive(Clone)]
pub struct PluginServices(pub Arc<dyn HostServices>);

impl From<Arc<Mutex<HashMap<String, String>>>> for PluginServices {
    fn from(kv: Arc<Mutex<HashMap<String, String>>>) -> Self {
        Self(Arc::new(LocalHostServices::new(kv)))
    }
}

impl From<Arc<dyn HostServices>> for PluginServices {
    fn from(services: Arc<dyn HostServices>) -> Self {
        Self(services)
    }
}
