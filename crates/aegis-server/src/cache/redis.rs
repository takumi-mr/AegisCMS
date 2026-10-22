use aegis_core::{AegisError, CacheBackend, Result};
use async_trait::async_trait;
use redis::aio::{ConnectionManager, ConnectionManagerConfig};
use redis::AsyncCommands;
use std::time::Duration;

/// Reconnection attempts after the first failed connection (also used when a live connection drops)
const CONNECT_RETRIES: usize = 2;
/// Upper bound of the wait between reconnection attempts
const MAX_RETRY_DELAY_MS: u64 = 1000;

#[derive(Clone)]
pub struct RedisCacheBackend {
    manager: ConnectionManager,
    url: String,
}

impl RedisCacheBackend {
    /// Connects with `timeout` (`[cache.redis].timeout_ms`) as both the per-attempt connection
    /// timeout and the response timeout of every command
    pub async fn connect(url: &str, timeout: Duration) -> Result<Self> {
        let client = redis::Client::open(url)
            .map_err(|e| AegisError::Database(format!("Invalid Redis URL '{}': {}", url, e)))?;

        let config = ConnectionManagerConfig::new()
            .set_connection_timeout(timeout)
            .set_response_timeout(timeout)
            // redis 0.27 の既定（factor 100 の指数バックオフ・6 回再試行）では、2 回目以降の待機が
            // 100 秒単位に伸び、応答しない Redis で起動や再接続が数分止まる。再試行は 2 回、待機は
            // 最大 1 秒にして、接続できなければすぐにエラーにする
            .set_number_of_retries(CONNECT_RETRIES)
            .set_factor(2)
            .set_max_delay(MAX_RETRY_DELAY_MS);
        let manager = ConnectionManager::new_with_config(client, config)
            .await
            .map_err(|e| {
                AegisError::Database(format!("Failed to connect to Redis '{}': {}", url, e))
            })?;

        Ok(Self {
            manager,
            url: url.to_string(),
        })
    }

    #[allow(dead_code)]
    pub fn url(&self) -> &str {
        &self.url
    }
}

#[async_trait]
impl CacheBackend for RedisCacheBackend {
    async fn get(&self, key: &str) -> Result<Option<String>> {
        let mut conn = self.manager.clone();
        let val: Option<String> = conn
            .get(key)
            .await
            .map_err(|e| AegisError::Database(format!("Redis GET error: {}", e)))?;
        Ok(val)
    }

    async fn set(&self, key: &str, value: &str, ttl_secs: Option<u64>) -> Result<()> {
        let mut conn = self.manager.clone();
        if let Some(ttl) = ttl_secs {
            conn.set_ex::<_, _, ()>(key, value, ttl)
                .await
                .map_err(|e| AegisError::Database(format!("Redis SETEX error: {}", e)))?;
        } else {
            conn.set::<_, _, ()>(key, value)
                .await
                .map_err(|e| AegisError::Database(format!("Redis SET error: {}", e)))?;
        }
        Ok(())
    }

    async fn delete(&self, key: &str) -> Result<()> {
        let mut conn = self.manager.clone();
        conn.del::<_, ()>(key)
            .await
            .map_err(|e| AegisError::Database(format!("Redis DEL error: {}", e)))?;
        Ok(())
    }

    async fn exists(&self, key: &str) -> Result<bool> {
        let mut conn = self.manager.clone();
        let count: i64 = conn
            .exists(key)
            .await
            .map_err(|e| AegisError::Database(format!("Redis EXISTS error: {}", e)))?;
        Ok(count > 0)
    }

    async fn increment(&self, key: &str, ttl_secs: Option<u64>) -> Result<u64> {
        // INCR と初回のみの EXPIRE を 1 つのスクリプトで実行し、TTL の付け忘れ（永続カウンタ化）を防ぐ
        let script = redis::Script::new(
            r"local count = redis.call('INCR', KEYS[1])
if count == 1 and tonumber(ARGV[1]) > 0 then
  redis.call('EXPIRE', KEYS[1], ARGV[1])
end
return count",
        );
        let mut conn = self.manager.clone();
        let count: u64 = script
            .key(key)
            .arg(ttl_secs.unwrap_or(0))
            .invoke_async(&mut conn)
            .await
            .map_err(|e| AegisError::Database(format!("Redis INCR error: {}", e)))?;
        Ok(count)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_invalid_redis_url() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let res = RedisCacheBackend::connect("not-a-valid-url", Duration::from_secs(1)).await;
            assert!(res.is_err());
        });
    }

    /// A "Redis" that accepts TCP connections but never answers: the configured timeout must
    /// turn the hang into an error instead of blocking forever
    #[tokio::test]
    async fn test_unresponsive_redis_times_out() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            let mut held = Vec::new();
            while let Ok((socket, _)) = listener.accept().await {
                held.push(socket);
            }
        });

        let started = std::time::Instant::now();
        let url = format!("redis://{}", addr);
        let outcome = tokio::time::timeout(Duration::from_secs(30), async {
            let backend = RedisCacheBackend::connect(&url, Duration::from_millis(200)).await?;
            backend.get("key").await
        })
        .await
        .expect("the Redis timeout should fire well before the 30s guard");
        assert!(
            outcome.is_err(),
            "an unresponsive server must yield an error"
        );
        // 3 attempts x 200ms timeout + 2 backoffs of at most 1s (plus jitter); without the
        // retry settings this takes minutes
        assert!(
            started.elapsed() < Duration::from_secs(10),
            "took {:?}",
            started.elapsed()
        );
    }
}
