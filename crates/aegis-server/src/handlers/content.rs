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
use crate::db::{
    is_standard_column, resolve_relations_for_records, ContentListQuery, ContentRecord,
    DatabaseAdapter, ResolvedRelation,
};
use crate::render;
use crate::state::AppState;
use aegis_ast::BlockNode;
use aegis_core::{AegisError, AuditAction, AuditOutcome, PostContext};
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

pub async fn create_content(
    State(state): State<Arc<AppState>>,
    RequireRole(session): EditorSession,
    Path(content_type): Path<String>,
    audit: AuditContext,
    Json(mut payload): Json<CreateContentRequest>,
) -> Result<impl IntoResponse, (StatusCode, Json<serde_json::Value>)> {
    // 著者は既定でログインユーザー。他人を著者にできるのは admin のみ
    match &payload.author_id {
        None => payload.author_id = Some(session.user_id.clone()),
        Some(author) if author != &session.user_id && role_level(&session.role) < ADMIN => {
            return Err((
                StatusCode::FORBIDDEN,
                Json(json!({ "error": "Only admins can set another user as the author" })),
            ));
        }
        Some(_) => {}
    }

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

    // 1. AST 変換（組み込みパイプライン + プラグインの transform-content-ast フック）
    let transformed = render::transform_blocks(&state, payload.blocks).await;
    if let Some(e) = &transformed.plugin_error {
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
                        "error": e,
                    })),
            )
            .await;
    }
    let transformed_blocks = transformed.blocks;

    // 2. on-before-save フック（プラグインが返した title / slug / blocks を保存する）
    let post_ctx = PostContext {
        id: id.clone(),
        title: payload.title.clone(),
        slug: payload.slug.clone(),
        author_id: payload.author_id.clone().unwrap_or_default(),
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
                            .with_resource("content", Some(id.clone()))
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
            return Err(if rejected {
                (
                    StatusCode::UNPROCESSABLE_ENTITY,
                    Json(json!({ "error": e.to_string() })),
                )
            } else {
                // プラグインの不具合（トラップ・タイムアウト・不正な出力）はサーバー側のエラー
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(json!({ "error": format!("Plugin hook failed: {}", e) })),
                )
            });
        }
    };
    payload.title = saved.title;
    payload.slug = saved.slug;
    let transformed_blocks = saved.blocks;

    let blocks_json =
        serde_json::to_string(&transformed_blocks).unwrap_or_else(|_| "[]".to_string());
    let rendered_html = render::render_blocks_html(&state.config.security, &transformed_blocks);

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
        // カスタムフィールドの値が列型に変換できない場合はクライアントの入力エラー
        if let AegisError::Validation(_) = e {
            return Err((
                StatusCode::BAD_REQUEST,
                Json(json!({ "error": e.to_string() })),
            ));
        }
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
                        created_at: format!("2026-10-22T00:00:0{}Z", n),
                        updated_at: format!("2026-10-22T00:00:0{}Z", n),
                        custom_fields: HashMap::new(),
                    },
                )
                .await
                .unwrap();
        }
        let viewer = || {
            RequireRole(crate::auth::SessionData {
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
        let editor = RequireRole(crate::auth::SessionData {
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
