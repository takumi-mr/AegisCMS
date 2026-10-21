use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::HashMap;
use std::sync::Arc;
use uuid::Uuid;

use crate::audit::AuditContext;
use crate::db::{is_standard_column, ContentRecord, DatabaseAdapter, ResolvedRelation};
use crate::state::AppState;
use aegis_ast::{
    AstPipeline, BlockNode, HeadingIdTransformer, HtmlRenderer, HtmlRewriterConfig,
    StreamingHtmlRewriter,
};
use aegis_core::{AuditAction, AuditOutcome, PostContext};
use aegis_schema::SchemaDefinition;

#[derive(Debug, Deserialize)]
pub struct CreateContentRequest {
    pub title: String,
    pub slug: String,
    #[serde(default = "default_status")]
    pub status: String,
    #[serde(default)]
    pub blocks: Vec<BlockNode>,
    #[serde(default)]
    pub author_id: Option<String>,
    #[serde(flatten)]
    pub fields: HashMap<String, serde_json::Value>,
}

fn default_status() -> String {
    "draft".to_string()
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ContentResponse {
    pub id: String,
    pub content_type: String,
    pub title: String,
    pub slug: String,
    pub status: String,
    pub blocks: Vec<BlockNode>,
    pub rendered_html: String,
    pub author_id: Option<String>,
    pub published_at: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub fields: HashMap<String, serde_json::Value>,
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub relations: HashMap<String, ResolvedRelation>,
}

async fn resolve_relations_for_item(
    db: &dyn DatabaseAdapter,
    schemas: &HashMap<String, SchemaDefinition>,
    schema: &SchemaDefinition,
    fields: &HashMap<String, serde_json::Value>,
) -> HashMap<String, ResolvedRelation> {
    let mut relations = HashMap::new();

    for (field_def, target) in schema.relation_fields() {
        if let Some(val_json) = fields.get(&field_def.name) {
            let ref_id = match val_json {
                serde_json::Value::String(s) => s.trim().to_string(),
                _ => continue,
            };
            if ref_id.is_empty() {
                continue;
            }

            if let Some(target_schema) = schemas.get(&target.content_type) {
                if let Ok(Some(rel)) = db
                    .resolve_relation(target_schema, &target.field, &ref_id)
                    .await
                {
                    relations.insert(field_def.name.clone(), rel);
                }
            }
        }
    }

    relations
}

pub async fn create_content(
    State(state): State<Arc<AppState>>,
    Path(content_type): Path<String>,
    audit: AuditContext,
    Json(payload): Json<CreateContentRequest>,
) -> Result<impl IntoResponse, (StatusCode, Json<serde_json::Value>)> {
    let schema_clone = {
        let schemas = state.schemas.read().await;
        let schema = schemas.get(&content_type).ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                Json(json!({ "error": format!("Schema '{}' not found", content_type) })),
            )
        })?;
        schema.clone()
    };

    let id = Uuid::new_v4().to_string();
    let now = chrono::Utc::now().to_rfc3339();

    // 1. AST変換パイプラインの適用
    let pipeline = AstPipeline::new().add(HeadingIdTransformer);
    let mut transformed_blocks = pipeline.execute(payload.blocks);

    // 2. プラグインスーパーバイザーによるAST変換フックの適用
    {
        let supervisor = state.supervisor.read().await;
        match supervisor
            .apply_ast_transforms(transformed_blocks.clone())
            .await
        {
            Ok(ast_result) => transformed_blocks = ast_result,
            Err(e) => {
                // AST 変換フックの失敗は保存を妨げない（組み込みパイプラインの結果で続行）
                tracing::warn!("Plugin hook 'transform-content-ast' failed: {}", e);
                state
                    .audit
                    .record(
                        audit
                            .event(AuditAction::PluginError, AuditOutcome::Failure)
                            .with_resource("content", Some(id.clone()))
                            .with_details(json!({
                                "hook": "transform-content-ast",
                                "content_type": content_type,
                                "error": e.to_string(),
                            })),
                    )
                    .await;
            }
        }

        // on-before-save フック実行
        let post_ctx = PostContext {
            id: id.clone(),
            title: payload.title.clone(),
            slug: payload.slug.clone(),
            author_id: payload.author_id.clone().unwrap_or_default(),
            blocks_json: serde_json::to_string(&transformed_blocks).unwrap_or_default(),
        };
        if let Err(e) = supervisor.apply_before_save(post_ctx).await {
            state
                .audit
                .record(
                    audit
                        .event(AuditAction::PluginError, AuditOutcome::Failure)
                        .with_resource("content", Some(id.clone()))
                        .with_details(json!({
                            "hook": "on-before-save",
                            "content_type": content_type,
                            "error": e.to_string(),
                        })),
                )
                .await;
            state
                .audit
                .record(
                    audit
                        .event(AuditAction::ContentCreate, AuditOutcome::Denied)
                        .with_resource("content", Some(id.clone()))
                        .with_details(json!({
                            "content_type": content_type,
                            "slug": payload.slug,
                            "title": payload.title,
                            "reason": e.to_string(),
                        })),
                )
                .await;
            return Err((
                StatusCode::BAD_REQUEST,
                Json(json!({ "error": format!("Plugin hook rejected save: {}", e) })),
            ));
        }
    }

    let blocks_json =
        serde_json::to_string(&transformed_blocks).unwrap_or_else(|_| "[]".to_string());
    let raw_html = HtmlRenderer::render_nodes(&transformed_blocks);
    let rewriter_config = HtmlRewriterConfig::from(state.config.security.clone());
    let rendered_html =
        StreamingHtmlRewriter::rewrite(&raw_html, &rewriter_config).unwrap_or(raw_html);

    let published_at = if payload.status == "published" {
        Some(now.clone())
    } else {
        None
    };

    // 3. カスタムフィールドの抽出
    let mut custom_fields_saved = HashMap::new();
    for field in &schema_clone.fields {
        if is_standard_column(&field.name) {
            continue;
        }
        if let Some(val) = payload.fields.get(&field.name) {
            custom_fields_saved.insert(field.name.clone(), val.clone());
        }
    }

    // 4. ContentRecord 作成および DatabaseAdapter による永続化
    let record = ContentRecord {
        id: id.clone(),
        content_type: content_type.clone(),
        title: payload.title.clone(),
        slug: payload.slug.clone(),
        status: payload.status.clone(),
        blocks_json,
        author_id: payload.author_id.clone(),
        published_at: published_at.clone(),
        created_at: now.clone(),
        updated_at: now.clone(),
        custom_fields: custom_fields_saved.clone(),
    };

    let content_event = |outcome: AuditOutcome, extra: serde_json::Value| {
        let mut details = json!({
            "content_type": content_type,
            "slug": record.slug,
            "title": record.title,
            "status": record.status,
        });
        if let (Some(map), serde_json::Value::Object(extra)) = (details.as_object_mut(), extra) {
            map.extend(extra);
        }
        audit
            .event(AuditAction::ContentCreate, outcome)
            .with_resource("content", Some(record.id.clone()))
            .with_details(details)
    };

    if let Err(e) = state.db.insert_content(&schema_clone, &record).await {
        state
            .audit
            .record(content_event(
                AuditOutcome::Failure,
                json!({ "error": e.to_string() }),
            ))
            .await;
        return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": format!("Database error: {}", e) })),
        ));
    }
    state
        .audit
        .record(content_event(AuditOutcome::Success, json!({})))
        .await;

    // 5. リレーション解決
    let relations = {
        let schemas = state.schemas.read().await;
        resolve_relations_for_item(&*state.db, &schemas, &schema_clone, &custom_fields_saved).await
    };

    Ok((
        StatusCode::CREATED,
        Json(ContentResponse {
            id,
            content_type,
            title: payload.title,
            slug: payload.slug,
            status: payload.status,
            blocks: transformed_blocks,
            rendered_html,
            author_id: payload.author_id,
            published_at,
            created_at: now.clone(),
            updated_at: now,
            fields: custom_fields_saved,
            relations,
        }),
    ))
}

