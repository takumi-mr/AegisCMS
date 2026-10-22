pub mod memory;
pub mod redis;

pub use memory::MemoryCacheBackend;
pub use redis::RedisCacheBackend;

use aegis_core::{CacheBackend, CacheBackendKind, CacheConfig, Result};
use std::sync::Arc;

pub async fn create_cache_backend(config: &CacheConfig) -> Result<Arc<dyn CacheBackend>> {
    match config.backend {
        CacheBackendKind::Memory => {
            tracing::info!(
                "Initializing In-Memory CacheBackend (default TTL: {}s)",
                config.default_ttl_secs
            );
            Ok(Arc::new(MemoryCacheBackend::new()))
        }
        CacheBackendKind::Redis => {
            tracing::info!(
                "Initializing Redis CacheBackend at {} (default TTL: {}s, timeout: {}ms)",
                config.redis.url,
                config.default_ttl_secs,
                config.redis.timeout_ms
            );
            let backend = RedisCacheBackend::connect(
                &config.redis.url,
                std::time::Duration::from_millis(config.redis.timeout_ms),
            )
            .await?;
            Ok(Arc::new(backend))
        }
    }
}
