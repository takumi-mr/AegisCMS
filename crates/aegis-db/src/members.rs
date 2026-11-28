//! 会員テーブル（`_aegis_members`）とストレージ。
//! AegisCMS の管理ユーザー（`_aegis_users`）とは完全に別管理の、EC 会員アカウント。

use aegis_core::Result;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

/// 会員テーブル名
pub const MEMBER_TABLE: &str = "_aegis_members";

/// INSERT / SELECT で使う会員テーブルのカラム（順序固定）
pub const MEMBER_COLUMNS: [&str; 13] = [
    "id",
    "email",
    "password_hash",
    "name",
    "nickname",
    "avatar_key",
    "phone",
    "status",
    "level",
    "shipping_address_json",
    "created_at",
    "updated_at",
    "last_login_at",
];

/// 権限レベルの既定値（古いバックアップの会員、レベルを決めていない会員）
pub fn default_level() -> String {
    "free".to_string()
}

/// 会員（`_aegis_members` の 1 行）
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct MemberRecord {
    pub id: String,
    /// メールアドレス（小文字正規化済み、一意）
    pub email: String,
    pub password_hash: String,
    pub name: String,
    /// 公開名（掲示板・レビューに出す名前）。`name` は配送先にも使う本名なので、別に持つ。
    /// 未設定なら `None`（古いバックアップにも無い）
    #[serde(default)]
    pub nickname: Option<String>,
    /// アバター画像のストレージキー（`avatars/...`）。未設定なら `None`（古いバックアップにも無い）
    #[serde(default)]
    pub avatar_key: Option<String>,
    pub phone: Option<String>,
    /// `active` / `suspended` / `unverified`（メールの確認待ち）/ `deleting`（退会の手続き中）
    pub status: String,
    /// 権限レベル（`[members].levels` のどれか。有料メンバー限定の閲覧制御。docs/members.md 15）。
    /// 古いバックアップには無いので `free`
    #[serde(default = "default_level")]
    pub level: String,
    /// 既定の配送先住所 (`ShippingAddress` の JSON)
    pub shipping_address_json: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub last_login_at: Option<String>,
}

/// `list_member_avatar_keys` が 1 回の SELECT で引く ID の数（バインド変数の上限に収める）
pub const AVATAR_KEYS_CHUNK: usize = 500;

/// 会員の INSERT 文
pub fn build_member_insert(placeholder: impl Fn(usize) -> String) -> String {
    let placeholders: Vec<String> = (1..=MEMBER_COLUMNS.len()).map(placeholder).collect();
    format!(
        "INSERT INTO {} ({}) VALUES ({});",
        MEMBER_TABLE,
        MEMBER_COLUMNS.join(", "),
        placeholders.join(", ")
    )
}

/// sqlx の行を `MemberRecord` に変換する
macro_rules! member_row_from_sqlx {
    ($row:expr) => {{
        let row = $row;
        let get = |e: sqlx::Error| aegis_core::AegisError::Database(e.to_string());
        $crate::members::MemberRecord {
            id: row.try_get("id").map_err(get)?,
            email: row.try_get("email").map_err(get)?,
            password_hash: row.try_get("password_hash").map_err(get)?,
            name: row.try_get("name").map_err(get)?,
            nickname: row.try_get("nickname").map_err(get)?,
            avatar_key: row.try_get("avatar_key").map_err(get)?,
            phone: row.try_get("phone").map_err(get)?,
            status: row.try_get("status").map_err(get)?,
            level: row.try_get("level").map_err(get)?,
            shipping_address_json: row.try_get("shipping_address_json").map_err(get)?,
            created_at: row.try_get("created_at").map_err(get)?,
            updated_at: row.try_get("updated_at").map_err(get)?,
            last_login_at: row.try_get("last_login_at").map_err(get)?,
        }
    }};
}
pub(crate) use member_row_from_sqlx;

/// 会員が自分で変えるプロフィールの列（[`MemberStorage::update_member_profile`]）
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct MemberProfileUpdate {
    pub name: String,
    pub nickname: Option<String>,
    pub phone: Option<String>,
    pub shipping_address_json: Option<String>,
    pub updated_at: String,
}

