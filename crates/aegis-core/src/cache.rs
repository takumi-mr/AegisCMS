use crate::error::Result;
use async_trait::async_trait;
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

#[async_trait]
pub trait CacheBackend: Send + Sync {
    /// Retrieve value by key from cache/session store
    async fn get(&self, key: &str) -> Result<Option<String>>;

    /// Store a value with optional TTL in seconds
    async fn set(&self, key: &str, value: &str, ttl_secs: Option<u64>) -> Result<()>;

    /// Delete a key
    async fn delete(&self, key: &str) -> Result<()>;

    /// Check if key exists
    async fn exists(&self, key: &str) -> Result<bool>;

    /// Atomically increment an integer counter and return the new value.
    /// A missing (or expired) key starts from 0; `ttl_secs` is applied only when the key is
    /// created, so the counter expires a fixed time after the first increment
    async fn increment(&self, key: &str, ttl_secs: Option<u64>) -> Result<u64>;
}
