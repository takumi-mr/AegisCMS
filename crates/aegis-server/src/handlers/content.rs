use axum::{
    extract::{Path, Query, State},
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
use crate::auth::authorize::{role_level, RequireRole, ADMIN};
use crate::auth::{EditorSession, ViewerSession};
use crate::render;
use crate::state::AppState;
use aegis_ast::BlockNode;
use aegis_core::{AegisError, AuditAction, AuditOutcome, PostContext};
use aegis_db::{
    is_standard_column, resolve_relations_for_records, ContentListQuery, ContentRecord,
    DatabaseAdapter, ResolvedRelation,
};
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
    /// 予約公開（`status: "scheduled"`）の公開日時（RFC 3339）。他の状態では無視する
    #[serde(default)]
    pub published_at: Option<String>,
    /// 予約非公開（アーカイブ化）の日時（RFC 3339）。`published` または `scheduled` のみ有効
    #[serde(default)]
    pub unpublish_at: Option<String>,
    #[serde(flatten)]
    pub fields: HashMap<String, serde_json::Value>,
}

fn default_status() -> String {
    "draft".to_string()
}

/// 予約公開の状態。`published_at` の日時になると `scheduler` が `published` にする
pub const STATUS_SCHEDULED: &str = "scheduled";
const STATUS_PUBLISHED: &str = "published";

/// 保存する公開状態と `published_at`, `unpublish_at` を決める。
/// - `published`: 最初に公開した日時を保つ（今まで公開されていなければ `now`）
/// - `scheduled`: `requested_at` が必須。過ぎた日時ならその場で `published` にする
/// - `unpublish_at`: `published` または `scheduled` でのみ有効。指定があれば有効なRFC 3339で、公開日時より後であること。
/// - それ以外 (`draft`, `archived`): `published_at` / `unpublish_at` はなし (`None`)
fn resolve_publication(
    status: String,
    requested_at: Option<&str>,
    requested_unpublish_at: Option<&str>,
    current: Option<&ContentRecord>,
    now: chrono::DateTime<chrono::Utc>,
) -> Result<(String, Option<String>, Option<String>), ApiError> {
    let validate_unpublish = |unpub_str: Option<&str>,
                              pub_dt: Option<chrono::DateTime<chrono::Utc>>|
     -> Result<Option<String>, ApiError> {
        let Some(raw) = unpub_str.map(str::trim).filter(|s| !s.is_empty()) else {
            return Ok(None);
        };
        let unpub = chrono::DateTime::parse_from_rfc3339(raw)
            .map_err(|e| {
                api_error(
                    StatusCode::BAD_REQUEST,
                    format!("Invalid 'unpublish_at' '{}': {}", raw, e),
                )
            })?
            .with_timezone(&chrono::Utc);

        if let Some(pub_time) = pub_dt {
            if unpub <= pub_time {
                return Err(api_error(
                    StatusCode::BAD_REQUEST,
                    "Unpublish date ('unpublish_at') must be after publication date",
                ));
            }
        }
        Ok(Some(
            unpub.to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
        ))
    };

    let published = |requested_unpub: Option<&str>| -> Result<(String, Option<String>, Option<String>), ApiError> {
        let first = current
            .filter(|c| c.status == STATUS_PUBLISHED)
            .and_then(|c| c.published_at.clone());
        let pub_str = first.unwrap_or_else(|| now.to_rfc3339());
        let pub_dt = chrono::DateTime::parse_from_rfc3339(&pub_str)
            .ok()
            .map(|dt| dt.with_timezone(&chrono::Utc));
        let unpub = validate_unpublish(requested_unpub, pub_dt)?;
        Ok((
            STATUS_PUBLISHED.to_string(),
            Some(pub_str),
            unpub,
        ))
    };

    match status.as_str() {
        STATUS_PUBLISHED => published(requested_unpublish_at),
        STATUS_SCHEDULED => {
            let requested = requested_at.map(str::trim).filter(|s| !s.is_empty());
            let Some(requested) = requested else {
                return Err(api_error(
                    StatusCode::BAD_REQUEST,
                    "Scheduled content needs 'published_at' (when to publish it)",
                ));
            };
            let at = chrono::DateTime::parse_from_rfc3339(requested)
                .map_err(|e| {
                    api_error(
                        StatusCode::BAD_REQUEST,
                        format!("Invalid 'published_at' '{}': {}", requested, e),
                    )
                })?
                .with_timezone(&chrono::Utc);
            if at <= now {
                published(requested_unpublish_at)
            } else {
                let unpub = validate_unpublish(requested_unpublish_at, Some(at))?;
                Ok((
                    status,
                    Some(at.to_rfc3339_opts(chrono::SecondsFormat::Secs, true)),
                    unpub,
                ))
            }
        }
        _ => Ok((status, None, None)),
    }
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
    pub unpublish_at: Option<String>,
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
    resolve_relations_for_records(db, schemas, schema, &[fields])
        .await
        .pop()
        .unwrap_or_default()
}

/// `GET /api/v1/contents/{content_type}` のページング（`limit` の既定値と上限）
const DEFAULT_LIST_LIMIT: u64 = 50;
const MAX_LIST_LIMIT: u64 = 500;