/// 会員の保存（`DatabaseAdapter` のスーパートレイト）。
///
/// 行の全部の列を書き戻す更新は持たない。読んでから書くまでの間に、ほかの操作（管理者の停止・レベルの変更・
/// パスワードの変更）が変えた列を、古い値で上書きしてしまうため（Lost Update）。更新は、変える列だけを書き、
/// 前提の状態（`status` など）を `WHERE` に入れる。条件つきの更新は、変えたかどうかを返す
#[async_trait]
pub trait MemberStorage: Send + Sync {
    async fn insert_member(&self, member: &MemberRecord) -> Result<()>;
    async fn get_member_by_id(&self, id: &str) -> Result<Option<MemberRecord>>;
    async fn get_member_by_email(&self, email: &str) -> Result<Option<MemberRecord>>;
    async fn update_member_last_login(&self, id: &str, last_login_at: &str) -> Result<()>;
    async fn list_members(&self, limit: u64, offset: u64) -> Result<(Vec<MemberRecord>, u64)>;
    /// 状態を変える。`from` を渡すと、今の状態がそのどれかのときだけ変える。変えたら `true`
    async fn set_member_status(
        &self,
        id: &str,
        status: &str,
        updated_at: &str,
        from: Option<&[&str]>,
    ) -> Result<bool>;
    /// 確認待ち（`unverified`）の会員を `active` にして、確認した登録の内容（パスワード・名前・電話）を
    /// 反映する。確認待ちでなければ何もしない。反映したら `true`
    async fn activate_member(
        &self,
        id: &str,
        password_hash: &str,
        name: &str,
        phone: Option<&str>,
        updated_at: &str,
    ) -> Result<bool>;
    /// 会員が自分で変えるプロフィール（名前・公開名・電話・配送先）。`active` の会員のときだけ変える。
    /// 変えたら `true`
    async fn update_member_profile(&self, id: &str, profile: &MemberProfileUpdate) -> Result<bool>;
    /// パスワードを変える。`active` の会員で、今のハッシュが `current_hash` のときだけ（確かめたパスワードの
    /// あとに、ほかの変更や停止が挟まっていたら変えない）。変えたら `true`
    async fn update_member_password(
        &self,
        id: &str,
        password_hash: &str,
        current_hash: &str,
        updated_at: &str,
    ) -> Result<bool>;
    /// パスワードの再設定（メールのリンク）。`active` か `unverified` の会員のときだけ変え、`unverified` は
    /// `active` にする（リンクを開けたので、アドレスの確認にもなる）。変えたら `true`
    async fn reset_member_password(
        &self,
        id: &str,
        password_hash: &str,
        updated_at: &str,
    ) -> Result<bool>;
    /// アバター画像のキーを変える（`None` で外す）。`current` を渡すと、今のキーがそれのとき・会員が
    /// `active` のときだけ変える（会員自身の差し替え。`Some(None)` は「今は無い」）。変えたら `true`
    async fn update_member_avatar(
        &self,
        id: &str,
        avatar_key: Option<&str>,
        updated_at: &str,
        current: Option<Option<&str>>,
    ) -> Result<bool>;
    /// 権限レベルを変える（管理者）。変えたら `true`
    async fn set_member_level(&self, id: &str, level: &str, updated_at: &str) -> Result<bool>;
    async fn delete_member(&self, id: &str) -> Result<bool>;
    /// 会員 ID の一覧から、アバター画像のある会員の（ID, `avatar_key`）をまとめて引く。掲示板・レビューの表示用
    async fn list_member_avatar_keys(&self, ids: &[String]) -> Result<Vec<(String, String)>>;
}

