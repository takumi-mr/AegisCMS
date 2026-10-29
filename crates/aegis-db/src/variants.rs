//! 商品バリエーションテーブル（`_aegis_product_variants`）とストレージ。
//! サイズ・色ごとの価格上書き（`price_override`）と在庫（`stock`）を管理する

use aegis_core::Result;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

/// 商品バリエーションのテーブル名
pub const PRODUCT_VARIANT_TABLE: &str = "_aegis_product_variants";

/// `_aegis_product_variants` の全カラム名
pub const PRODUCT_VARIANT_COLUMNS: [&str; 10] = [
    "id",
    "product_id",
    "sku",
    "title",
    "price_override",
    "stock",
    "attributes_json",
    "position",
    "created_at",
    "updated_at",
];

/// 商品バリエーション（`_aegis_product_variants` の 1 行）
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct ProductVariantRecord {
    pub id: String,
    pub product_id: String,
    pub sku: Option<String>,
    pub title: String,
    pub price_override: Option<i64>,
    pub stock: Option<i64>,
    pub attributes_json: String,
    pub position: i32,
    pub created_at: String,
    pub updated_at: String,
}

impl From<ProductVariantRecord> for aegis_core::ProductVariant {
    fn from(r: ProductVariantRecord) -> Self {
        let attributes: std::collections::HashMap<String, String> =
            serde_json::from_str(&r.attributes_json).unwrap_or_default();
        Self {
            id: r.id,
            product_id: r.product_id,
            sku: r.sku,
            title: r.title,
            price_override: r.price_override,
            stock: r.stock,
            attributes,
            position: r.position,
            created_at: r.created_at,
            updated_at: r.updated_at,
        }
    }
}

impl From<&ProductVariantRecord> for aegis_core::ProductVariant {
    fn from(r: &ProductVariantRecord) -> Self {
        let attributes: std::collections::HashMap<String, String> =
            serde_json::from_str(&r.attributes_json).unwrap_or_default();
        Self {
            id: r.id.clone(),
            product_id: r.product_id.clone(),
            sku: r.sku.clone(),
            title: r.title.clone(),
            price_override: r.price_override,
            stock: r.stock,
            attributes,
            position: r.position,
            created_at: r.created_at.clone(),
            updated_at: r.updated_at.clone(),
        }
    }
}

impl From<aegis_core::ProductVariant> for ProductVariantRecord {
    fn from(v: aegis_core::ProductVariant) -> Self {
        let attributes_json =
            serde_json::to_string(&v.attributes).unwrap_or_else(|_| "{}".to_string());
        Self {
            id: v.id,
            product_id: v.product_id,
            sku: v.sku,
            title: v.title,
            price_override: v.price_override,
            stock: v.stock,
            attributes_json,
            position: v.position,
            created_at: v.created_at,
            updated_at: v.updated_at,
        }
    }
}

impl From<&aegis_core::ProductVariant> for ProductVariantRecord {
    fn from(v: &aegis_core::ProductVariant) -> Self {
        let attributes_json =
            serde_json::to_string(&v.attributes).unwrap_or_else(|_| "{}".to_string());
        Self {
            id: v.id.clone(),
            product_id: v.product_id.clone(),
            sku: v.sku.clone(),
            title: v.title.clone(),
            price_override: v.price_override,
            stock: v.stock,
            attributes_json,
            position: v.position,
            created_at: v.created_at.clone(),
            updated_at: v.updated_at.clone(),
        }
    }
}

/// 商品バリエーションのストレージトレイト
#[async_trait]
pub trait ProductVariantStorage: Send + Sync {
    /// 商品に属する全バリエーションを position 順（同順は created_at 順）で取得
    async fn list_product_variants(&self, product_id: &str) -> Result<Vec<ProductVariantRecord>>;

    /// 単一バリエーションの取得
    async fn get_product_variant(&self, id: &str) -> Result<Option<ProductVariantRecord>>;

    /// SKU によるバリエーションの取得
    async fn get_product_variant_by_sku(&self, sku: &str) -> Result<Option<ProductVariantRecord>>;

    /// バリエーションの作成または更新（存在しなければ INSERT、あれば UPDATE）
    async fn upsert_product_variant(&self, variant: &ProductVariantRecord) -> Result<()>;

    /// バリエーションの削除
    async fn delete_product_variant(&self, id: &str) -> Result<bool>;

    /// 指定商品の全バリエーションの削除（商品削除時に呼び出す）
    async fn delete_product_variants_by_product_id(&self, product_id: &str) -> Result<u64>;

    /// 全バリエーションの取得（バックアップ・ダンプ用）
    async fn list_all_product_variants(&self) -> Result<Vec<ProductVariantRecord>> {
        Ok(Vec::new())
    }
}

