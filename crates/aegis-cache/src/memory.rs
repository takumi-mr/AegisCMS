use aegis_core::{AegisError, CacheBackend, Result};
use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
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

/// 期限切れエントリを掃除するエントリ数の下限
const MIN_SWEEP_THRESHOLD: usize = 1024;

#[derive(Clone, Debug)]
pub struct MemoryCacheBackend {
    store: Arc<RwLock<HashMap<String, CacheEntry>>>,
    /// 書き込み時にエントリ数がこれを超えたら期限切れを掃除する（次回は掃除後の 2 倍）。
    /// 読まれないまま期限切れになったキー（例: ログイン失敗の計数）が溜まり続けないようにする
    sweep_threshold: Arc<AtomicUsize>,
}

impl Default for MemoryCacheBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl MemoryCacheBackend {
    pub fn new() -> Self {
        Self {
            store: Arc::new(RwLock::new(HashMap::new())),
            sweep_threshold: Arc::new(AtomicUsize::new(MIN_SWEEP_THRESHOLD)),
        }
    }

    /// 書き込みロックを持った状態で呼ぶ。掃除の総コストは書き込み回数に対して償却 O(1)
    fn sweep_if_needed(&self, store: &mut HashMap<String, CacheEntry>) {
        if store.len() < self.sweep_threshold.load(Ordering::Relaxed) {
            return;
        }
        store.retain(|_, entry| !entry.is_expired());
        self.sweep_threshold.store(
            (store.len() * 2).max(MIN_SWEEP_THRESHOLD),
            Ordering::Relaxed,
        );
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
        self.sweep_if_needed(&mut store);
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

    async fn increment(&self, key: &str, ttl_secs: Option<u64>) -> Result<u64> {
        let mut store = self.store.write().await;
        let current = match store.get(key) {
            Some(entry) if !entry.is_expired() => Some(entry.clone()),
            _ => None,
        };
        let (count, expires_at) = match current {
            Some(entry) => {
                let count = entry.value.parse::<u64>().map_err(|_| {
                    AegisError::Validation(format!("Cache value of '{}' is not a counter", key))
                })?;
                (count + 1, entry.expires_at)
            }
            None => {
                self.sweep_if_needed(&mut store);
                (1, ttl_secs.map(|s| Instant::now() + Duration::from_secs(s)))
            }
        };
        store.insert(
            key.to_string(),
            CacheEntry {
                value: count.to_string(),
                expires_at,
            },
        );
        Ok(count)
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

    #[tokio::test]
    async fn test_memory_cache_increment_keeps_first_ttl() {
        let cache = MemoryCacheBackend::new();

        assert_eq!(cache.increment("counter", Some(1)).await.unwrap(), 1);
        tokio::time::sleep(Duration::from_millis(600)).await;
        // 2 回目以降の加算では有効期限を延長しない（固定ウィンドウ）
        assert_eq!(cache.increment("counter", Some(1)).await.unwrap(), 2);
        tokio::time::sleep(Duration::from_millis(600)).await;
        assert_eq!(cache.increment("counter", Some(1)).await.unwrap(), 1);

        cache.set("text", "abc", None).await.unwrap();
        assert!(cache.increment("text", None).await.is_err());
    }

    #[tokio::test]
    async fn test_memory_cache_sweeps_unread_expired_entries() {
        let cache = MemoryCacheBackend::new();
        for i in 0..3 {
            cache.set(&format!("old{}", i), "x", Some(1)).await.unwrap();
        }
        cache.set("keep", "x", None).await.unwrap();
        tokio::time::sleep(Duration::from_millis(1100)).await;

        // 閾値に達した状態で書き込むと、読まれていない期限切れエントリも消える
        cache.sweep_threshold.store(4, Ordering::Relaxed);
        cache.increment("new", Some(60)).await.unwrap();
        let store = cache.store.read().await;
        assert_eq!(store.len(), 2);
        assert!(store.contains_key("keep") && store.contains_key("new"));
        assert_eq!(
            cache.sweep_threshold.load(Ordering::Relaxed),
            MIN_SWEEP_THRESHOLD
        );
    }
}