/// `MemberStorage` の実装マクロ
macro_rules! impl_member_storage {
    ($adapter:ty, $ph:expr, $label:literal) => {
        #[async_trait::async_trait]
        impl $crate::members::MemberStorage for $adapter {
            async fn insert_member(
                &self,
                c: &$crate::members::MemberRecord,
            ) -> aegis_core::Result<()> {
                let db_err =
                    |e: sqlx::Error| aegis_core::AegisError::Database(format!("{}: {}", $label, e));
                sqlx::query(&$crate::members::build_member_insert($ph))
                    .bind(&c.id)
                    .bind(&c.email)
                    .bind(&c.password_hash)
                    .bind(&c.name)
                    .bind(&c.nickname)
                    .bind(&c.avatar_key)
                    .bind(&c.phone)
                    .bind(&c.status)
                    .bind(&c.level)
                    .bind(&c.shipping_address_json)
                    .bind(&c.created_at)
                    .bind(&c.updated_at)
                    .bind(&c.last_login_at)
                    .execute(&self.pool)
                    .await
                    .map_err(db_err)?;
                Ok(())
            }

            async fn get_member_by_id(
                &self,
                id: &str,
            ) -> aegis_core::Result<Option<$crate::members::MemberRecord>> {
                let ph = $ph;
                let sql = format!(
                    "SELECT {} FROM {} WHERE id = {};",
                    $crate::members::MEMBER_COLUMNS.join(", "),
                    $crate::members::MEMBER_TABLE,
                    ph(1)
                );
                let db_err =
                    |e: sqlx::Error| aegis_core::AegisError::Database(format!("{}: {}", $label, e));
                let row = sqlx::query(&sql)
                    .bind(id)
                    .fetch_optional(&self.pool)
                    .await
                    .map_err(db_err)?;
                match row {
                    Some(r) => Ok(Some($crate::members::member_row_from_sqlx!(&r))),
                    None => Ok(None),
                }
            }

            async fn get_member_by_email(
                &self,
                email: &str,
            ) -> aegis_core::Result<Option<$crate::members::MemberRecord>> {
                let ph = $ph;
                let sql = format!(
                    "SELECT {} FROM {} WHERE email = {};",
                    $crate::members::MEMBER_COLUMNS.join(", "),
                    $crate::members::MEMBER_TABLE,
                    ph(1)
                );
                let db_err =
                    |e: sqlx::Error| aegis_core::AegisError::Database(format!("{}: {}", $label, e));
                let row = sqlx::query(&sql)
                    .bind(email)
                    .fetch_optional(&self.pool)
                    .await
                    .map_err(db_err)?;
                match row {
                    Some(r) => Ok(Some($crate::members::member_row_from_sqlx!(&r))),
                    None => Ok(None),
                }
            }

            async fn update_member_last_login(
                &self,
                id: &str,
                last_login_at: &str,
            ) -> aegis_core::Result<()> {
                let ph = $ph;
                let sql = format!(
                    "UPDATE {} SET last_login_at = {} WHERE id = {};",
                    $crate::members::MEMBER_TABLE,
                    ph(1), ph(2)
                );
                let db_err =
                    |e: sqlx::Error| aegis_core::AegisError::Database(format!("{}: {}", $label, e));
                sqlx::query(&sql)
                    .bind(last_login_at)
                    .bind(id)
                    .execute(&self.pool)
                    .await
                    .map_err(db_err)?;
                Ok(())
            }

            async fn list_members(
                &self,
                limit: u64,
                offset: u64,
            ) -> aegis_core::Result<(Vec<$crate::members::MemberRecord>, u64)> {
                let ph = $ph;
                let count_sql = format!("SELECT COUNT(*) FROM {};", $crate::members::MEMBER_TABLE);
                let select_sql = format!(
                    "SELECT {} FROM {} ORDER BY created_at DESC LIMIT {} OFFSET {};",
                    $crate::members::MEMBER_COLUMNS.join(", "),
                    $crate::members::MEMBER_TABLE,
                    ph(1), ph(2)
                );
                let db_err =
                    |e: sqlx::Error| aegis_core::AegisError::Database(format!("{}: {}", $label, e));

                let total_i64: i64 = sqlx::query_scalar(&count_sql)
                    .fetch_one(&self.pool)
                    .await
                    .map_err(db_err)?;
                let total = total_i64.max(0) as u64;

                let rows = sqlx::query(&select_sql)
                    .bind(limit as i64)
                    .bind(offset as i64)
                    .fetch_all(&self.pool)
                    .await
                    .map_err(db_err)?;

                let mut records = Vec::with_capacity(rows.len());
                for r in &rows {
                    records.push($crate::members::member_row_from_sqlx!(r));
                }

                Ok((records, total))
            }

            async fn list_member_avatar_keys(
                &self,
                ids: &[String],
            ) -> aegis_core::Result<Vec<(String, String)>> {
                use sqlx::Row;
                let ph = $ph;
                let db_err =
                    |e: sqlx::Error| aegis_core::AegisError::Database(format!("{}: {}", $label, e));
                let mut found = Vec::new();
                // バインド変数の上限に収まるよう、まとめて引く数を区切る
                for chunk in ids.chunks($crate::members::AVATAR_KEYS_CHUNK) {
                    let placeholders: Vec<String> = (1..=chunk.len()).map(&ph).collect();
                    let sql = format!(
                        "SELECT id, avatar_key FROM {} WHERE avatar_key IS NOT NULL AND id IN ({});",
                        $crate::members::MEMBER_TABLE,
                        placeholders.join(", ")
                    );
                    let mut query = sqlx::query(&sql);
                    for id in chunk {
                        query = query.bind(id);
                    }
                    let rows = query.fetch_all(&self.pool).await.map_err(db_err)?;
                    for row in &rows {
                        found.push((
                            row.try_get("id").map_err(db_err)?,
                            row.try_get("avatar_key").map_err(db_err)?,
                        ));
                    }
                }
                Ok(found)
            }

            async fn set_member_status(
                &self,
                id: &str,
                status: &str,
                updated_at: &str,
                from: Option<&[&str]>,
            ) -> aegis_core::Result<bool> {
                let ph = $ph;
                let mut sql = format!(
                    "UPDATE {} SET status = {}, updated_at = {} WHERE id = {}",
                    $crate::members::MEMBER_TABLE,
                    ph(1), ph(2), ph(3)
                );
                if let Some(from) = from {
                    if from.is_empty() {
                        return Ok(false);
                    }
                    let list: Vec<String> = (0..from.len()).map(|i| ph(4 + i)).collect();
                    sql.push_str(&format!(" AND status IN ({})", list.join(", ")));
                }
                let db_err =
                    |e: sqlx::Error| aegis_core::AegisError::Database(format!("{}: {}", $label, e));
                let mut query = sqlx::query(&sql).bind(status).bind(updated_at).bind(id);
                for s in from.unwrap_or(&[]) {
                    query = query.bind(*s);
                }
                let res = query.execute(&self.pool).await.map_err(db_err)?;
                Ok(res.rows_affected() > 0)
            }

            async fn activate_member(
                &self,
                id: &str,
                password_hash: &str,
                name: &str,
                phone: Option<&str>,
                updated_at: &str,
            ) -> aegis_core::Result<bool> {
                let ph = $ph;
                let sql = format!(
                    "UPDATE {} SET status = 'active', password_hash = {}, name = {}, phone = {}, updated_at = {} \
                     WHERE id = {} AND status = 'unverified'",
                    $crate::members::MEMBER_TABLE,
                    ph(1), ph(2), ph(3), ph(4), ph(5)
                );
                let db_err =
                    |e: sqlx::Error| aegis_core::AegisError::Database(format!("{}: {}", $label, e));
                let res = sqlx::query(&sql)
                    .bind(password_hash)
                    .bind(name)
                    .bind(phone)
                    .bind(updated_at)
                    .bind(id)
                    .execute(&self.pool)
                    .await
                    .map_err(db_err)?;
                Ok(res.rows_affected() > 0)
            }

            async fn update_member_profile(
                &self,
                id: &str,
                p: &$crate::members::MemberProfileUpdate,
            ) -> aegis_core::Result<bool> {
                let ph = $ph;
                let sql = format!(
                    "UPDATE {} SET name = {}, nickname = {}, phone = {}, shipping_address_json = {}, updated_at = {} \
                     WHERE id = {} AND status = 'active'",
                    $crate::members::MEMBER_TABLE,
                    ph(1), ph(2), ph(3), ph(4), ph(5), ph(6)
                );
                let db_err =
                    |e: sqlx::Error| aegis_core::AegisError::Database(format!("{}: {}", $label, e));
                let res = sqlx::query(&sql)
                    .bind(&p.name)
                    .bind(&p.nickname)
                    .bind(&p.phone)
                    .bind(&p.shipping_address_json)
                    .bind(&p.updated_at)
                    .bind(id)
                    .execute(&self.pool)
                    .await
                    .map_err(db_err)?;
                Ok(res.rows_affected() > 0)
            }

            async fn update_member_password(
                &self,
                id: &str,
                password_hash: &str,
                current_hash: &str,
                updated_at: &str,
            ) -> aegis_core::Result<bool> {
                let ph = $ph;
                let sql = format!(
                    "UPDATE {} SET password_hash = {}, updated_at = {} \
                     WHERE id = {} AND status = 'active' AND password_hash = {}",
                    $crate::members::MEMBER_TABLE,
                    ph(1), ph(2), ph(3), ph(4)
                );
                let db_err =
                    |e: sqlx::Error| aegis_core::AegisError::Database(format!("{}: {}", $label, e));
                let res = sqlx::query(&sql)
                    .bind(password_hash)
                    .bind(updated_at)
                    .bind(id)
                    .bind(current_hash)
                    .execute(&self.pool)
                    .await
                    .map_err(db_err)?;
                Ok(res.rows_affected() > 0)
            }

            async fn reset_member_password(
                &self,
                id: &str,
                password_hash: &str,
                updated_at: &str,
            ) -> aegis_core::Result<bool> {
                let ph = $ph;
                let sql = format!(
                    "UPDATE {} SET password_hash = {}, status = 'active', updated_at = {} \
                     WHERE id = {} AND status IN ('active', 'unverified')",
                    $crate::members::MEMBER_TABLE,
                    ph(1), ph(2), ph(3)
                );
                let db_err =
                    |e: sqlx::Error| aegis_core::AegisError::Database(format!("{}: {}", $label, e));
                let res = sqlx::query(&sql)
                    .bind(password_hash)
                    .bind(updated_at)
                    .bind(id)
                    .execute(&self.pool)
                    .await
                    .map_err(db_err)?;
                Ok(res.rows_affected() > 0)
            }

            async fn update_member_avatar(
                &self,
                id: &str,
                avatar_key: Option<&str>,
                updated_at: &str,
                current: Option<Option<&str>>,
            ) -> aegis_core::Result<bool> {
                let ph = $ph;
                let mut sql = format!(
                    "UPDATE {} SET avatar_key = {}, updated_at = {} WHERE id = {}",
                    $crate::members::MEMBER_TABLE,
                    ph(1), ph(2), ph(3)
                );
                match current {
                    None => {}
                    Some(None) => sql.push_str(" AND status = 'active' AND avatar_key IS NULL"),
                    Some(Some(_)) => {
                        sql.push_str(&format!(" AND status = 'active' AND avatar_key = {}", ph(4)))
                    }
                }
                let db_err =
                    |e: sqlx::Error| aegis_core::AegisError::Database(format!("{}: {}", $label, e));
                let mut query = sqlx::query(&sql).bind(avatar_key).bind(updated_at).bind(id);
                if let Some(Some(key)) = current {
                    query = query.bind(key);
                }
                let res = query.execute(&self.pool).await.map_err(db_err)?;
                Ok(res.rows_affected() > 0)
            }

            async fn set_member_level(
                &self,
                id: &str,
                level: &str,
                updated_at: &str,
            ) -> aegis_core::Result<bool> {
                let ph = $ph;
                let sql = format!(
                    "UPDATE {} SET level = {}, updated_at = {} WHERE id = {}",
                    $crate::members::MEMBER_TABLE,
                    ph(1), ph(2), ph(3)
                );
                let db_err =
                    |e: sqlx::Error| aegis_core::AegisError::Database(format!("{}: {}", $label, e));
                let res = sqlx::query(&sql)
                    .bind(level)
                    .bind(updated_at)
                    .bind(id)
                    .execute(&self.pool)
                    .await
                    .map_err(db_err)?;
                Ok(res.rows_affected() > 0)
            }

            async fn delete_member(&self, id: &str) -> aegis_core::Result<bool> {
                let ph = $ph;
                let sql = format!(
                    "DELETE FROM {} WHERE id = {};",
                    $crate::members::MEMBER_TABLE,
                    ph(1)
                );
                let db_err =
                    |e: sqlx::Error| aegis_core::AegisError::Database(format!("{}: {}", $label, e));
                let res = sqlx::query(&sql)
                    .bind(id)
                    .execute(&self.pool)
                    .await
                    .map_err(db_err)?;
                Ok(res.rows_affected() > 0)
            }
        }
    };
}
pub(crate) use impl_member_storage;