#[derive(Debug, Default, Deserialize)]
pub struct ListContentParams {
    /// 1 ページの件数（既定 50、最大 500）
    pub limit: Option<u64>,
    #[serde(default)]
    pub offset: u64,
    /// `published` / `draft` など。省略時はすべて
    pub status: Option<String>,
}

/// The parts of a post `on-before-save` may change
#[derive(Debug)]
struct BeforeSaveOutput {
    title: String,
    slug: String,
    blocks: Vec<BlockNode>,
}

/// Takes the title, slug and blocks from the post the `on-before-save` chain returned. `id` and
/// `author_id` stay the server's: a plugin cannot reassign the record or its authorship. Blocks a
/// plugin changed go through the built-in transforms again (heading ids) and are sanitized when
/// rendered, like any other blocks.
fn accept_before_save(
    original: &PostContext,
    original_blocks: Vec<BlockNode>,
    returned: PostContext,
) -> Result<BeforeSaveOutput, AegisError> {
    if returned.id != original.id || returned.author_id != original.author_id {
        tracing::warn!(
            "Plugin hook 'on-before-save' changed id or author_id of '{}'; the change is ignored",
            original.slug
        );
    }
    if returned.slug.trim().is_empty() {
        return Err(AegisError::PluginRuntime(
            "on-before-save returned an empty slug".to_string(),
        ));
    }
    let blocks = if returned.blocks_json == original.blocks_json {
        original_blocks
    } else if returned.blocks_json.trim().is_empty() {
        Vec::new()
    } else {
        let blocks = serde_json::from_str(&returned.blocks_json).map_err(|e| {
            AegisError::PluginRuntime(format!(
                "on-before-save returned invalid blocks_json: {}",
                e
            ))
        })?;
        render::builtin_transforms(blocks)
    };
    Ok(BeforeSaveOutput {
        title: returned.title,
        slug: returned.slug,
        blocks,
    })
}

type ApiError = (StatusCode, Json<serde_json::Value>);

fn api_error(status: StatusCode, message: impl Into<String>) -> ApiError {
    (status, Json(json!({ "error": message.into() })))
}

async fn find_schema(state: &AppState, content_type: &str) -> Result<SchemaDefinition, ApiError> {
    state
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
        })
}

/// What the save pipeline produced for a create or update request
struct SavedContent {
    title: String,
    slug: String,
    blocks: Vec<BlockNode>,
    rendered_html: String,
    custom_fields: HashMap<String, serde_json::Value>,
}

/// The save pipeline shared by create and update: AST transforms (built-in + plugins'
/// `transform-content-ast`), `on-before-save`, rendering and custom field extraction. Plugin
/// failures and refusals are audited (`plugin.error`, and `action` as `denied`)
async fn run_save_pipeline(
    state: &AppState,
    audit: &AuditContext,
    action: AuditAction,
    schema: &SchemaDefinition,
    id: &str,
    author_id: &str,
    payload: CreateContentRequest,
) -> Result<SavedContent, ApiError> {
    let content_type = &schema.content_type;

    // 1. AST 変換（組み込みパイプライン + プラグインの transform-content-ast フック）
    let transformed = render::transform_blocks(state, payload.blocks).await;
    if let Some(e) = &transformed.plugin_error {
        // AST 変換フックの失敗は保存を妨げない（組み込みパイプラインの結果で続行）
        tracing::warn!("Plugin hook 'transform-content-ast' failed: {}", e);
        state
            .audit
            .record(
                audit
                    .event(AuditAction::PluginError, AuditOutcome::Failure)
                    .with_resource("content", Some(id.to_string()))
                    .with_details(json!({
                        "hook": "transform-content-ast",
                        "content_type": content_type,
                        "error": e,
                    })),
            )
            .await;
    }
    let transformed_blocks = transformed.blocks;

    // 2. on-before-save フック（プラグインが返した title / slug / blocks を保存する）
    let post_ctx = PostContext {
        id: id.to_string(),
        title: payload.title.clone(),
        slug: payload.slug.clone(),
        author_id: author_id.to_string(),
        blocks_json: serde_json::to_string(&transformed_blocks).unwrap_or_default(),
    };
    let hook_result = {
        let supervisor = state.supervisor.read().await;
        supervisor.apply_before_save(post_ctx.clone()).await
    };
    let saved = match hook_result
        .and_then(|post| accept_before_save(&post_ctx, transformed_blocks, post))
    {
        Ok(saved) => saved,
        Err(e) => {
            let rejected = matches!(e, AegisError::PluginRejected(_));
            // 明示的な拒否はプラグインの失敗ではないので plugin.error は記録しない
            if !rejected {
                state
                    .audit
                    .record(
                        audit
                            .event(AuditAction::PluginError, AuditOutcome::Failure)
                            .with_resource("content", Some(id.to_string()))
                            .with_details(json!({
                                "hook": "on-before-save",
                                "content_type": content_type,
                                "error": e.to_string(),
                            })),
                    )
                    .await;
            }
            state
                .audit
                .record(
                    audit
                        .event(action, AuditOutcome::Denied)
                        .with_resource("content", Some(id.to_string()))
                        .with_details(json!({
                            "content_type": content_type,
                            "slug": payload.slug,
                            "title": payload.title,
                            "reason": e.to_string(),
                        })),
                )
                .await;
            return Err(if rejected {
                api_error(StatusCode::UNPROCESSABLE_ENTITY, e.to_string())
            } else {
                // プラグインの不具合（トラップ・タイムアウト・不正な出力）はサーバー側のエラー
                api_error(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    format!("Plugin hook failed: {}", e),
                )
            });
        }
    };

    let rendered_html = render::render_blocks_html(&state.config.security, &saved.blocks);

    // 3. カスタムフィールドの抽出
    let custom_fields = schema
        .fields
        .iter()
        .filter(|field| !is_standard_column(&field.name))
        .filter_map(|field| {
            payload
                .fields
                .get(&field.name)
                .map(|val| (field.name.clone(), val.clone()))
        })
        .collect();

    Ok(SavedContent {
        title: saved.title,
        slug: saved.slug,
        blocks: saved.blocks,
        rendered_html,
        custom_fields,
    })
}

