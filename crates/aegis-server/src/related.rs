//! 関連記事の推薦・抽出機能（Strategy パターン）。
//!
//! 将来的に全文検索（`aegis-search` / tantivy）やセマンティック検索、外部レコメンドエンジン等へ
//! 差し替えられるよう [`RelatedArticlesStrategy`] トレイトで抽象化し、既定ではタグ一致・カテゴリ一致・
//! 新着フォールバックを行う [`TagCategoryRelatedStrategy`]（パターン 1）を提供します。

use std::collections::HashMap;

use aegis_ast::BlockNode;
use aegis_core::ContentSummary;
use aegis_db::{ContentListQuery, ContentRecord};
use aegis_schema::SchemaDefinition;
use async_trait::async_trait;

use crate::handlers::public::extract_excerpt;
use crate::handlers::tags_public::TagLookup;
use crate::render;
use crate::state::AppState;

/// 関連記事の推薦・抽出を行うストラテジー
#[async_trait]
pub trait RelatedArticlesStrategy: Send + Sync {
    /// 対象記事に対する関連記事の一覧を返す
    async fn find_related(
        &self,
        state: &AppState,
        schema: &SchemaDefinition,
        current_record: &ContentRecord,
        limit: usize,
    ) -> Vec<ContentSummary>;
}

/// パターン 1: タグ・カテゴリ重み付け一致ストラテジー
///
/// 1. 共通タグが多い公開記事（一致タグ数の降順）
/// 2. 同じカテゴリ（`category_id`）の公開記事
/// 3. 最新の公開記事（フォールバック）
///
/// 自分自身の記事は除外、最大 `limit` 件
#[derive(Debug, Clone, Default)]
pub struct TagCategoryRelatedStrategy;

impl TagCategoryRelatedStrategy {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl RelatedArticlesStrategy for TagCategoryRelatedStrategy {
    async fn find_related(
        &self,
        state: &AppState,
        schema: &SchemaDefinition,
        current_record: &ContentRecord,
        limit: usize,
    ) -> Vec<ContentSummary> {
        if limit == 0 {
            return Vec::new();
        }

        // 1. 対象記事のタグからタグごとの一致数を集計
        let mut tag_match_counts: HashMap<String, usize> = HashMap::new();
        if let Ok(current_tags) = state
            .db
            .list_content_tags(&schema.content_type, &current_record.id)
            .await
        {
            for tag in current_tags {
                if let Ok(links) = state
                    .db
                    .list_tagged_contents(&tag.id, Some(&schema.content_type))
                    .await
                {
                    for link in links {
                        if link.content_id != current_record.id {
                            *tag_match_counts.entry(link.content_id).or_insert(0) += 1;
                        }
                    }
                }
            }
        }

        // 2. 公開中の記事一覧を取得
        let query = ContentListQuery {
            limit: Some(100),
            ..Default::default()
        };
        let published_records = match state.db.list_published_contents_page(schema, &query).await {
            Ok(page) => page.records,
            Err(_) => Vec::new(),
        };

        let current_category = current_record
            .custom_fields
            .get("category_id")
            .and_then(|v| v.as_str())
            .filter(|s| !s.trim().is_empty());

        // 候補記事を優先度で分類
        // グループ 0: タグ一致あり (tag_count 降順)
        // グループ 1: 同一カテゴリ (タグ一致なし)
        // グループ 2: その他 (フォールバック)
        let mut scored_records: Vec<(u8, usize, ContentRecord)> = Vec::new();

        for record in published_records {
            if record.id == current_record.id {
                continue;
            }

            let tag_matches = tag_match_counts.get(&record.id).copied().unwrap_or(0);
            let same_category = current_category.is_some()
                && record
                    .custom_fields
                    .get("category_id")
                    .and_then(|v| v.as_str())
                    == current_category;

            let group = if tag_matches > 0 {
                0
            } else if same_category {
                1
            } else {
                2
            };

            scored_records.push((group, tag_matches, record));
        }

        // ソート: グループ昇順 (0 -> 1 -> 2)、タグ一致数降順、公開日時降順、作成日時降順
        scored_records.sort_by(|a, b| {
            a.0.cmp(&b.0)
                .then_with(|| b.1.cmp(&a.1))
                .then_with(|| b.2.published_at.cmp(&a.2.published_at))
                .then_with(|| b.2.created_at.cmp(&a.2.created_at))
        });

        let candidate_records: Vec<ContentRecord> = scored_records
            .into_iter()
            .take(limit)
            .map(|(_, _, record)| record)
            .collect();

        convert_records_to_summaries(state, schema, candidate_records).await
    }
}

/// ContentRecord の一覧を ContentSummary の一覧に変換する
pub async fn convert_records_to_summaries(
    state: &AppState,
    schema: &SchemaDefinition,
    records: Vec<ContentRecord>,
) -> Vec<ContentSummary> {
    if records.is_empty() {
        return Vec::new();
    }

    let ids: Vec<String> = records.iter().map(|r| r.id.clone()).collect();
    let tag_lookup = TagLookup::load(state, &schema.content_type, &ids).await;

    let schemas = state.schemas.read().await;
    let field_maps: Vec<&HashMap<String, serde_json::Value>> =
        records.iter().map(|r| &r.custom_fields).collect();
    let resolved_relations =
        aegis_db::resolve_relations_for_records(&*state.db, &schemas, schema, &field_maps).await;

    let mut summaries = Vec::with_capacity(records.len());
    for (r, rel_map) in records.into_iter().zip(resolved_relations) {
        let blocks: Vec<BlockNode> = serde_json::from_str(&r.blocks_json).unwrap_or_default();
        let mut fields = r.custom_fields;
        let mut dummy_tags = crate::cdn::CacheTags::new();
        tag_lookup.attach(&r.id, &mut fields, &mut dummy_tags);

        let relations: HashMap<String, serde_json::Value> = rel_map
            .into_iter()
            .map(|(name, rel)| (name, render::relation_summary(&rel)))
            .collect();

        summaries.push(ContentSummary {
            id: r.id,
            content_type: r.content_type,
            title: r.title,
            slug: r.slug,
            excerpt: extract_excerpt(&blocks, 140),
            author_id: r.author_id,
            published_at: r.published_at,
            created_at: r.created_at,
            fields,
            relations,
        });
    }

    summaries
}

#[cfg(test)]
mod tests {
    use super::*;
    use aegis_db::TagRecord;
    use aegis_runtime::PluginSupervisor;
    use serde_json::json;
    use std::sync::Arc;

