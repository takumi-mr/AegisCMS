//! 公開する 1 件の形。返すフィールドはスキーマが決める。
//!
//! 返すもの: `id`・`content_type`・`title`・`slug`・`excerpt`・`published_at`・`updated_at`、
//! 公開してよいカスタムフィールド（`fields`）、解決済みのリレーション（`relations`）、本文（`blocks`。単体のみ）。
//! 返さないもの: `author_id`（内部のユーザー ID）・`status`・`unpublish_at`・`comments_open`・
//! `public: false` のフィールド・スキーマに無いフィールド。

use std::collections::HashMap;

use aegis_ast::BlockNode;
use aegis_db::{ContentRecord, ResolvedRelation};
use aegis_schema::{is_standard_column, SchemaDefinition};
use serde_json::{json, Map, Value};

/// 要約の長さ（文字数）。編集用 API の一覧と同じ
const EXCERPT_CHARS: usize = 140;

/// 公開してよい（スキーマにあり、`public` が真で、標準カラムでない）カスタムフィールドの名前
pub fn public_field_names(schema: &SchemaDefinition) -> impl Iterator<Item = &str> {
    schema
        .fields
        .iter()
        .filter(|field| field.public && !is_standard_column(&field.name))
        .map(|field| field.name.as_str())
}

/// 要約。公開してよい `excerpt` フィールドがあればそれ、無ければ本文の最初の段落の先頭
fn excerpt(schema: &SchemaDefinition, record: &ContentRecord, blocks: &[BlockNode]) -> String {
    let field_is_public = public_field_names(schema).any(|name| name == "excerpt");
    if field_is_public {
        if let Some(text) = record
            .custom_fields
            .get("excerpt")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|text| !text.is_empty())
        {
            return text.to_string();
        }
    }
    first_paragraph(blocks)
}

fn first_paragraph(blocks: &[BlockNode]) -> String {
    for block in blocks {
        if block.name != "core/paragraph" {
            continue;
        }
        let Some(text) = &block.text else { continue };
        let plain = aegis_ast::plain_text(text);
        let trimmed = plain.trim();
        if trimmed.is_empty() {
            continue;
        }
        if trimmed.chars().count() > EXCERPT_CHARS {
            let mut shortened: String = trimmed.chars().take(EXCERPT_CHARS).collect();
            shortened.push_str("...");
            return shortened;
        }
        return trimmed.to_string();
    }
    String::new()
}

/// 1 件の公開用の形。`with_blocks` が真なら本文（`blocks`）も含める
pub fn project(
    schema: &SchemaDefinition,
    record: &ContentRecord,
    relations: &HashMap<String, ResolvedRelation>,
    with_blocks: bool,
) -> Value {
    let blocks: Vec<BlockNode> = serde_json::from_str(&record.blocks_json).unwrap_or_default();

    let mut out = Map::new();
    out.insert("id".into(), json!(record.id));
    out.insert("content_type".into(), json!(record.content_type));
    out.insert("title".into(), json!(record.title));
    out.insert("slug".into(), json!(record.slug));
    out.insert("excerpt".into(), json!(excerpt(schema, record, &blocks)));
    out.insert("published_at".into(), json!(record.published_at));
    out.insert("updated_at".into(), json!(record.updated_at));

    let fields: Map<String, Value> = public_field_names(schema)
        .filter_map(|name| {
            record
                .custom_fields
                .get(name)
                .map(|value| (name.to_string(), value.clone()))
        })
        .collect();
    if !fields.is_empty() {
        out.insert("fields".into(), Value::Object(fields));
    }

    if !relations.is_empty() {
        out.insert("relations".into(), json!(relations));
    }
    if with_blocks {
        out.insert("blocks".into(), json!(blocks));
    }
    Value::Object(out)
}