pub async fn list_content(
    State(state): State<Arc<AppState>>,
    Path(content_type): Path<String>,
) -> Result<impl IntoResponse, (StatusCode, Json<serde_json::Value>)> {
    let schema_clone = {
        let schemas = state.schemas.read().await;
        let schema = schemas.get(&content_type).ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                Json(json!({ "error": format!("Schema '{}' not found", content_type) })),
            )
        })?;
        schema.clone()
    };

    let records = state.db.list_contents(&schema_clone).await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": format!("Database error: {}", e) })),
        )
    })?;

    let schemas_guard = state.schemas.read().await;
    let mut list = Vec::new();

    for r in records {
        let blocks: Vec<BlockNode> = serde_json::from_str(&r.blocks_json).unwrap_or_default();
        let rendered_html = HtmlRenderer::render_nodes(&blocks);
        let relations =
            resolve_relations_for_item(&*state.db, &schemas_guard, &schema_clone, &r.custom_fields)
                .await;

        list.push(ContentResponse {
            id: r.id,
            content_type: r.content_type,
            title: r.title,
            slug: r.slug,
            status: r.status,
            blocks,
            rendered_html,
            author_id: r.author_id,
            published_at: r.published_at,
            created_at: r.created_at,
            updated_at: r.updated_at,
            fields: r.custom_fields,
            relations,
        });
    }

    Ok(Json(json!({
        "contents": list,
        "total": list.len(),
    })))
}