/// テーブル定義 SQL。MySQL は inline_indexes = true、SQLite / PostgreSQL は false
pub fn build_product_variant_table(
    text: &str,
    varchar: &str,
    bigint: &str,
    inline_indexes: bool,
) -> (String, Vec<String>) {
    let indexes = [
        ("idx_product_variants_product_id", "product_id"),
        ("idx_product_variants_sku", "sku"),
    ];

    let inline = if inline_indexes {
        format!(
            ",\n    KEY {} ({}),\n    UNIQUE KEY {} ({})",
            indexes[0].0, indexes[0].1, indexes[1].0, indexes[1].1
        )
    } else {
        String::new()
    };

    let table = format!(
        "CREATE TABLE IF NOT EXISTS {table} (\n\
         \x20   id {varchar} PRIMARY KEY,\n\
         \x20   product_id {varchar} NOT NULL,\n\
         \x20   sku {varchar},\n\
         \x20   title {varchar} NOT NULL,\n\
         \x20   price_override {bigint},\n\
         \x20   stock {bigint},\n\
         \x20   attributes_json {text} NOT NULL,\n\
         \x20   position INTEGER NOT NULL DEFAULT 0,\n\
         \x20   created_at {varchar} NOT NULL,\n\
         \x20   updated_at {varchar} NOT NULL{inline}\n\
         );",
        table = PRODUCT_VARIANT_TABLE,
        varchar = varchar,
        text = text,
        bigint = bigint,
        inline = inline,
    );

    let separate = if inline_indexes {
        Vec::new()
    } else {
        vec![
            format!(
                "CREATE INDEX IF NOT EXISTS idx_product_variants_product_id ON {} (product_id);",
                PRODUCT_VARIANT_TABLE
            ),
            format!(
                "CREATE INDEX IF NOT EXISTS idx_product_variants_sku ON {} (sku);",
                PRODUCT_VARIANT_TABLE
            ),
        ]
    };

    (table, separate)
}

/// バリエーションの INSERT 文
pub fn build_variant_insert(placeholder: impl Fn(usize) -> String) -> String {
    let placeholders: Vec<String> = (1..=PRODUCT_VARIANT_COLUMNS.len())
        .map(placeholder)
        .collect();
    format!(
        "INSERT INTO {} ({}) VALUES ({});",
        PRODUCT_VARIANT_TABLE,
        PRODUCT_VARIANT_COLUMNS.join(", "),
        placeholders.join(", ")
    )
}

/// バリエーションの UPDATE 文（id を除いたカラムを更新）
pub fn build_variant_update(placeholder: impl Fn(usize) -> String) -> String {
    // 1: sku, 2: title, 3: price_override, 4: stock, 5: attributes_json, 6: position, 7: updated_at, 8: id
    format!(
        "UPDATE {} SET sku = {}, title = {}, price_override = {}, stock = {}, \
         attributes_json = {}, position = {}, updated_at = {} WHERE id = {};",
        PRODUCT_VARIANT_TABLE,
        placeholder(1),
        placeholder(2),
        placeholder(3),
        placeholder(4),
        placeholder(5),
        placeholder(6),
        placeholder(7),
        placeholder(8)
    )
}

/// バリエーションの SELECT 文
pub fn build_variant_select(where_clause: Option<&str>) -> String {
    format!(
        "SELECT {} FROM {}{} ORDER BY position ASC, created_at ASC, id ASC;",
        PRODUCT_VARIANT_COLUMNS.join(", "),
        PRODUCT_VARIANT_TABLE,
        where_clause
            .map(|w| format!(" WHERE {}", w))
            .unwrap_or_default()
    )
}

/// sqlx の行を `ProductVariantRecord` に変換するマクロ
macro_rules! variant_row_from_sqlx {
    ($row:expr) => {{
        let row = $row;
        let get = |e: sqlx::Error| aegis_core::AegisError::Database(e.to_string());
        $crate::variants::ProductVariantRecord {
            id: row.try_get("id").map_err(get)?,
            product_id: row.try_get("product_id").map_err(get)?,
            sku: row.try_get("sku").map_err(get).unwrap_or(None),
            title: row.try_get("title").map_err(get)?,
            price_override: row.try_get("price_override").map_err(get).unwrap_or(None),
            stock: row.try_get("stock").map_err(get).unwrap_or(None),
            attributes_json: row
                .try_get("attributes_json")
                .map_err(get)
                .unwrap_or_else(|_| "{}".to_string()),
            position: row.try_get("position").map_err(get).unwrap_or(0),
            created_at: row.try_get("created_at").map_err(get)?,
            updated_at: row.try_get("updated_at").map_err(get)?,
        }
    }};
}
pub(crate) use variant_row_from_sqlx;

