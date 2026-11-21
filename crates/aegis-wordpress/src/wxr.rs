//! WordPress の書き出しファイル（WXR。「ツール > エクスポート」で作る XML）の読み取り。
//!
//! ファイルは `<rss><channel>…</channel></rss>` の形で、`channel` の直下に、サイトの情報・著者
//! （`wp:author`）・カテゴリ（`wp:category`）・タグ（`wp:tag`）・その他の分類（`wp:term`）・投稿など
//! （`item`）が並ぶ。大きなサイトでもファイル全体を木にしないよう、`channel` の直下の 1 件ずつを
//! 読み取って捨てる。
//!
//! 読み取るだけで、AegisCMS の型への割り当て（投稿 → 記事など）はしない。

use std::collections::BTreeMap;

use jiff::tz::TimeZone;
use jiff::Timestamp;
use quick_xml::events::{BytesStart, Event};
use quick_xml::Reader;
use serde::{Deserialize, Serialize};

/// 読み取りに失敗した理由
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum WxrError {
    #[error("malformed XML: {0}")]
    Xml(String),
    #[error("this is not a WordPress export: {0}")]
    NotWordPress(String),
}

/// サイトの情報
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct WpSite {
    pub title: String,
    /// サイトの URL（`<link>`）
    pub link: String,
    pub description: String,
    pub language: String,
    /// `wp:base_site_url`（WordPress 本体の URL）
    pub base_site_url: String,
    /// `wp:base_blog_url`（ブログの URL。サブディレクトリに入れたサイトでは本体と違う）
    pub base_blog_url: String,
    pub wxr_version: String,
}

/// 著者（`wp:author`）
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct WpAuthor {
    pub id: u64,
    pub login: String,
    pub email: String,
    pub display_name: String,
    pub first_name: String,
    pub last_name: String,
}

/// カテゴリ・タグ・その他の分類の 1 件
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct WpTerm {
    pub id: u64,
    /// `category` / `post_tag` / 独自の分類の名前
    pub taxonomy: String,
    pub slug: String,
    pub name: String,
    /// 親の slug（無ければ空）
    pub parent: String,
    pub description: String,
}

/// 投稿に付いた分類（`<category domain="…" nicename="…">`）
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct WpTermRef {
    pub taxonomy: String,
    pub slug: String,
    pub name: String,
}

/// コメント（`wp:comment`）
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct WpComment {
    pub id: u64,
    pub author: String,
    pub author_email: String,
    pub author_url: String,
    pub author_ip: String,
    pub date: Option<Timestamp>,
    pub content: String,
    /// `1`（承認済み）・`0`（承認待ち）・`spam`・`trash`
    pub approved: String,
    /// 通常のコメントは空か `comment`。`pingback` / `trackback` など
    pub kind: String,
    /// 返信先のコメントの ID（0 は返信ではない）
    pub parent: u64,
    /// ログインしたユーザーの ID（0 は未ログイン）
    pub user_id: u64,
}

/// 投稿・固定ページ・添付ファイル・メニューの項目・独自の投稿タイプの 1 件（`item`）
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct WpItem {
    pub id: u64,
    pub title: String,
    /// 公開 URL（`<link>`）
    pub link: String,
    pub guid: String,
    /// `post`・`page`・`attachment`・`nav_menu_item`・独自の投稿タイプ
    pub post_type: String,
    /// `publish`・`draft`・`pending`・`private`・`future`・`inherit`（添付）・`trash`
    pub status: String,
    /// URL に使う名前（`wp:post_name`。下書きでは空のことがある）
    pub slug: String,
    /// 公開日時（UTC。下書きなど、決まっていないときは `None`）
    pub published_at: Option<Timestamp>,
    pub modified_at: Option<Timestamp>,
    /// 著者のログイン名（`dc:creator`）
    pub author: String,
    /// 本文（WordPress が保存している HTML。Gutenberg のコメントを含む）
    pub content: String,
    pub excerpt: String,
    /// 親の投稿の ID（添付ファイルは付いている投稿、固定ページの階層など。0 は親なし）
    pub parent: u64,
    pub menu_order: i64,
    /// 閲覧にパスワードが要る投稿のパスワード
    pub password: Option<String>,
    pub sticky: bool,
    pub comment_status: String,
    /// 添付ファイルの元のファイルの URL
    pub attachment_url: Option<String>,
    pub terms: Vec<WpTermRef>,
    /// カスタムフィールド（`wp:postmeta`。同じキーが複数あれば、後のものが残る）
    pub meta: BTreeMap<String, String>,
    pub comments: Vec<WpComment>,
}

