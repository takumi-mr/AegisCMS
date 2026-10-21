use aegis_core::{AegisError, CacheBackend, Result};
use async_trait::async_trait;
use redis::aio::ConnectionManager;
use redis::AsyncCommands;

#[derive(Clone)]
pub struct RedisCacheBackend {
    manager: ConnectionManager,
    url: String,
}

impl RedisCacheBackend {
    pub async fn connect(url: &str) -> Result<Self> {
        let client = redis::Client::open(url)
            .map_err(|e| AegisError::Database(format!("Invalid Redis URL '{}': {}", url, e)))?;

        let manager = ConnectionManager::new(client).await.map_err(|e| {
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
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_invalid_redis_url() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let res = RedisCacheBackend::connect("not-a-valid-url").await;
            assert!(res.is_err());
        });
    }
}