/// Records the outcome of storing a content record (`content.create` / `content.update`) and
/// maps a database error to the response
async fn record_store_result(
    state: &AppState,
    audit: &AuditContext,
    action: AuditAction,
    record: &ContentRecord,
    result: aegis_core::Result<()>,
) -> Result<(), ApiError> {
    let mut details = json!({
        "content_type": record.content_type,
        "slug": record.slug,
        "title": record.title,
        "status": record.status,
    });
    let outcome = match &result {
        Ok(()) => AuditOutcome::Success,
        Err(e) => {
            details["error"] = json!(e.to_string());
            AuditOutcome::Failure
        }
    };
    state
        .audit
        .record(
            audit
                .event(action, outcome)
                .with_resource("content", Some(record.id.clone()))
                .with_details(details),
        )
        .await;
    result.map_err(|e| match e {
        // カスタムフィールドの値が列型に変換できない場合はクライアントの入力エラー
        AegisError::Validation(_) => api_error(StatusCode::BAD_REQUEST, e.to_string()),
        other => api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Database error: {}", other),
        ),
    })
}

async fn content_response(
    state: &AppState,
    schema: &SchemaDefinition,
    record: ContentRecord,
    blocks: Vec<BlockNode>,
    rendered_html: String,
) -> ContentResponse {
    let relations = {
        let schemas = state.schemas.read().await;
        resolve_relations_for_item(&*state.db, &schemas, schema, &record.custom_fields).await
    };
    ContentResponse {
        id: record.id,
        content_type: record.content_type,
        title: record.title,
        slug: record.slug,
        status: record.status,
        blocks,
        rendered_html,
        author_id: record.author_id,
        published_at: record.published_at,
        unpublish_at: record.unpublish_at,
        created_at: record.created_at,
        updated_at: record.updated_at,
        fields: record.custom_fields,
        relations,
    }
}

pub async fn create_content(
    State(state): State<Arc<AppState>>,
    RequireRole(session): EditorSession,
    Path(content_type): Path<String>,
    audit: AuditContext,
    Json(mut payload): Json<CreateContentRequest>,
) -> Result<impl IntoResponse, ApiError> {
    // 著者は既定でログインユーザー。他人を著者にできるのは admin のみ
    match &payload.author_id {
        None => payload.author_id = Some(session.user_id.clone()),
        Some(author) if author != &session.user_id && role_level(&session.role) < ADMIN => {
            return Err(api_error(
                StatusCode::FORBIDDEN,
                "Only admins can set another user as the author",
            ));
        }
        Some(_) => {}
    }

    let schema = find_schema(&state, &content_type).await?;
    let id = Uuid::new_v4().to_string();
    let now_at = chrono::Utc::now();
    let now = now_at.to_rfc3339();
    let author_id = payload.author_id.clone();
    let (status, published_at, unpublish_at) = resolve_publication(
        payload.status.clone(),
        payload.published_at.as_deref(),
        payload.unpublish_at.as_deref(),
        None,
        now_at,
    )?;

    let saved = run_save_pipeline(
        &state,
        &audit,
        AuditAction::ContentCreate,
        &schema,
        &id,
        author_id.as_deref().unwrap_or_default(),
        payload,
    )
    .await?;

    // 4. ContentRecord 作成および DatabaseAdapter による永続化
    let record = ContentRecord {
        id,
        content_type,
        title: saved.title,
        slug: saved.slug,
        status,
        blocks_json: serde_json::to_string(&saved.blocks).unwrap_or_else(|_| "[]".to_string()),
        author_id,
        published_at,
        unpublish_at,
        created_at: now.clone(),
        updated_at: now,
        custom_fields: saved.custom_fields,
    };
    let result = state.db.insert_content(&schema, &record).await;
    record_store_result(&state, &audit, AuditAction::ContentCreate, &record, result).await?;

    Ok((
        StatusCode::CREATED,
        Json(content_response(&state, &schema, record, saved.blocks, saved.rendered_html).await),
    ))
}

