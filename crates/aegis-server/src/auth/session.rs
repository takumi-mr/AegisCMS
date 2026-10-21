use aegis_core::{CacheBackend, Result};
use chrono::{Duration, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SessionData {
    pub id: String,
    pub user_id: String,
    pub role: String,
    pub created_at: String,
    pub expires_at: String,
    #[serde(default)]
    pub metadata: HashMap<String, serde_json::Value>,
}

#[derive(Clone)]
pub struct SessionService {
    cache: Arc<dyn CacheBackend>,
    default_ttl_secs: u64,
}

impl SessionService {
    pub fn new(cache: Arc<dyn CacheBackend>, default_ttl_secs: u64) -> Self {
        Self {
            cache,
            default_ttl_secs,
        }
    }

    fn session_key(session_id: &str) -> String {
        format!("session:{}", session_id)
    }

    pub async fn create_session(
        &self,
        user_id: &str,
        role: &str,
        metadata: HashMap<String, serde_json::Value>,
        ttl_secs: Option<u64>,
    ) -> Result<SessionData> {
        let ttl = ttl_secs.unwrap_or(self.default_ttl_secs);
        let id = format!("ses_{}", Uuid::new_v4().simple());
        let now = Utc::now();
        let expires = now + Duration::seconds(ttl as i64);

        let session = SessionData {
            id: id.clone(),
            user_id: user_id.to_string(),
            role: role.to_string(),
            created_at: now.to_rfc3339(),
            expires_at: expires.to_rfc3339(),
            metadata,
        };

        let json_str = serde_json::to_string(&session)?;
        self.cache
            .set(&Self::session_key(&id), &json_str, Some(ttl))
            .await?;

        Ok(session)
    }

    pub async fn get_session(&self, session_id: &str) -> Result<Option<SessionData>> {
        let key = Self::session_key(session_id);
        if let Some(json_str) = self.cache.get(&key).await? {
            let session: SessionData = serde_json::from_str(&json_str)?;
            Ok(Some(session))
        } else {
            Ok(None)
        }
    }

    pub async fn destroy_session(&self, session_id: &str) -> Result<()> {
        let key = Self::session_key(session_id);
        self.cache.delete(&key).await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cache::MemoryCacheBackend;

    #[tokio::test]
    async fn test_stateless_session_lifecycle() {
        let cache = Arc::new(MemoryCacheBackend::new());
        let service = SessionService::new(cache, 3600);

        let mut meta = HashMap::new();
        meta.insert("ip".to_string(), serde_json::json!("127.0.0.1"));

        // 1. Create session
        let session = service
            .create_session("user-42", "admin", meta, Some(60))
            .await
            .expect("create_session should succeed");

        assert_eq!(session.user_id, "user-42");
        assert_eq!(session.role, "admin");
        assert!(session.id.starts_with("ses_"));

        // 2. Fetch session
        let fetched = service
            .get_session(&session.id)
            .await
            .expect("get_session should succeed");
        assert_eq!(fetched, Some(session.clone()));

        // 3. Destroy session
        service
            .destroy_session(&session.id)
            .await
            .expect("destroy_session should succeed");
        let after_destroy = service
            .get_session(&session.id)
            .await
            .expect("get_session should succeed");
        assert_eq!(after_destroy, None);
    }
}
