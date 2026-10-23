//! `[cache]` の設定

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CacheBackendKind {
    #[default]
    Memory,
    Redis,
}

impl std::str::FromStr for CacheBackendKind {
    type Err = String;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s.to_ascii_lowercase().as_str() {
            "memory" | "inmemory" | "local" => Ok(Self::Memory),
            "redis" => Ok(Self::Redis),
            other => Err(format!("Unknown cache backend kind: {}", other)),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct CacheConfig {
    #[serde(default)]
    pub backend: CacheBackendKind,
    #[serde(default = "default_cache_ttl")]
    pub default_ttl_secs: u64,
    #[serde(default)]
    pub redis: RedisCacheConfig,
}

fn default_cache_ttl() -> u64 {
    3600 // 1 hour
}

impl Default for CacheConfig {
    fn default() -> Self {
        Self {
            backend: CacheBackendKind::Memory,
            default_ttl_secs: default_cache_ttl(),
            redis: RedisCacheConfig::default(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct RedisCacheConfig {
    pub url: String,
    #[serde(default = "default_redis_timeout")]
    pub timeout_ms: u64,
}

fn default_redis_timeout() -> u64 {
    2000
}

impl Default for RedisCacheConfig {
    fn default() -> Self {
        Self {
            url: "redis://127.0.0.1:6379".to_string(),
            timeout_ms: default_redis_timeout(),
        }
    }
}