    struct FixedTitlesStrategy {
        fixed_titles: Vec<String>,
    }

    #[async_trait]
    impl RelatedArticlesStrategy for FixedTitlesStrategy {
        async fn find_related(
            &self,
            _state: &AppState,
            _schema: &SchemaDefinition,
            _current_record: &ContentRecord,
            _limit: usize,
        ) -> Vec<ContentSummary> {
            self.fixed_titles
                .iter()
                .map(|t| ContentSummary {
                    id: t.clone(),
                    title: t.clone(),
                    ..Default::default()
                })
                .collect()
        }
    }

    #[tokio::test]
    async fn test_custom_strategy_swap() {
        let config = crate::config::AegisConfig::default();
        let article_schema: SchemaDefinition = serde_json::from_value(serde_json::json!({
            "content_type": "article",
            "label": "Articles",
            "fields": [
                { "name": "title", "type": "string", "required": true },
                { "name": "category_id", "type": "string", "required": false, "nullable": true }
            ]
        }))
        .unwrap();
        let state = crate::test_support::test_app_state(
            config,
            PluginSupervisor::new(),
            vec![article_schema],
        )
        .await;

        let custom = Arc::new(FixedTitlesStrategy {
            fixed_titles: vec!["Custom 1".to_string(), "Custom 2".to_string()],
        });
        let state = state.with_related_strategy(custom);

        let schema = state.schemas.read().await["article"].clone();
        let record = ContentRecord {
            id: "article-dummy".to_string(),
            content_type: "article".to_string(),
            title: "Dummy".to_string(),
            slug: "dummy".to_string(),
            status: "published".to_string(),
            blocks_json: "[]".to_string(),
            author_id: None,
            published_at: Some("2026-10-01T00:00:00Z".to_string()),
            unpublish_at: None,
            created_at: "2026-10-01T00:00:00Z".to_string(),
            updated_at: "2026-10-01T00:00:00Z".to_string(),
            custom_fields: Default::default(),
        };

        let related = state
            .related_strategy
            .find_related(&state, &schema, &record, 4)
            .await;
        assert_eq!(related.len(), 2);
        assert_eq!(related[0].title, "Custom 1");
        assert_eq!(related[1].title, "Custom 2");
    }

