//! タグの API（管理）。一覧は `viewer` 以上、作成・更新・削除とコンテンツへの付け外しは `editor` 以上。
//! タグはどの種類のコンテンツにも付けられる（`/api/v1/contents/{type}/{slug}/tags`）

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Json,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::sync::Arc;

use crate::audit::AuditContext;
use crate::auth::{EditorSession, ViewerSession};
use crate::state::AppState;
use aegis_core::{AuditAction, AuditOutcome};
use aegis_db::{TagListQuery, TagRecord, TagWithUsage};
use aegis_tags::{TagError, TagInput, TagService};

type ApiError = (StatusCode, Json<serde_json::Value>);

/// 一覧の既定の件数と上限
const DEFAULT_LIMIT: u64 = 50;
const MAX_LIMIT: u64 = 200;
/// 検索語の最大文字数
const MAX_SEARCH_CHARS: usize = 100;

fn api_error(status: StatusCode, message: impl Into<String>) -> ApiError {
    (status, Json(json!({ "error": message.into() })))
}

fn tag_error(e: TagError) -> ApiError {
    let status = match &e {
        TagError::Invalid(_) => StatusCode::BAD_REQUEST,
        TagError::NotFound(_) => StatusCode::NOT_FOUND,
        TagError::Conflict(_) => StatusCode::CONFLICT,
        TagError::Database(inner) => {
            tracing::error!("Tag request failed: {}", inner);
            return api_error(StatusCode::INTERNAL_SERVER_ERROR, "Internal server error");
        }
    };
    api_error(status, e.to_string())
}

fn tags(state: &AppState) -> Result<Arc<TagService>, ApiError> {
    state.tags.clone().ok_or_else(|| {
        api_error(
            StatusCode::NOT_FOUND,
            "Tags are not enabled ([tags].enabled)",
        )
    })
}

/// API で返すタグ
#[derive(Debug, Serialize)]
pub struct TagResponse {
    pub id: String,
    pub slug: String,
    pub name: String,
    pub description: Option<String>,
    /// このタグが付いているコンテンツの数（一覧のときだけ）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub usage: Option<u64>,
    pub created_at: String,
    pub updated_at: String,
}

impl TagResponse {
    fn new(tag: TagRecord, usage: Option<u64>) -> Self {
        Self {
            id: tag.id,
            slug: tag.slug,
            name: tag.name,
            description: tag.description,
            usage,
            created_at: tag.created_at,
            updated_at: tag.updated_at,
        }
    }
}

#[derive(Debug, Default, Deserialize)]
pub struct ListTagsParams {
    /// 名前・slug への部分一致
    pub q: Option<String>,
    pub limit: Option<u64>,
    pub offset: Option<u64>,
}

/// `GET /api/v1/tags?q=&limit=&offset=` — タグの一覧（名前の昇順。付いているコンテンツの数つき。`viewer`）
pub async fn list_tags(
    State(state): State<Arc<AppState>>,
    _session: ViewerSession,
    Query(params): Query<ListTagsParams>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let service = tags(&state)?;
    let limit = params.limit.unwrap_or(DEFAULT_LIMIT).clamp(1, MAX_LIMIT);
    let offset = params.offset.unwrap_or(0);
    let page = service
        .list(&TagListQuery {
            search: params
                .q
                .as_deref()
                .map(str::trim)
                .filter(|q| !q.is_empty())
                .map(|q| q.chars().take(MAX_SEARCH_CHARS).collect()),
            limit: Some(limit),
            offset,
        })
        .await
        .map_err(tag_error)?;
    let items: Vec<TagResponse> = page
        .tags
        .into_iter()
        .map(|TagWithUsage { tag, usage }| TagResponse::new(tag, Some(usage)))
        .collect();
    Ok(Json(json!({
        "tags": items,
        "total": page.total,
        "limit": limit,
        "offset": offset,
    })))
}

#[derive(Debug, Deserialize)]
pub struct TagRequest {
    pub name: String,
    /// 省略（空）なら、作成のときは名前から作り、更新のときは今のまま
    #[serde(default)]
    pub slug: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
}

impl From<TagRequest> for TagInput {
    fn from(request: TagRequest) -> Self {
        Self {
            name: request.name,
            slug: request.slug,
            description: request.description,
        }
    }
}

async fn audit_tag(
    state: &AppState,
    audit: &AuditContext,
    action: AuditAction,
    outcome: AuditOutcome,
    details: serde_json::Value,
) {
    state
        .audit
        .record(audit.event(action, outcome).with_details(details))
        .await;
}

/// `POST /api/v1/tags` — タグを作る（`editor`）。slug は半角英数字とハイフン。名前・slug は重複できない（409）
pub async fn create_tag(
    State(state): State<Arc<AppState>>,
    _session: EditorSession,
    audit: AuditContext,
    Json(payload): Json<TagRequest>,
) -> Result<(StatusCode, Json<TagResponse>), ApiError> {
    let service = tags(&state)?;
    match service.create(payload.into()).await {
        Ok(tag) => {
            audit_tag(
                &state,
                &audit,
                AuditAction::TagCreate,
                AuditOutcome::Success,
                json!({ "tag_id": tag.id, "slug": tag.slug, "name": tag.name }),
            )
            .await;
            Ok((StatusCode::CREATED, Json(TagResponse::new(tag, Some(0)))))
        }
        Err(e) => {
            if !matches!(e, TagError::Database(_)) {
                audit_tag(
                    &state,
                    &audit,
                    AuditAction::TagCreate,
                    AuditOutcome::Failure,
                    json!({ "error": e.to_string() }),
                )
                .await;
            }
            Err(tag_error(e))
        }
    }
}

