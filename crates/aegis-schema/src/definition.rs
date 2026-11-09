use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FieldType {
    String,
    Text,
    Integer,
    Boolean,
    Timestamp,
    Relation,
    Jsonb,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FieldDefinition {
    pub name: String,
    #[serde(rename = "type")]
    pub field_type: FieldType,
    #[serde(default)]
    pub required: bool,
    #[serde(default)]
    pub nullable: bool,
    #[serde(default)]
    pub unique: bool,
    #[serde(default)]
    pub index: bool,
    #[serde(default)]
    pub max_length: Option<usize>,
    #[serde(default)]
    pub target: Option<String>,
    #[serde(default)]
    pub indexed_paths: Vec<String>,
    /// 公開の配信 API（ヘッドレス）に出すか。既定は出す。内部向けのフィールドにだけ `false` を書く
    #[serde(default = "default_public")]
    pub public: bool,
}

fn default_public() -> bool {
    true
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SchemaDefinition {
    pub content_type: String,
    pub label: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub fields: Vec<FieldDefinition>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RelationTarget {
    pub content_type: String,
    pub field: String,
}

impl RelationTarget {
    pub fn parse(target: &str) -> Option<Self> {
        let parts: Vec<&str> = target.split('.').collect();
        match parts.as_slice() {
            [ct, field] => Some(Self {
                content_type: ct.to_string(),
                field: field.to_string(),
            }),
            [ct] => Some(Self {
                content_type: ct.to_string(),
                field: "id".to_string(),
            }),
            _ => None,
        }
    }
}

/// 全コンテンツ型が持つ組み込みのカテゴリ参照フィールド名
pub const CATEGORY_FIELD: &str = "category_id";

/// 全コンテンツ型が持つ組み込みの閲覧制限フィールド名（必要な会員レベル。docs/members.md 15）
pub const ACCESS_FIELD: &str = "access";

/// 全コンテンツ型が持つ組み込みの単独購入の価格フィールド名（通貨の最小単位。docs/members.md 16）。
/// 商品（ショップ）の `price` とは別の名前にして、商品型の定義を上書きしない
pub const PURCHASE_PRICE_FIELD: &str = "purchase_price";

impl FieldDefinition {
    /// 組み込みの `access`（必要な会員レベル。未設定・`public` は誰でも）。
    /// 閲覧の制限が YAML の書き忘れで外れないよう、全コンテンツ型に必ず持たせる。
    /// 値は公開してよい（ヘッドレス API で、クライアントが鍵の印を出すのに使う）
    pub fn builtin_access() -> Self {
        Self {
            name: ACCESS_FIELD.to_string(),
            field_type: FieldType::String,
            required: false,
            nullable: true,
            unique: false,
            index: false,
            max_length: Some(32),
            target: None,
            indexed_paths: Vec::new(),
            public: true,
        }
    }

    /// 組み込みの `purchase_price`（記事・ページを単独で買うときの価格。通貨の最小単位。未設定は単独では売らない）。
    /// `access` と同じく全コンテンツ型に持たせる。値は公開してよい（購入ボタンの金額を出すのに使う）
    pub fn builtin_purchase_price() -> Self {
        Self {
            name: PURCHASE_PRICE_FIELD.to_string(),
            field_type: FieldType::Integer,
            required: false,
            nullable: true,
            unique: false,
            index: false,
            max_length: None,
            target: None,
            indexed_paths: Vec::new(),
            public: true,
        }
    }

    /// 組み込みの `category_id`（カテゴリへのリレーション。未選択を許し、一覧の絞り込み用に索引を持つ）
    pub fn builtin_category() -> Self {
        Self {
            name: CATEGORY_FIELD.to_string(),
            field_type: FieldType::Relation,
            required: false,
            nullable: true,
            unique: false,
            index: true,
            max_length: None,
            target: Some("category.id".to_string()),
            indexed_paths: Vec::new(),
            public: true,
        }
    }
}

impl SchemaDefinition {
    /// 組み込みフィールド（`category_id`・`access`・`purchase_price`）を先頭に差し込む。
    /// YAML に同名の定義があっても組み込みの定義で置き換える（型ごとに食い違わせない）
    pub fn with_builtin_fields(mut self) -> Self {
        self.fields.retain(|f| {
            f.name != CATEGORY_FIELD && f.name != ACCESS_FIELD && f.name != PURCHASE_PRICE_FIELD
        });
        self.fields
            .insert(0, FieldDefinition::builtin_purchase_price());
        self.fields.insert(0, FieldDefinition::builtin_access());
        self.fields.insert(0, FieldDefinition::builtin_category());
        self
    }

    pub fn table_name(&self) -> String {
        format!("content_{}", self.content_type.replace('-', "_"))
    }

    /// Return list of fields that define relations to other content types along with parsed target
    pub fn relation_fields(&self) -> Vec<(&FieldDefinition, RelationTarget)> {
        self.fields
            .iter()
            .filter(|f| f.field_type == FieldType::Relation)
            .filter_map(|f| {
                f.target
                    .as_deref()
                    .and_then(RelationTarget::parse)
                    .map(|t| (f, t))
            })
            .collect()
    }
}

/// 全コンテンツ共通のシステム標準カラム・プロパティ名
pub const STANDARD_COLUMNS: &[&str] = &[
    "id",
    "content_type",
    "title",
    "slug",
    "status",
    "blocks",
    "blocks_json",
    "rendered_html",
    "author_id",
    "published_at",
    "unpublish_at",
    "created_at",
    "updated_at",
];

/// 指定されたフィールド名がシステム標準カラム・プロパティであるかを判定する
pub fn is_standard_column(name: &str) -> bool {
    STANDARD_COLUMNS.contains(&name)
}
