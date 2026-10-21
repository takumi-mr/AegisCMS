use aegis_core::{CacheBackend, Result};
use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;

#[derive(Clone, Debug)]
struct CacheEntry {
    value: String,
    expires_at: Option<Instant>,
}

impl CacheEntry {
    fn is_expired(&self) -> bool {
        if let Some(exp) = self.expires_at {
            Instant::now() >= exp
        } else {
            false
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct MemoryCacheBackend {
    store: Arc<RwLock<HashMap<String, CacheEntry>>>,
}

impl MemoryCacheBackend {
    pub fn new() -> Self {
        Self {
            store: Arc::new(RwLock::new(HashMap::new())),
        }
    }
}

#[async_trait]
impl CacheBackend for MemoryCacheBackend {
    async fn get(&self, key: &str) -> Result<Option<String>> {
        let mut store = self.store.write().await;
        if let Some(entry) = store.get(key) {
            if entry.is_expired() {
                store.remove(key);
                Ok(None)
            } else {
                Ok(Some(entry.value.clone()))
            }
        } else {
            Ok(None)
        }
    }

    async fn set(&self, key: &str, value: &str, ttl_secs: Option<u64>) -> Result<()> {
        let expires_at = ttl_secs.map(|s| Instant::now() + Duration::from_secs(s));
        let mut store = self.store.write().await;
        store.insert(
            key.to_string(),
            CacheEntry {
                value: value.to_string(),
                expires_at,
            },
        );
        Ok(())
    }

    async fn delete(&self, key: &str) -> Result<()> {
        let mut store = self.store.write().await;
        store.remove(key);
        Ok(())
    }

    async fn exists(&self, key: &str) -> Result<bool> {
        let mut store = self.store.write().await;
        if let Some(entry) = store.get(key) {
            if entry.is_expired() {
                store.remove(key);
                Ok(false)
            } else {
                Ok(true)
            }
        } else {
            Ok(false)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_memory_cache_crud() {
        let cache = MemoryCacheBackend::new();

        // 1. Set without TTL
        cache.set("key1", "val1", None).await.unwrap();
        assert_eq!(cache.get("key1").await.unwrap(), Some("val1".to_string()));
        assert!(cache.exists("key1").await.unwrap());

        // 2. Delete
        cache.delete("key1").await.unwrap();
        assert_eq!(cache.get("key1").await.unwrap(), None);
        assert!(!cache.exists("key1").await.unwrap());

        // 3. TTL Expire
        cache.set("key_ttl", "val_ttl", Some(1)).await.unwrap();
        assert_eq!(
            cache.get("key_ttl").await.unwrap(),
            Some("val_ttl".to_string())
        );
        tokio::time::sleep(Duration::from_millis(1100)).await;
        assert_eq!(cache.get("key_ttl").await.unwrap(), None);
        assert!(!cache.exists("key_ttl").await.unwrap());
    }
}
