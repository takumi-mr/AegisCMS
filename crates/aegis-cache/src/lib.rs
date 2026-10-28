//! キャッシュ・セッションストアのバックエンド（`[cache]`: メモリ / Redis）、同じバックエンドを使う
//! サーバー間のイベントバス、および CDN の Cache-Tag / Surrogate-Key パージ抽象（`[cdn]`）

pub mod bus;
pub mod cdn;
pub mod config;
pub mod memory;
pub mod redis;

pub use bus::{BusMessage, BusSubscription, EventBus, MemoryEventBus, RedisEventBus};
pub use cdn::{
    create_cdn_purger, CacheTags, CdnConfig, CdnProviderKind, CdnPurger, CloudflareCdnConfig,
    CloudflareCdnPurger, FastlyCdnConfig, FastlyCdnPurger, LogCdnPurger, NoopCdnPurger,
    RecordingCdnPurger, WebhookCdnConfig, WebhookCdnPurger, DEFAULT_CDN_CACHE_CONTROL,
};
pub use config::{CacheBackendKind, CacheConfig, RedisCacheConfig};
pub use memory::MemoryCacheBackend;
pub use redis::RedisCacheBackend;

use aegis_core::{CacheBackend, Result};
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

/// `[cache]` と同じバックエンドのイベントバス（メモリ: このプロセスの中だけ、Redis: Pub/Sub で全サーバー）
pub async fn create_event_bus(config: &CacheConfig) -> Result<Arc<dyn EventBus>> {
    match config.backend {
        CacheBackendKind::Memory => Ok(Arc::new(MemoryEventBus::new())),
        CacheBackendKind::Redis => {
            let bus = RedisEventBus::connect(
                &config.redis.url,
                std::time::Duration::from_millis(config.redis.timeout_ms),
            )
            .await?;
            tracing::info!(
                "Event bus: Redis Pub/Sub channel '{}' at {}",
                bus::BUS_CHANNEL,
                config.redis.url
            );
            Ok(Arc::new(bus))
        }
    }
}
