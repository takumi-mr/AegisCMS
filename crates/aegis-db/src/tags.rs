//! タグのテーブル（`_aegis_tags`、`_aegis_content_tags`）とストレージ。タグは名前と slug を持つ独立した
//! 行で、コンテンツ（記事・固定ページ・商品など、どの種類でも）には `(content_type, content_id)` で結びつく。
//! 3 つのダイアレクトで SQL を共有し、プレースホルダだけを呼び出し側（`?N` / `$N` / `?`）が決める

use aegis_core::Result;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

/// タグのテーブル名
pub const TAG_TABLE: &str = "_aegis_tags";
/// タグとコンテンツの結びつきのテーブル名
pub const CONTENT_TAG_TABLE: &str = "_aegis_content_tags";

/// `_aegis_tags` のカラム（順序固定）
pub const TAG_COLUMNS: [&str; 6] = [
    "id",
    "slug",
    "name",
    "description",
    "created_at",
    "updated_at",
];

/// タグ（`_aegis_tags` の 1 行）
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct TagRecord {
    pub id: String,
    /// URL に使う名前（`/tags/{slug}`）。全タグで重複しない
    pub slug: String,
    /// 表示する名前
    pub name: String,
    pub description: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

/// 付いているコンテンツの数つきのタグ
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TagWithUsage {
    pub tag: TagRecord,
    /// このタグが付いているコンテンツの数（種類を問わない）
    pub usage: u64,
}

/// タグとコンテンツの結びつき（`_aegis_content_tags` の 1 行）
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct ContentTagRecord {
    pub content_type: String,
    pub content_id: String,
    pub tag_id: String,
    pub created_at: String,
}

/// タグ一覧の条件
#[derive(Debug, Clone, Default)]
pub struct TagListQuery {
    /// 名前・slug への部分一致（大文字小文字を区別しない、ASCII のみ）
    pub search: Option<String>,
    /// 取得する最大件数。`None` なら全件（`offset` は無視）
    pub limit: Option<u64>,
    pub offset: u64,
}

/// `list_tags` の結果: 名前の昇順の行と、ページングしない場合の総件数
#[derive(Debug, Clone, Default)]
pub struct TagPage {
    pub tags: Vec<TagWithUsage>,
    pub total: u64,
}

/// タグの保存（`DatabaseAdapter` のスーパートレイト）。実装は `impl_tag_storage!` で生成する
#[async_trait]
pub trait TagStorage: Send + Sync {
    async fn insert_tag(&self, tag: &TagRecord) -> Result<()>;

    async fn get_tag(&self, id: &str) -> Result<Option<TagRecord>>;

    async fn get_tag_by_slug(&self, slug: &str) -> Result<Option<TagRecord>>;

    /// Tags matching `query` (by name) with the number of contents each one is on
    async fn list_tags(&self, query: &TagListQuery) -> Result<TagPage>;

    /// Change the slug, name and description. Returns false when no tag has that id
    async fn update_tag(&self, tag: &TagRecord) -> Result<bool>;

    /// Delete a tag and take it off every content record. Returns false when no tag has that id
    async fn delete_tag(&self, id: &str) -> Result<bool>;

    /// Make `tag_ids` the tags of a content record (the others are taken off)
    async fn set_content_tags(
        &self,
        content_type: &str,
        content_id: &str,
        tag_ids: &[String],
        created_at: &str,
    ) -> Result<()>;

    /// The tags of one content record, by name
    async fn list_content_tags(
        &self,
        content_type: &str,
        content_id: &str,
    ) -> Result<Vec<TagRecord>>;

    /// The tags of several content records of one type: `(content_id, tag)`, by name
    async fn list_tags_for_contents(
        &self,
        content_type: &str,
        content_ids: &[String],
    ) -> Result<Vec<(String, TagRecord)>>;

    /// The content records a tag is on (every type, or only `content_type`)
    async fn list_tagged_contents(
        &self,
        tag_id: &str,
        content_type: Option<&str>,
    ) -> Result<Vec<ContentTagRecord>>;

    /// Take every tag off a content record (when the record is deleted)
    async fn delete_content_tags(&self, content_type: &str, content_id: &str) -> Result<u64>;

    /// Every tag/content link (for backups)
    async fn list_all_content_tags(&self) -> Result<Vec<ContentTagRecord>>;

    /// Restore a tag/content link (for restores; one that exists is left alone)
    async fn insert_content_tag(&self, link: &ContentTagRecord) -> Result<()>;
}

/// タグのテーブルの CREATE 文と索引。`text` は長い文字列、`key` は索引を張る文字列（MySQL は
/// `VARCHAR(255)`）。`inline_indexes` なら索引を CREATE TABLE に含める（MySQL）
pub fn build_tag_tables(text: &str, key: &str, inline_indexes: bool) -> (Vec<String>, Vec<String>) {
    // MySQL（utf8mb4）の索引は 3072 バイトまで: 結びつきの主キーは VARCHAR(255) が 3 つで 3060 バイト
    let link_index = ("idx_aegis_content_tags_tag", "tag_id, content_type");
    let inline = if inline_indexes {
        format!(", INDEX {} ({})", link_index.0, link_index.1)
    } else {
        String::new()
    };
    let tables = vec![
        format!(
            "CREATE TABLE IF NOT EXISTS {table} (\
             id {key} PRIMARY KEY, \
             slug {key} NOT NULL UNIQUE, \
             name {key} NOT NULL, \
             description {text}, \
             created_at {key} NOT NULL, \
             updated_at {key} NOT NULL);",
            table = TAG_TABLE,
            key = key,
            text = text,
        ),
        format!(
            "CREATE TABLE IF NOT EXISTS {table} (\
             content_type {key} NOT NULL, \
             content_id {key} NOT NULL, \
             tag_id {key} NOT NULL, \
             created_at {key} NOT NULL, \
             PRIMARY KEY (content_type, content_id, tag_id){inline});",
            table = CONTENT_TAG_TABLE,
            key = key,
            inline = inline,
        ),
    ];
    let separate = if inline_indexes {
        Vec::new()
    } else {
        vec![format!(
            "CREATE INDEX IF NOT EXISTS {} ON {} ({});",
            link_index.0, CONTENT_TAG_TABLE, link_index.1
        )]
    };
    (tables, separate)
}

/// 一覧の SELECT と COUNT、両方に共通のバインド値（検索語のパターン。名前と slug の 2 つ）。
/// 付いているコンテンツの数は `use_count` 列。LIMIT / OFFSET のみ数値を埋め込む
pub fn build_tag_list(
    query: &TagListQuery,
    placeholder: impl Fn(usize) -> String,
) -> (String, String, Vec<String>) {
    let mut binds = Vec::new();
    let where_clause = match query
        .search
        .as_deref()
        .map(str::trim)
        .filter(|term| !term.is_empty())
    {
        Some(term) => {
            let pattern = format!("%{}%", crate::escape_like(&term.to_lowercase()));
            // 値は列の数だけバインドする（`?` の MySQL は位置で対応づけるため）
            binds.push(pattern.clone());
            let name = placeholder(binds.len());
            binds.push(pattern);
            let slug = placeholder(binds.len());
            format!(
                " WHERE (LOWER(t.name) LIKE {} ESCAPE '!' OR LOWER(t.slug) LIKE {} ESCAPE '!')",
                name, slug
            )
        }
        None => String::new(),
    };
    let paging = query
        .limit
        .map(|limit| format!(" LIMIT {} OFFSET {}", limit, query.offset))
        .unwrap_or_default();
    let columns: Vec<String> = TAG_COLUMNS.iter().map(|c| format!("t.{}", c)).collect();
    (
        format!(
            "SELECT {columns}, (SELECT COUNT(*) FROM {links} c WHERE c.tag_id = t.id) AS use_count \
             FROM {tags} t{where_clause} ORDER BY t.name ASC, t.id ASC{paging};",
            columns = columns.join(", "),
            links = CONTENT_TAG_TABLE,
            tags = TAG_TABLE,
            where_clause = where_clause,
            paging = paging,
        ),
        format!("SELECT COUNT(*) FROM {} t{};", TAG_TABLE, where_clause),
        binds,
    )
}

/// `count` 件の値を並べた `IN (...)` のプレースホルダ（`first` 番から）
fn in_placeholders(first: usize, count: usize, placeholder: &impl Fn(usize) -> String) -> String {
    (first..first + count)
        .map(placeholder)
        .collect::<Vec<_>>()
        .join(", ")
}

/// 複数のコンテンツのタグの SELECT。バインド順は content_type, content_id...
pub fn build_tags_for_contents(count: usize, placeholder: impl Fn(usize) -> String) -> String {
    let columns: Vec<String> = TAG_COLUMNS.iter().map(|c| format!("t.{}", c)).collect();
    format!(
        "SELECT c.content_id AS content_id, {columns} FROM {links} c \
         JOIN {tags} t ON t.id = c.tag_id \
         WHERE c.content_type = {ty} AND c.content_id IN ({ids}) \
         ORDER BY t.name ASC, t.id ASC;",
        columns = columns.join(", "),
        links = CONTENT_TAG_TABLE,
        tags = TAG_TABLE,
        ty = placeholder(1),
        ids = in_placeholders(2, count, &placeholder),
    )
}

/// sqlx の行 (`SqliteRow` / `PgRow` / `MySqlRow`) を `TagRecord` に変換する
macro_rules! tag_row_from_sqlx {
    ($row:expr) => {{
        let row = $row;
        let get = |e: sqlx::Error| aegis_core::AegisError::Database(e.to_string());
        $crate::tags::TagRecord {
            id: row.try_get("id").map_err(get)?,
            slug: row.try_get("slug").map_err(get)?,
            name: row.try_get("name").map_err(get)?,
            description: row.try_get("description").map_err(get)?,
            created_at: row.try_get("created_at").map_err(get)?,
            updated_at: row.try_get("updated_at").map_err(get)?,
        }
    }};
}
pub(crate) use tag_row_from_sqlx;

/// sqlx の行を `ContentTagRecord` に変換する
macro_rules! content_tag_row_from_sqlx {
    ($row:expr) => {{
        let row = $row;
        let get = |e: sqlx::Error| aegis_core::AegisError::Database(e.to_string());
        $crate::tags::ContentTagRecord {
            content_type: row.try_get("content_type").map_err(get)?,
            content_id: row.try_get("content_id").map_err(get)?,
            tag_id: row.try_get("tag_id").map_err(get)?,
            created_at: row.try_get("created_at").map_err(get)?,
        }
    }};
}
pub(crate) use content_tag_row_from_sqlx;

/// 各 DB アダプタに `TagStorage` を実装するマクロ
macro_rules! impl_tag_storage {
    ($adapter:ty, $ph:expr, $label:literal) => {
        #[async_trait::async_trait]
        impl $crate::tags::TagStorage for $adapter {
            async fn insert_tag(&self, t: &$crate::tags::TagRecord) -> aegis_core::Result<()> {
                let ph = $ph;
                let placeholders: Vec<String> = (1..=$crate::tags::TAG_COLUMNS.len())
                    .map(&ph)
                    .collect();
                let sql = format!(
                    "INSERT INTO {} ({}) VALUES ({});",
                    $crate::tags::TAG_TABLE,
                    $crate::tags::TAG_COLUMNS.join(", "),
                    placeholders.join(", ")
                );
                let db_err =
                    |e: sqlx::Error| aegis_core::AegisError::Database(format!("{}: {}", $label, e));
                sqlx::query(&sql)
                    .bind(&t.id)
                    .bind(&t.slug)
                    .bind(&t.name)
                    .bind(&t.description)
                    .bind(&t.created_at)
                    .bind(&t.updated_at)
                    .execute(&self.pool)
                    .await
                    .map_err(db_err)?;
                Ok(())
            }

            async fn get_tag(
                &self,
                id: &str,
            ) -> aegis_core::Result<Option<$crate::tags::TagRecord>> {
                let ph = $ph;
                let sql = format!(
                    "SELECT {} FROM {} WHERE id = {};",
                    $crate::tags::TAG_COLUMNS.join(", "),
                    $crate::tags::TAG_TABLE,
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
                    Some(r) => Ok(Some($crate::tags::tag_row_from_sqlx!(&r))),
                    None => Ok(None),
                }
            }

            async fn get_tag_by_slug(
                &self,
                slug: &str,
            ) -> aegis_core::Result<Option<$crate::tags::TagRecord>> {
                let ph = $ph;
                let sql = format!(
                    "SELECT {} FROM {} WHERE slug = {};",
                    $crate::tags::TAG_COLUMNS.join(", "),
                    $crate::tags::TAG_TABLE,
                    ph(1)
                );
                let db_err =
                    |e: sqlx::Error| aegis_core::AegisError::Database(format!("{}: {}", $label, e));
                let row = sqlx::query(&sql)
                    .bind(slug)
                    .fetch_optional(&self.pool)
                    .await
                    .map_err(db_err)?;
                match row {
                    Some(r) => Ok(Some($crate::tags::tag_row_from_sqlx!(&r))),
                    None => Ok(None),
                }
            }

            async fn list_tags(
                &self,
                query: &$crate::tags::TagListQuery,
            ) -> aegis_core::Result<$crate::tags::TagPage> {
                let (select, count, binds) = $crate::tags::build_tag_list(query, $ph);
                let db_err =
                    |e: sqlx::Error| aegis_core::AegisError::Database(format!("{}: {}", $label, e));
                let mut q = sqlx::query(&select);
                for value in &binds {
                    q = q.bind(value);
                }
                let rows = q.fetch_all(&self.pool).await.map_err(db_err)?;
                let mut tags = Vec::with_capacity(rows.len());
                for r in &rows {
                    let usage: i64 = r.try_get("use_count").map_err(db_err)?;
                    tags.push($crate::tags::TagWithUsage {
                        tag: $crate::tags::tag_row_from_sqlx!(r),
                        usage: usage.max(0) as u64,
                    });
                }
                let mut q = sqlx::query_scalar::<_, i64>(&count);
                for value in &binds {
                    q = q.bind(value);
                }
                let total = q.fetch_one(&self.pool).await.map_err(db_err)?;
                Ok($crate::tags::TagPage {
                    tags,
                    total: total.max(0) as u64,
                })
            }

            async fn update_tag(&self, t: &$crate::tags::TagRecord) -> aegis_core::Result<bool> {
                let ph = $ph;
                let sql = format!(
                    "UPDATE {} SET slug = {}, name = {}, description = {}, updated_at = {} WHERE id = {};",
                    $crate::tags::TAG_TABLE,
                    ph(1),
                    ph(2),
                    ph(3),
                    ph(4),
                    ph(5)
                );
                let db_err =
                    |e: sqlx::Error| aegis_core::AegisError::Database(format!("{}: {}", $label, e));
                let res = sqlx::query(&sql)
                    .bind(&t.slug)
                    .bind(&t.name)
                    .bind(&t.description)
                    .bind(&t.updated_at)
                    .bind(&t.id)
                    .execute(&self.pool)
                    .await
                    .map_err(db_err)?;
                Ok(res.rows_affected() > 0)
            }

            async fn delete_tag(&self, id: &str) -> aegis_core::Result<bool> {
                let ph = $ph;
                let db_err =
                    |e: sqlx::Error| aegis_core::AegisError::Database(format!("{}: {}", $label, e));
                let mut tx = self.pool.begin().await.map_err(db_err)?;
                sqlx::query(&format!(
                    "DELETE FROM {} WHERE tag_id = {};",
                    $crate::tags::CONTENT_TAG_TABLE,
                    ph(1)
                ))
                .bind(id)
                .execute(&mut *tx)
                .await
                .map_err(db_err)?;
                let res = sqlx::query(&format!(
                    "DELETE FROM {} WHERE id = {};",
                    $crate::tags::TAG_TABLE,
                    ph(1)
                ))
                .bind(id)
                .execute(&mut *tx)
                .await
                .map_err(db_err)?;
                tx.commit().await.map_err(db_err)?;
                Ok(res.rows_affected() > 0)
            }

            async fn set_content_tags(
                &self,
                content_type: &str,
                content_id: &str,
                tag_ids: &[String],
                created_at: &str,
            ) -> aegis_core::Result<()> {
                let ph = $ph;
                let db_err =
                    |e: sqlx::Error| aegis_core::AegisError::Database(format!("{}: {}", $label, e));
                let mut tx = self.pool.begin().await.map_err(db_err)?;
                sqlx::query(&format!(
                    "DELETE FROM {} WHERE content_type = {} AND content_id = {};",
                    $crate::tags::CONTENT_TAG_TABLE,
                    ph(1),
                    ph(2)
                ))
                .bind(content_type)
                .bind(content_id)
                .execute(&mut *tx)
                .await
                .map_err(db_err)?;
                let insert = format!(
                    "INSERT INTO {} (content_type, content_id, tag_id, created_at) VALUES ({}, {}, {}, {});",
                    $crate::tags::CONTENT_TAG_TABLE,
                    ph(1),
                    ph(2),
                    ph(3),
                    ph(4)
                );
                let mut seen = std::collections::HashSet::new();
                for tag_id in tag_ids {
                    if !seen.insert(tag_id) {
                        continue;
                    }
                    sqlx::query(&insert)
                        .bind(content_type)
                        .bind(content_id)
                        .bind(tag_id)
                        .bind(created_at)
                        .execute(&mut *tx)
                        .await
                        .map_err(db_err)?;
                }
                tx.commit().await.map_err(db_err)?;
                Ok(())
            }

            async fn list_content_tags(
                &self,
                content_type: &str,
                content_id: &str,
            ) -> aegis_core::Result<Vec<$crate::tags::TagRecord>> {
                let ph = $ph;
                let columns: Vec<String> = $crate::tags::TAG_COLUMNS
                    .iter()
                    .map(|c| format!("t.{}", c))
                    .collect();
                let sql = format!(
                    "SELECT {} FROM {} c JOIN {} t ON t.id = c.tag_id \
                     WHERE c.content_type = {} AND c.content_id = {} ORDER BY t.name ASC, t.id ASC;",
                    columns.join(", "),
                    $crate::tags::CONTENT_TAG_TABLE,
                    $crate::tags::TAG_TABLE,
                    ph(1),
                    ph(2)
                );
                let db_err =
                    |e: sqlx::Error| aegis_core::AegisError::Database(format!("{}: {}", $label, e));
                let rows = sqlx::query(&sql)
                    .bind(content_type)
                    .bind(content_id)
                    .fetch_all(&self.pool)
                    .await
                    .map_err(db_err)?;
                let mut tags = Vec::with_capacity(rows.len());
                for r in &rows {
                    tags.push($crate::tags::tag_row_from_sqlx!(r));
                }
                Ok(tags)
            }

            async fn list_tags_for_contents(
                &self,
                content_type: &str,
                content_ids: &[String],
            ) -> aegis_core::Result<Vec<(String, $crate::tags::TagRecord)>> {
                if content_ids.is_empty() {
                    return Ok(Vec::new());
                }
                let sql = $crate::tags::build_tags_for_contents(content_ids.len(), $ph);
                let db_err =
                    |e: sqlx::Error| aegis_core::AegisError::Database(format!("{}: {}", $label, e));
                let mut q = sqlx::query(&sql).bind(content_type);
                for id in content_ids {
                    q = q.bind(id);
                }
                let rows = q.fetch_all(&self.pool).await.map_err(db_err)?;
                let mut out = Vec::with_capacity(rows.len());
                for r in &rows {
                    let content_id: String = r.try_get("content_id").map_err(db_err)?;
                    out.push((content_id, $crate::tags::tag_row_from_sqlx!(r)));
                }
                Ok(out)
            }

            async fn list_tagged_contents(
                &self,
                tag_id: &str,
                content_type: Option<&str>,
            ) -> aegis_core::Result<Vec<$crate::tags::ContentTagRecord>> {
                let ph = $ph;
                let mut sql = format!(
                    "SELECT content_type, content_id, tag_id, created_at FROM {} WHERE tag_id = {}",
                    $crate::tags::CONTENT_TAG_TABLE,
                    ph(1)
                );
                if content_type.is_some() {
                    sql.push_str(&format!(" AND content_type = {}", ph(2)));
                }
                sql.push_str(" ORDER BY content_type ASC, content_id ASC;");
                let db_err =
                    |e: sqlx::Error| aegis_core::AegisError::Database(format!("{}: {}", $label, e));
                let mut q = sqlx::query(&sql).bind(tag_id);
                if let Some(content_type) = content_type {
                    q = q.bind(content_type);
                }
                let rows = q.fetch_all(&self.pool).await.map_err(db_err)?;
                let mut out = Vec::with_capacity(rows.len());
                for r in &rows {
                    out.push($crate::tags::content_tag_row_from_sqlx!(r));
                }
                Ok(out)
            }

            async fn delete_content_tags(
                &self,
                content_type: &str,
                content_id: &str,
            ) -> aegis_core::Result<u64> {
                let ph = $ph;
                let sql = format!(
                    "DELETE FROM {} WHERE content_type = {} AND content_id = {};",
                    $crate::tags::CONTENT_TAG_TABLE,
                    ph(1),
                    ph(2)
                );
                let db_err =
                    |e: sqlx::Error| aegis_core::AegisError::Database(format!("{}: {}", $label, e));
                let res = sqlx::query(&sql)
                    .bind(content_type)
                    .bind(content_id)
                    .execute(&self.pool)
                    .await
                    .map_err(db_err)?;
                Ok(res.rows_affected())
            }

            async fn list_all_content_tags(
                &self,
            ) -> aegis_core::Result<Vec<$crate::tags::ContentTagRecord>> {
                let sql = format!(
                    "SELECT content_type, content_id, tag_id, created_at FROM {} \
                     ORDER BY content_type ASC, content_id ASC, tag_id ASC;",
                    $crate::tags::CONTENT_TAG_TABLE
                );
                let db_err =
                    |e: sqlx::Error| aegis_core::AegisError::Database(format!("{}: {}", $label, e));
                let rows = sqlx::query(&sql)
                    .fetch_all(&self.pool)
                    .await
                    .map_err(db_err)?;
                let mut out = Vec::with_capacity(rows.len());
                for r in &rows {
                    out.push($crate::tags::content_tag_row_from_sqlx!(r));
                }
                Ok(out)
            }

            async fn insert_content_tag(
                &self,
                link: &$crate::tags::ContentTagRecord,
            ) -> aegis_core::Result<()> {
                let ph = $ph;
                let exists = format!(
                    "SELECT COUNT(*) FROM {} WHERE content_type = {} AND content_id = {} AND tag_id = {};",
                    $crate::tags::CONTENT_TAG_TABLE,
                    ph(1),
                    ph(2),
                    ph(3)
                );
                let db_err =
                    |e: sqlx::Error| aegis_core::AegisError::Database(format!("{}: {}", $label, e));
                let count: i64 = sqlx::query_scalar(&exists)
                    .bind(&link.content_type)
                    .bind(&link.content_id)
                    .bind(&link.tag_id)
                    .fetch_one(&self.pool)
                    .await
                    .map_err(db_err)?;
                if count > 0 {
                    return Ok(());
                }
                let insert = format!(
                    "INSERT INTO {} (content_type, content_id, tag_id, created_at) VALUES ({}, {}, {}, {});",
                    $crate::tags::CONTENT_TAG_TABLE,
                    ph(1),
                    ph(2),
                    ph(3),
                    ph(4)
                );
                sqlx::query(&insert)
                    .bind(&link.content_type)
                    .bind(&link.content_id)
                    .bind(&link.tag_id)
                    .bind(&link.created_at)
                    .execute(&self.pool)
                    .await
                    .map_err(db_err)?;
                Ok(())
            }
        }
    };
}
pub(crate) use impl_tag_storage;
