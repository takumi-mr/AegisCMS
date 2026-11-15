//! OAuth のリフレッシュトークン（`_aegis_oauth_refresh_tokens`。docs/mcp.md）。
//! MCP クライアントが、30 日の API トークンの代わりに短いトークンを取り直すための資格情報。
//! 平文は保存せず、SHA-256 のハッシュだけを持つ。使うたびに作り直す（1 回しか使えない）。

use aegis_core::Result;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

/// リフレッシュトークンのテーブル名
pub const OAUTH_REFRESH_TABLE: &str = "_aegis_oauth_refresh_tokens";

/// INSERT / SELECT で使うカラム（順序固定）
pub const OAUTH_REFRESH_COLUMNS: [&str; 10] = [
    "id",
    "token_hash",
    "username",
    "client_hash",
    "client_name",
    "scope",
    "role",
    "access_token_id",
    "created_at",
    "expires_at",
];

/// リフレッシュトークン（`_aegis_oauth_refresh_tokens` の 1 行）
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct OAuthRefreshRecord {
    pub id: String,
    /// トークンの SHA-256（16 進）。平文は保存しない
    pub token_hash: String,
    /// 持ち主のユーザー名
    pub username: String,
    /// 登録したクライアント（`client_id`）の SHA-256。別のクライアントが使えないようにする
    pub client_hash: String,
    /// 同意画面に出したアプリの名前
    pub client_name: String,
    /// 許可した範囲（空白区切り）
    pub scope: String,
    /// 許可したロールの上限
    pub role: String,
    /// いま有効なアクセストークン（API トークン）の ID。プロフィールで失効されたら、この許可も終わる
    pub access_token_id: String,
    pub created_at: String,
    /// 有効期限（RFC 3339）
    pub expires_at: String,
}

/// リフレッシュトークンの INSERT 文
pub fn build_oauth_refresh_insert(placeholder: impl Fn(usize) -> String) -> String {
    let placeholders: Vec<String> = (1..=OAUTH_REFRESH_COLUMNS.len()).map(placeholder).collect();
    format!(
        "INSERT INTO {} ({}) VALUES ({});",
        OAUTH_REFRESH_TABLE,
        OAUTH_REFRESH_COLUMNS.join(", "),
        placeholders.join(", ")
    )
}

/// sqlx の行を `OAuthRefreshRecord` に変換する
macro_rules! oauth_refresh_row_from_sqlx {
    ($row:expr) => {{
        let row = $row;
        let get = |e: sqlx::Error| aegis_core::AegisError::Database(e.to_string());
        $crate::oauth_refresh::OAuthRefreshRecord {
            id: row.try_get("id").map_err(get)?,
            token_hash: row.try_get("token_hash").map_err(get)?,
            username: row.try_get("username").map_err(get)?,
            client_hash: row.try_get("client_hash").map_err(get)?,
            client_name: row.try_get("client_name").map_err(get)?,
            scope: row.try_get("scope").map_err(get)?,
            role: row.try_get("role").map_err(get)?,
            access_token_id: row.try_get("access_token_id").map_err(get)?,
            created_at: row.try_get("created_at").map_err(get)?,
            expires_at: row.try_get("expires_at").map_err(get)?,
        }
    }};
}
pub(crate) use oauth_refresh_row_from_sqlx;

/// リフレッシュトークンの保存（`DatabaseAdapter` のスーパートレイト）
#[async_trait]
pub trait OAuthRefreshStorage: Send + Sync {
    async fn insert_oauth_refresh(&self, token: &OAuthRefreshRecord) -> Result<()>;
    /// ハッシュで引いて、その場で消す。消せた（=自分が最初に使った）ときだけ `Some`。
    /// 同じトークンを 2 回使うと、2 回目は `None`
    async fn take_oauth_refresh_by_hash(
        &self,
        token_hash: &str,
    ) -> Result<Option<OAuthRefreshRecord>>;
    /// そのアクセストークンに結び付いたリフレッシュトークンを消す（プロフィールでトークンを失効したとき）。消した件数
    async fn delete_oauth_refresh_by_access_token(
        &self,
        username: &str,
        access_token_id: &str,
    ) -> Result<u64>;
    /// ユーザーのリフレッシュトークンをすべて消す（ユーザーの削除時）。消した件数
    async fn delete_user_oauth_refresh(&self, username: &str) -> Result<u64>;
    /// 期限が `now`（RFC 3339）より前のものを消す。消した件数
    async fn delete_expired_oauth_refresh(&self, now: &str) -> Result<u64>;
}

