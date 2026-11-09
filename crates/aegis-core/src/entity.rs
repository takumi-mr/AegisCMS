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
    /// 本文（`content_html`）の前に足す HTML。`on-render-page` プラグインの書き込み専用で、
    /// プラグインには常に空で渡される。返された分をホストが貯め（後のプラグインのものが外側）、
    /// 全プラグインのあとで 1 回だけ本文と結合する。結合後は空
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub prepend_content: String,
    /// 本文の後ろに足す HTML。仕組みは `prepend_content` と同じ（先のプラグインのものが内側）
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub append_content: String,
    /// 本文（`content_html`）のバイト数。ホストが各プラグインの呼び出しの前に計算する。
    /// 本文を受け取らないプラグインが「本文があるか」を知るための値
    #[serde(default)]
    pub content_length: usize,
    pub author: Option<String>,
    pub published_at: Option<String>,
    pub updated_at: Option<String>,
    #[serde(default)]
    pub fields: std::collections::HashMap<String, serde_json::Value>,
    #[serde(default)]
    pub relations: std::collections::HashMap<String, serde_json::Value>,
    #[serde(default)]
    pub items: Vec<ContentSummary>,
    /// 記事詳細ページ等の関連記事
    #[serde(default)]
    pub related_articles: Vec<ContentSummary>,
    /// 直前の記事（詳細ページ等）。無ければ `None`
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prev_article: Option<ContentSummary>,
    /// 直後の記事（詳細ページ等）。無ければ `None`
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next_article: Option<ContentSummary>,
    #[serde(default)]
    pub extra_css: String,
    #[serde(default)]
    pub extra_head: String,
    /// `<head>` に足すスクリプト（構造化した注入。docs/csp.md の 4）。`on-render-page` プラグインの書き込み専用で、
    /// プラグインには常に空で渡される。ホストが検査して `<script>` のタグにし、全プラグインのあとで
    /// `extra_head` の後ろに足す。`extra_head` などの HTML の中の `<script>` は、ホストが取り除く
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub scripts: Vec<crate::InjectedScript>,
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