    #[tokio::test]
    async fn test_tag_category_strategy_ordering() {
        let config = crate::config::AegisConfig::default();
        let article_schema: SchemaDefinition = serde_json::from_value(serde_json::json!({
            "content_type": "article",
            "label": "Articles",
            "fields": [
                { "name": "title", "type": "string", "required": true },
                { "name": "category_id", "type": "string", "required": false, "nullable": true }
            ]
        }))
        .unwrap();
        let state = crate::test_support::test_app_state(
            config,
            PluginSupervisor::new(),
            vec![article_schema],
        )
        .await;
        let schema = state.schemas.read().await["article"].clone();

        // タグを作成
        state
            .db
            .insert_tag(&TagRecord {
                id: "tag-a".to_string(),
                slug: "tag-a".to_string(),
                name: "Tag A".to_string(),
                description: None,
                created_at: "2026-10-01T00:00:00Z".to_string(),
                updated_at: "2026-10-01T00:00:00Z".to_string(),
            })
            .await
            .unwrap();
        state
            .db
            .insert_tag(&TagRecord {
                id: "tag-b".to_string(),
                slug: "tag-b".to_string(),
                name: "Tag B".to_string(),
                description: None,
                created_at: "2026-10-01T00:00:00Z".to_string(),
                updated_at: "2026-10-01T00:00:00Z".to_string(),
            })
            .await
            .unwrap();

        // 記事を登録するヘルパー
        let insert_article = |id: &str, slug: &str, category: Option<&str>, date: &str| {
            let state = state.clone();
            let schema = schema.clone();
            let id = id.to_string();
            let slug = slug.to_string();
            let category = category.map(str::to_string);
            let date = date.to_string();
            async move {
                let mut custom_fields = HashMap::new();
                if let Some(cat) = category {
                    custom_fields.insert("category_id".to_string(), json!(cat));
                }
                state
                    .db
                    .insert_content(
                        &schema,
                        &ContentRecord {
                            id: id.clone(),
                            content_type: "article".to_string(),
                            title: format!("Title {}", slug),
                            slug: slug.clone(),
                            status: "published".to_string(),
                            blocks_json: "[]".to_string(),
                            author_id: None,
                            published_at: Some(date.clone()),
                            unpublish_at: None,
                            created_at: date.clone(),
                            updated_at: date,
                            custom_fields,
                        },
                    )
                    .await
                    .unwrap();
            }
        };

        // 1. 対象記事 (タグ: A, B / カテゴリ: cat-1)
        insert_article("art-main", "main", Some("cat-1"), "2026-10-01T00:00:00Z").await;
        state
            .db
            .set_content_tags(
                "article",
                "art-main",
                &["tag-a".to_string(), "tag-b".to_string()],
                "2026-10-01T00:00:00Z",
            )
            .await
            .unwrap();

        // 2. タグ2個一致 (タグ: A, B / カテゴリ: cat-2)
        insert_article(
            "art-tag-both",
            "tag-both",
            Some("cat-2"),
            "2026-10-02T00:00:00Z",
        )
        .await;
        state
            .db
            .set_content_tags(
                "article",
                "art-tag-both",
                &["tag-a".to_string(), "tag-b".to_string()],
                "2026-10-02T00:00:00Z",
            )
            .await
            .unwrap();

        // 3. タグ1個一致 (タグ: A / カテゴリ: cat-2)
        insert_article(
            "art-tag-one",
            "tag-one",
            Some("cat-2"),
            "2026-10-03T00:00:00Z",
        )
        .await;
        state
            .db
            .set_content_tags(
                "article",
                "art-tag-one",
                &["tag-a".to_string()],
                "2026-10-03T00:00:00Z",
            )
            .await
            .unwrap();

        // 4. カテゴリのみ一致 (タグなし / カテゴリ: cat-1)
        insert_article(
            "art-same-cat",
            "same-cat",
            Some("cat-1"),
            "2026-10-04T00:00:00Z",
        )
        .await;

        // 5. 新着フォールバック (タグなし / カテゴリ: cat-2)
        insert_article(
            "art-fallback",
            "fallback",
            Some("cat-2"),
            "2026-10-05T00:00:00Z",
        )
        .await;

        let main_record = state
            .db
            .get_content(&schema, "art-main")
            .await
            .unwrap()
            .unwrap();
        let related = state
            .related_strategy
            .find_related(&state, &schema, &main_record, 4)
            .await;

        assert_eq!(related.len(), 4);
        // 順序: タグ2個一致 -> タグ1個一致 -> カテゴリ一致 -> 新着フォールバック
        assert_eq!(related[0].id, "art-tag-both");
        assert_eq!(related[1].id, "art-tag-one");
        assert_eq!(related[2].id, "art-same-cat");
        assert_eq!(related[3].id, "art-fallback");

        // 自分自身（art-main）が含まれていないこと
        assert!(!related.iter().any(|r| r.id == "art-main"));
    }
}