/// `OAuthRefreshStorage` の実装マクロ
macro_rules! impl_oauth_refresh_storage {
    ($adapter:ty, $ph:expr, $label:literal) => {
        #[async_trait::async_trait]
        impl $crate::oauth_refresh::OAuthRefreshStorage for $adapter {
            async fn insert_oauth_refresh(
                &self,
                t: &$crate::oauth_refresh::OAuthRefreshRecord,
            ) -> aegis_core::Result<()> {
                let db_err =
                    |e: sqlx::Error| aegis_core::AegisError::Database(format!("{}: {}", $label, e));
                sqlx::query(&$crate::oauth_refresh::build_oauth_refresh_insert($ph))
                    .bind(&t.id)
                    .bind(&t.token_hash)
                    .bind(&t.username)
                    .bind(&t.client_hash)
                    .bind(&t.client_name)
                    .bind(&t.scope)
                    .bind(&t.role)
                    .bind(&t.access_token_id)
                    .bind(&t.created_at)
                    .bind(&t.expires_at)
                    .execute(&self.pool)
                    .await
                    .map_err(db_err)?;
                Ok(())
            }

            async fn take_oauth_refresh_by_hash(
                &self,
                token_hash: &str,
            ) -> aegis_core::Result<Option<$crate::oauth_refresh::OAuthRefreshRecord>> {
                use sqlx::Row;
                let ph = $ph;
                let db_err =
                    |e: sqlx::Error| aegis_core::AegisError::Database(format!("{}: {}", $label, e));
                let select = format!(
                    "SELECT {} FROM {} WHERE token_hash = {};",
                    $crate::oauth_refresh::OAUTH_REFRESH_COLUMNS.join(", "),
                    $crate::oauth_refresh::OAUTH_REFRESH_TABLE,
                    ph(1)
                );
                let row = sqlx::query(&select)
                    .bind(token_hash)
                    .fetch_optional(&self.pool)
                    .await
                    .map_err(db_err)?;
                let Some(row) = row else {
                    return Ok(None);
                };
                let record = $crate::oauth_refresh::oauth_refresh_row_from_sqlx!(&row);
                // 同時に使われても、消せた 1 つだけが先へ進む
                let delete = format!(
                    "DELETE FROM {} WHERE id = {};",
                    $crate::oauth_refresh::OAUTH_REFRESH_TABLE,
                    ph(1)
                );
                let res = sqlx::query(&delete)
                    .bind(&record.id)
                    .execute(&self.pool)
                    .await
                    .map_err(db_err)?;
                Ok((res.rows_affected() > 0).then_some(record))
            }

            async fn delete_oauth_refresh_by_access_token(
                &self,
                username: &str,
                access_token_id: &str,
            ) -> aegis_core::Result<u64> {
                let ph = $ph;
                let sql = format!(
                    "DELETE FROM {} WHERE username = {} AND access_token_id = {};",
                    $crate::oauth_refresh::OAUTH_REFRESH_TABLE,
                    ph(1),
                    ph(2)
                );
                let db_err =
                    |e: sqlx::Error| aegis_core::AegisError::Database(format!("{}: {}", $label, e));
                let res = sqlx::query(&sql)
                    .bind(username)
                    .bind(access_token_id)
                    .execute(&self.pool)
                    .await
                    .map_err(db_err)?;
                Ok(res.rows_affected())
            }

            async fn delete_user_oauth_refresh(&self, username: &str) -> aegis_core::Result<u64> {
                let ph = $ph;
                let sql = format!(
                    "DELETE FROM {} WHERE username = {};",
                    $crate::oauth_refresh::OAUTH_REFRESH_TABLE,
                    ph(1)
                );
                let db_err =
                    |e: sqlx::Error| aegis_core::AegisError::Database(format!("{}: {}", $label, e));
                let res = sqlx::query(&sql)
                    .bind(username)
                    .execute(&self.pool)
                    .await
                    .map_err(db_err)?;
                Ok(res.rows_affected())
            }

            async fn delete_expired_oauth_refresh(&self, now: &str) -> aegis_core::Result<u64> {
                let ph = $ph;
                let sql = format!(
                    "DELETE FROM {} WHERE expires_at < {};",
                    $crate::oauth_refresh::OAUTH_REFRESH_TABLE,
                    ph(1)
                );
                let db_err =
                    |e: sqlx::Error| aegis_core::AegisError::Database(format!("{}: {}", $label, e));
                let res = sqlx::query(&sql)
                    .bind(now)
                    .execute(&self.pool)
                    .await
                    .map_err(db_err)?;
                Ok(res.rows_affected())
            }
        }
    };
}
pub(crate) use impl_oauth_refresh_storage;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{DatabaseAdapter, SqliteAdapter};

    fn token(id: &str, user: &str, expires_at: &str) -> OAuthRefreshRecord {
        OAuthRefreshRecord {
            id: id.to_string(),
            token_hash: format!("hash-{id}"),
            username: user.to_string(),
            client_hash: "client".to_string(),
            client_name: "Agent".to_string(),
            scope: "content:read".to_string(),
            role: "viewer".to_string(),
            access_token_id: format!("access-{id}"),
            created_at: "2026-11-15T00:00:00Z".to_string(),
            expires_at: expires_at.to_string(),
        }
    }

    #[tokio::test]
    async fn test_a_refresh_token_can_be_taken_only_once() {
        let db = SqliteAdapter::connect("sqlite::memory:", 1).await.unwrap();
        db.init().await.unwrap();
        db.insert_oauth_refresh(&token("a", "alice", "2027-01-01T00:00:00Z"))
            .await
            .unwrap();
        // ハッシュは一意
        assert!(db
            .insert_oauth_refresh(&token("a2", "alice", "2027-01-01T00:00:00Z"))
            .await
            .is_ok());
        let mut twin = token("b", "alice", "2027-01-01T00:00:00Z");
        twin.token_hash = "hash-a".to_string();
        assert!(db.insert_oauth_refresh(&twin).await.is_err());

        let taken = db.take_oauth_refresh_by_hash("hash-a").await.unwrap();
        assert_eq!(taken.unwrap().access_token_id, "access-a");
        assert!(db
            .take_oauth_refresh_by_hash("hash-a")
            .await
            .unwrap()
            .is_none());
        assert!(db
            .take_oauth_refresh_by_hash("unknown")
            .await
            .unwrap()
            .is_none());
    }

    #[tokio::test]
    async fn test_delete_for_a_user_and_expired() {
        let db = SqliteAdapter::connect("sqlite::memory:", 1).await.unwrap();
        db.init().await.unwrap();
        db.insert_oauth_refresh(&token("old", "alice", "2026-01-01T00:00:00Z"))
            .await
            .unwrap();
        db.insert_oauth_refresh(&token("new", "alice", "2027-01-01T00:00:00Z"))
            .await
            .unwrap();
        db.insert_oauth_refresh(&token("bob", "bob", "2027-01-01T00:00:00Z"))
            .await
            .unwrap();
        assert_eq!(
            db.delete_expired_oauth_refresh("2026-11-15T00:00:00Z")
                .await
                .unwrap(),
            1
        );
        assert_eq!(
            db.delete_oauth_refresh_by_access_token("alice", "access-new")
                .await
                .unwrap(),
            1
        );
        // 別のユーザーのトークンは消せない
        assert_eq!(
            db.delete_oauth_refresh_by_access_token("alice", "access-bob")
                .await
                .unwrap(),
            0
        );
        assert_eq!(db.delete_user_oauth_refresh("alice").await.unwrap(), 0);
        assert!(db
            .take_oauth_refresh_by_hash("hash-bob")
            .await
            .unwrap()
            .is_some());
    }
}
