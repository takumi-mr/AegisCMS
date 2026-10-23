use super::config::LoginRateLimitConfig;
use aegis_core::{CacheBackend, Result};
use chrono::Utc;
use sha2::{Digest, Sha256};
use std::sync::Arc;

/// ロックの単位
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LockScope {
    Username,
    Ip,
}

impl LockScope {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Username => "username",
            Self::Ip => "ip",
        }
    }
}

/// ログインがロックされている状態
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoginLockout {
    pub scope: LockScope,
    /// ロック解除までの秒数（`Retry-After` に使う）
    pub retry_after_secs: u64,
}

/// `POST /api/v1/auth/session` のログイン失敗をユーザー名・クライアント IP 単位で数え、
/// 上限に達したら一定時間ログインを拒否する。
///
/// キャッシュには次のキーを置く（`{id}` はユーザー名なら小文字化した名前の SHA-256、IP はそのまま）:
/// - `login:fail:{scope}:{id}`: 失敗回数。最初の失敗から `window_secs` で消える
/// - `login:lock:{scope}:{id}`: ロック解除時刻（UNIX 秒）。`lockout_secs` で消える
pub struct LoginRateLimiter {
    cache: Arc<dyn CacheBackend>,
    config: LoginRateLimitConfig,
}

impl LoginRateLimiter {
    pub fn new(cache: Arc<dyn CacheBackend>, config: LoginRateLimitConfig) -> Self {
        Self { cache, config }
    }

    /// 有効な (単位, キャッシュキー用 ID, 失敗回数の上限) の一覧
    fn scopes(&self, username: &str, ip: Option<&str>) -> Vec<(LockScope, String, u64)> {
        if !self.config.enabled {
            return Vec::new();
        }
        let mut scopes = Vec::new();
        if self.config.max_failures_per_user > 0 {
            // DB によってはユーザー名の大文字・小文字を区別しないため、区別せずに数える
            let digest = Sha256::digest(username.to_ascii_lowercase().as_bytes());
            scopes.push((
                LockScope::Username,
                hex::encode(digest),
                self.config.max_failures_per_user,
            ));
        }
        if let (Some(ip), true) = (ip, self.config.max_failures_per_ip > 0) {
            scopes.push((
                LockScope::Ip,
                ip.to_string(),
                self.config.max_failures_per_ip,
            ));
        }
        scopes
    }

    fn fail_key(scope: LockScope, id: &str) -> String {
        format!("login:fail:{}:{}", scope.as_str(), id)
    }

    fn lock_key(scope: LockScope, id: &str) -> String {
        format!("login:lock:{}:{}", scope.as_str(), id)
    }

    /// ロック中なら残り時間の最も長いロックを返す
    pub async fn check(&self, username: &str, ip: Option<&str>) -> Result<Option<LoginLockout>> {
        let now = Utc::now().timestamp();
        let mut lockout: Option<LoginLockout> = None;
        for (scope, id, _) in self.scopes(username, ip) {
            let Some(value) = self.cache.get(&Self::lock_key(scope, &id)).await? else {
                continue;
            };
            // 解除時刻が読めない値でもキーが残っている間はロックとして扱う
            let retry_after_secs = value
                .parse::<i64>()
                .map(|until| (until - now).max(1) as u64)
                .unwrap_or(self.config.lockout_secs.max(1));
            if lockout
                .as_ref()
                .is_none_or(|l| retry_after_secs > l.retry_after_secs)
            {
                lockout = Some(LoginLockout {
                    scope,
                    retry_after_secs,
                });
            }
        }
        Ok(lockout)
    }

