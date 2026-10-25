//! 掲示板テーブル（`_aegis_forum_categories`, `_aegis_forum_threads`, `_aegis_forum_posts`）とストレージ。
//! SQLite / PostgreSQL / MySQL の 3 つのダイアレクトで SQL を共有し、
//! キーセット（カーソル）ページネーションで高速にスレッド・投稿を取得する。

use aegis_core::Result;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

/// カテゴリのテーブル名
pub const FORUM_CATEGORY_TABLE: &str = "_aegis_forum_categories";
/// スレッドのテーブル名
pub const FORUM_THREAD_TABLE: &str = "_aegis_forum_threads";
/// 投稿（レス）のテーブル名
pub const FORUM_POST_TABLE: &str = "_aegis_forum_posts";

pub const CATEGORY_COLUMNS: [&str; 8] = [
    "id",
    "slug",
    "name",
    "description",
    "position",
    "is_private",
    "created_at",
    "updated_at",
];

pub const THREAD_COLUMNS: [&str; 11] = [
    "id",
    "category_id",
    "title",
    "author_id",
    "author_name",
    "is_pinned",
    "is_locked",
    "post_count",
    "last_post_at",
    "created_at",
    "updated_at",
];

pub const POST_COLUMNS: [&str; 13] = [
    "id",
    "thread_id",
    "post_number",
    "author_id",
    "author_name",
    "body",
    "reply_to_post_id",
    "reply_to_number",
    "is_deleted",
    "ip",
    "user_agent",
    "created_at",
    "updated_at",
];

/// カテゴリ（板）のレコード
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct ForumCategoryRecord {
    pub id: String,
    pub slug: String,
    pub name: String,
    pub description: String,
    pub position: i64,
    pub is_private: bool,
    pub created_at: String,
    pub updated_at: String,
}

/// スレッドのレコード
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct ForumThreadRecord {
    pub id: String,
    pub category_id: String,
    pub title: String,
    pub author_id: Option<String>,
    pub author_name: String,
    pub is_pinned: bool,
    pub is_locked: bool,
    pub post_count: u64,
    pub last_post_at: String,
    pub created_at: String,
    pub updated_at: String,
}

