use aegis_core::time::TimestampExt;
use aegis_core::{CacheBackend, Result};
use aegis_db::DatabaseAdapter;
use jiff::Timestamp;
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
    /// API トークン（`aegis_pat_...`）の検索先。無ければ API トークンは使えない
    api_tokens: Option<Arc<dyn DatabaseAdapter>>,
}

impl SessionService {
    pub fn new(cache: Arc<dyn CacheBackend>, default_ttl_secs: u64) -> Self {
        Self {
            cache,
            default_ttl_secs,
            api_tokens: None,
        }
    }

    /// API トークンを `get_session` で受け付ける（トークンは DB に保存してある）
    pub fn with_api_tokens(mut self, db: Arc<dyn DatabaseAdapter>) -> Self {
        self.api_tokens = Some(db);
        self
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
        self.create_session_issued_at(user_id, role, metadata, ttl_secs, Timestamp::now())
            .await
    }

    /// `issued_at` を発行時刻（`created_at`）にしてセッションを作る。有効期限は今から数える。
    ///
    /// ログインでは、資格情報を読み始めた時刻を渡す。パスワードの検証中や 2FA の入力待ちの間に
    /// アカウントの無効化・ロールの変更・パスワードの変更（どれも [`revoke_user_sessions`] を呼ぶ）が
    /// あれば、作ったばかりのセッションも失効した扱いになる
    ///
    /// [`revoke_user_sessions`]: Self::revoke_user_sessions
    pub async fn create_session_issued_at(
        &self,
        user_id: &str,
        role: &str,
        metadata: HashMap<String, serde_json::Value>,
        ttl_secs: Option<u64>,
        issued_at: Timestamp,
    ) -> Result<SessionData> {
        let ttl = ttl_secs.unwrap_or(self.default_ttl_secs);
        let id = format!("ses_{}", Uuid::new_v4().simple());
        let now = Timestamp::now();
        let expires = now + jiff::SignedDuration::from_secs(ttl as i64);

        let session = SessionData {
            id: id.clone(),
            user_id: user_id.to_string(),
            role: role.to_string(),
            created_at: issued_at.min(now).to_rfc3339_auto(),
            expires_at: expires.to_rfc3339_auto(),
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
        let now = Timestamp::now().to_rfc3339_auto();
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
                aegis_core::time::parse_rfc3339(&revoked_at_str),
                aegis_core::time::parse_rfc3339(&session.created_at),
            ) {
                (Ok(revoked_at), Ok(created_at)) => Ok(created_at <= revoked_at),
                _ => Ok(true),
            }
        } else {
            Ok(false)
        }
    }

    pub async fn get_session(&self, session_id: &str) -> Result<Option<SessionData>> {
        if crate::api_tokens::is_api_token(session_id) {
            return match &self.api_tokens {
                Some(db) => crate::api_tokens::resolve_api_token(db.as_ref(), session_id).await,
                None => Ok(None),
            };
        }
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

    /// ログインを始めた後（パスワードの検証中・2FA の入力待ち）に失効があれば、
    /// その後に作ったセッションも使えない
    #[tokio::test]
    async fn test_session_issued_before_a_revocation_is_rejected() {
        let cache = Arc::new(MemoryCacheBackend::new());
        let service = SessionService::new(cache, 3600);

        let login_started = Timestamp::now();
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        service.revoke_user_sessions("user-a").await.unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;

        let late = service
            .create_session_issued_at("user-a", "admin", HashMap::new(), None, login_started)
            .await
            .unwrap();
        assert_eq!(late.created_at, login_started.to_rfc3339_auto());
        assert_eq!(service.get_session(&late.id).await.unwrap(), None);

        // 失効の後に始めたログインは通る
        let fresh = service
            .create_session_issued_at("user-a", "admin", HashMap::new(), None, Timestamp::now())
            .await
            .unwrap();
        assert!(service.get_session(&fresh.id).await.unwrap().is_some());
    }

    /// 未来の時刻は渡されても今に揃える（失効をすり抜ける抜け道にしない）
    #[tokio::test]
    async fn test_session_issued_at_is_never_in_the_future() {
        let cache = Arc::new(MemoryCacheBackend::new());
        let service = SessionService::new(cache, 3600);
        let future = Timestamp::now() + jiff::SignedDuration::from_secs(3600);
        let session = service
            .create_session_issued_at("user-a", "editor", HashMap::new(), None, future)
            .await
            .unwrap();
        let created = aegis_core::time::parse_rfc3339(&session.created_at).unwrap();
        assert!(created <= Timestamp::now());
    }
}