impl WpItem {
    /// 分類（カテゴリ・タグなど）の slug。`taxonomy` は `category` / `post_tag`
    pub fn term_slugs<'a>(&'a self, taxonomy: &'a str) -> impl Iterator<Item = &'a str> + 'a {
        self.terms
            .iter()
            .filter(move |t| t.taxonomy == taxonomy)
            .map(|t| t.slug.as_str())
    }
}

/// 書き出しファイルの全体
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct WxrExport {
    pub site: WpSite,
    pub authors: Vec<WpAuthor>,
    pub categories: Vec<WpTerm>,
    pub tags: Vec<WpTerm>,
    /// その他の分類（`wp:term`。独自の分類、メニューなど）
    pub terms: Vec<WpTerm>,
    pub items: Vec<WpItem>,
}

/// 木の 1 要素（`channel` の直下の 1 件を読むためだけの小さな表現。名前は `wp:post_id` のように接頭辞付き）
#[derive(Debug, Default)]
struct Elem {
    name: String,
    attrs: Vec<(String, String)>,
    text: String,
    children: Vec<Elem>,
}

impl Elem {
    fn child(&self, name: &str) -> Option<&Elem> {
        self.children.iter().find(|c| c.name == name)
    }

    /// 子の本文（前後の空白は除く。無ければ空）
    fn text_of(&self, name: &str) -> String {
        self.child(name)
            .map(|c| c.text.trim().to_string())
            .unwrap_or_default()
    }

    /// 子の本文（前後の空白をそのまま残す。投稿の本文用）
    fn raw_text_of(&self, name: &str) -> String {
        self.child(name).map(|c| c.text.clone()).unwrap_or_default()
    }

    fn number_of(&self, name: &str) -> u64 {
        self.child(name)
            .and_then(|c| c.text.trim().parse().ok())
            .unwrap_or(0)
    }

    fn attr(&self, name: &str) -> &str {
        self.attrs
            .iter()
            .find(|(k, _)| k == name)
            .map_or("", |(_, v)| v.as_str())
    }
}

/// `0000-00-00 00:00:00`（未定）や読めない値は `None`。WXR の日時は `YYYY-MM-DD HH:MM:SS`
fn parse_date(value: &str) -> Option<Timestamp> {
    let value = value.trim();
    if value.is_empty() || value.starts_with("0000-00-00") {
        return None;
    }
    let civil = jiff::civil::DateTime::strptime("%Y-%m-%d %H:%M:%S", value).ok()?;
    TimeZone::UTC.to_timestamp(civil).ok()
}

fn start_elem(start: &BytesStart<'_>) -> Result<Elem, WxrError> {
    let name = start.name().as_ref().to_string();
    let mut attrs = Vec::new();
    for attr in start.attributes() {
        let attr = attr.map_err(|e| WxrError::Xml(e.to_string()))?;
        let key = attr.key.as_ref().to_string();
        let value = attr
            .normalized_value(quick_xml::XmlVersion::Implicit1_0)
            .map_err(|e| WxrError::Xml(e.to_string()))?
            .into_owned();
        attrs.push((key, value));
    }
    Ok(Elem {
        name,
        attrs,
        ..Elem::default()
    })
}

fn named_entity(name: &str) -> Option<char> {
    match name {
        "lt" => Some('<'),
        "gt" => Some('>'),
        "amp" => Some('&'),
        "apos" => Some('\''),
        "quot" => Some('"'),
        _ => None,
    }
}