/// 投稿（レス）のレコード
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct ForumPostRecord {
    pub id: String,
    pub thread_id: String,
    pub post_number: u64,
    pub author_id: Option<String>,
    pub author_name: String,
    pub body: String,
    pub reply_to_post_id: Option<String>,
    pub reply_to_number: Option<u64>,
    pub is_deleted: bool,
    pub ip: Option<String>,
    pub user_agent: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

/// スレッド一覧取得条件（キーセットページネーション対応）
#[derive(Debug, Clone, Default)]
pub struct ThreadListQuery {
    pub category_id: Option<String>,
    /// キーセット: カーソル (last_post_at, id)
    pub before_last_post_at: Option<String>,
    pub before_id: Option<String>,
    pub limit: u64,
}

/// 投稿一覧取得条件（キーセットページネーション対応）
#[derive(Debug, Clone, Default)]
pub struct PostListQuery {
    pub thread_id: String,
    /// キーセット: 指定した post_number より後のレスを取得（0なら最初から）
    pub after_post_number: u64,
    pub limit: u64,
}

#[async_trait]
pub trait ForumStorage: Send + Sync {
    // カテゴリ操作
    async fn insert_category(&self, cat: &ForumCategoryRecord) -> Result<()>;
    async fn get_category(&self, id: &str) -> Result<Option<ForumCategoryRecord>>;
    async fn get_category_by_slug(&self, slug: &str) -> Result<Option<ForumCategoryRecord>>;
    async fn list_categories(&self) -> Result<Vec<ForumCategoryRecord>>;
    async fn update_category(&self, cat: &ForumCategoryRecord) -> Result<bool>;
    async fn delete_category(&self, id: &str) -> Result<bool>;

    // スレッド操作
    async fn insert_thread(
        &self,
        thread: &ForumThreadRecord,
        first_post: &ForumPostRecord,
    ) -> Result<()>;
    async fn get_thread(&self, id: &str) -> Result<Option<ForumThreadRecord>>;
    async fn list_threads(&self, query: &ThreadListQuery) -> Result<Vec<ForumThreadRecord>>;
    async fn set_thread_pinned(&self, id: &str, pinned: bool, updated_at: &str) -> Result<bool>;
    async fn set_thread_locked(&self, id: &str, locked: bool, updated_at: &str) -> Result<bool>;
    async fn delete_thread(&self, id: &str) -> Result<bool>;

    // 投稿操作
    async fn insert_post(&self, post: &mut ForumPostRecord) -> Result<u64>;
    async fn get_post(&self, id: &str) -> Result<Option<ForumPostRecord>>;
    async fn list_posts(&self, query: &PostListQuery) -> Result<Vec<ForumPostRecord>>;
    async fn delete_post(&self, id: &str, updated_at: &str) -> Result<bool>;
}

/// テーブル作成 SQL とインデックスの生成
pub fn build_forum_tables(
    text: &str,
    key: &str,
    bigint: &str,
    inline_indexes: bool,
) -> (Vec<String>, Vec<String>) {
    let mut table_sqls = Vec::new();
    let mut index_sqls = Vec::new();

    // 1. Categories
    let cat_indexes = [("idx_aegis_forum_cat_pos", "position, created_at")];
    let cat_inline = if inline_indexes {
        cat_indexes
            .iter()
            .map(|(name, columns)| format!(", INDEX {} ({})", name, columns))
            .collect::<String>()
    } else {
        String::new()
    };
    table_sqls.push(format!(
        "CREATE TABLE IF NOT EXISTS {table} (\
         id {key} PRIMARY KEY, \
         slug {key} NOT NULL UNIQUE, \
         name {text} NOT NULL, \
         description {text} NOT NULL, \
         position {bigint} NOT NULL DEFAULT 0, \
         is_private {bigint} NOT NULL DEFAULT 0, \
         created_at {key} NOT NULL, \
         updated_at {key} NOT NULL{inline});",
        table = FORUM_CATEGORY_TABLE,
        key = key,
        text = text,
        bigint = bigint,
        inline = cat_inline,
    ));
    if !inline_indexes {
        for (name, cols) in cat_indexes {
            index_sqls.push(format!(
                "CREATE INDEX IF NOT EXISTS {} ON {} ({});",
                name, FORUM_CATEGORY_TABLE, cols
            ));
        }
    }

    // 2. Threads
    let th_indexes = [
        (
            "idx_aegis_forum_th_cat",
            "category_id, is_pinned, last_post_at",
        ),
        ("idx_aegis_forum_th_pinned", "is_pinned, last_post_at"),
    ];
    let th_inline = if inline_indexes {
        th_indexes
            .iter()
            .map(|(name, columns)| format!(", INDEX {} ({})", name, columns))
            .collect::<String>()
    } else {
        String::new()
    };
    table_sqls.push(format!(
        "CREATE TABLE IF NOT EXISTS {table} (\
         id {key} PRIMARY KEY, \
         category_id {key} NOT NULL, \
         title {text} NOT NULL, \
         author_id {key}, \
         author_name {text} NOT NULL, \
         is_pinned {bigint} NOT NULL DEFAULT 0, \
         is_locked {bigint} NOT NULL DEFAULT 0, \
         post_count {bigint} NOT NULL DEFAULT 0, \
         last_post_at {key} NOT NULL, \
         created_at {key} NOT NULL, \
         updated_at {key} NOT NULL{inline});",
        table = FORUM_THREAD_TABLE,
        key = key,
        text = text,
        bigint = bigint,
        inline = th_inline,
    ));
    if !inline_indexes {
        for (name, cols) in th_indexes {
            index_sqls.push(format!(
                "CREATE INDEX IF NOT EXISTS {} ON {} ({});",
                name, FORUM_THREAD_TABLE, cols
            ));
        }
    }

    // 3. Posts
    let post_indexes = [
        ("idx_aegis_forum_post_th_num", "thread_id, post_number"),
        ("idx_aegis_forum_post_th_created", "thread_id, created_at"),
    ];
    let post_inline = if inline_indexes {
        post_indexes
            .iter()
            .map(|(name, columns)| format!(", INDEX {} ({})", name, columns))
            .collect::<String>()
    } else {
        String::new()
    };
    table_sqls.push(format!(
        "CREATE TABLE IF NOT EXISTS {table} (\
         id {key} PRIMARY KEY, \
         thread_id {key} NOT NULL, \
         post_number {bigint} NOT NULL, \
         author_id {key}, \
         author_name {text} NOT NULL, \
         body {text} NOT NULL, \
         reply_to_post_id {key}, \
         reply_to_number {bigint}, \
         is_deleted {bigint} NOT NULL DEFAULT 0, \
         ip {key}, \
         user_agent {text}, \
         created_at {key} NOT NULL, \
         updated_at {key} NOT NULL{inline});",
        table = FORUM_POST_TABLE,
        key = key,
        text = text,
        bigint = bigint,
        inline = post_inline,
    ));
    if !inline_indexes {
        for (name, cols) in post_indexes {
            index_sqls.push(format!(
                "CREATE INDEX IF NOT EXISTS {} ON {} ({});",
                name, FORUM_POST_TABLE, cols
            ));
        }
    }

    (table_sqls, index_sqls)
}

macro_rules! forum_cat_row_from_sqlx {
    ($row:expr) => {{
        let row = $row;
        let get = |e: sqlx::Error| aegis_core::AegisError::Database(e.to_string());
        $crate::forum::ForumCategoryRecord {
            id: row.try_get("id").map_err(get)?,
            slug: row.try_get("slug").map_err(get)?,
            name: row.try_get("name").map_err(get)?,
            description: row.try_get("description").map_err(get)?,
            position: row.try_get::<i64, _>("position").map_err(get)?,
            is_private: row.try_get::<i64, _>("is_private").map_err(get)? != 0,
            created_at: row.try_get("created_at").map_err(get)?,
            updated_at: row.try_get("updated_at").map_err(get)?,
        }
    }};
}

macro_rules! forum_th_row_from_sqlx {
    ($row:expr) => {{
        let row = $row;
        let get = |e: sqlx::Error| aegis_core::AegisError::Database(e.to_string());
        $crate::forum::ForumThreadRecord {
            id: row.try_get("id").map_err(get)?,
            category_id: row.try_get("category_id").map_err(get)?,
            title: row.try_get("title").map_err(get)?,
            author_id: row.try_get("author_id").map_err(get)?,
            author_name: row.try_get("author_name").map_err(get)?,
            is_pinned: row.try_get::<i64, _>("is_pinned").map_err(get)? != 0,
            is_locked: row.try_get::<i64, _>("is_locked").map_err(get)? != 0,
            post_count: row.try_get::<i64, _>("post_count").map_err(get)? as u64,
            last_post_at: row.try_get("last_post_at").map_err(get)?,
            created_at: row.try_get("created_at").map_err(get)?,
            updated_at: row.try_get("updated_at").map_err(get)?,
        }
    }};
}

macro_rules! forum_post_row_from_sqlx {
    ($row:expr) => {{
        let row = $row;
        let get = |e: sqlx::Error| aegis_core::AegisError::Database(e.to_string());
        $crate::forum::ForumPostRecord {
            id: row.try_get("id").map_err(get)?,
            thread_id: row.try_get("thread_id").map_err(get)?,
            post_number: row.try_get::<i64, _>("post_number").map_err(get)? as u64,
            author_id: row.try_get("author_id").map_err(get)?,
            author_name: row.try_get("author_name").map_err(get)?,
            body: row.try_get("body").map_err(get)?,
            reply_to_post_id: row.try_get("reply_to_post_id").map_err(get)?,
            reply_to_number: row
                .try_get::<Option<i64>, _>("reply_to_number")
                .map_err(get)?
                .map(|n| n as u64),
            is_deleted: row.try_get::<i64, _>("is_deleted").map_err(get)? != 0,
            ip: row.try_get("ip").map_err(get)?,
            user_agent: row.try_get("user_agent").map_err(get)?,
            created_at: row.try_get("created_at").map_err(get)?,
            updated_at: row.try_get("updated_at").map_err(get)?,
        }
    }};
}

pub(crate) use forum_cat_row_from_sqlx;
pub(crate) use forum_post_row_from_sqlx;
pub(crate) use forum_th_row_from_sqlx;

macro_rules! impl_forum_storage {
    ($adapter:ty, $ph:expr, $label:literal) => {
        #[async_trait::async_trait]
        impl $crate::forum::ForumStorage for $adapter {
            async fn insert_category(
                &self,
                cat: &$crate::forum::ForumCategoryRecord,
            ) -> aegis_core::Result<()> {
                let db_err =
                    |e: sqlx::Error| aegis_core::AegisError::Database(format!("{}: {}", $label, e));
                let ph = $ph;
                let placeholders: Vec<String> = (1..=8).map(ph).collect();
                let sql = format!(
                    "INSERT INTO {} ({}) VALUES ({});",
                    $crate::forum::FORUM_CATEGORY_TABLE,
                    $crate::forum::CATEGORY_COLUMNS.join(", "),
                    placeholders.join(", ")
                );
                sqlx::query(&sql)
                    .bind(&cat.id)
                    .bind(&cat.slug)
                    .bind(&cat.name)
                    .bind(&cat.description)
                    .bind(cat.position)
                    .bind(i64::from(cat.is_private))
                    .bind(&cat.created_at)
                    .bind(&cat.updated_at)
                    .execute(&self.pool)
                    .await
                    .map_err(db_err)?;
                Ok(())
            }

            async fn get_category(
                &self,
                id: &str,
            ) -> aegis_core::Result<Option<$crate::forum::ForumCategoryRecord>> {
                let db_err =
                    |e: sqlx::Error| aegis_core::AegisError::Database(format!("{}: {}", $label, e));
                let ph = $ph;
                let sql = format!(
                    "SELECT {} FROM {} WHERE id = {};",
                    $crate::forum::CATEGORY_COLUMNS.join(", "),
                    $crate::forum::FORUM_CATEGORY_TABLE,
                    ph(1)
                );
                let row = sqlx::query(&sql)
                    .bind(id)
                    .fetch_optional(&self.pool)
                    .await
                    .map_err(db_err)?;
                match row {
                    Some(r) => Ok(Some($crate::forum::forum_cat_row_from_sqlx!(&r))),
                    None => Ok(None),
                }
            }

            async fn get_category_by_slug(
                &self,
                slug: &str,
            ) -> aegis_core::Result<Option<$crate::forum::ForumCategoryRecord>> {
                let db_err =
                    |e: sqlx::Error| aegis_core::AegisError::Database(format!("{}: {}", $label, e));
                let ph = $ph;
                let sql = format!(
                    "SELECT {} FROM {} WHERE slug = {};",
                    $crate::forum::CATEGORY_COLUMNS.join(", "),
                    $crate::forum::FORUM_CATEGORY_TABLE,
                    ph(1)
                );
                let row = sqlx::query(&sql)
                    .bind(slug)
                    .fetch_optional(&self.pool)
                    .await
                    .map_err(db_err)?;
                match row {
                    Some(r) => Ok(Some($crate::forum::forum_cat_row_from_sqlx!(&r))),
                    None => Ok(None),
                }
            }

            async fn list_categories(
                &self,
            ) -> aegis_core::Result<Vec<$crate::forum::ForumCategoryRecord>> {
                let db_err =
                    |e: sqlx::Error| aegis_core::AegisError::Database(format!("{}: {}", $label, e));
                let sql = format!(
                    "SELECT {} FROM {} ORDER BY position ASC, created_at ASC;",
                    $crate::forum::CATEGORY_COLUMNS.join(", "),
                    $crate::forum::FORUM_CATEGORY_TABLE
                );
                let rows = sqlx::query(&sql)
                    .fetch_all(&self.pool)
                    .await
                    .map_err(db_err)?;
                let mut categories = Vec::with_capacity(rows.len());
                for r in &rows {
                    categories.push($crate::forum::forum_cat_row_from_sqlx!(r));
                }
                Ok(categories)
            }

            async fn update_category(
                &self,
                cat: &$crate::forum::ForumCategoryRecord,
            ) -> aegis_core::Result<bool> {
                let db_err =
                    |e: sqlx::Error| aegis_core::AegisError::Database(format!("{}: {}", $label, e));
                let ph = $ph;
                let sql = format!(
                    "UPDATE {} SET slug = {}, name = {}, description = {}, position = {}, is_private = {}, updated_at = {} WHERE id = {};",
                    $crate::forum::FORUM_CATEGORY_TABLE,
                    ph(1), ph(2), ph(3), ph(4), ph(5), ph(6), ph(7)
                );
                let res = sqlx::query(&sql)
                    .bind(&cat.slug)
                    .bind(&cat.name)
                    .bind(&cat.description)
                    .bind(cat.position)
                    .bind(i64::from(cat.is_private))
                    .bind(&cat.updated_at)
                    .bind(&cat.id)
                    .execute(&self.pool)
                    .await
                    .map_err(db_err)?;
                Ok(res.rows_affected() > 0)
            }

            async fn delete_category(&self, id: &str) -> aegis_core::Result<bool> {
                let db_err =
                    |e: sqlx::Error| aegis_core::AegisError::Database(format!("{}: {}", $label, e));
                let ph = $ph;
                let sql = format!(
                    "DELETE FROM {} WHERE id = {};",
                    $crate::forum::FORUM_CATEGORY_TABLE,
                    ph(1)
                );
                let res = sqlx::query(&sql)
                    .bind(id)
                    .execute(&self.pool)
                    .await
                    .map_err(db_err)?;
                Ok(res.rows_affected() > 0)
            }

            async fn insert_thread(
                &self,
                thread: &$crate::forum::ForumThreadRecord,
                first_post: &$crate::forum::ForumPostRecord,
            ) -> aegis_core::Result<()> {
                let db_err =
                    |e: sqlx::Error| aegis_core::AegisError::Database(format!("{}: {}", $label, e));
                let ph = $ph;

                let th_placeholders: Vec<String> = (1..=11).map(&ph).collect();
                let th_sql = format!(
                    "INSERT INTO {} ({}) VALUES ({});",
                    $crate::forum::FORUM_THREAD_TABLE,
                    $crate::forum::THREAD_COLUMNS.join(", "),
                    th_placeholders.join(", ")
                );

                let post_placeholders: Vec<String> = (1..=13).map(&ph).collect();
                let post_sql = format!(
                    "INSERT INTO {} ({}) VALUES ({});",
                    $crate::forum::FORUM_POST_TABLE,
                    $crate::forum::POST_COLUMNS.join(", "),
                    post_placeholders.join(", ")
                );

                let mut tx = self.pool.begin().await.map_err(db_err)?;

                sqlx::query(&th_sql)
                    .bind(&thread.id)
                    .bind(&thread.category_id)
                    .bind(&thread.title)
                    .bind(&thread.author_id)
                    .bind(&thread.author_name)
                    .bind(i64::from(thread.is_pinned))
                    .bind(i64::from(thread.is_locked))
                    .bind(thread.post_count as i64)
                    .bind(&thread.last_post_at)
                    .bind(&thread.created_at)
                    .bind(&thread.updated_at)
                    .execute(&mut *tx)
                    .await
                    .map_err(db_err)?;

                sqlx::query(&post_sql)
                    .bind(&first_post.id)
                    .bind(&first_post.thread_id)
                    .bind(first_post.post_number as i64)
                    .bind(&first_post.author_id)
                    .bind(&first_post.author_name)
                    .bind(&first_post.body)
                    .bind(&first_post.reply_to_post_id)
                    .bind(first_post.reply_to_number.map(|n| n as i64))
                    .bind(i64::from(first_post.is_deleted))
                    .bind(&first_post.ip)
                    .bind(&first_post.user_agent)
                    .bind(&first_post.created_at)
                    .bind(&first_post.updated_at)
                    .execute(&mut *tx)
                    .await
                    .map_err(db_err)?;

                tx.commit().await.map_err(db_err)?;
                Ok(())
            }

            async fn get_thread(
                &self,
                id: &str,
            ) -> aegis_core::Result<Option<$crate::forum::ForumThreadRecord>> {
                let db_err =
                    |e: sqlx::Error| aegis_core::AegisError::Database(format!("{}: {}", $label, e));
                let ph = $ph;
                let sql = format!(
                    "SELECT {} FROM {} WHERE id = {};",
                    $crate::forum::THREAD_COLUMNS.join(", "),
                    $crate::forum::FORUM_THREAD_TABLE,
                    ph(1)
                );
                let row = sqlx::query(&sql)
                    .bind(id)
                    .fetch_optional(&self.pool)
                    .await
                    .map_err(db_err)?;
                match row {
                    Some(r) => Ok(Some($crate::forum::forum_th_row_from_sqlx!(&r))),
                    None => Ok(None),
                }
            }

            async fn list_threads(
                &self,
                query: &$crate::forum::ThreadListQuery,
            ) -> aegis_core::Result<Vec<$crate::forum::ForumThreadRecord>> {
                let db_err =
                    |e: sqlx::Error| aegis_core::AegisError::Database(format!("{}: {}", $label, e));
                let ph = $ph;
                let mut conds = Vec::new();
                let mut binds = Vec::new();

                if let Some(cat_id) = &query.category_id {
                    binds.push(cat_id.clone());
                    conds.push(format!("category_id = {}", ph(binds.len())));
                }

                if let (Some(last_at), Some(bid)) = (&query.before_last_post_at, &query.before_id) {
                    let p1 = ph(binds.len() + 1);
                    let p2 = ph(binds.len() + 2);
                    let p3 = ph(binds.len() + 3);
                    conds.push(format!("(last_post_at < {} OR (last_post_at = {} AND id < {}))", p1, p2, p3));
                    binds.push(last_at.clone());
                    binds.push(last_at.clone());
                    binds.push(bid.clone());
                }

                let where_clause = if conds.is_empty() {
                    String::new()
                } else {
                    format!(" WHERE {}", conds.join(" AND "))
                };

                let limit_clause = if query.limit > 0 {
                    format!(" LIMIT {}", query.limit)
                } else {
                    " LIMIT 50".to_string()
                };

                let sql = format!(
                    "SELECT {} FROM {}{} ORDER BY is_pinned DESC, last_post_at DESC, id DESC{};",
                    $crate::forum::THREAD_COLUMNS.join(", "),
                    $crate::forum::FORUM_THREAD_TABLE,
                    where_clause,
                    limit_clause
                );

                let mut q = sqlx::query(&sql);
                for b in &binds {
                    q = q.bind(b);
                }
                let rows = q.fetch_all(&self.pool).await.map_err(db_err)?;
                let mut threads = Vec::with_capacity(rows.len());
                for r in &rows {
                    threads.push($crate::forum::forum_th_row_from_sqlx!(r));
                }
                Ok(threads)
            }

            async fn set_thread_pinned(
                &self,
                id: &str,
                pinned: bool,
                updated_at: &str,
            ) -> aegis_core::Result<bool> {
                let db_err =
                    |e: sqlx::Error| aegis_core::AegisError::Database(format!("{}: {}", $label, e));
                let ph = $ph;
                let sql = format!(
                    "UPDATE {} SET is_pinned = {}, updated_at = {} WHERE id = {};",
                    $crate::forum::FORUM_THREAD_TABLE,
                    ph(1), ph(2), ph(3)
                );
                let res = sqlx::query(&sql)
                    .bind(i64::from(pinned))
                    .bind(updated_at)
                    .bind(id)
                    .execute(&self.pool)
                    .await
                    .map_err(db_err)?;
                Ok(res.rows_affected() > 0)
            }

            async fn set_thread_locked(
                &self,
                id: &str,
                locked: bool,
                updated_at: &str,
            ) -> aegis_core::Result<bool> {
                let db_err =
                    |e: sqlx::Error| aegis_core::AegisError::Database(format!("{}: {}", $label, e));
                let ph = $ph;
                let sql = format!(
                    "UPDATE {} SET is_locked = {}, updated_at = {} WHERE id = {};",
                    $crate::forum::FORUM_THREAD_TABLE,
                    ph(1), ph(2), ph(3)
                );
                let res = sqlx::query(&sql)
                    .bind(i64::from(locked))
                    .bind(updated_at)
                    .bind(id)
                    .execute(&self.pool)
                    .await
                    .map_err(db_err)?;
                Ok(res.rows_affected() > 0)
            }

            async fn delete_thread(&self, id: &str) -> aegis_core::Result<bool> {
                let db_err =
                    |e: sqlx::Error| aegis_core::AegisError::Database(format!("{}: {}", $label, e));
                let ph = $ph;
                let mut tx = self.pool.begin().await.map_err(db_err)?;

                let del_posts_sql = format!(
                    "DELETE FROM {} WHERE thread_id = {};",
                    $crate::forum::FORUM_POST_TABLE,
                    ph(1)
                );
                sqlx::query(&del_posts_sql)
                    .bind(id)
                    .execute(&mut *tx)
                    .await
                    .map_err(db_err)?;

                let del_th_sql = format!(
                    "DELETE FROM {} WHERE id = {};",
                    $crate::forum::FORUM_THREAD_TABLE,
                    ph(1)
                );
                let res = sqlx::query(&del_th_sql)
                    .bind(id)
                    .execute(&mut *tx)
                    .await
                    .map_err(db_err)?;

                tx.commit().await.map_err(db_err)?;
                Ok(res.rows_affected() > 0)
            }

            async fn insert_post(
                &self,
                post: &mut $crate::forum::ForumPostRecord,
            ) -> aegis_core::Result<u64> {
                let db_err =
                    |e: sqlx::Error| aegis_core::AegisError::Database(format!("{}: {}", $label, e));
                let ph = $ph;

                let mut tx = self.pool.begin().await.map_err(db_err)?;

                // スレッドの post_count をインクリメント
                let inc_sql = format!(
                    "UPDATE {} SET post_count = post_count + 1, last_post_at = {}, updated_at = {} WHERE id = {};",
                    $crate::forum::FORUM_THREAD_TABLE,
                    ph(1), ph(2), ph(3)
                );
                let inc_res = sqlx::query(&inc_sql)
                    .bind(&post.created_at)
                    .bind(&post.updated_at)
                    .bind(&post.thread_id)
                    .execute(&mut *tx)
                    .await
                    .map_err(db_err)?;

                if inc_res.rows_affected() == 0 {
                    return Err(aegis_core::AegisError::NotFound(format!(
                        "Thread {} not found",
                        post.thread_id
                    )));
                }

                // 更新後の post_count を取得
                let count_sql = format!(
                    "SELECT post_count FROM {} WHERE id = {};",
                    $crate::forum::FORUM_THREAD_TABLE,
                    ph(1)
                );
                let post_count: i64 = sqlx::query_scalar(&count_sql)
                    .bind(&post.thread_id)
                    .fetch_one(&mut *tx)
                    .await
                    .map_err(db_err)?;

                post.post_number = post_count as u64;

                let post_placeholders: Vec<String> = (1..=13).map(&ph).collect();
                let insert_sql = format!(
                    "INSERT INTO {} ({}) VALUES ({});",
                    $crate::forum::FORUM_POST_TABLE,
                    $crate::forum::POST_COLUMNS.join(", "),
                    post_placeholders.join(", ")
                );

                sqlx::query(&insert_sql)
                    .bind(&post.id)
                    .bind(&post.thread_id)
                    .bind(post.post_number as i64)
                    .bind(&post.author_id)
                    .bind(&post.author_name)
                    .bind(&post.body)
                    .bind(&post.reply_to_post_id)
                    .bind(post.reply_to_number.map(|n| n as i64))
                    .bind(i64::from(post.is_deleted))
                    .bind(&post.ip)
                    .bind(&post.user_agent)
                    .bind(&post.created_at)
                    .bind(&post.updated_at)
                    .execute(&mut *tx)
                    .await
                    .map_err(db_err)?;

                tx.commit().await.map_err(db_err)?;
                Ok(post.post_number)
            }

            async fn get_post(
                &self,
                id: &str,
            ) -> aegis_core::Result<Option<$crate::forum::ForumPostRecord>> {
                let db_err =
                    |e: sqlx::Error| aegis_core::AegisError::Database(format!("{}: {}", $label, e));
                let ph = $ph;
                let sql = format!(
                    "SELECT {} FROM {} WHERE id = {};",
                    $crate::forum::POST_COLUMNS.join(", "),
                    $crate::forum::FORUM_POST_TABLE,
                    ph(1)
                );
                let row = sqlx::query(&sql)
                    .bind(id)
                    .fetch_optional(&self.pool)
                    .await
                    .map_err(db_err)?;
                match row {
                    Some(r) => Ok(Some($crate::forum::forum_post_row_from_sqlx!(&r))),
                    None => Ok(None),
                }
            }

            async fn list_posts(
                &self,
                query: &$crate::forum::PostListQuery,
            ) -> aegis_core::Result<Vec<$crate::forum::ForumPostRecord>> {
                let db_err =
                    |e: sqlx::Error| aegis_core::AegisError::Database(format!("{}: {}", $label, e));
                let ph = $ph;
                let mut conds = Vec::new();
                let binds = vec![query.thread_id.clone()];
                conds.push(format!("thread_id = {}", ph(1)));

                if query.after_post_number > 0 {
                    conds.push(format!("post_number > {}", query.after_post_number));
                }

                let where_clause = format!(" WHERE {}", conds.join(" AND "));
                let limit_clause = if query.limit > 0 {
                    format!(" LIMIT {}", query.limit)
                } else {
                    " LIMIT 100".to_string()
                };

                let sql = format!(
                    "SELECT {} FROM {}{} ORDER BY post_number ASC{};",
                    $crate::forum::POST_COLUMNS.join(", "),
                    $crate::forum::FORUM_POST_TABLE,
                    where_clause,
                    limit_clause
                );

                let mut q = sqlx::query(&sql);
                for b in &binds {
                    q = q.bind(b);
                }
                let rows = q.fetch_all(&self.pool).await.map_err(db_err)?;
                let mut posts = Vec::with_capacity(rows.len());
                for r in &rows {
                    posts.push($crate::forum::forum_post_row_from_sqlx!(r));
                }
                Ok(posts)
            }

            async fn delete_post(&self, id: &str, updated_at: &str) -> aegis_core::Result<bool> {
                let db_err =
                    |e: sqlx::Error| aegis_core::AegisError::Database(format!("{}: {}", $label, e));
                let ph = $ph;
                // ソフトデリート（is_deleted = 1）
                let sql = format!(
                    "UPDATE {} SET is_deleted = 1, updated_at = {} WHERE id = {};",
                    $crate::forum::FORUM_POST_TABLE,
                    ph(1), ph(2)
                );
                let res = sqlx::query(&sql)
                    .bind(updated_at)
                    .bind(id)
                    .execute(&self.pool)
                    .await
                    .map_err(db_err)?;
                Ok(res.rows_affected() > 0)
            }
        }
    };
}
pub(crate) use impl_forum_storage;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_forum_table_generation() {
        let (tables, indexes) = build_forum_tables("TEXT", "TEXT", "INTEGER", false);
        assert_eq!(tables.len(), 3);
        assert_eq!(indexes.len(), 5);
        assert!(tables[0].starts_with("CREATE TABLE IF NOT EXISTS _aegis_forum_categories"));
        assert!(tables[1].starts_with("CREATE TABLE IF NOT EXISTS _aegis_forum_threads"));
        assert!(tables[2].starts_with("CREATE TABLE IF NOT EXISTS _aegis_forum_posts"));

        let (mysql_tables, mysql_indexes) =
            build_forum_tables("TEXT", "VARCHAR(255)", "BIGINT", true);
        assert_eq!(mysql_tables.len(), 3);
        assert_eq!(mysql_indexes.len(), 0);
        assert!(mysql_tables[0].contains("INDEX idx_aegis_forum_cat_pos"));
        assert!(mysql_tables[1].contains("INDEX idx_aegis_forum_th_pinned"));
        assert!(mysql_tables[2].contains("INDEX idx_aegis_forum_post_th_num"));
    }
}