/// `PUT /api/v1/contents/{content_type}/{slug_or_id}` — 保存済みのコンテンツを上書きする（`editor`）。
/// 本文は作成と同じで、保存と同じパイプライン（AST 変換・`on-before-save`）を通る。
/// `author_id` を省略すると今の著者のまま。初めて `published` になったときに `published_at` を付ける
/// （`scheduled` は `published_at` に予約した日時）
pub async fn update_content(
    State(state): State<Arc<AppState>>,
    RequireRole(session): EditorSession,
    Path((content_type, slug_or_id)): Path<(String, String)>,
    audit: AuditContext,
    Json(payload): Json<CreateContentRequest>,
) -> Result<Json<ContentResponse>, ApiError> {
    let schema = find_schema(&state, &content_type).await?;
    let current = state
        .db
        .get_content(&schema, &slug_or_id)
        .await
        .map_err(|e| {
            api_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Database error: {}", e),
            )
        })?
        .ok_or_else(|| {
            api_error(
                StatusCode::NOT_FOUND,
                format!("Content '{}' not found", slug_or_id),
            )
        })?;

    let author_id = match &payload.author_id {
        None => current.author_id.clone(),
        Some(author)
            if Some(author) != current.author_id.as_ref()
                && author != &session.user_id
                && role_level(&session.role) < ADMIN =>
        {
            return Err(api_error(
                StatusCode::FORBIDDEN,
                "Only admins can set another user as the author",
            ));
        }
        Some(author) => Some(author.clone()),
    };

    let now_at = chrono::Utc::now();
    let now = now_at.to_rfc3339();
    let (status, published_at, unpublish_at) = resolve_publication(
        payload.status.clone(),
        payload.published_at.as_deref(),
        payload.unpublish_at.as_deref(),
        Some(&current),
        now_at,
    )?;
    let saved = run_save_pipeline(
        &state,
        &audit,
        AuditAction::ContentUpdate,
        &schema,
        &current.id,
        author_id.as_deref().unwrap_or_default(),
        payload,
    )
    .await?;

    let record = ContentRecord {
        id: current.id,
        content_type,
        title: saved.title,
        slug: saved.slug,
        status,
        blocks_json: serde_json::to_string(&saved.blocks).unwrap_or_else(|_| "[]".to_string()),
        author_id,
        published_at,
        unpublish_at,
        created_at: current.created_at,
        updated_at: now,
        custom_fields: saved.custom_fields,
    };
    let result = match state.db.update_content(&schema, &record).await {
        Ok(true) => Ok(()),
        // 取得と更新の間に削除された
        Ok(false) => {
            return Err(api_error(
                StatusCode::NOT_FOUND,
                format!("Content '{}' not found", slug_or_id),
            ))
        }
        Err(e) => Err(e),
    };
    record_store_result(&state, &audit, AuditAction::ContentUpdate, &record, result).await?;

    Ok(Json(
        content_response(&state, &schema, record, saved.blocks, saved.rendered_html).await,
    ))
}

/// `DELETE /api/v1/contents/{content_type}/{slug_or_id}` — コンテンツを削除する（`editor`）
pub async fn delete_content(
    State(state): State<Arc<AppState>>,
    _session: EditorSession,
    Path((content_type, slug_or_id)): Path<(String, String)>,
    audit: AuditContext,
) -> Result<StatusCode, ApiError> {
    let schema = find_schema(&state, &content_type).await?;
    let db_error = |e: AegisError| {
        api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Database error: {}", e),
        )
    };
    let current = state
        .db
        .get_content(&schema, &slug_or_id)
        .await
        .map_err(db_error)?
        .ok_or_else(|| {
            api_error(
                StatusCode::NOT_FOUND,
                format!("Content '{}' not found", slug_or_id),
            )
        })?;
    let result = state.db.delete_content(&schema, &current.id).await;
    let mut details = json!({
        "content_type": content_type,
        "slug": current.slug,
        "title": current.title,
    });
    let outcome = match &result {
        Ok(_) => AuditOutcome::Success,
        Err(e) => {
            details["error"] = json!(e.to_string());
            AuditOutcome::Failure
        }
    };
    state
        .audit
        .record(
            audit
                .event(AuditAction::ContentDelete, outcome)
                .with_resource("content", Some(current.id.clone()))
                .with_details(details),
        )
        .await;
    result.map_err(db_error)?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn list_content(
    State(state): State<Arc<AppState>>,
    _session: ViewerSession,
    Path(content_type): Path<String>,
    Query(params): Query<ListContentParams>,
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

    let limit = params
        .limit
        .unwrap_or(DEFAULT_LIST_LIMIT)
        .clamp(1, MAX_LIST_LIMIT);
    let query = ContentListQuery {
        status: params.status.filter(|s| !s.is_empty()),
        limit: Some(limit),
        offset: params.offset,
    };
    let page = state
        .db
        .list_contents_page(&schema_clone, &query)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": format!("Database error: {}", e) })),
            )
        })?;

    let relations = {
        let schemas = state.schemas.read().await;
        let fields: Vec<_> = page.records.iter().map(|r| &r.custom_fields).collect();
        resolve_relations_for_records(&*state.db, &schemas, &schema_clone, &fields).await
    };

    let list: Vec<ContentResponse> = page
        .records
        .into_iter()
        .zip(relations)
        .map(|(r, relations)| {
            let blocks: Vec<BlockNode> = serde_json::from_str(&r.blocks_json).unwrap_or_default();
            let rendered_html = render::render_blocks_html(&state.config.security, &blocks);
            ContentResponse {
                id: r.id,
                content_type: r.content_type,
                title: r.title,
                slug: r.slug,
                status: r.status,
                blocks,
                rendered_html,
                author_id: r.author_id,
                published_at: r.published_at,
                unpublish_at: r.unpublish_at,
                created_at: r.created_at,
                updated_at: r.updated_at,
                fields: r.custom_fields,
                relations,
            }
        })
        .collect();

    Ok(Json(json!({
        "contents": list,
        "total": page.total,
        "limit": limit,
        "offset": params.offset,
    })))
}

