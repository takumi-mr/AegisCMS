use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use sqlx::Row;
use std::sync::Arc;
use uuid::Uuid;

use crate::state::AppState;
use aegis_ast::{AstPipeline, BlockNode, HeadingIdTransformer, HtmlRenderer};
use aegis_core::PostContext;

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
}

fn default_status() -> String {
    "draft".to_string()
}

#[derive(Debug, Serialize)]
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
}

pub async fn create_content(
    State(state): State<Arc<AppState>>,
    Path(content_type): Path<String>,
    Json(payload): Json<CreateContentRequest>,
) -> Result<impl IntoResponse, (StatusCode, Json<serde_json::Value>)> {
    let table = {
        let schemas = state.schemas.read().await;
        let schema = schemas.get(&content_type).ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                Json(json!({ "error": format!("Schema '{}' not found", content_type) })),
            )
        })?;
        schema.table_name()
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
    let rendered_html = HtmlRenderer::render_nodes(&transformed_blocks);

    let query = format!(
        r#"
        INSERT INTO {} (id, content_type, title, slug, status, blocks_json, author_id, published_at, created_at, updated_at)
        VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10);
        "#,
        table
    );

    let published_at = if payload.status == "published" {
        Some(now.clone())
    } else {
        None
    };

    sqlx::query(&query)
        .bind(&id)
        .bind(&content_type)
        .bind(&payload.title)
        .bind(&payload.slug)
        .bind(&payload.status)
        .bind(&blocks_json)
        .bind(&payload.author_id)
        .bind(&published_at)
        .bind(&now)
        .bind(&now)
        .execute(&state.db)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": format!("Database error: {}", e) })),
            )
        })?;

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
        }),
    ))
}

pub async fn list_content(
    State(state): State<Arc<AppState>>,
    Path(content_type): Path<String>,
) -> Result<impl IntoResponse, (StatusCode, Json<serde_json::Value>)> {
    let table = {
        let schemas = state.schemas.read().await;
        let schema = schemas.get(&content_type).ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                Json(json!({ "error": format!("Schema '{}' not found", content_type) })),
            )
        })?;
        schema.table_name()
    };

    let query = format!(
        r#"
        SELECT id, content_type, title, slug, status, blocks_json, author_id, published_at, created_at, updated_at
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

    let mut list = Vec::new();
    for row in rows {
        let blocks_json: String = row.get("blocks_json");
        let blocks: Vec<BlockNode> = serde_json::from_str(&blocks_json).unwrap_or_default();
        let rendered_html = HtmlRenderer::render_nodes(&blocks);

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
    let table = {
        let schemas = state.schemas.read().await;
        let schema = schemas.get(&content_type).ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                Json(json!({ "error": format!("Schema '{}' not found", content_type) })),
            )
        })?;
        schema.table_name()
    };

    let query = format!(
        r#"
        SELECT id, content_type, title, slug, status, blocks_json, author_id, published_at, created_at, updated_at
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
            }))
        }
        None => Err((
            StatusCode::NOT_FOUND,
            Json(json!({ "error": format!("Content '{}' not found", slug_or_id) })),
        )),
    }
}