/// 書き出しファイル（WXR）を読む
pub fn parse_wxr(xml: &str) -> Result<WxrExport, WxrError> {
    let mut reader = Reader::from_str(xml);
    let mut stack: Vec<Elem> = Vec::new();
    let mut export = WxrExport::default();
    let mut saw_channel = false;

    loop {
        let event = reader
            .read_event()
            .map_err(|e| WxrError::Xml(format!("at {}: {}", reader.error_position(), e)))?;
        match event {
            Event::Start(ref start) | Event::Empty(ref start) => {
                let elem = start_elem(start)?;
                if stack.is_empty() && elem.name != "rss" {
                    return Err(WxrError::NotWordPress(format!(
                        "the root element is <{}>, not <rss>",
                        elem.name
                    )));
                }
                if stack.len() == 1 && elem.name != "channel" {
                    return Err(WxrError::NotWordPress(format!(
                        "expected <channel>, found <{}>",
                        elem.name
                    )));
                }
                if stack.len() == 1 {
                    saw_channel = true;
                }
                stack.push(elem);
                if matches!(event, Event::Empty(_)) {
                    finish(&mut stack, &mut export);
                }
            }
            Event::End(_) => finish(&mut stack, &mut export),
            Event::Text(text) => {
                if let Some(elem) = stack.last_mut() {
                    elem.text.push_str(&text.xml10_content());
                }
            }
            Event::CData(data) => {
                if let Some(elem) = stack.last_mut() {
                    elem.text.push_str(&data.xml10_content());
                }
            }
            Event::GeneralRef(reference) => {
                if let Some(elem) = stack.last_mut() {
                    match reference.resolve_char_ref() {
                        Ok(Some(c)) => elem.text.push(c),
                        _ => {
                            let name: &str = &reference;
                            match named_entity(name) {
                                Some(c) => elem.text.push(c),
                                // 定義の無い実体参照は、そのまま残す
                                None => {
                                    elem.text.push('&');
                                    elem.text.push_str(name);
                                    elem.text.push(';');
                                }
                            }
                        }
                    }
                }
            }
            // WordPress の書き出しには DTD が無い。外部の実体を読み込むことはしない
            Event::DocType(_) => {
                return Err(WxrError::NotWordPress("DTDs are not supported".to_string()))
            }
            Event::Comment(_) | Event::Decl(_) | Event::PI(_) => {}
            Event::Eof => break,
        }
    }
    if !stack.is_empty() {
        return Err(WxrError::Xml("unclosed element".to_string()));
    }
    if !saw_channel {
        return Err(WxrError::NotWordPress("no <channel> found".to_string()));
    }
    Ok(export)
}

/// 閉じた要素を処理する。`channel` の直下の要素は取り込んで捨て、それ以外は親に付ける
fn finish(stack: &mut Vec<Elem>, export: &mut WxrExport) {
    let Some(elem) = stack.pop() else { return };
    if stack.len() == 2 {
        record(export, &elem);
    } else if let Some(parent) = stack.last_mut() {
        parent.children.push(elem);
    }
}

fn record(export: &mut WxrExport, elem: &Elem) {
    match elem.name.as_str() {
        "title" => export.site.title = elem.text.trim().to_string(),
        "link" => export.site.link = elem.text.trim().to_string(),
        "description" => export.site.description = elem.text.trim().to_string(),
        "language" => export.site.language = elem.text.trim().to_string(),
        "wp:base_site_url" => export.site.base_site_url = elem.text.trim().to_string(),
        "wp:base_blog_url" => export.site.base_blog_url = elem.text.trim().to_string(),
        "wp:wxr_version" => export.site.wxr_version = elem.text.trim().to_string(),
        "wp:author" => export.authors.push(WpAuthor {
            id: elem.number_of("wp:author_id"),
            login: elem.text_of("wp:author_login"),
            email: elem.text_of("wp:author_email"),
            display_name: elem.text_of("wp:author_display_name"),
            first_name: elem.text_of("wp:author_first_name"),
            last_name: elem.text_of("wp:author_last_name"),
        }),
        "wp:category" => export.categories.push(WpTerm {
            id: elem.number_of("wp:term_id"),
            taxonomy: "category".to_string(),
            slug: elem.text_of("wp:category_nicename"),
            name: elem.text_of("wp:cat_name"),
            parent: elem.text_of("wp:category_parent"),
            description: elem.text_of("wp:category_description"),
        }),
        "wp:tag" => export.tags.push(WpTerm {
            id: elem.number_of("wp:term_id"),
            taxonomy: "post_tag".to_string(),
            slug: elem.text_of("wp:tag_slug"),
            name: elem.text_of("wp:tag_name"),
            parent: String::new(),
            description: elem.text_of("wp:tag_description"),
        }),
        "wp:term" => export.terms.push(WpTerm {
            id: elem.number_of("wp:term_id"),
            taxonomy: elem.text_of("wp:term_taxonomy"),
            slug: elem.text_of("wp:term_slug"),
            name: elem.text_of("wp:term_name"),
            parent: elem.text_of("wp:term_parent"),
            description: elem.text_of("wp:term_description"),
        }),
        "item" => export.items.push(item(elem)),
        _ => {}
    }
}