pub async fn get_content(
    State(state): State<Arc<AppState>>,
    _session: ViewerSession,
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
            let rendered_html = render::render_blocks_html(&state.config.security, &blocks);
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
                unpublish_at: r.unpublish_at,
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

/// コンテンツが現在公開中であるか（status == "published" かつ unpublish_at を迎えていない）を判定する
pub fn is_currently_published(
    status: &str,
    unpublish_at: Option<&str>,
    now: chrono::DateTime<chrono::Utc>,
) -> bool {
    if status != STATUS_PUBLISHED {
        return false;
    }
    if let Some(unpub) = unpublish_at {
        if let Ok(due) = chrono::DateTime::parse_from_rfc3339(unpub) {
            if due <= now {
                return false;
            }
        }
    }
    true
}

#[derive(Debug, Default, Deserialize)]
pub struct ScheduleParams {
    /// 絞り込むコンテンツタイプ（省略時は全スキーマ）
    pub content_type: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ScheduleItem {
    pub id: String,
    pub content_type: String,
    pub title: String,
    pub slug: String,
    pub status: String,
    pub published_at: Option<String>,
    pub unpublish_at: Option<String>,
    pub author_id: Option<String>,
    pub updated_at: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ScheduleResponse {
    pub items: Vec<ScheduleItem>,
    pub total: usize,
}

/// `GET /api/v1/schedule` — 公開予定（status: "scheduled"）または非公開予定（unpublish_at あり）または公開中コンテンツの一覧
pub async fn list_schedule(
    State(state): State<Arc<AppState>>,
    _session: ViewerSession,
    Query(params): Query<ScheduleParams>,
) -> Result<Json<ScheduleResponse>, (StatusCode, Json<serde_json::Value>)> {
    let schemas: Vec<SchemaDefinition> = {
        let read = state.schemas.read().await;
        if let Some(ct) = &params.content_type {
            match read.get(ct) {
                Some(s) => vec![s.clone()],
                None => {
                    return Err((
                        StatusCode::NOT_FOUND,
                        Json(json!({ "error": format!("Schema '{}' not found", ct) })),
                    ));
                }
            }
        } else {
            read.values().cloned().collect()
        }
    };

    let mut items = Vec::new();
    for schema in schemas {
        let query = ContentListQuery::default();
        if let Ok(page) = state.db.list_contents_page(&schema, &query).await {
            for record in page.records {
                if record.status == STATUS_SCHEDULED
                    || record.unpublish_at.is_some()
                    || (record.status == STATUS_PUBLISHED && record.published_at.is_some())
                {
                    items.push(ScheduleItem {
                        id: record.id,
                        content_type: record.content_type,
                        title: record.title,
                        slug: record.slug,
                        status: record.status,
                        published_at: record.published_at,
                        unpublish_at: record.unpublish_at,
                        author_id: record.author_id,
                        updated_at: record.updated_at,
                    });
                }
            }
        }
    }

    items.sort_by(|a, b| {
        let time_a = a
            .published_at
            .as_deref()
            .or(a.unpublish_at.as_deref())
            .unwrap_or(&a.updated_at);
        let time_b = b
            .published_at
            .as_deref()
            .or(b.unpublish_at.as_deref())
            .unwrap_or(&b.updated_at);
        time_a.cmp(time_b)
    });

    let total = items.len();
    Ok(Json(ScheduleResponse { items, total }))
}

/// 商品バリエーションの新規作成・更新リクエスト
#[derive(Debug, Deserialize)]
pub struct UpsertVariantRequest {
    #[serde(default)]
    pub id: Option<String>,
    #[serde(default)]
    pub sku: Option<String>,
    pub title: String,
    #[serde(default)]
    pub price_override: Option<i64>,
    #[serde(default)]
    pub stock: Option<i64>,
    #[serde(default)]
    pub attributes: HashMap<String, String>,
    #[serde(default)]
    pub position: i32,
}

/// `GET /api/v1/contents/{content_type}/{id}/variants`
/// 指定された商品のバリエーション一覧を取得（viewer以上）
pub async fn list_variants(
    State(state): State<Arc<AppState>>,
    _session: ViewerSession,
    Path((_content_type, id)): Path<(String, String)>,
) -> Result<Json<Vec<aegis_core::ProductVariant>>, ApiError> {
    let variants: Vec<aegis_core::ProductVariant> = state
        .db
        .list_product_variants(&id)
        .await
        .map_err(|e| api_error(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .into_iter()
        .map(Into::into)
        .collect();
    Ok(Json(variants))
}

/// `POST /api/v1/contents/{content_type}/{id}/variants`
/// 商品バリエーションを作成または更新（editor以上）
pub async fn upsert_variant(
    State(state): State<Arc<AppState>>,
    RequireRole(_session): EditorSession,
    Path((content_type, id)): Path<(String, String)>,
    Json(payload): Json<UpsertVariantRequest>,
) -> Result<(StatusCode, Json<aegis_core::ProductVariant>), ApiError> {
    let schema = find_schema(&state, &content_type).await?;
    let product = state
        .db
        .get_content(&schema, &id)
        .await
        .map_err(|e| api_error(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .ok_or_else(|| api_error(StatusCode::NOT_FOUND, format!("Product '{}' not found", id)))?;

    if payload.title.trim().is_empty() {
        return Err(api_error(
            StatusCode::BAD_REQUEST,
            "Variant title is required",
        ));
    }

    let now = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    let variant_id = payload
        .id
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());

    let existing = state
        .db
        .get_product_variant(&variant_id)
        .await
        .map_err(|e| api_error(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    let created_at = existing
        .as_ref()
        .map(|e| e.created_at.clone())
        .unwrap_or_else(|| now.clone());

    let variant = aegis_core::ProductVariant {
        id: variant_id,
        product_id: product.id,
        sku: payload.sku.filter(|s| !s.trim().is_empty()),
        title: payload.title.trim().to_string(),
        price_override: payload.price_override,
        stock: payload.stock,
        attributes: payload.attributes,
        position: payload.position,
        created_at,
        updated_at: now,
    };
    let record: aegis_db::ProductVariantRecord = (&variant).into();

    state
        .db
        .upsert_product_variant(&record)
        .await
        .map_err(|e| api_error(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok((StatusCode::OK, Json(variant)))
}

/// `DELETE /api/v1/contents/{content_type}/{id}/variants/{variant_id}`
/// 商品バリエーションを削除（editor以上）
pub async fn delete_variant(
    State(state): State<Arc<AppState>>,
    RequireRole(_session): EditorSession,
    Path((_content_type, _id, variant_id)): Path<(String, String, String)>,
) -> Result<StatusCode, ApiError> {
    state
        .db
        .delete_product_variant(&variant_id)
        .await
        .map_err(|e| api_error(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    Ok(StatusCode::NO_CONTENT)
}

#[cfg(test)]
mod tests {
    use super::*;
    use aegis_db::SqliteAdapter;
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
            unpublish_at: None,
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

    /// What the test `on-before-save` plugin does
    #[derive(Clone, Copy)]
    enum BeforeSave {
        /// Rewrites the title and slug, appends a heading, and tries to change the author
        Edit,
        /// Rejects the save (an SDK plugin returning `Err`)
        Reject,
        /// Fails (a trap or timeout)
        Fail,
    }

    struct BeforeSavePlugin {
        manifest: aegis_core::PluginManifest,
        mode: BeforeSave,
    }

    #[async_trait::async_trait]
    impl aegis_runtime::AegisPlugin for BeforeSavePlugin {
        fn manifest(&self) -> &aegis_core::PluginManifest {
            &self.manifest
        }

        async fn on_before_save(&self, mut post: PostContext) -> aegis_core::Result<PostContext> {
            match self.mode {
                BeforeSave::Edit => {
                    post.title = format!("{} (edited)", post.title.trim());
                    post.slug = post.slug.to_lowercase();
                    post.author_id = "mallory".to_string();
                    let mut blocks: Vec<BlockNode> = serde_json::from_str(&post.blocks_json)?;
                    let mut heading = BlockNode::new("core/heading");
                    heading.attributes = json!({ "level": 2 });
                    heading.text = Some("Added Section".to_string());
                    blocks.push(heading);
                    post.blocks_json = serde_json::to_string(&blocks)?;
                    Ok(post)
                }
                BeforeSave::Reject => Err(AegisError::PluginRejected(
                    "guard: title is required".to_string(),
                )),
                BeforeSave::Fail => Err(AegisError::PluginRuntime("trap".to_string())),
            }
        }
    }

    async fn state_with(mode: BeforeSave) -> Arc<AppState> {
        let mut supervisor = aegis_runtime::PluginSupervisor::new();
        supervisor.register(Arc::new(BeforeSavePlugin {
            manifest: aegis_core::PluginManifest {
                name: "guard".to_string(),
                version: "1.0.0".to_string(),
                entrypoint: "plugin.wasm".to_string(),
                capabilities: aegis_core::PluginCapabilities {
                    network: None,
                    storage: None,
                    hooks: vec!["on-before-save".to_string()],
                },
                resource_limits: Default::default(),
                ..Default::default()
            },
            mode,
        }));
        crate::test_support::test_state(supervisor).await
    }

    #[tokio::test]
    async fn test_list_content_pages_and_filters() {
        let state = crate::test_support::test_state(aegis_runtime::PluginSupervisor::new()).await;
        let schema = state.schemas.read().await["article"].clone();
        for n in 1..=5 {
            let status = if n == 5 { "draft" } else { "published" };
            state
                .db
                .insert_content(
                    &schema,
                    &ContentRecord {
                        id: format!("id-{}", n),
                        content_type: "article".to_string(),
                        title: format!("Post {}", n),
                        slug: format!("post-{}", n),
                        status: status.to_string(),
                        blocks_json: r#"[{"name":"core/paragraph","text":"Body"}]"#.to_string(),
                        author_id: None,
                        published_at: None,
                        unpublish_at: None,
                        created_at: format!("2026-10-22T00:00:0{}Z", n),
                        updated_at: format!("2026-10-22T00:00:0{}Z", n),
                        custom_fields: HashMap::new(),
                    },
                )
                .await
                .unwrap();
        }
        let viewer = || {
            RequireRole(aegis_auth::SessionData {
                id: "ses_viewer".to_string(),
                user_id: "viewer1".to_string(),
                role: "viewer".to_string(),
                created_at: String::new(),
                expires_at: String::new(),
                metadata: Default::default(),
            })
        };
        let list = |params: ListContentParams| {
            let state = state.clone();
            async move {
                let response = list_content(
                    State(state),
                    viewer(),
                    Path("article".to_string()),
                    Query(params),
                )
                .await
                .map_err(|(status, _)| status)
                .unwrap()
                .into_response();
                let body = axum::body::to_bytes(response.into_body(), usize::MAX)
                    .await
                    .unwrap();
                serde_json::from_slice::<serde_json::Value>(&body).unwrap()
            }
        };
        let slugs = |body: &serde_json::Value| -> Vec<String> {
            body["contents"]
                .as_array()
                .unwrap()
                .iter()
                .map(|c| c["slug"].as_str().unwrap().to_string())
                .collect()
        };

        let all = list(ListContentParams::default()).await;
        assert_eq!(all["total"], 5);
        assert_eq!(all["limit"], DEFAULT_LIST_LIMIT);
        assert_eq!(slugs(&all).len(), 5);
        assert!(all["contents"][0]["rendered_html"]
            .as_str()
            .unwrap()
            .contains("Body"));

        let page = list(ListContentParams {
            limit: Some(2),
            offset: 1,
            status: Some("published".to_string()),
        })
        .await;
        assert_eq!(page["total"], 4);
        assert_eq!(slugs(&page), vec!["post-3", "post-2"]);

        let capped = list(ListContentParams {
            limit: Some(100_000),
            ..Default::default()
        })
        .await;
        assert_eq!(capped["limit"], MAX_LIST_LIMIT);
    }

    async fn create(
        state: &Arc<AppState>,
    ) -> Result<ContentResponse, (StatusCode, serde_json::Value)> {
        let editor = RequireRole(aegis_auth::SessionData {
            id: "ses_test".to_string(),
            user_id: "editor1".to_string(),
            role: "editor".to_string(),
            created_at: String::new(),
            expires_at: String::new(),
            metadata: Default::default(),
        });
        let payload: CreateContentRequest = serde_json::from_value(json!({
            "title": "Hello",
            "slug": "Hello-World",
            "blocks": [{ "name": "core/paragraph", "text": "Body" }]
        }))
        .unwrap();
        let response = create_content(
            State(state.clone()),
            editor,
            Path("article".to_string()),
            AuditContext::default(),
            Json(payload),
        )
        .await
        .map_err(|(status, Json(body))| (status, body))?
        .into_response();
        let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        Ok(serde_json::from_slice(&bytes).unwrap())
    }

    async fn audit_actions(state: &Arc<AppState>) -> Vec<(AuditAction, AuditOutcome)> {
        let mut events = state
            .db
            .query_audit_events(&aegis_core::AuditQuery {
                limit: 50,
                ..Default::default()
            })
            .await
            .unwrap();
        events.reverse();
        events.into_iter().map(|e| (e.action, e.outcome)).collect()
    }

    fn session(user_id: &str, role: &str) -> EditorSession {
        RequireRole(aegis_auth::SessionData {
            id: format!("ses_{}", user_id),
            user_id: user_id.to_string(),
            role: role.to_string(),
            created_at: String::new(),
            expires_at: String::new(),
            metadata: Default::default(),
        })
    }

    async fn update(
        state: &Arc<AppState>,
        actor: EditorSession,
        slug_or_id: &str,
        payload: serde_json::Value,
    ) -> Result<ContentResponse, StatusCode> {
        update_content(
            State(state.clone()),
            actor,
            Path(("article".to_string(), slug_or_id.to_string())),
            AuditContext::default(),
            Json(serde_json::from_value(payload).unwrap()),
        )
        .await
        .map(|Json(body)| body)
        .map_err(|(status, _)| status)
    }

    #[tokio::test]
    async fn test_update_and_delete_content() {
        let state = state_with(BeforeSave::Edit).await;
        let created = create(&state).await.unwrap();
        assert_eq!(created.published_at, None);

        // Publishing sets published_at once; hooks run again on update
        let published = update(
            &state,
            session("editor2", "editor"),
            &created.id,
            json!({ "title": "Second", "slug": "second", "status": "published",
                    "blocks": [{ "name": "core/paragraph", "text": "New body" }] }),
        )
        .await
        .unwrap();
        assert_eq!(published.id, created.id);
        assert_eq!(published.title, "Second (edited)");
        assert_eq!(published.created_at, created.created_at);
        // The author stays when author_id is omitted
        assert_eq!(published.author_id.as_deref(), Some("editor1"));
        let first_published = published.published_at.clone().expect("published");

        let again = update(
            &state,
            session("editor2", "editor"),
            "second",
            json!({ "title": "Third", "slug": "second", "status": "published" }),
        )
        .await
        .unwrap();
        assert_eq!(
            again.published_at.as_deref(),
            Some(first_published.as_str())
        );

        // Only admins may hand the content to someone else
        assert_eq!(
            update(
                &state,
                session("editor2", "editor"),
                "second",
                json!({ "title": "x", "slug": "second", "author_id": "mallory" }),
            )
            .await
            .unwrap_err(),
            StatusCode::FORBIDDEN
        );
        assert_eq!(
            update(
                &state,
                session("editor2", "editor"),
                "missing",
                json!({ "title": "x", "slug": "x" })
            )
            .await
            .unwrap_err(),
            StatusCode::NOT_FOUND
        );

        let delete = |slug: &'static str| {
            let state = state.clone();
            async move {
                delete_content(
                    State(state),
                    session("editor2", "editor"),
                    Path(("article".to_string(), slug.to_string())),
                    AuditContext::default(),
                )
                .await
                .map_err(|(status, _)| status)
            }
        };
        assert_eq!(delete("second").await, Ok(StatusCode::NO_CONTENT));
        assert_eq!(delete("second").await, Err(StatusCode::NOT_FOUND));

        let actions = audit_actions(&state).await;
        assert_eq!(
            actions,
            vec![
                (AuditAction::ContentCreate, AuditOutcome::Success),
                (AuditAction::ContentUpdate, AuditOutcome::Success),
                (AuditAction::ContentUpdate, AuditOutcome::Success),
                (AuditAction::ContentDelete, AuditOutcome::Success),
            ]
        );
    }

    #[tokio::test]
    async fn test_before_save_changes_are_saved() {
        let state = state_with(BeforeSave::Edit).await;
        let created = create(&state).await.unwrap();
        assert_eq!(created.title, "Hello (edited)");
        assert_eq!(created.slug, "hello-world");
        // A plugin cannot reassign authorship
        assert_eq!(created.author_id.as_deref(), Some("editor1"));
        // The added heading goes through the built-in transforms and the renderer
        assert_eq!(created.blocks.len(), 2);
        assert_eq!(created.blocks[1].attributes["id"], json!("added-section"));
        assert!(created
            .rendered_html
            .contains("<h2 id=\"added-section\">Added Section</h2>"));

        let schema = state.schemas.read().await["article"].clone();
        let stored = state
            .db
            .get_content(&schema, "hello-world")
            .await
            .unwrap()
            .expect("saved under the slug the plugin returned");
        assert_eq!(stored.title, "Hello (edited)");
        assert_eq!(stored.author_id.as_deref(), Some("editor1"));
        assert!(stored.blocks_json.contains("Added Section"));
    }

    #[tokio::test]
    async fn test_before_save_rejection_blocks_the_save() {
        let state = state_with(BeforeSave::Reject).await;
        let (status, body) = create(&state).await.unwrap_err();
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
        assert_eq!(
            body["error"],
            "Rejected by plugin: guard: title is required"
        );

        let schema = state.schemas.read().await["article"].clone();
        assert!(state.db.list_contents(&schema).await.unwrap().is_empty());
        // A rejection is not a plugin failure
        assert_eq!(
            audit_actions(&state).await,
            vec![(AuditAction::ContentCreate, AuditOutcome::Denied)]
        );
    }

    #[tokio::test]
    async fn test_before_save_failure_blocks_the_save() {
        let state = state_with(BeforeSave::Fail).await;
        let (status, _) = create(&state).await.unwrap_err();
        assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);

        let schema = state.schemas.read().await["article"].clone();
        assert!(state.db.list_contents(&schema).await.unwrap().is_empty());
        assert_eq!(
            audit_actions(&state).await,
            vec![
                (AuditAction::PluginError, AuditOutcome::Failure),
                (AuditAction::ContentCreate, AuditOutcome::Denied),
            ]
        );
    }

    #[test]
    fn test_accept_before_save_validates_the_returned_post() {
        let original = PostContext {
            id: "id-1".to_string(),
            title: "T".to_string(),
            slug: "t".to_string(),
            author_id: "editor1".to_string(),
            blocks_json: "[]".to_string(),
        };
        let with = |f: fn(&mut PostContext)| {
            let mut post = original.clone();
            f(&mut post);
            accept_before_save(&original, Vec::new(), post)
        };
        assert!(with(|p| p.slug = "  ".to_string()).is_err());
        assert!(with(|p| p.blocks_json = "{not json".to_string()).is_err());
        assert!(with(|p| p.blocks_json = String::new())
            .unwrap()
            .blocks
            .is_empty());
        assert_eq!(with(|_| {}).unwrap().title, "T");
    }
}
