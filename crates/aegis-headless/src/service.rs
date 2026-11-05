//! 一覧と 1 件の取得。公開済みのものだけを返す。

use std::collections::{BTreeSet, HashMap};

use aegis_db::{ContentListQuery, ContentRecord, ResolvedRelation};
use aegis_schema::SchemaDefinition;
use chrono::{DateTime, Utc};
use serde::Serialize;
use serde_json::Value;
use sha2::{Digest, Sha256};

use crate::error::HeadlessError;
use crate::params::{CategoryFilter, Cursor, ListParams};
use crate::source::PublishedSource;
use crate::view::project;

/// 配信 API の設定（サーバーの `[headless]` から作る）
#[derive(Debug, Clone)]
pub struct HeadlessConfig {
    /// 公開する型。ここに無い型は存在しないものとして扱う
    pub content_types: Vec<String>,
    /// 1 ページの件数の上限
    pub max_page_size: u32,
}

impl Default for HeadlessConfig {
    fn default() -> Self {
        Self {
            content_types: vec!["article".to_string(), "page".to_string()],
            max_page_size: 100,
        }
    }
}

/// 一覧の応答
#[derive(Debug, Clone, Serialize)]
pub struct ListResponse {
    pub items: Vec<Value>,
    pub next_cursor: Option<String>,
    pub total: u64,
}

/// 応答の本文と、キャッシュの検証に使う `ETag`
#[derive(Debug, Clone)]
pub struct Delivered<T> {
    pub body: T,
    pub etag: String,
}

/// 公開中か。状態が `published` で、公開期間（`unpublish_at`）を過ぎていない。
/// スケジューラのアーカイブ化を待たずに、リクエスト時に確かめる。読めない日時は公開していないものとして扱う
fn is_live(record: &ContentRecord, now: DateTime<Utc>) -> bool {
    if record.status != "published" {
        return false;
    }
    match &record.unpublish_at {
        None => true,
        Some(raw) => DateTime::parse_from_rfc3339(raw)
            .map(|at| at.with_timezone(&Utc) > now)
            .unwrap_or(false),
    }
}

fn schema_for<'a>(
    config: &HeadlessConfig,
    schemas: &'a HashMap<String, SchemaDefinition>,
    content_type: &str,
) -> Result<&'a SchemaDefinition, HeadlessError> {
    if !config.content_types.iter().any(|t| t == content_type) {
        return Err(HeadlessError::NotFound);
    }
    schemas.get(content_type).ok_or(HeadlessError::NotFound)
}

/// 公開してよいリレーションの参照先を、公開中のものだけ引く。
/// 参照先の型が公開する型に入っているかは問わない（返すのは `id`・`content_type`・`title`・`slug` だけ）
async fn resolve_relations<S: PublishedSource + ?Sized>(
    source: &S,
    schemas: &HashMap<String, SchemaDefinition>,
    schema: &SchemaDefinition,
    records: &[&ContentRecord],
) -> Result<Vec<HashMap<String, ResolvedRelation>>, HeadlessError> {
    let mut resolved = vec![HashMap::new(); records.len()];
    for (field, target) in schema.relation_fields() {
        if !field.public {
            continue;
        }
        let Some(target_schema) = schemas.get(&target.content_type) else {
            continue;
        };
        let refs: Vec<Option<&str>> = records
            .iter()
            .map(|record| {
                record
                    .custom_fields
                    .get(&field.name)
                    .and_then(Value::as_str)
                    .map(str::trim)
                    .filter(|id| !id.is_empty())
            })
            .collect();
        let ids: BTreeSet<String> = refs.iter().flatten().map(|id| id.to_string()).collect();
        if ids.is_empty() {
            continue;
        }
        let ids: Vec<String> = ids.into_iter().collect();
        let found = source
            .resolve_published_relations(target_schema, &target.field, &ids)
            .await?;
        for (relations, id) in resolved.iter_mut().zip(&refs) {
            if let Some(relation) = id.and_then(|id| found.get(id)) {
                relations.insert(field.name.clone(), relation.clone());
            }
        }
    }
    Ok(resolved)
}

