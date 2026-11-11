//! API トークン（`_aegis_api_tokens`。docs/authentication.md 13）。
//! LLM やスクリプトに、ユーザー名とパスワードの代わりに渡す長命の資格情報。平文は保存せず、
//! SHA-256 のハッシュだけを持つ。

use aegis_core::Result;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

/// API トークンのテーブル名
pub const API_TOKEN_TABLE: &str = "_aegis_api_tokens";

/// INSERT / SELECT で使うカラム（順序固定）
pub const API_TOKEN_COLUMNS: [&str; 9] = [
    "id",
    "username",
    "name",
    "token_hash",
    "token_prefix",
    "role",
    "created_at",
    "expires_at",
    "last_used_at",
];

/// API トークン（`_aegis_api_tokens` の 1 行）
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct ApiTokenRecord {
    pub id: String,
    /// 持ち主のユーザー名
    pub username: String,
    /// 用途のメモ（例: `claude-code`）
    pub name: String,
    /// トークンの SHA-256（16 進）。平文は保存しない
    pub token_hash: String,
    /// 一覧で見分けるための、トークンの先頭部分（秘密ではない）
    pub token_prefix: String,
    /// このトークンに許すロールの上限（持ち主のロールがそれより低ければ、持ち主のロール）
    pub role: String,
    pub created_at: String,
    /// 有効期限（RFC 3339）。`None` は無期限
    pub expires_at: Option<String>,
    pub last_used_at: Option<String>,
}

/// API トークンの INSERT 文
pub fn build_api_token_insert(placeholder: impl Fn(usize) -> String) -> String {
    let placeholders: Vec<String> = (1..=API_TOKEN_COLUMNS.len()).map(placeholder).collect();
    format!(
        "INSERT INTO {} ({}) VALUES ({});",
        API_TOKEN_TABLE,
        API_TOKEN_COLUMNS.join(", "),
        placeholders.join(", ")
    )
}

/// sqlx の行を `ApiTokenRecord` に変換する
macro_rules! api_token_row_from_sqlx {
    ($row:expr) => {{
        let row = $row;
        let get = |e: sqlx::Error| aegis_core::AegisError::Database(e.to_string());
        $crate::api_tokens::ApiTokenRecord {
            id: row.try_get("id").map_err(get)?,
            username: row.try_get("username").map_err(get)?,
            name: row.try_get("name").map_err(get)?,
            token_hash: row.try_get("token_hash").map_err(get)?,
            token_prefix: row.try_get("token_prefix").map_err(get)?,
            role: row.try_get("role").map_err(get)?,
            created_at: row.try_get("created_at").map_err(get)?,
            expires_at: row.try_get("expires_at").map_err(get)?,
            last_used_at: row.try_get("last_used_at").map_err(get)?,
        }
    }};
}
pub(crate) use api_token_row_from_sqlx;

/// API トークンの保存（`DatabaseAdapter` のスーパートレイト）
#[async_trait]
pub trait ApiTokenStorage: Send + Sync {
    async fn insert_api_token(&self, token: &ApiTokenRecord) -> Result<()>;
    /// ハッシュからトークンを引く（リクエストの認証で使う）
    async fn get_api_token_by_hash(&self, token_hash: &str) -> Result<Option<ApiTokenRecord>>;
    /// ユーザーのトークン（作成日の新しい順）
    async fn list_api_tokens(&self, username: &str) -> Result<Vec<ApiTokenRecord>>;
    /// 最終使用日時を記録する
    async fn touch_api_token(&self, id: &str, used_at: &str) -> Result<()>;
    /// 1 本を失効させる（行を消す）。持ち主のユーザー名も合わせて確かめ、消せたら `true`
    async fn delete_api_token(&self, username: &str, id: &str) -> Result<bool>;
    /// ユーザーのトークンをすべて消す（ユーザーの削除時）。消した件数
    async fn delete_user_api_tokens(&self, username: &str) -> Result<u64>;
}

