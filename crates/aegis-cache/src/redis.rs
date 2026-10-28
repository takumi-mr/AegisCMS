use aegis_core::{AegisError, CacheBackend, Result};
use async_trait::async_trait;
use redis::aio::{ConnectionManager, ConnectionManagerConfig};
use redis::AsyncCommands;
use std::time::Duration;

/// Reconnection attempts after the first failed connection (also used when a live connection drops)
const CONNECT_RETRIES: usize = 2;
/// Upper bound of the wait between reconnection attempts
const MAX_RETRY_DELAY_MS: u64 = 1000;

/// Opens `url` and connects a [`ConnectionManager`] that reconnects by itself (shared with the event bus)
pub(crate) async fn connect_manager(
    url: &str,
    timeout: Duration,
) -> Result<(redis::Client, ConnectionManager)> {
    let client = redis::Client::open(url)
        .map_err(|e| AegisError::Database(format!("Invalid Redis URL '{}': {}", url, e)))?;

    let config = ConnectionManagerConfig::new()
        .set_connection_timeout(Some(timeout))
        .set_response_timeout(Some(timeout))
        // redis 1.8 の既定（指数バックオフ・6 回再試行）から、再試行は 2 回、待機は
        // 最大 1 秒にして、接続できなければすぐにエラーにする
        .set_number_of_retries(CONNECT_RETRIES)
        .set_exponent_base(2.0)
        .set_max_delay(Duration::from_millis(MAX_RETRY_DELAY_MS));
    let manager = ConnectionManager::new_with_config(client.clone(), config)
        .await
        .map_err(|e| {
            AegisError::Database(format!("Failed to connect to Redis '{}': {}", url, e))
        })?;
    Ok((client, manager))
}

#[derive(Clone)]
pub struct RedisCacheBackend {
    manager: ConnectionManager,
    url: String,
}

impl RedisCacheBackend {
    /// Connects with `timeout` (`[cache.redis].timeout_ms`) as both the per-attempt connection
    /// timeout and the response timeout of every command
    pub async fn connect(url: &str, timeout: Duration) -> Result<Self> {
        let (_, manager) = connect_manager(url, timeout).await?;
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

/// `ConnectionManager` は接続が切れていると次の 1 回をエラーにしてから繋ぎ直す（Redis の再起動など）。
/// 何度送っても結果が同じコマンドは、接続のエラーのときだけもう 1 回送る
pub(crate) async fn retry_on_reconnect<T, F, Fut>(mut command: F) -> redis::RedisResult<T>
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = redis::RedisResult<T>>,
{
    match command().await {
        Err(e) if e.is_connection_dropped() || e.is_io_error() => command().await,
        result => result,
    }
}

#[async_trait]
impl CacheBackend for RedisCacheBackend {
    async fn get(&self, key: &str) -> Result<Option<String>> {
        retry_on_reconnect(|| {
            let mut conn = self.manager.clone();
            async move { conn.get(key).await }
        })
        .await
        .map_err(|e| AegisError::Database(format!("Redis GET error: {}", e)))
    }

    async fn set(&self, key: &str, value: &str, ttl_secs: Option<u64>) -> Result<()> {
        retry_on_reconnect(|| {
            let mut conn = self.manager.clone();
            async move {
                match ttl_secs {
                    Some(ttl) => conn.set_ex::<_, _, ()>(key, value, ttl).await,
                    None => conn.set::<_, _, ()>(key, value).await,
                }
            }
        })
        .await
        .map_err(|e| AegisError::Database(format!("Redis SET error: {}", e)))
    }

    async fn delete(&self, key: &str) -> Result<()> {
        retry_on_reconnect(|| {
            let mut conn = self.manager.clone();
            async move { conn.del::<_, ()>(key).await }
        })
        .await
        .map_err(|e| AegisError::Database(format!("Redis DEL error: {}", e)))
    }

    async fn exists(&self, key: &str) -> Result<bool> {
        let count: i64 = retry_on_reconnect(|| {
            let mut conn = self.manager.clone();
            async move { conn.exists(key).await }
        })
        .await
        .map_err(|e| AegisError::Database(format!("Redis EXISTS error: {}", e)))?;
        Ok(count > 0)
    }

    /// INCR は送り直すと二重に数えるおそれがあるので、接続のエラーでも送り直さない
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
