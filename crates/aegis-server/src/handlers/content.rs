use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use sqlx::Row;
use std::collections::HashMap;
use std::sync::Arc;
use uuid::Uuid;

use crate::state::AppState;
use aegis_ast::{
    AstPipeline, BlockNode, HeadingIdTransformer, HtmlRenderer, HtmlRewriterConfig,
    StreamingHtmlRewriter,
};
use aegis_core::PostContext;
use aegis_schema::{FieldType, SchemaDefinition};

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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResolvedRelation {
    pub id: String,
    pub content_type: String,
    pub title: String,
    pub slug: String,
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

fn is_standard_column(name: &str) -> bool {
    matches!(
        name,
        "id" | "content_type"
            | "title"
            | "slug"
            | "status"
            | "blocks_json"
            | "author_id"
            | "published_at"
            | "created_at"
            | "updated_at"
    )
}

fn extract_custom_fields(
    row: &sqlx::sqlite::SqliteRow,
    schema: &SchemaDefinition,
) -> HashMap<String, serde_json::Value> {
    let mut fields = HashMap::new();
    for field in &schema.fields {
        if is_standard_column(&field.name) {
            continue;
        }
        if let Ok(val) = row.try_get::<String, _>(field.name.as_str()) {
            if !val.is_empty() {
                match field.field_type {
                    FieldType::Integer => {
                        if let Ok(num) = val.parse::<i64>() {
                            fields.insert(field.name.clone(), json!(num));
                        } else {
                            fields.insert(field.name.clone(), json!(val));
                        }
                    }
                    FieldType::Boolean => {
                        let b = val == "1" || val.eq_ignore_ascii_case("true");
                        fields.insert(field.name.clone(), json!(b));
                    }
                    FieldType::Jsonb => {
                        if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&val) {
                            fields.insert(field.name.clone(), parsed);
                        } else {
                            fields.insert(field.name.clone(), json!(val));
                        }
                    }
                    _ => {
                        fields.insert(field.name.clone(), json!(val));
                    }
                }
            }
        }
    }
    fields
}