fn item(elem: &Elem) -> WpItem {
    let mut terms = Vec::new();
    let mut meta = BTreeMap::new();
    let mut comments = Vec::new();
    for child in &elem.children {
        match child.name.as_str() {
            "category" => terms.push(WpTermRef {
                taxonomy: child.attr("domain").to_string(),
                slug: child.attr("nicename").to_string(),
                name: child.text.trim().to_string(),
            }),
            "wp:postmeta" => {
                let key = child.text_of("wp:meta_key");
                if !key.is_empty() {
                    meta.insert(key, child.raw_text_of("wp:meta_value"));
                }
            }
            "wp:comment" => comments.push(comment(child)),
            _ => {}
        }
    }
    let password = elem.text_of("wp:post_password");
    // 下書きなどで `post_date_gmt` が未定のときは、公開日時を決めない
    WpItem {
        id: elem.number_of("wp:post_id"),
        title: elem.text_of("title"),
        link: elem.text_of("link"),
        guid: elem.text_of("guid"),
        post_type: elem.text_of("wp:post_type"),
        status: elem.text_of("wp:status"),
        slug: elem.text_of("wp:post_name"),
        published_at: parse_date(&elem.text_of("wp:post_date_gmt")),
        modified_at: parse_date(&elem.text_of("wp:post_modified_gmt")),
        author: elem.text_of("dc:creator"),
        content: elem.raw_text_of("content:encoded"),
        excerpt: elem.raw_text_of("excerpt:encoded"),
        parent: elem.number_of("wp:post_parent"),
        menu_order: elem
            .child("wp:menu_order")
            .and_then(|c| c.text.trim().parse().ok())
            .unwrap_or(0),
        password: (!password.is_empty()).then_some(password),
        sticky: elem.text_of("wp:is_sticky") == "1",
        comment_status: elem.text_of("wp:comment_status"),
        attachment_url: Some(elem.text_of("wp:attachment_url")).filter(|u| !u.is_empty()),
        terms,
        meta,
        comments,
    }
}

fn comment(elem: &Elem) -> WpComment {
    WpComment {
        id: elem.number_of("wp:comment_id"),
        author: elem.text_of("wp:comment_author"),
        author_email: elem.text_of("wp:comment_author_email"),
        author_url: elem.text_of("wp:comment_author_url"),
        author_ip: elem.text_of("wp:comment_author_IP"),
        date: parse_date(&elem.text_of("wp:comment_date_gmt")),
        content: elem.raw_text_of("wp:comment_content"),
        approved: elem.text_of("wp:comment_approved"),
        kind: elem.text_of("wp:comment_type"),
        parent: elem.number_of("wp:comment_parent"),
        user_id: elem.number_of("wp:user_id"),
    }
}

/// 書き出しの中身の件数（取り込む前の確認用）
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct WxrSummary {
    pub authors: usize,
    pub categories: usize,
    pub tags: usize,
    /// 投稿タイプ → 状態 → 件数
    pub items: BTreeMap<String, BTreeMap<String, usize>>,
    /// 全コメント数（スパム・ゴミ箱を含む）
    pub comments: usize,
}