fn etag(parts: impl IntoIterator<Item = String>) -> String {
    let mut hasher = Sha256::new();
    for part in parts {
        hasher.update(part.as_bytes());
        hasher.update([0u8]);
    }
    let digest = hasher.finalize();
    let hex: String = digest.iter().take(8).map(|b| format!("{b:02x}")).collect();
    format!("W/\"{hex}\"")
}

/// 公開済みの一覧（作成日の新しい順）
pub async fn list<S: PublishedSource + ?Sized>(
    source: &S,
    config: &HeadlessConfig,
    schemas: &HashMap<String, SchemaDefinition>,
    content_type: &str,
    params: &ListParams,
    now: DateTime<Utc>,
) -> Result<Delivered<ListResponse>, HeadlessError> {
    let schema = schema_for(config, schemas, content_type)?;
    let query = params.validate(config.max_page_size)?;

    let tag_id = match &query.tag {
        None => None,
        Some(slug) => match source.tag_id_by_slug(slug).await? {
            Some(id) => Some(id),
            // 存在しないタグ: 一致するものが無い（エラーにはしない）
            None => {
                return Ok(Delivered {
                    etag: etag(["empty".to_string()]),
                    body: ListResponse {
                        items: Vec::new(),
                        next_cursor: None,
                        total: 0,
                    },
                })
            }
        },
    };
    let filters = match &query.category {
        None => Vec::new(),
        Some(CategoryFilter::None) => vec![("category_id".to_string(), None)],
        Some(CategoryFilter::Id(id)) => vec![("category_id".to_string(), Some(id.clone()))],
    };

    // 1 件多く読んで、続きがあるかを知る
    let page = source
        .list_published(
            schema,
            &ContentListQuery {
                status: Some("published".to_string()),
                filters,
                tag_id,
                limit: Some(u64::from(query.limit) + 1),
                cursor_created_at: query.cursor.as_ref().map(|c| c.created_at.clone()),
                cursor_id: query.cursor.as_ref().map(|c| c.id.clone()),
                ..Default::default()
            },
        )
        .await?;

    let limit = query.limit as usize;
    let has_more = page.records.len() > limit;
    let mut records = page.records;
    records.truncate(limit);
    // 続きの位置は、公開期間で除く前の最後の行から取る（除いても、ページ送りは途切れない）
    let next_cursor = if has_more {
        records.last().map(|last| {
            Cursor {
                created_at: last.created_at.clone(),
                id: last.id.clone(),
            }
            .encode()
        })
    } else {
        None
    };
    records.retain(|record| is_live(record, now));

    let refs: Vec<&ContentRecord> = records.iter().collect();
    let relations = resolve_relations(source, schemas, schema, &refs).await?;
    let items: Vec<Value> = records
        .iter()
        .zip(&relations)
        .map(|(record, relations)| project(schema, record, relations, false))
        .collect();

    let tag = etag(
        records
            .iter()
            .map(|r| format!("{}:{}", r.id, r.updated_at))
            .chain([
                next_cursor.clone().unwrap_or_default(),
                page.total.to_string(),
            ]),
    );
    Ok(Delivered {
        etag: tag,
        body: ListResponse {
            items,
            next_cursor,
            total: page.total,
        },
    })
}

/// 公開済みの 1 件（slug または ID）。本文（`blocks`）つき
pub async fn get<S: PublishedSource + ?Sized>(
    source: &S,
    config: &HeadlessConfig,
    schemas: &HashMap<String, SchemaDefinition>,
    content_type: &str,
    slug_or_id: &str,
    now: DateTime<Utc>,
) -> Result<Delivered<Value>, HeadlessError> {
    let schema = schema_for(config, schemas, content_type)?;
    let record = source
        .get_published(schema, slug_or_id)
        .await?
        .filter(|record| is_live(record, now))
        .ok_or(HeadlessError::NotFound)?;

    let relations = resolve_relations(source, schemas, schema, &[&record])
        .await?
        .pop()
        .unwrap_or_default();
    Ok(Delivered {
        etag: etag([format!("{}:{}", record.id, record.updated_at)]),
        body: project(schema, &record, &relations, true),
    })
}
