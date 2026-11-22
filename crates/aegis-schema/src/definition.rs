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
    /// 公開ページの URL の先頭（`/news` など。`/news/{slug}` で詳細ページを出す）。
    /// 省略時は `article` が `/articles`、`page` が `/`、それ以外は公開ページを持たない
    ///
    /// `auto`（YAML の `true` でも可）は、型名から `/<content_type>` を作る（読み込み時に置き換わる）。
    /// `false` は書かないのと同じ
    #[serde(default, deserialize_with = "deserialize_public_path")]
    pub public_path: Option<String>,
}

/// `public_path` の値の書き方: 文字列（`/news`・`auto`）、`true`（`auto` と同じ）、`false`（なし）
fn deserialize_public_path<'de, D>(deserializer: D) -> Result<Option<String>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Raw {
        Flag(bool),
        Path(String),
    }
    Ok(match Option::<Raw>::deserialize(deserializer)? {
        Some(Raw::Flag(true)) => Some(PUBLIC_PATH_AUTO.to_string()),
        Some(Raw::Path(path)) => Some(path),
        Some(Raw::Flag(false)) | None => None,
    })
}

/// `public_path` に型名から決めさせる指定
pub const PUBLIC_PATH_AUTO: &str = "auto";

/// `public_path` の先頭に使えない名前（サーバーや機能が先に使っている URL）
const RESERVED_PUBLIC_PATHS: &[&str] = &[
    "api",
    "uploads",
    "category",
    "tags",
    "products",
    "cart",
    "orders",
    "shop",
    "account",
    "forum",
    "theme",
    "themes",
    "feed.xml",
    "sitemap.xml",
    "robots.txt",
    "oembed",
    "metrics",
    "admin",
    "contact",
    "search",
];

impl SchemaDefinition {
    /// `public_path: auto` を `/<content_type>` に置き換える（検査の前に呼ぶ）
    pub fn resolve_public_path(mut self) -> Self {
        if self.public_path.as_deref() == Some(PUBLIC_PATH_AUTO) {
            self.public_path = Some(format!("/{}", self.content_type));
        }
        self
    }

    /// `public_path` の検査。`/` に続く 1 つの区切りだけ（小文字・数字・`-`・`_`）で、予約済みの名前や、
    /// 公開ページを別に持つ型（`page`・`category`）では使えない
    pub fn validate_public_path(&self) -> Result<(), String> {
        let Some(path) = self.public_path.as_deref() else {
            return Ok(());
        };
        if matches!(self.content_type.as_str(), "page" | "category") {
            return Err(format!(
                "schema '{}': public_path cannot be set (this type has its own public URL)",
                self.content_type
            ));
        }
        let name = path.strip_prefix('/').unwrap_or("");
        let valid = !name.is_empty()
            && name.len() <= 64
            && name.starts_with(|c: char| c.is_ascii_lowercase() || c.is_ascii_digit())
            && name
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '_');
        if !valid {
            return Err(format!(
                "schema '{}': public_path '{}' must be a single segment like \"/news\" (a-z, 0-9, '-', '_')",
                self.content_type, path
            ));
        }
        if RESERVED_PUBLIC_PATHS.contains(&name) || name.starts_with("wp-") {
            return Err(format!(
                "schema '{}': public_path '{}' is reserved by the server",
                self.content_type, path
            ));
        }
        Ok(())
    }
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

/// 全コンテンツ型が持つ組み込みの閲覧制限フィールド名（必要な会員レベル・保護方式。docs/members.md 15）
pub const ACCESS_FIELD: &str = "access";

/// 全コンテンツ型が持つ組み込みのパスワード（合言葉）保護のハッシュフィールド名（`access = "password"` のとき。公開 API には出さない）
pub const ACCESS_PASSWORD_FIELD: &str = "access_password";

/// 全コンテンツ型が持つ組み込みの単独購入の価格フィールド名（通貨の最小単位。docs/members.md 16）。
/// 商品（ショップ）の `price` とは別の名前にして、商品型の定義を上書きしない
pub const PURCHASE_PRICE_FIELD: &str = "purchase_price";

impl FieldDefinition {
    /// 組み込みの `access`（必要な会員レベル、または `"password"`。未設定・`public` は誰でも）。
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

    /// 組み込みの `access_password`（記事・ページのパスワード保護の Argon2id ハッシュ）。
    /// `public: false` にして、ヘッドレス配信 API や公開クエリには一切出さない
    pub fn builtin_access_password() -> Self {
        Self {
            name: ACCESS_PASSWORD_FIELD.to_string(),
            field_type: FieldType::String,
            required: false,
            nullable: true,
            unique: false,
            index: false,
            max_length: Some(255),
            target: None,
            indexed_paths: Vec::new(),
            public: false,
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
    /// 組み込みフィールド（`category_id`・`access`・`access_password`・`purchase_price`）を先頭に差し込む。
    /// YAML に同名の定義があっても組み込みの定義で置き換える（型ごとに食い違わせない）
    pub fn with_builtin_fields(mut self) -> Self {
        self.fields.retain(|f| {
            f.name != CATEGORY_FIELD
                && f.name != ACCESS_FIELD
                && f.name != ACCESS_PASSWORD_FIELD
                && f.name != PURCHASE_PRICE_FIELD
        });
        self.fields
            .insert(0, FieldDefinition::builtin_purchase_price());
        self.fields
            .insert(0, FieldDefinition::builtin_access_password());
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