impl WxrExport {
    pub fn summary(&self) -> WxrSummary {
        let mut items: BTreeMap<String, BTreeMap<String, usize>> = BTreeMap::new();
        for item in &self.items {
            *items
                .entry(item.post_type.clone())
                .or_default()
                .entry(item.status.clone())
                .or_default() += 1;
        }
        WxrSummary {
            authors: self.authors.len(),
            categories: self.categories.len(),
            tags: self.tags.len(),
            items,
            comments: self.items.iter().map(|i| i.comments.len()).sum(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"<?xml version="1.0" encoding="UTF-8" ?>
<!-- generator="WordPress/6.8" created="2026-11-01 10:00" -->
<rss version="2.0"
	xmlns:excerpt="http://wordpress.org/export/1.2/excerpt/"
	xmlns:content="http://purl.org/rss/1.0/modules/content/"
	xmlns:dc="http://purl.org/dc/elements/1.1/"
	xmlns:wp="http://wordpress.org/export/1.2/">
<channel>
	<title>My &amp; Blog</title>
	<link>https://example.com</link>
	<description>Just another site</description>
	<language>ja</language>
	<wp:wxr_version>1.2</wp:wxr_version>
	<wp:base_site_url>https://example.com</wp:base_site_url>
	<wp:base_blog_url>https://example.com</wp:base_blog_url>
	<wp:author><wp:author_id>1</wp:author_id><wp:author_login><![CDATA[admin]]></wp:author_login><wp:author_email><![CDATA[admin@example.com]]></wp:author_email><wp:author_display_name><![CDATA[Admin User]]></wp:author_display_name><wp:author_first_name><![CDATA[]]></wp:author_first_name><wp:author_last_name><![CDATA[]]></wp:author_last_name></wp:author>
	<wp:category><wp:term_id>2</wp:term_id><wp:category_nicename><![CDATA[news]]></wp:category_nicename><wp:category_parent><![CDATA[]]></wp:category_parent><wp:cat_name><![CDATA[News]]></wp:cat_name></wp:category>
	<wp:category><wp:term_id>3</wp:term_id><wp:category_nicename><![CDATA[tech]]></wp:category_nicename><wp:category_parent><![CDATA[news]]></wp:category_parent><wp:cat_name><![CDATA[Tech]]></wp:cat_name><wp:category_description><![CDATA[Gadgets]]></wp:category_description></wp:category>
	<wp:tag><wp:term_id>5</wp:term_id><wp:tag_slug><![CDATA[rust]]></wp:tag_slug><wp:tag_name><![CDATA[Rust]]></wp:tag_name></wp:tag>
	<wp:term><wp:term_id>9</wp:term_id><wp:term_taxonomy><![CDATA[genre]]></wp:term_taxonomy><wp:term_slug><![CDATA[jazz]]></wp:term_slug><wp:term_parent><![CDATA[]]></wp:term_parent><wp:term_name><![CDATA[Jazz]]></wp:term_name></wp:term>
	<item>
		<title>Hello &amp; welcome</title>
		<link>https://example.com/2024/05/hello/</link>
		<pubDate>Wed, 01 May 2024 10:00:00 +0000</pubDate>
		<dc:creator><![CDATA[admin]]></dc:creator>
		<guid isPermaLink="false">https://example.com/?p=10</guid>
		<description></description>
		<content:encoded><![CDATA[<!-- wp:paragraph -->
<p>Hello <strong>world</strong> &amp; more.</p>
<!-- /wp:paragraph -->]]></content:encoded>
		<excerpt:encoded><![CDATA[A short note]]></excerpt:encoded>
		<wp:post_id>10</wp:post_id>
		<wp:post_date><![CDATA[2024-05-01 19:00:00]]></wp:post_date>
		<wp:post_date_gmt><![CDATA[2024-05-01 10:00:00]]></wp:post_date_gmt>
		<wp:post_modified_gmt><![CDATA[2024-05-02 08:30:00]]></wp:post_modified_gmt>
		<wp:comment_status><![CDATA[open]]></wp:comment_status>
		<wp:post_name><![CDATA[hello]]></wp:post_name>
		<wp:status><![CDATA[publish]]></wp:status>
		<wp:post_parent>0</wp:post_parent>
		<wp:menu_order>0</wp:menu_order>
		<wp:post_type><![CDATA[post]]></wp:post_type>
		<wp:post_password><![CDATA[]]></wp:post_password>
		<wp:is_sticky>1</wp:is_sticky>
		<category domain="category" nicename="tech"><![CDATA[Tech]]></category>
		<category domain="post_tag" nicename="rust"><![CDATA[Rust]]></category>
		<wp:postmeta><wp:meta_key><![CDATA[_thumbnail_id]]></wp:meta_key><wp:meta_value><![CDATA[11]]></wp:meta_value></wp:postmeta>
		<wp:postmeta><wp:meta_key><![CDATA[_yoast_wpseo_metadesc]]></wp:meta_key><wp:meta_value><![CDATA[Meta description]]></wp:meta_value></wp:postmeta>
		<wp:comment>
			<wp:comment_id>1</wp:comment_id>
			<wp:comment_author><![CDATA[Taro]]></wp:comment_author>
			<wp:comment_author_email><![CDATA[taro@example.com]]></wp:comment_author_email>
			<wp:comment_author_url>https://taro.example</wp:comment_author_url>
			<wp:comment_author_IP><![CDATA[203.0.113.5]]></wp:comment_author_IP>
			<wp:comment_date_gmt><![CDATA[2024-05-01 12:00:00]]></wp:comment_date_gmt>
			<wp:comment_content><![CDATA[Nice post!]]></wp:comment_content>
			<wp:comment_approved><![CDATA[1]]></wp:comment_approved>
			<wp:comment_type><![CDATA[comment]]></wp:comment_type>
			<wp:comment_parent>0</wp:comment_parent>
			<wp:user_id>0</wp:user_id>
		</wp:comment>
		<wp:comment>
			<wp:comment_id>2</wp:comment_id>
			<wp:comment_author><![CDATA[Admin]]></wp:comment_author>
			<wp:comment_date_gmt><![CDATA[2024-05-01 13:00:00]]></wp:comment_date_gmt>
			<wp:comment_content><![CDATA[Thanks!]]></wp:comment_content>
			<wp:comment_approved><![CDATA[1]]></wp:comment_approved>
			<wp:comment_parent>1</wp:comment_parent>
			<wp:user_id>1</wp:user_id>
		</wp:comment>
	</item>
	<item>
		<title>About</title>
		<link>https://example.com/about/</link>
		<dc:creator><![CDATA[admin]]></dc:creator>
		<guid isPermaLink="false">https://example.com/?page_id=20</guid>
		<content:encoded><![CDATA[About us.]]></content:encoded>
		<excerpt:encoded><![CDATA[]]></excerpt:encoded>
		<wp:post_id>20</wp:post_id>
		<wp:post_date_gmt><![CDATA[0000-00-00 00:00:00]]></wp:post_date_gmt>
		<wp:post_name><![CDATA[]]></wp:post_name>
		<wp:status><![CDATA[draft]]></wp:status>
		<wp:post_parent>0</wp:post_parent>
		<wp:menu_order>3</wp:menu_order>
		<wp:post_type><![CDATA[page]]></wp:post_type>
		<wp:post_password><![CDATA[secret]]></wp:post_password>
	</item>
	<item>
		<title>photo</title>
		<link>https://example.com/hello/photo/</link>
		<guid isPermaLink="false">https://example.com/wp-content/uploads/2024/05/photo.jpg</guid>
		<wp:post_id>11</wp:post_id>
		<wp:post_date_gmt><![CDATA[2024-05-01 09:00:00]]></wp:post_date_gmt>
		<wp:post_name><![CDATA[photo]]></wp:post_name>
		<wp:status><![CDATA[inherit]]></wp:status>
		<wp:post_parent>10</wp:post_parent>
		<wp:post_type><![CDATA[attachment]]></wp:post_type>
		<wp:attachment_url><![CDATA[https://example.com/wp-content/uploads/2024/05/photo.jpg]]></wp:attachment_url>
	</item>
</channel>
</rss>
"#;

    #[test]
    fn test_reads_the_site_authors_and_terms() {
        let export = parse_wxr(SAMPLE).unwrap();
        assert_eq!(export.site.title, "My & Blog");
        assert_eq!(export.site.link, "https://example.com");
        assert_eq!(export.site.language, "ja");
        assert_eq!(export.site.wxr_version, "1.2");

        assert_eq!(export.authors.len(), 1);
        assert_eq!(export.authors[0].login, "admin");
        assert_eq!(export.authors[0].display_name, "Admin User");
        assert_eq!(export.authors[0].email, "admin@example.com");

        assert_eq!(export.categories.len(), 2);
        assert_eq!(export.categories[1].slug, "tech");
        assert_eq!(export.categories[1].parent, "news");
        assert_eq!(export.categories[1].description, "Gadgets");
        assert_eq!(export.tags[0].name, "Rust");
        assert_eq!(export.terms[0].taxonomy, "genre");
        assert_eq!(export.terms[0].slug, "jazz");
    }

    #[test]
    fn test_reads_a_post_with_its_terms_meta_and_comments() {
        let export = parse_wxr(SAMPLE).unwrap();
        let post = &export.items[0];
        assert_eq!(post.id, 10);
        assert_eq!(post.title, "Hello & welcome");
        assert_eq!(post.post_type, "post");
        assert_eq!(post.status, "publish");
        assert_eq!(post.slug, "hello");
        assert_eq!(post.author, "admin");
        assert_eq!(post.link, "https://example.com/2024/05/hello/");
        assert_eq!(
            post.published_at.unwrap().to_string(),
            "2024-05-01T10:00:00Z"
        );
        assert_eq!(
            post.modified_at.unwrap().to_string(),
            "2024-05-02T08:30:00Z"
        );
        assert!(post.sticky);
        assert_eq!(post.password, None);
        assert!(post.content.contains("<!-- wp:paragraph -->"));
        assert!(post.content.contains("<strong>world</strong> &amp; more."));
        assert_eq!(post.excerpt, "A short note");
        assert_eq!(post.term_slugs("category").collect::<Vec<_>>(), ["tech"]);
        assert_eq!(post.term_slugs("post_tag").collect::<Vec<_>>(), ["rust"]);
        assert_eq!(post.meta["_thumbnail_id"], "11");
        assert_eq!(post.meta["_yoast_wpseo_metadesc"], "Meta description");

        assert_eq!(post.comments.len(), 2);
        let first = &post.comments[0];
        assert_eq!(first.author, "Taro");
        assert_eq!(first.author_email, "taro@example.com");
        assert_eq!(first.author_ip, "203.0.113.5");
        assert_eq!(first.approved, "1");
        assert_eq!(first.date.unwrap().to_string(), "2024-05-01T12:00:00Z");
        assert_eq!(first.content, "Nice post!");
        assert_eq!(post.comments[1].parent, 1);
        assert_eq!(post.comments[1].user_id, 1);
    }

    #[test]
    fn test_drafts_pages_and_attachments() {
        let export = parse_wxr(SAMPLE).unwrap();
        let page = &export.items[1];
        assert_eq!(page.post_type, "page");
        assert_eq!(page.status, "draft");
        assert_eq!(page.slug, "");
        // 未定の日時は公開日時にしない
        assert_eq!(page.published_at, None);
        assert_eq!(page.password.as_deref(), Some("secret"));
        assert_eq!(page.menu_order, 3);

        let attachment = &export.items[2];
        assert_eq!(attachment.post_type, "attachment");
        assert_eq!(attachment.parent, 10);
        assert_eq!(
            attachment.attachment_url.as_deref(),
            Some("https://example.com/wp-content/uploads/2024/05/photo.jpg")
        );
    }

    #[test]
    fn test_summary_counts_by_type_and_status() {
        let summary = parse_wxr(SAMPLE).unwrap().summary();
        assert_eq!(summary.authors, 1);
        assert_eq!(summary.categories, 2);
        assert_eq!(summary.tags, 1);
        assert_eq!(summary.comments, 2);
        assert_eq!(summary.items["post"]["publish"], 1);
        assert_eq!(summary.items["page"]["draft"], 1);
        assert_eq!(summary.items["attachment"]["inherit"], 1);
    }

    #[test]
    fn test_rejects_files_that_are_not_a_wordpress_export() {
        assert!(matches!(
            parse_wxr("<html><body>no</body></html>"),
            Err(WxrError::NotWordPress(_))
        ));
        assert!(matches!(
            parse_wxr("<rss><nochannel/></rss>"),
            Err(WxrError::NotWordPress(_))
        ));
        assert!(matches!(parse_wxr(""), Err(WxrError::NotWordPress(_))));
        assert!(matches!(
            parse_wxr("<rss><channel><title>x</title>"),
            Err(WxrError::Xml(_))
        ));
        assert!(matches!(
            parse_wxr("<!DOCTYPE rss [<!ENTITY a \"b\">]><rss><channel/></rss>"),
            Err(WxrError::NotWordPress(_))
        ));
    }

    #[test]
    fn test_keeps_unknown_entities_and_survives_odd_characters() {
        let xml = "<rss><channel><title>A &nbsp; B &#169;</title>\
            <item><wp:post_id>1</wp:post_id><title>x\u{1}y</title><wp:post_type>post</wp:post_type></item>\
            </channel></rss>";
        let export = parse_wxr(xml).unwrap();
        assert_eq!(export.site.title, "A &nbsp; B ©");
        assert_eq!(export.items.len(), 1);
    }
}