/// `PUT /api/v1/tags/{id}` — 名前・slug・説明を変える（`editor`）。slug を省略したら今のまま
pub async fn update_tag(
    State(state): State<Arc<AppState>>,
    _session: EditorSession,
    Path(id): Path<String>,
    audit: AuditContext,
    Json(payload): Json<TagRequest>,
) -> Result<Json<TagResponse>, ApiError> {
    let service = tags(&state)?;
    match service.update(&id, payload.into()).await {
        Ok(tag) => {
            audit_tag(
                &state,
                &audit,
                AuditAction::TagUpdate,
                AuditOutcome::Success,
                json!({ "tag_id": tag.id, "slug": tag.slug, "name": tag.name }),
            )
            .await;
            Ok(Json(TagResponse::new(tag, None)))
        }
        Err(e) => {
            if !matches!(e, TagError::Database(_)) {
                audit_tag(
                    &state,
                    &audit,
                    AuditAction::TagUpdate,
                    AuditOutcome::Failure,
                    json!({ "tag_id": id, "error": e.to_string() }),
                )
                .await;
            }
            Err(tag_error(e))
        }
    }
}

/// `DELETE /api/v1/tags/{id}` — タグを消す（`editor`）。付いていたコンテンツからも外れる
pub async fn delete_tag(
    State(state): State<Arc<AppState>>,
    _session: EditorSession,
    Path(id): Path<String>,
    audit: AuditContext,
) -> Result<StatusCode, ApiError> {
    let service = tags(&state)?;
    let tag = service.delete(&id).await.map_err(tag_error)?;
    audit_tag(
        &state,
        &audit,
        AuditAction::TagDelete,
        AuditOutcome::Success,
        json!({ "tag_id": tag.id, "slug": tag.slug, "name": tag.name }),
    )
    .await;
    Ok(StatusCode::NO_CONTENT)
}

/// コンテンツ（スラッグか ID）の ID を調べる。無ければ 404
async fn content_id(
    state: &AppState,
    content_type: &str,
    slug_or_id: &str,
) -> Result<String, ApiError> {
    let schema = state
        .schemas
        .read()
        .await
        .get(content_type)
        .cloned()
        .ok_or_else(|| {
            api_error(
                StatusCode::NOT_FOUND,
                format!("Schema '{}' not found", content_type),
            )
        })?;
    let record = state
        .db
        .get_content(&schema, slug_or_id)
        .await
        .map_err(|e| {
            tracing::error!("Could not read {}/{}: {}", content_type, slug_or_id, e);
            api_error(StatusCode::INTERNAL_SERVER_ERROR, "Internal server error")
        })?
        .ok_or_else(|| {
            api_error(
                StatusCode::NOT_FOUND,
                format!("Content '{}' not found", slug_or_id),
            )
        })?;
    Ok(record.id)
}

/// `GET /api/v1/contents/{content_type}/{slug}/tags` — そのコンテンツのタグ（名前の昇順。`viewer`）
pub async fn get_content_tags(
    State(state): State<Arc<AppState>>,
    _session: ViewerSession,
    Path((content_type, slug_or_id)): Path<(String, String)>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let service = tags(&state)?;
    let id = content_id(&state, &content_type, &slug_or_id).await?;
    let found = service
        .content_tags(&content_type, &id)
        .await
        .map_err(tag_error)?;
    Ok(Json(json!({
        "content_type": content_type,
        "content_id": id,
        "tags": found.into_iter().map(|t| TagResponse::new(t, None)).collect::<Vec<_>>(),
    })))
}

#[derive(Debug, Deserialize)]
pub struct ContentTagsRequest {
    /// コンテンツに付けるタグの ID（これに置き換わる。空なら全部外す）
    pub tag_ids: Vec<String>,
}

/// `PUT /api/v1/contents/{content_type}/{slug}/tags` — そのコンテンツのタグを置き換える（`editor`）
pub async fn set_content_tags(
    State(state): State<Arc<AppState>>,
    _session: EditorSession,
    Path((content_type, slug_or_id)): Path<(String, String)>,
    audit: AuditContext,
    Json(payload): Json<ContentTagsRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let service = tags(&state)?;
    let id = content_id(&state, &content_type, &slug_or_id).await?;
    let set = service
        .set_content_tags(&content_type, &id, &payload.tag_ids)
        .await
        .map_err(tag_error)?;
    audit_tag(
        &state,
        &audit,
        AuditAction::ContentTags,
        AuditOutcome::Success,
        json!({
            "content_type": content_type,
            "content_id": id,
            "tags": set.iter().map(|t| t.slug.clone()).collect::<Vec<_>>(),
        }),
    )
    .await;
    Ok(Json(json!({
        "content_type": content_type,
        "content_id": id,
        "tags": set.into_iter().map(|t| TagResponse::new(t, None)).collect::<Vec<_>>(),
    })))
}