/// `ApiTokenStorage` の実装マクロ
macro_rules! impl_api_token_storage {
    ($adapter:ty, $ph:expr, $label:literal) => {
        #[async_trait::async_trait]
        impl $crate::api_tokens::ApiTokenStorage for $adapter {
            async fn insert_api_token(
                &self,
                t: &$crate::api_tokens::ApiTokenRecord,
            ) -> aegis_core::Result<()> {
                let db_err =
                    |e: sqlx::Error| aegis_core::AegisError::Database(format!("{}: {}", $label, e));
                sqlx::query(&$crate::api_tokens::build_api_token_insert($ph))
                    .bind(&t.id)
                    .bind(&t.username)
                    .bind(&t.name)
                    .bind(&t.token_hash)
                    .bind(&t.token_prefix)
                    .bind(&t.role)
                    .bind(&t.created_at)
                    .bind(&t.expires_at)
                    .bind(&t.last_used_at)
                    .execute(&self.pool)
                    .await
                    .map_err(db_err)?;
                Ok(())
            }

            async fn get_api_token_by_hash(
                &self,
                token_hash: &str,
            ) -> aegis_core::Result<Option<$crate::api_tokens::ApiTokenRecord>> {
                use sqlx::Row;
                let ph = $ph;
                let sql = format!(
                    "SELECT {} FROM {} WHERE token_hash = {};",
                    $crate::api_tokens::API_TOKEN_COLUMNS.join(", "),
                    $crate::api_tokens::API_TOKEN_TABLE,
                    ph(1)
                );
                let db_err =
                    |e: sqlx::Error| aegis_core::AegisError::Database(format!("{}: {}", $label, e));
                let row = sqlx::query(&sql)
                    .bind(token_hash)
                    .fetch_optional(&self.pool)
                    .await
                    .map_err(db_err)?;
                match row {
                    Some(r) => Ok(Some($crate::api_tokens::api_token_row_from_sqlx!(&r))),
                    None => Ok(None),
                }
            }

            async fn list_api_tokens(
                &self,
                username: &str,
            ) -> aegis_core::Result<Vec<$crate::api_tokens::ApiTokenRecord>> {
                use sqlx::Row;
                let ph = $ph;
                let sql = format!(
                    "SELECT {} FROM {} WHERE username = {} ORDER BY created_at DESC, id ASC;",
                    $crate::api_tokens::API_TOKEN_COLUMNS.join(", "),
                    $crate::api_tokens::API_TOKEN_TABLE,
                    ph(1)
                );
                let db_err =
                    |e: sqlx::Error| aegis_core::AegisError::Database(format!("{}: {}", $label, e));
                let rows = sqlx::query(&sql)
                    .bind(username)
                    .fetch_all(&self.pool)
                    .await
                    .map_err(db_err)?;
                let mut out = Vec::with_capacity(rows.len());
                for r in &rows {
                    out.push($crate::api_tokens::api_token_row_from_sqlx!(r));
                }
                Ok(out)
            }

            async fn touch_api_token(&self, id: &str, used_at: &str) -> aegis_core::Result<()> {
                let ph = $ph;
                let sql = format!(
                    "UPDATE {} SET last_used_at = {} WHERE id = {};",
                    $crate::api_tokens::API_TOKEN_TABLE,
                    ph(1),
                    ph(2)
                );
                let db_err =
                    |e: sqlx::Error| aegis_core::AegisError::Database(format!("{}: {}", $label, e));
                sqlx::query(&sql)
                    .bind(used_at)
                    .bind(id)
                    .execute(&self.pool)
                    .await
                    .map_err(db_err)?;
                Ok(())
            }

            async fn delete_api_token(&self, username: &str, id: &str) -> aegis_core::Result<bool> {
                let ph = $ph;
                let sql = format!(
                    "DELETE FROM {} WHERE id = {} AND username = {};",
                    $crate::api_tokens::API_TOKEN_TABLE,
                    ph(1),
                    ph(2)
                );
                let db_err =
                    |e: sqlx::Error| aegis_core::AegisError::Database(format!("{}: {}", $label, e));
                let res = sqlx::query(&sql)
                    .bind(id)
                    .bind(username)
                    .execute(&self.pool)
                    .await
                    .map_err(db_err)?;
                Ok(res.rows_affected() > 0)
            }

            async fn delete_user_api_tokens(&self, username: &str) -> aegis_core::Result<u64> {
                let ph = $ph;
                let sql = format!(
                    "DELETE FROM {} WHERE username = {};",
                    $crate::api_tokens::API_TOKEN_TABLE,
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
        }
    };
}
pub(crate) use impl_api_token_storage;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{DatabaseAdapter, SqliteAdapter};

    fn token(id: &str, user: &str) -> ApiTokenRecord {
        ApiTokenRecord {
            id: id.to_string(),
            username: user.to_string(),
            name: format!("name-{id}"),
            token_hash: format!("hash-{id}"),
            token_prefix: "aegis_pat_ab".to_string(),
            role: "editor".to_string(),
            created_at: format!("2026-11-12T00:00:0{}Z", id.len()),
            expires_at: Some("2027-02-10T00:00:00Z".to_string()),
            last_used_at: None,
        }
    }

    #[tokio::test]
    async fn test_store_lookup_touch_and_delete() {
        let db = SqliteAdapter::connect("sqlite::memory:", 1).await.unwrap();
        db.init().await.unwrap();
        db.insert_api_token(&token("a", "alice")).await.unwrap();
        db.insert_api_token(&token("bb", "alice")).await.unwrap();
        db.insert_api_token(&token("ccc", "bob")).await.unwrap();
        // ハッシュは一意
        let mut twin = token("dddd", "alice");
        twin.token_hash = "hash-a".to_string();
        assert!(db.insert_api_token(&twin).await.is_err());

        let got = db.get_api_token_by_hash("hash-a").await.unwrap().unwrap();
        assert_eq!(got.username, "alice");
        assert_eq!(got.last_used_at, None);
        assert!(db.get_api_token_by_hash("nope").await.unwrap().is_none());

        db.touch_api_token("a", "2026-11-12T01:00:00Z")
            .await
            .unwrap();
        let got = db.get_api_token_by_hash("hash-a").await.unwrap().unwrap();
        assert_eq!(got.last_used_at.as_deref(), Some("2026-11-12T01:00:00Z"));

        let mine = db.list_api_tokens("alice").await.unwrap();
        assert_eq!(
            mine.iter().map(|t| t.id.as_str()).collect::<Vec<_>>(),
            vec!["bb", "a"]
        );

        // 他人のトークンは消せない
        assert!(!db.delete_api_token("bob", "a").await.unwrap());
        assert!(db.delete_api_token("alice", "a").await.unwrap());
        assert!(!db.delete_api_token("alice", "a").await.unwrap());

        assert_eq!(db.delete_user_api_tokens("alice").await.unwrap(), 1);
        assert!(db.list_api_tokens("alice").await.unwrap().is_empty());
        assert_eq!(db.list_api_tokens("bob").await.unwrap().len(), 1);
    }
}
