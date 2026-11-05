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

impl FieldDefinition {
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
    /// 組み込みフィールド（`category_id`）を先頭に差し込む。
    /// YAML に同名の定義があっても組み込みの定義で置き換える（型ごとに食い違わせない）
    pub fn with_builtin_fields(mut self) -> Self {
        self.fields.retain(|f| f.name != CATEGORY_FIELD);
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
