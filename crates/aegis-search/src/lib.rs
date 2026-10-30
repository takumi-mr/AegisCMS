//! サイト内検索（`[search]`）: 公開中のコンテンツの全文検索。
//! HTTP 層（API・検索ページ）と DB からの読み込みはサーバー側にあり、ここは索引と検索だけを扱う
//!
//! - 索引は tantivy。日本語は辞書を使わず 2 文字ずつに分けて索引する（[`tokenizer`]）。
//!   検索語は空白で区切った語をすべて含む文書（AND）を探し、語の中は語順どおりに続いているもの（フレーズ）だけを一致とする
//! - タイトルの一致は本文の一致より重く数える。並びは一致の良い順（既定）か、公開日時の新しい順
//! - カテゴリ（`category_id`）とコンテンツの種類で絞り込める
//! - 索引はデータから作り直せるもの（正は DB）。ディスクに置くと再起動後すぐ検索できるが、置かなくても動く

pub mod snippet;
pub mod text;
pub mod tokenizer;

use std::path::Path;
use std::sync::Mutex;

use serde::Serialize;
use tantivy::collector::{Count, TopDocs};
use tantivy::directory::MmapDirectory;
use tantivy::query::{BooleanQuery, BoostQuery, Occur, PhraseQuery, Query, TermQuery};
use tantivy::schema::{
    Field, IndexRecordOption, Schema, TextFieldIndexing, TextOptions, Value, FAST, STORED, STRING,
};
use tantivy::Order;
use tantivy::{Index, IndexReader, IndexWriter, ReloadPolicy, TantivyDocument, Term};

pub use snippet::snippet_html;
pub use text::blocks_text;
pub use tokenizer::{tokenize, CjkBigramTokenizer, TOKENIZER_NAME};

/// 検索語の長さの上限（文字数）
pub const MAX_QUERY_CHARS: usize = 200;
/// 検索語に入れられる語の数の上限
pub const MAX_QUERY_TERMS: usize = 10;
/// タイトルの一致の重み（本文は 1）
const TITLE_BOOST: f32 = 3.0;
/// 索引を書くときのメモリ（tantivy の下限は 15MB）
const WRITER_MEMORY_BYTES: usize = 30_000_000;

#[derive(Debug, thiserror::Error)]
pub enum SearchError {
    #[error("search index error: {0}")]
    Index(#[from] tantivy::TantivyError),
    #[error("search index directory error: {0}")]
    Directory(String),
}

pub type Result<T> = std::result::Result<T, SearchError>;

/// 索引に入れる 1 件
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchDocument {
    /// コンテンツの種類（`article`、`page` など）
    pub content_type: String,
    pub id: String,
    pub slug: String,
    pub title: String,
    /// 本文の文字（[`blocks_text`] など）
    pub body: String,
    /// 公開ページのパス（`/articles/hello` など）
    pub url: String,
    pub published_at: Option<String>,
    /// カテゴリの ID（`category_id` フィールド。無ければ `None`）
    pub category_id: Option<String>,
}

/// 索引の変更
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SearchChange {
    /// 追加するか、同じコンテンツがあれば置き換える
    Upsert(SearchDocument),
    /// 索引から除く（非公開・削除）
    Remove { content_type: String, id: String },
}

/// 検索結果の並び
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum SearchSort {
    /// 一致の良い順（タイトルの一致が先）
    #[default]
    Relevance,
    /// 公開日時の新しい順（同じ日時は不定）
    Newest,
}

impl SearchSort {
    /// API・検索ページのパラメーター（`relevance` / `newest`）から。知らない値は `None`
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "relevance" => Some(Self::Relevance),
            "newest" => Some(Self::Newest),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct SearchQuery<'a> {
    pub text: &'a str,
    /// 空ならすべての種類
    pub content_types: &'a [String],
    /// このカテゴリ（ID）のものだけ。`None` ならすべて
    pub category: Option<&'a str>,
    pub sort: SearchSort,
    pub limit: usize,
    pub offset: usize,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct SearchHit {
    pub content_type: String,
    pub id: String,
    pub slug: String,
    pub title: String,
    pub url: String,
    pub published_at: Option<String>,
    /// 本文の抜粋（HTML。エスケープ済みで、検索語は `<mark>`）
    pub snippet_html: String,
    /// 一致の度合い。公開日時の新しい順（`SearchSort::Newest`）のときは 0
    pub score: f32,
}

#[derive(Debug, Clone, Serialize, PartialEq, Default)]
pub struct SearchResults {
    /// 一致した件数（`hits` は `offset` からの `limit` 件）
    pub total: usize,
    pub hits: Vec<SearchHit>,
}

#[derive(Debug, Clone, Copy)]
struct Fields {
    key: Field,
    content_type: Field,
    id: Field,
    slug: Field,
    title: Field,
    body: Field,
    url: Field,
    published_at: Field,
    category_id: Field,
    /// 公開日時の UNIX 秒（新しい順の並びに使う。無ければ 0）
    published_ts: Field,
}

fn build_schema() -> (Schema, Fields) {
    let mut builder = Schema::builder();
    let text = TextOptions::default()
        .set_indexing_options(
            TextFieldIndexing::default()
                .set_tokenizer(TOKENIZER_NAME)
                .set_index_option(IndexRecordOption::WithFreqsAndPositions),
        )
        .set_stored();
    let fields = Fields {
        key: builder.add_text_field("key", STRING),
        content_type: builder.add_text_field("content_type", STRING | STORED),
        id: builder.add_text_field("id", STORED),
        slug: builder.add_text_field("slug", STORED),
        title: builder.add_text_field("title", text.clone()),
        body: builder.add_text_field("body", text),
        url: builder.add_text_field("url", STORED),
        published_at: builder.add_text_field("published_at", STORED),
        category_id: builder.add_text_field("category_id", STRING),
        published_ts: builder.add_i64_field("published_ts", FAST),
    };
    (builder.build(), fields)
}

/// 公開日時（RFC 3339）の UNIX 秒。無いか読めなければ 0
fn published_timestamp(published_at: Option<&str>) -> i64 {
    published_at
        .and_then(|value| chrono::DateTime::parse_from_rfc3339(value).ok())
        .map(|time| time.timestamp())
        .unwrap_or(0)
}

fn doc_key(content_type: &str, id: &str) -> String {
    format!("{}:{}", content_type, id)
}

/// 検索の索引。書き込みは 1 つずつ（内部で排他）、検索は並行してできる。
/// 書き込みはディスクへの書き出しを含むので、非同期の処理からは `spawn_blocking` で呼ぶ
pub struct SearchIndex {
    reader: IndexReader,
    writer: Mutex<IndexWriter<TantivyDocument>>,
    fields: Fields,
}

impl SearchIndex {
    /// メモリの中だけの索引
    pub fn in_memory() -> Result<Self> {
        let (schema, fields) = build_schema();
        Self::from_index(Index::create_in_ram(schema), fields)
    }