    /// 認証失敗を記録する。この失敗で上限に達した場合は、かけたロックを返す
    pub async fn record_failure(
        &self,
        username: &str,
        ip: Option<&str>,
    ) -> Result<Option<LoginLockout>> {
        let window = self.config.window_secs.max(1);
        let lockout_secs = self.config.lockout_secs.max(1);
        let mut lockout = None;
        for (scope, id, max_failures) in self.scopes(username, ip) {
            let count = self
                .cache
                .increment(&Self::fail_key(scope, &id), Some(window))
                .await?;
            if count < max_failures {
                continue;
            }
            let until = Utc::now().timestamp() + lockout_secs as i64;
            self.cache
                .set(
                    &Self::lock_key(scope, &id),
                    &until.to_string(),
                    Some(lockout_secs),
                )
                .await?;
            // ロック解除後は 0 回から数え直す
            self.cache.delete(&Self::fail_key(scope, &id)).await?;
            lockout.get_or_insert(LoginLockout {
                scope,
                retry_after_secs: lockout_secs,
            });
        }
        Ok(lockout)
    }

    /// ログイン成功時にユーザー名の失敗回数をリセットする。
    /// IP の失敗回数は、1 つの有効なアカウントで総当たりの計数を消せないようにリセットしない
    pub async fn record_success(&self, username: &str) -> Result<()> {
        for (scope, id, _) in self.scopes(username, None) {
            self.cache.delete(&Self::fail_key(scope, &id)).await?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use aegis_cache::MemoryCacheBackend;

    fn limiter(config: LoginRateLimitConfig) -> LoginRateLimiter {
        LoginRateLimiter::new(Arc::new(MemoryCacheBackend::new()), config)
    }

    fn config(per_user: u64, per_ip: u64) -> LoginRateLimitConfig {
        LoginRateLimitConfig {
            enabled: true,
            max_failures_per_user: per_user,
            max_failures_per_ip: per_ip,
            window_secs: 60,
            lockout_secs: 120,
        }
    }

    #[tokio::test]
    async fn test_locks_username_after_max_failures() {
        let limiter = limiter(config(3, 0));
        let ip = Some("203.0.113.7");

        assert_eq!(limiter.record_failure("alice", ip).await.unwrap(), None);
        assert_eq!(limiter.record_failure("alice", ip).await.unwrap(), None);
        assert_eq!(limiter.check("alice", ip).await.unwrap(), None);

        let locked = limiter.record_failure("ALICE", ip).await.unwrap().unwrap();
        assert_eq!(locked.scope, LockScope::Username);
        assert_eq!(locked.retry_after_secs, 120);

        let lockout = limiter.check("Alice", None).await.unwrap().unwrap();
        assert_eq!(lockout.scope, LockScope::Username);
        assert!(lockout.retry_after_secs > 0 && lockout.retry_after_secs <= 120);
        // 他のユーザーには影響しない（IP 単位の制限は無効）
        assert_eq!(limiter.check("bob", ip).await.unwrap(), None);
    }

    #[tokio::test]
    async fn test_locks_ip_across_usernames_and_success_keeps_ip_count() {
        let limiter = limiter(config(0, 3));
        let ip = Some("203.0.113.7");

        limiter.record_failure("alice", ip).await.unwrap();
        limiter.record_failure("bob", ip).await.unwrap();
        limiter.record_success("carol").await.unwrap();
        let locked = limiter.record_failure("dave", ip).await.unwrap().unwrap();
        assert_eq!(locked.scope, LockScope::Ip);

        assert!(limiter.check("erin", ip).await.unwrap().is_some());
        assert_eq!(
            limiter.check("erin", Some("198.51.100.1")).await.unwrap(),
            None
        );
    }

    #[tokio::test]
    async fn test_success_resets_username_failures() {
        let limiter = limiter(config(2, 0));

        limiter.record_failure("alice", None).await.unwrap();
        limiter.record_success("alice").await.unwrap();
        assert_eq!(limiter.record_failure("alice", None).await.unwrap(), None);
        assert!(limiter
            .record_failure("alice", None)
            .await
            .unwrap()
            .is_some());
    }

    #[tokio::test]
    async fn test_disabled_never_locks() {
        let limiter = limiter(LoginRateLimitConfig {
            enabled: false,
            ..config(1, 1)
        });
        for _ in 0..5 {
            assert_eq!(
                limiter
                    .record_failure("alice", Some("203.0.113.7"))
                    .await
                    .unwrap(),
                None
            );
        }
        assert_eq!(
            limiter.check("alice", Some("203.0.113.7")).await.unwrap(),
            None
        );
    }
}