async fn resolve_relations_for_item(
    pool: &sqlx::SqlitePool,
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
                let target_table = target_schema.table_name();
                let query = format!(
                    "SELECT id, content_type, title, slug FROM {} WHERE {} = ?1 LIMIT 1;",
                    target_table, target.field
                );
                if let Ok(Some(row)) = sqlx::query(&query).bind(&ref_id).fetch_optional(pool).await
                {
                    let rel = ResolvedRelation {
                        id: row.get("id"),
                        content_type: row.get("content_type"),
                        title: row.get("title"),
                        slug: row.get("slug"),
                    };
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
    Json(payload): Json<CreateContentRequest>,
) -> Result<impl IntoResponse, (StatusCode, Json<serde_json::Value>)> {
    let (table, schema_clone) = {
        let schemas = state.schemas.read().await;
        let schema = schemas.get(&content_type).ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                Json(json!({ "error": format!("Schema '{}' not found", content_type) })),
            )
        })?;
        (schema.table_name(), schema.clone())
    };

    let id = Uuid::new_v4().to_string();
    let now = chrono::Utc::now().to_rfc3339();

    // 1. AST変換パイプラインの適用
    let pipeline = AstPipeline::new().add(HeadingIdTransformer);
    let mut transformed_blocks = pipeline.execute(payload.blocks);

    // 2. プラグインスーパーバイザーによるAST変換フックの適用
    {
        let supervisor = state.supervisor.read().await;
        if let Ok(ast_result) = supervisor
            .apply_ast_transforms(transformed_blocks.clone())
            .await
        {
            transformed_blocks = ast_result;
        }

        // on-before-save フック実行
        let post_ctx = PostContext {
            id: id.clone(),
            slug: payload.slug.clone(),
            author_id: payload.author_id.clone().unwrap_or_default(),
            blocks_json: serde_json::to_string(&transformed_blocks).unwrap_or_default(),
        };
        if let Err(e) = supervisor.apply_before_save(post_ctx).await {
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

    // 3. カラムと値の動的構築（スキーマ定義のカスタムフィールド対応）
    let mut columns = vec![
        "id".to_string(),
        "content_type".to_string(),
        "title".to_string(),
        "slug".to_string(),
        "status".to_string(),
        "blocks_json".to_string(),
        "author_id".to_string(),
        "published_at".to_string(),
        "created_at".to_string(),
        "updated_at".to_string(),
    ];
    let mut custom_fields_saved = HashMap::new();
    let mut custom_values: Vec<Option<String>> = Vec::new();

    for field in &schema_clone.fields {
        if is_standard_column(&field.name) {
            continue;
        }
        columns.push(field.name.clone());
        if let Some(val) = payload.fields.get(&field.name) {
            let s = match val {
                serde_json::Value::String(s) => s.clone(),
                serde_json::Value::Number(n) => n.to_string(),
                serde_json::Value::Bool(b) => {
                    if *b {
                        "1".to_string()
                    } else {
                        "0".to_string()
                    }
                }
                serde_json::Value::Null => String::new(),
                other => other.to_string(),
            };
            custom_values.push(Some(s));
            custom_fields_saved.insert(field.name.clone(), val.clone());
        } else {
            custom_values.push(None);
        }
    }

    let placeholders: Vec<String> = (1..=columns.len()).map(|i| format!("?{}", i)).collect();
    let query_str = format!(
        "INSERT INTO {} ({}) VALUES ({});",
        table,
        columns.join(", "),
        placeholders.join(", ")
    );

    let mut query = sqlx::query(&query_str)
        .bind(&id)
        .bind(&content_type)
        .bind(&payload.title)
        .bind(&payload.slug)
        .bind(&payload.status)
        .bind(&blocks_json)
        .bind(&payload.author_id)
        .bind(&published_at)
        .bind(&now)
        .bind(&now);

    for val in &custom_values {
        query = query.bind(val);
    }

    query.execute(&state.db).await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": format!("Database error: {}", e) })),
        )
    })?;

    // 4. リレーション解決
    let relations = {
        let schemas = state.schemas.read().await;
        resolve_relations_for_item(&state.db, &schemas, &schema_clone, &custom_fields_saved).await
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
    let (table, schema_clone) = {
        let schemas = state.schemas.read().await;
        let schema = schemas.get(&content_type).ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                Json(json!({ "error": format!("Schema '{}' not found", content_type) })),
            )
        })?;
        (schema.table_name(), schema.clone())
    };

    let query = format!(
        r#"
        SELECT *
        FROM {}
        ORDER BY created_at DESC;
        "#,
        table
    );

    let rows = sqlx::query(&query)
        .fetch_all(&state.db)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": format!("Database error: {}", e) })),
            )
        })?;

    let schemas_guard = state.schemas.read().await;
    let mut list = Vec::new();

    for row in rows {
        let blocks_json: String = row.get("blocks_json");
        let blocks: Vec<BlockNode> = serde_json::from_str(&blocks_json).unwrap_or_default();
        let rendered_html = HtmlRenderer::render_nodes(&blocks);
        let custom_fields = extract_custom_fields(&row, &schema_clone);
        let relations =
            resolve_relations_for_item(&state.db, &schemas_guard, &schema_clone, &custom_fields)
                .await;

        list.push(ContentResponse {
            id: row.get("id"),
            content_type: row.get("content_type"),
            title: row.get("title"),
            slug: row.get("slug"),
            status: row.get("status"),
            blocks,
            rendered_html,
            author_id: row.get("author_id"),
            published_at: row.get("published_at"),
            created_at: row.get("created_at"),
            updated_at: row.get("updated_at"),
            fields: custom_fields,
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
    let (table, schema_clone) = {
        let schemas = state.schemas.read().await;
        let schema = schemas.get(&content_type).ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                Json(json!({ "error": format!("Schema '{}' not found", content_type) })),
            )
        })?;
        (schema.table_name(), schema.clone())
    };

    let query = format!(
        r#"
        SELECT *
        FROM {}
        WHERE slug = ?1 OR id = ?1
        LIMIT 1;
        "#,
        table
    );

    let row = sqlx::query(&query)
        .bind(&slug_or_id)
        .fetch_optional(&state.db)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": format!("Database error: {}", e) })),
            )
        })?;

    match row {
        Some(row) => {
            let blocks_json: String = row.get("blocks_json");
            let blocks: Vec<BlockNode> = serde_json::from_str(&blocks_json).unwrap_or_default();
            let rendered_html = HtmlRenderer::render_nodes(&blocks);
            let custom_fields = extract_custom_fields(&row, &schema_clone);
            let relations = {
                let schemas = state.schemas.read().await;
                resolve_relations_for_item(&state.db, &schemas, &schema_clone, &custom_fields).await
            };

            Ok(Json(ContentResponse {
                id: row.get("id"),
                content_type: row.get("content_type"),
                title: row.get("title"),
                slug: row.get("slug"),
                status: row.get("status"),
                blocks,
                rendered_html,
                author_id: row.get("author_id"),
                published_at: row.get("published_at"),
                created_at: row.get("created_at"),
                updated_at: row.get("updated_at"),
                fields: custom_fields,
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
    use aegis_schema::{FieldDefinition, FieldType, SqliteMigration};
    use sqlx::sqlite::SqlitePoolOptions;

    #[tokio::test]
    async fn test_relation_resolution() {
        let pool = SqlitePoolOptions::new()
            .connect("sqlite::memory:")
            .await
            .expect("Failed to connect to in-memory sqlite");

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

        // 2. マイグレーション実行
        for schema in [&category_schema, &article_schema] {
            let sqls = SqliteMigration::generate_schema_migration_sql(schema);
            for sql in sqls {
                sqlx::query(&sql).execute(&pool).await.unwrap();
            }
        }

        // 3. カテゴリレコード挿入
        sqlx::query(
            "INSERT INTO content_category (id, content_type, title, slug, status, blocks_json, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        )
        .bind("cat-101")
        .bind("category")
        .bind("Systems Programming")
        .bind("systems")
        .bind("published")
        .bind("[]")
        .bind("2026-10-22T00:00:00Z")
        .bind("2026-10-22T00:00:00Z")
        .execute(&pool)
        .await
        .unwrap();

        let mut schemas_map = HashMap::new();
        schemas_map.insert("category".to_string(), category_schema);
        schemas_map.insert("article".to_string(), article_schema.clone());

        // 4. 記事のリレーション解決テスト
        let mut article_fields = HashMap::new();
        article_fields.insert("category_id".to_string(), json!("cat-101"));

        let relations =
            resolve_relations_for_item(&pool, &schemas_map, &article_schema, &article_fields).await;

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
            resolve_relations_for_item(&pool, &schemas_map, &article_schema, &invalid_fields).await;

        assert!(empty_relations.is_empty());
    }
}
