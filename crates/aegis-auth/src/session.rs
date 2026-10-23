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

    fn revocation_key(user_id: &str) -> String {
        format!("auth:revoked_user:{}", user_id)
    }

    /// Mark all existing sessions for this user as revoked (blacklisted until they expire).
    /// New sessions created after this timestamp will remain valid.
    pub async fn revoke_user_sessions(&self, user_id: &str) -> Result<()> {
        let key = Self::revocation_key(user_id);
        let now = Utc::now().to_rfc3339();
        self.cache
            .set(&key, &now, Some(self.default_ttl_secs))
            .await?;
        Ok(())
    }

    /// Check if a session has been revoked by a user-level revocation marker.
    pub async fn is_session_revoked(&self, session: &SessionData) -> Result<bool> {
        let key = Self::revocation_key(&session.user_id);
        if let Some(revoked_at_str) = self.cache.get(&key).await? {
            match (
                chrono::DateTime::parse_from_rfc3339(&revoked_at_str),
                chrono::DateTime::parse_from_rfc3339(&session.created_at),
            ) {
                (Ok(revoked_at), Ok(created_at)) => Ok(created_at <= revoked_at),
                _ => Ok(true),
            }
        } else {
            Ok(false)
        }
    }

    pub async fn get_session(&self, session_id: &str) -> Result<Option<SessionData>> {
        let key = Self::session_key(session_id);
        if let Some(json_str) = self.cache.get(&key).await? {
            let session: SessionData = serde_json::from_str(&json_str)?;
            if self.is_session_revoked(&session).await? {
                let _ = self.cache.delete(&key).await;
                return Ok(None);
            }
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
    use aegis_cache::MemoryCacheBackend;

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

    #[tokio::test]
    async fn test_session_revocation_blacklist() {
        let cache = Arc::new(MemoryCacheBackend::new());
        let service = SessionService::new(cache, 3600);

        // 1. Create two sessions for user-a and one for user-b
        let s1 = service
            .create_session("user-a", "editor", HashMap::new(), None)
            .await
            .unwrap();
        let s2 = service
            .create_session("user-a", "editor", HashMap::new(), None)
            .await
            .unwrap();
        let s_other = service
            .create_session("user-b", "viewer", HashMap::new(), None)
            .await
            .unwrap();

        assert!(service.get_session(&s1.id).await.unwrap().is_some());
        assert!(service.get_session(&s2.id).await.unwrap().is_some());
        assert!(service.get_session(&s_other.id).await.unwrap().is_some());

        // Ensure clock advances slightly before revocation
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;

        // 2. Revoke user-a sessions (e.g. role change, disable, delete)
        service.revoke_user_sessions("user-a").await.unwrap();

        // user-a's existing sessions must now be rejected (None)
        assert_eq!(service.get_session(&s1.id).await.unwrap(), None);
        assert_eq!(service.get_session(&s2.id).await.unwrap(), None);

        // user-b's session is unaffected
        assert!(service.get_session(&s_other.id).await.unwrap().is_some());

        // Ensure clock advances before re-login
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;

        // 3. A new session created after revocation should be valid
        let s_new = service
            .create_session("user-a", "admin", HashMap::new(), None)
            .await
            .unwrap();
        assert!(service.get_session(&s_new.id).await.unwrap().is_some());
    }
}