    /// `dir` の索引を開く（無ければ作る）。形式が変わって開けないときは、中を消して作り直す
    /// （索引は DB から作り直せるので、呼び出し側は空の索引に全件を入れ直す）
    pub fn open_in_dir(dir: &Path) -> Result<Self> {
        std::fs::create_dir_all(dir).map_err(|e| SearchError::Directory(e.to_string()))?;
        let (schema, fields) = build_schema();
        let open = |schema: Schema| -> Result<Index> {
            let directory =
                MmapDirectory::open(dir).map_err(|e| SearchError::Directory(e.to_string()))?;
            Ok(Index::open_or_create(directory, schema)?)
        };
        let index = match open(schema.clone()) {
            Ok(index) => index,
            Err(e) => {
                tracing::warn!(
                    "The search index in {} could not be opened ({}); recreating it",
                    dir.display(),
                    e
                );
                clear_dir(dir)?;
                open(schema)?
            }
        };
        Self::from_index(index, fields)
    }

    fn from_index(index: Index, fields: Fields) -> Result<Self> {
        index
            .tokenizers()
            .register(TOKENIZER_NAME, CjkBigramTokenizer);
        let writer = index.writer_with_num_threads(1, WRITER_MEMORY_BYTES)?;
        let reader = index
            .reader_builder()
            .reload_policy(ReloadPolicy::Manual)
            .try_into()?;
        Ok(Self {
            reader,
            writer: Mutex::new(writer),
            fields,
        })
    }