/// 各 DB アダプタに `ProductVariantStorage` を実装するマクロ
macro_rules! impl_product_variant_storage {
    ($adapter:ty, $ph:expr, $label:literal) => {
        #[async_trait::async_trait]
        impl $crate::variants::ProductVariantStorage for $adapter {
            async fn list_product_variants(
                &self,
                product_id: &str,
            ) -> aegis_core::Result<Vec<$crate::variants::ProductVariantRecord>> {
                let ph = $ph;
                let sql = $crate::variants::build_variant_select(Some(&format!(
                    "product_id = {}",
                    ph(1)
                )));
                let db_err =
                    |e: sqlx::Error| aegis_core::AegisError::Database(format!("{}: {}", $label, e));
                let rows = sqlx::query(&sql)
                    .bind(product_id)
                    .fetch_all(&self.pool)
                    .await
                    .map_err(db_err)?;
                let mut list = Vec::with_capacity(rows.len());
                for r in &rows {
                    list.push($crate::variants::variant_row_from_sqlx!(r));
                }
                Ok(list)
            }

            async fn get_product_variant(
                &self,
                id: &str,
            ) -> aegis_core::Result<Option<$crate::variants::ProductVariantRecord>> {
                let ph = $ph;
                let sql = $crate::variants::build_variant_select(Some(&format!("id = {}", ph(1))));
                let db_err =
                    |e: sqlx::Error| aegis_core::AegisError::Database(format!("{}: {}", $label, e));
                let row = sqlx::query(&sql)
                    .bind(id)
                    .fetch_optional(&self.pool)
                    .await
                    .map_err(db_err)?;
                match row {
                    Some(r) => Ok(Some($crate::variants::variant_row_from_sqlx!(&r))),
                    None => Ok(None),
                }
            }

            async fn get_product_variant_by_sku(
                &self,
                sku: &str,
            ) -> aegis_core::Result<Option<$crate::variants::ProductVariantRecord>> {
                let ph = $ph;
                let sql = $crate::variants::build_variant_select(Some(&format!("sku = {}", ph(1))));
                let db_err =
                    |e: sqlx::Error| aegis_core::AegisError::Database(format!("{}: {}", $label, e));
                let row = sqlx::query(&sql)
                    .bind(sku)
                    .fetch_optional(&self.pool)
                    .await
                    .map_err(db_err)?;
                match row {
                    Some(r) => Ok(Some($crate::variants::variant_row_from_sqlx!(&r))),
                    None => Ok(None),
                }
            }

            async fn upsert_product_variant(
                &self,
                v: &$crate::variants::ProductVariantRecord,
            ) -> aegis_core::Result<()> {
                let db_err =
                    |e: sqlx::Error| aegis_core::AegisError::Database(format!("{}: {}", $label, e));
                let existing = self.get_product_variant(&v.id).await?;
                if existing.is_some() {
                    let update_sql = $crate::variants::build_variant_update($ph);
                    sqlx::query(&update_sql)
                        .bind(&v.sku)
                        .bind(&v.title)
                        .bind(v.price_override)
                        .bind(v.stock)
                        .bind(&v.attributes_json)
                        .bind(v.position)
                        .bind(&v.updated_at)
                        .bind(&v.id)
                        .execute(&self.pool)
                        .await
                        .map_err(db_err)?;
                } else {
                    let insert_sql = $crate::variants::build_variant_insert($ph);
                    sqlx::query(&insert_sql)
                        .bind(&v.id)
                        .bind(&v.product_id)
                        .bind(&v.sku)
                        .bind(&v.title)
                        .bind(v.price_override)
                        .bind(v.stock)
                        .bind(&v.attributes_json)
                        .bind(v.position)
                        .bind(&v.created_at)
                        .bind(&v.updated_at)
                        .execute(&self.pool)
                        .await
                        .map_err(db_err)?;
                }
                Ok(())
            }

            async fn delete_product_variant(&self, id: &str) -> aegis_core::Result<bool> {
                let ph = $ph;
                let sql = format!(
                    "DELETE FROM {} WHERE id = {};",
                    $crate::variants::PRODUCT_VARIANT_TABLE,
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

            async fn delete_product_variants_by_product_id(
                &self,
                product_id: &str,
            ) -> aegis_core::Result<u64> {
                let ph = $ph;
                let sql = format!(
                    "DELETE FROM {} WHERE product_id = {};",
                    $crate::variants::PRODUCT_VARIANT_TABLE,
                    ph(1)
                );
                let db_err =
                    |e: sqlx::Error| aegis_core::AegisError::Database(format!("{}: {}", $label, e));
                let res = sqlx::query(&sql)
                    .bind(product_id)
                    .execute(&self.pool)
                    .await
                    .map_err(db_err)?;
                Ok(res.rows_affected())
            }

            async fn list_all_product_variants(
                &self,
            ) -> aegis_core::Result<Vec<$crate::variants::ProductVariantRecord>> {
                let sql = $crate::variants::build_variant_select(None);
                let db_err =
                    |e: sqlx::Error| aegis_core::AegisError::Database(format!("{}: {}", $label, e));
                let rows = sqlx::query(&sql)
                    .fetch_all(&self.pool)
                    .await
                    .map_err(db_err)?;
                let mut list = Vec::with_capacity(rows.len());
                for r in &rows {
                    list.push($crate::variants::variant_row_from_sqlx!(r));
                }
                Ok(list)
            }
        }
    };
}
pub(crate) use impl_product_variant_storage;
