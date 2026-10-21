use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fmt;

/// 記事保存時にプラグインへ渡されるコンテキスト
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct PostContext {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub title: String,
    pub slug: String,
    #[serde(default)]
    pub author_id: String,
    #[serde(default)]
    pub blocks_json: String,
}

impl PostContext {
    pub fn new(title: impl Into<String>, slug: impl Into<String>) -> Self {
        Self {
            id: String::new(),
            title: title.into(),
            slug: slug.into(),
            author_id: String::new(),
            blocks_json: "[]".to_string(),
        }
    }

    /// blocks_json を BlockNode のベクタとしてパースする
    pub fn blocks(&self) -> Result<Vec<BlockNode>> {
        if self.blocks_json.is_empty() {
            return Ok(Vec::new());
        }
        serde_json::from_str(&self.blocks_json).map_err(Into::into)
    }

    /// BlockNode のベクタを JSON 文字列として blocks_json に格納する
    pub fn set_blocks(&mut self, blocks: &[BlockNode]) -> Result<()> {
        self.blocks_json = serde_json::to_string(blocks)?;
        Ok(())
    }
}

/// 構造化コンテンツの木構造ブロックノード (AST)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BlockNode {
    pub name: String,
    #[serde(default)]
    pub attributes: HashMap<String, serde_json::Value>,
    #[serde(default)]
    pub text: Option<String>,
    #[serde(default)]
    pub children: Vec<BlockNode>,
}

impl BlockNode {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            attributes: HashMap::new(),
            text: None,
            children: Vec::new(),
        }
    }

    pub fn paragraph(text: impl Into<String>) -> Self {
        Self {
            name: "core/paragraph".to_string(),
            attributes: HashMap::new(),
            text: Some(text.into()),
            children: Vec::new(),
        }
    }

    pub fn heading(level: u8, text: impl Into<String>) -> Self {
        let mut attributes = HashMap::new();
        attributes.insert("level".to_string(), serde_json::json!(level));
        Self {
            name: "core/heading".to_string(),
            attributes,
            text: Some(text.into()),
            children: Vec::new(),
        }
    }

    pub fn code(language: impl Into<String>, code: impl Into<String>) -> Self {
        let mut attributes = HashMap::new();
        attributes.insert("language".to_string(), serde_json::json!(language.into()));
        Self {
            name: "core/code".to_string(),
            attributes,
            text: Some(code.into()),
            children: Vec::new(),
        }
    }

    pub fn attribute(&self, key: &str) -> Option<&serde_json::Value> {
        self.attributes.get(key)
    }

    pub fn set_attribute(&mut self, key: impl Into<String>, value: serde_json::Value) {
        self.attributes.insert(key.into(), value);
    }
}

/// プラグイン処理中のエラー型
#[derive(Debug, Clone, PartialEq)]
pub enum PluginError {
    Execution(String),
    Serialization(String),
    Host(String),
}

impl fmt::Display for PluginError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Execution(msg) => write!(f, "Execution error: {}", msg),
            Self::Serialization(msg) => write!(f, "Serialization error: {}", msg),
            Self::Host(msg) => write!(f, "Host API error: {}", msg),
        }
    }
}

impl std::error::Error for PluginError {}

impl From<serde_json::Error> for PluginError {
    fn from(err: serde_json::Error) -> Self {
        Self::Serialization(err.to_string())
    }
}

pub type Result<T> = std::result::Result<T, PluginError>;

/// AegisCMS プラグインが実装するコアトレイト
pub trait Plugin: Default {
    /// 記事保存前フック (`on-before-save`)
    /// 記事の検証、タイトルの整形、自動タグ付け、監査ログ送信などを実行可能
    fn on_before_save(&mut self, post: PostContext) -> Result<PostContext> {
        Ok(post)
    }

    /// コンテンツ AST 変換フック (`transform-content-ast`)
    /// 見出しへのアンカー自動付与、ショートコード展開、シンタックスハイライト加工などを実行可能
    fn transform_content_ast(&mut self, blocks: Vec<BlockNode>) -> Result<Vec<BlockNode>> {
        Ok(blocks)
    }
}