    /// 索引にある件数
    pub fn len(&self) -> u64 {
        self.reader.searcher().num_docs()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    fn to_document(&self, doc: &SearchDocument) -> TantivyDocument {
        let f = &self.fields;
        let mut out = TantivyDocument::default();
        out.add_text(f.key, doc_key(&doc.content_type, &doc.id));
        out.add_text(f.content_type, &doc.content_type);
        out.add_text(f.id, &doc.id);
        out.add_text(f.slug, &doc.slug);
        out.add_text(f.title, &doc.title);
        out.add_text(f.body, &doc.body);
        out.add_text(f.url, &doc.url);
        if let Some(published_at) = &doc.published_at {
            out.add_text(f.published_at, published_at);
        }
        if let Some(category_id) = doc.category_id.as_deref().filter(|c| !c.is_empty()) {
            out.add_text(f.category_id, category_id);
        }
        out.add_i64(
            f.published_ts,
            published_timestamp(doc.published_at.as_deref()),
        );
        out
    }

    fn lock_writer(&self) -> std::sync::MutexGuard<'_, IndexWriter<TantivyDocument>> {
        self.writer
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// 変更をまとめて書き込み、検索に反映する
    pub fn apply(&self, changes: &[SearchChange]) -> Result<()> {
        if changes.is_empty() {
            return Ok(());
        }
        let mut writer = self.lock_writer();
        for change in changes {
            let (content_type, id) = match change {
                SearchChange::Upsert(doc) => (&doc.content_type, &doc.id),
                SearchChange::Remove { content_type, id } => (content_type, id),
            };
            writer.delete_term(Term::from_field_text(
                self.fields.key,
                &doc_key(content_type, id),
            ));
            if let SearchChange::Upsert(doc) = change {
                writer.add_document(self.to_document(doc))?;
            }
        }
        writer.commit()?;
        self.reader.reload()?;
        Ok(())
    }

    /// 索引の中身を `docs` だけにする
    pub fn rebuild(&self, docs: &[SearchDocument]) -> Result<()> {
        let mut writer = self.lock_writer();
        writer.delete_all_documents()?;
        for doc in docs {
            writer.add_document(self.to_document(doc))?;
        }
        writer.commit()?;
        self.reader.reload()?;
        Ok(())
    }

    /// 検索語を tantivy の検索条件にする。探す語が無ければ `None`
    fn build_query(&self, query: &SearchQuery<'_>) -> Option<Box<dyn Query>> {
        let text: String = query.text.chars().take(MAX_QUERY_CHARS).collect();
        let mut clauses: Vec<(Occur, Box<dyn Query>)> = Vec::new();
        for word in text.split_whitespace().take(MAX_QUERY_TERMS) {
            let tokens = tokenize(word, false);
            if tokens.is_empty() {
                continue;
            }
            let field_query = |field: Field| -> Box<dyn Query> {
                if tokens.len() == 1 {
                    Box::new(TermQuery::new(
                        Term::from_field_text(field, &tokens[0].text),
                        IndexRecordOption::WithFreqs,
                    ))
                } else {
                    Box::new(PhraseQuery::new_with_offset(
                        tokens
                            .iter()
                            .map(|t| (t.position, Term::from_field_text(field, &t.text)))
                            .collect(),
                    ))
                }
            };
            let either: Box<dyn Query> = Box::new(BooleanQuery::new(vec![
                (
                    Occur::Should,
                    Box::new(BoostQuery::new(field_query(self.fields.title), TITLE_BOOST)),
                ),
                (Occur::Should, field_query(self.fields.body)),
            ]));
            clauses.push((Occur::Must, either));
        }
        if clauses.is_empty() {
            return None;
        }
        if !query.content_types.is_empty() {
            let types: Vec<(Occur, Box<dyn Query>)> = query
                .content_types
                .iter()
                .map(|t| -> (Occur, Box<dyn Query>) {
                    (
                        Occur::Should,
                        Box::new(TermQuery::new(
                            Term::from_field_text(self.fields.content_type, t),
                            IndexRecordOption::Basic,
                        )),
                    )
                })
                .collect();
            clauses.push((Occur::Must, Box::new(BooleanQuery::new(types))));
        }
        if let Some(category) = query.category.filter(|c| !c.is_empty()) {
            clauses.push((
                Occur::Must,
                Box::new(TermQuery::new(
                    Term::from_field_text(self.fields.category_id, category),
                    IndexRecordOption::Basic,
                )),
            ));
        }
        Some(Box::new(BooleanQuery::new(clauses)))
    }

    /// 検索する。並びは `query.sort`（既定は一致の良い順）
    pub fn search(&self, query: &SearchQuery<'_>) -> Result<SearchResults> {
        let Some(tantivy_query) = self.build_query(query) else {
            return Ok(SearchResults::default());
        };
        let searcher = self.reader.searcher();
        let limit = query.limit.max(1);
        let (top, total): (Vec<(f32, tantivy::DocAddress)>, usize) = match query.sort {
            SearchSort::Relevance => searcher.search(
                &tantivy_query,
                &(
                    TopDocs::with_limit(limit)
                        .and_offset(query.offset)
                        .order_by_score(),
                    Count,
                ),
            )?,
            SearchSort::Newest => {
                let (newest, total) = searcher.search(
                    &tantivy_query,
                    &(
                        TopDocs::with_limit(limit)
                            .and_offset(query.offset)
                            .order_by_fast_field::<i64>("published_ts", Order::Desc),
                        Count,
                    ),
                )?;
                (
                    newest
                        .into_iter()
                        .map(|(_, address)| (0.0, address))
                        .collect(),
                    total,
                )
            }
        };
        let f = &self.fields;
        let mut hits = Vec::with_capacity(top.len());
        for (score, address) in top {
            let doc: TantivyDocument = searcher.doc(address)?;
            let get = |field: Field| {
                doc.get_first(field)
                    .and_then(|v| v.as_str())
                    .unwrap_or_default()
                    .to_string()
            };
            let published_at = Some(get(f.published_at)).filter(|s| !s.is_empty());
            hits.push(SearchHit {
                content_type: get(f.content_type),
                id: get(f.id),
                slug: get(f.slug),
                title: get(f.title),
                url: get(f.url),
                published_at,
                snippet_html: snippet_html(&get(f.body), query.text),
                score,
            });
        }
        Ok(SearchResults { total, hits })
    }
}

fn clear_dir(dir: &Path) -> Result<()> {
    let entries = std::fs::read_dir(dir).map_err(|e| SearchError::Directory(e.to_string()))?;
    for entry in entries.flatten() {
        let path = entry.path();
        let removed = if path.is_dir() {
            std::fs::remove_dir_all(&path)
        } else {
            std::fs::remove_file(&path)
        };
        removed.map_err(|e| SearchError::Directory(e.to_string()))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests;
