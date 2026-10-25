use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PostContext {
    pub id: String,
    #[serde(default)]
    pub title: String,
    pub slug: String,
    pub author_id: String,
    pub blocks_json: String,
}

/// `on-content-published` フックに渡す、公開されたコンテンツの情報
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct PublishedContent {
    pub id: String,
    pub content_type: String,
    pub title: String,
    pub slug: String,
    pub published_at: String,
    #[serde(default)]
    pub author_id: Option<String>,
    #[serde(default, skip_serializing_if = "std::collections::HashMap::is_empty")]
    pub fields: std::collections::HashMap<String, serde_json::Value>,
}

impl PublishedContent {
    pub fn new(
        id: impl Into<String>,
        content_type: impl Into<String>,
        title: impl Into<String>,
        slug: impl Into<String>,
        published_at: impl Into<String>,
    ) -> Self {
        Self {
            id: id.into(),
            content_type: content_type.into(),
            title: title.into(),
            slug: slug.into(),
            published_at: published_at.into(),
            author_id: None,
            fields: std::collections::HashMap::new(),
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ContentSummary {
    pub id: String,
    pub content_type: String,
    pub title: String,
    pub slug: String,
    #[serde(default)]
    pub excerpt: String,
    pub author_id: Option<String>,
    pub published_at: Option<String>,
    pub created_at: String,
    #[serde(default, skip_serializing_if = "std::collections::HashMap::is_empty")]
    pub fields: std::collections::HashMap<String, serde_json::Value>,
    #[serde(default, skip_serializing_if = "std::collections::HashMap::is_empty")]
    pub relations: std::collections::HashMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PageRenderContext {
    pub site_title: String,
    pub site_description: String,
    pub template: String,
    pub content_type: String,
    pub title: String,
    pub slug: String,
    pub content_html: String,
    pub author: Option<String>,
    pub published_at: Option<String>,
    pub updated_at: Option<String>,
    #[serde(default)]
    pub fields: std::collections::HashMap<String, serde_json::Value>,
    #[serde(default)]
    pub relations: std::collections::HashMap<String, serde_json::Value>,
    #[serde(default)]
    pub items: Vec<ContentSummary>,
    #[serde(default)]
    pub extra_css: String,
    #[serde(default)]
    pub extra_head: String,
    #[serde(default)]
    pub theme_vars: std::collections::HashMap<String, String>,
    /// 一覧ページ（`index.html`）のページ送り。それ以外のページでは `None`
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pagination: Option<Pagination>,
    /// コメント欄（`[comments].content_types` の種類の公開ページ）。それ以外では `None`
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub comments: Option<crate::CommentSection>,
}

/// 一覧ページのページ送り情報（テンプレートの `pagination`）
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Pagination {
    /// 現在のページ（1 始まり）
    pub page: u64,
    pub per_page: u64,
    pub total_items: u64,
    pub total_pages: u64,
    /// 前・次のページの URL（`[server].base_path` 込み）。無ければ `None`
    pub prev_url: Option<String>,
    pub next_url: Option<String>,
}