pub async fn get_content(
    State(state): State<Arc<AppState>>,
    Path((content_type, slug_or_id)): Path<(String, String)>,
) -> Result<impl IntoResponse, (StatusCode, Json<serde_json::Value>)> {
    let schema_clone = {
        let schemas = state.schemas.read().await;
        let schema = schemas.get(&content_type).ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                Json(json!({ "error": format!("Schema '{}' not found", content_type) })),
            )
        })?;
        schema.clone()
    };

    let maybe_record = state
        .db
        .get_content(&schema_clone, &slug_or_id)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": format!("Database error: {}", e) })),
            )
        })?;

    match maybe_record {
        Some(r) => {
            let blocks: Vec<BlockNode> = serde_json::from_str(&r.blocks_json).unwrap_or_default();
            let rendered_html = HtmlRenderer::render_nodes(&blocks);
            let relations = {
                let schemas = state.schemas.read().await;
                resolve_relations_for_item(&*state.db, &schemas, &schema_clone, &r.custom_fields)
                    .await
            };

            Ok(Json(ContentResponse {
                id: r.id,
                content_type: r.content_type,
                title: r.title,
                slug: r.slug,
                status: r.status,
                blocks,
                rendered_html,
                author_id: r.author_id,
                published_at: r.published_at,
                created_at: r.created_at,
                updated_at: r.updated_at,
                fields: r.custom_fields,
                relations,
            }))
        }
        None => Err((
            StatusCode::NOT_FOUND,
            Json(json!({ "error": format!("Content '{}' not found", slug_or_id) })),
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::SqliteAdapter;
    use aegis_schema::{FieldDefinition, FieldType};
    use sqlx::sqlite::SqlitePoolOptions;

    #[tokio::test]
    async fn test_relation_resolution() {
        let pool = SqlitePoolOptions::new()
            .connect("sqlite::memory:")
            .await
            .expect("Failed to connect to in-memory sqlite");

        let adapter = SqliteAdapter::new(pool);
        adapter.init().await.expect("init should succeed");

        // 1. スキーマ定義
        let category_schema = SchemaDefinition {
            content_type: "category".to_string(),
            label: "カテゴリ".to_string(),
            description: None,
            fields: vec![
                FieldDefinition {
                    name: "title".to_string(),
                    field_type: FieldType::String,
                    required: true,
                    nullable: false,
                    unique: false,
                    index: true,
                    max_length: Some(100),
                    target: None,
                    indexed_paths: vec![],
                },
                FieldDefinition {
                    name: "slug".to_string(),
                    field_type: FieldType::String,
                    required: true,
                    nullable: false,
                    unique: true,
                    index: true,
                    max_length: None,
                    target: None,
                    indexed_paths: vec![],
                },
            ],
        };

        let article_schema = SchemaDefinition {
            content_type: "article".to_string(),
            label: "技術記事".to_string(),
            description: None,
            fields: vec![
                FieldDefinition {
                    name: "title".to_string(),
                    field_type: FieldType::String,
                    required: true,
                    nullable: false,
                    unique: false,
                    index: true,
                    max_length: Some(255),
                    target: None,
                    indexed_paths: vec![],
                },
                FieldDefinition {
                    name: "category_id".to_string(),
                    field_type: FieldType::Relation,
                    required: false,
                    nullable: true,
                    unique: false,
                    index: true,
                    max_length: None,
                    target: Some("category.id".to_string()),
                    indexed_paths: vec![],
                },
            ],
        };

        let mut schemas_map = HashMap::new();
        schemas_map.insert("category".to_string(), category_schema.clone());
        schemas_map.insert("article".to_string(), article_schema.clone());

        // 2. マイグレーション実行
        adapter
            .run_migrations(&schemas_map)
            .await
            .expect("migrations should succeed");

        // 3. カテゴリレコード挿入
        let cat_record = ContentRecord {
            id: "cat-101".to_string(),
            content_type: "category".to_string(),
            title: "Systems Programming".to_string(),
            slug: "systems".to_string(),
            status: "published".to_string(),
            blocks_json: "[]".to_string(),
            author_id: None,
            published_at: Some("2026-10-22T00:00:00Z".to_string()),
            created_at: "2026-10-22T00:00:00Z".to_string(),
            updated_at: "2026-10-22T00:00:00Z".to_string(),
            custom_fields: HashMap::new(),
        };
        adapter
            .insert_content(&category_schema, &cat_record)
            .await
            .expect("insert category should succeed");

        // 4. 記事のリレーション解決テスト
        let mut article_fields = HashMap::new();
        article_fields.insert("category_id".to_string(), json!("cat-101"));

        let relations =
            resolve_relations_for_item(&adapter, &schemas_map, &article_schema, &article_fields)
                .await;

        assert_eq!(relations.len(), 1);
        let resolved_cat = relations
            .get("category_id")
            .expect("category_id relation should be resolved");
        assert_eq!(resolved_cat.id, "cat-101");
        assert_eq!(resolved_cat.content_type, "category");
        assert_eq!(resolved_cat.title, "Systems Programming");
        assert_eq!(resolved_cat.slug, "systems");

        // 5. 存在しないIDの場合はリレーションが空になることをテスト
        let mut invalid_fields = HashMap::new();
        invalid_fields.insert("category_id".to_string(), json!("non-existent-id"));

        let empty_relations =
            resolve_relations_for_item(&adapter, &schemas_map, &article_schema, &invalid_fields)
                .await;

        assert!(empty_relations.is_empty());
    }
}
