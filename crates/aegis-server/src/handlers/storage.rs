use axum::{
    extract::{Multipart, Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::sync::Arc;
use uuid::Uuid;

use crate::audit::AuditContext;
use crate::auth::{EditorSession, ViewerSession};
use crate::state::AppState;
use aegis_core::{AuditAction, AuditOutcome, StorageObject};

const DEFAULT_LIST_LIMIT: usize = 50;
const MAX_LIST_LIMIT: usize = 500;
/// 一覧のために走査するオブジェクト数の上限（超えた分は `truncated: true` で通知）
const MAX_LIST_SCAN: usize = 10_000;

#[derive(Debug, Default, Deserialize)]
pub struct ListObjectsParams {
    /// キーの前方一致（例: `2026/10/`）
    pub prefix: Option<String>,
    /// mime type の前方一致（例: `image/`）
    pub content_type: Option<String>,
    /// キーの部分一致（大文字小文字を区別しない）
    pub q: Option<String>,
    pub limit: Option<usize>,
    pub offset: Option<usize>,
    /// アバター画像（avatars/）を含めるか（既定: prefix が "avatars" で始まらない限り除外）
    pub include_avatars: Option<bool>,
}

/// 絞り込み（アバター除外・mime type の前方一致・キーの部分一致）後、新しい順に並べて 1 ページ分を返す。
/// 戻り値は (ページ, 絞り込み後の総件数)。
fn select_objects(
    mut objects: Vec<StorageObject>,
    content_type: Option<&str>,
    query: Option<&str>,
    limit: usize,
    offset: usize,
    include_avatars: bool,
) -> (Vec<StorageObject>, usize) {
    objects.retain(|o| {
        (include_avatars || !o.key.starts_with("avatars/"))
            && content_type.is_none_or(|ct| o.content_type.starts_with(ct))
            && query.is_none_or(|q| o.key.to_lowercase().contains(q))
    });
    // 日時が無いものは後ろ、同時刻はキーの降順
    objects.sort_by(|a, b| {
        b.last_modified
            .cmp(&a.last_modified)
            .then_with(|| b.key.cmp(&a.key))
    });
    let total = objects.len();
    (
        objects.into_iter().skip(offset).take(limit).collect(),
        total,
    )
}

/// `GET /api/v1/storage/objects` — アップロード済みファイルを新しい順に一覧する
pub async fn list_objects(
    State(state): State<Arc<AppState>>,
    _session: ViewerSession,
    Query(params): Query<ListObjectsParams>,
) -> Result<impl IntoResponse, (StatusCode, Json<serde_json::Value>)> {
    let non_empty = |v: Option<String>| v.map(|s| s.trim().to_string()).filter(|s| !s.is_empty());
    let prefix = non_empty(params.prefix).unwrap_or_default();
    let content_type = non_empty(params.content_type).map(|s| s.to_ascii_lowercase());
    let query = non_empty(params.q).map(|s| s.to_lowercase());
    let limit = params
        .limit
        .unwrap_or(DEFAULT_LIST_LIMIT)
        .clamp(1, MAX_LIST_LIMIT);
    let offset = params.offset.unwrap_or(0);
    let include_avatars = params
        .include_avatars
        .unwrap_or_else(|| prefix.starts_with("avatars/") || prefix == "avatars");

    // 上限 + 1 件取得して、走査しきれなかったかを判定する
    let mut objects = state
        .storage
        .list(prefix.trim_start_matches('/'), MAX_LIST_SCAN + 1)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": format!("Storage list error: {}", e) })),
            )
        })?;
    let truncated = objects.len() > MAX_LIST_SCAN;
    objects.truncate(MAX_LIST_SCAN);

    let (page, total) = select_objects(
        objects,
        content_type.as_deref(),
        query.as_deref(),
        limit,
        offset,
        include_avatars,
    );

    Ok(Json(json!({
        "objects": page,
        "total": total,
        "limit": limit,
        "offset": offset,
        "truncated": truncated,
    })))
}

pub async fn upload_file(
    State(state): State<Arc<AppState>>,
    _session: EditorSession,
    audit: AuditContext,
    mut multipart: Multipart,
) -> Result<impl IntoResponse, (StatusCode, Json<serde_json::Value>)> {
    let mut file_data: Option<Vec<u8>> = None;
    let mut file_name: Option<String> = None;
    let mut content_type: Option<String> = None;
    let mut custom_key: Option<String> = None;

    while let Ok(Some(field)) = multipart.next_field().await {
        let name = field.name().unwrap_or_default().to_string();
        if name == "file" {
            file_name = field.file_name().map(|s| s.to_string());
            content_type = field.content_type().map(|s| s.to_string());
            let bytes = field.bytes().await.map_err(|e| {
                // 上限超過は 413 として返す（axum の MultipartError が適切なステータスを持つ）
                let status = e.status();
                let message = if status == StatusCode::PAYLOAD_TOO_LARGE {
                    format!(
                        "File is too large (limit: {} MB)",
                        state.config.storage.max_upload_mb
                    )
                } else {
                    format!("Failed to read multipart file: {}", e)
                };
                (status, Json(json!({ "error": message })))
            })?;
            file_data = Some(bytes.to_vec());
        } else if name == "key" || name == "path" {
            if let Ok(text) = field.text().await {
                let trimmed = text.trim();
                if !trimmed.is_empty() {
                    custom_key = Some(trimmed.to_string());
                }
            }
        }
    }

    let data = file_data.ok_or_else(|| {
        (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "No 'file' field found in multipart request" })),
        )
    })?;

    let now = chrono::Utc::now();
    let original_name = file_name.unwrap_or_else(|| "file.bin".to_string());
    let clean_original_name = original_name
        .replace(['/', '\\', ' '], "_")
        .trim_matches('.')
        .to_string();

    let storage_key = if let Some(key) = custom_key {
        key
    } else {
        let date_folder = now.format("%Y/%m").to_string();
        let uid = Uuid::new_v4().to_string();
        format!("{}/{}-{}", date_folder, &uid[..8], clean_original_name)
    };

    let ct = content_type.unwrap_or_else(|| "application/octet-stream".to_string());

    let upload_event = |outcome: AuditOutcome, extra: serde_json::Value| {
        let mut details = json!({
            "original_name": original_name,
            "content_type": ct,
            "size": data.len(),
        });
        if let (Some(map), serde_json::Value::Object(extra)) = (details.as_object_mut(), extra) {
            map.extend(extra);
        }
        audit
            .event(AuditAction::StorageUpload, outcome)
            .with_resource("asset", Some(storage_key.clone()))
            .with_details(details)
    };

    let meta = match state.storage.put(&storage_key, &data, &ct).await {
        Ok(meta) => meta,
        Err(e) => {
            state
                .audit
                .record(upload_event(
                    AuditOutcome::Failure,
                    json!({ "error": e.to_string() }),
                ))
                .await;
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": format!("Storage put error: {}", e) })),
            ));
        }
    };
    state
        .audit
        .record(upload_event(
            AuditOutcome::Success,
            json!({ "url": meta.url }),
        ))
        .await;

    Ok((StatusCode::CREATED, Json(meta)))
}

#[derive(Debug, Default, Deserialize)]
pub struct DeleteObjectParams {
    /// 使用中（コンテンツから参照されている）でも削除する
    #[serde(default)]
    pub force: bool,
}

/// A saved content item or user profile that refers to a stored file
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ObjectReference {
    pub content_type: String,
    pub id: String,
    pub slug: String,
    pub title: String,
}

/// Saved contents whose blocks or custom fields contain `url`, or users whose `avatar_key`
/// matches `key`.
async fn find_references(
    state: &AppState,
    key: &str,
    url: &str,
) -> aegis_core::Result<Vec<ObjectReference>> {
    let schemas: Vec<_> = state.schemas.read().await.values().cloned().collect();
    let mut references = Vec::new();
    for schema in &schemas {
        for record in state.db.list_contents(schema).await? {
            let in_fields = || {
                serde_json::to_string(&record.custom_fields)
                    .is_ok_and(|fields| fields.contains(url))
            };
            if record.blocks_json.contains(url) || in_fields() {
                references.push(ObjectReference {
                    content_type: record.content_type,
                    id: record.id,
                    slug: record.slug,
                    title: record.title,
                });
            }
        }
    }

    // ユーザーのアバターとして使用されているか確認
    let users = state.db.list_users().await?;
    for user in users {
        if user.avatar_key.as_deref() == Some(key) {
            references.push(ObjectReference {
                content_type: "user".to_string(),
                id: user.id,
                slug: user.username.clone(),
                title: user.display_name.unwrap_or(user.username),
            });
        }
    }

    references.sort_by(|a, b| (&a.content_type, &a.slug).cmp(&(&b.content_type, &b.slug)));
    Ok(references)
}

/// `DELETE /api/v1/storage/objects/{*key}` — 保存済みファイルを削除する。
/// コンテンツやユーザーのアバターから参照されている場合は `409` と参照元を返し、`?force=true` のときだけ削除する
pub async fn delete_object(
    State(state): State<Arc<AppState>>,
    _session: EditorSession,
    Path(key): Path<String>,
    Query(params): Query<DeleteObjectParams>,
    audit: AuditContext,
) -> Result<StatusCode, (StatusCode, Json<serde_json::Value>)> {
    let key = key.trim().trim_start_matches('/').to_string();
    if key.is_empty() || key.split(['/', '\\']).any(|part| part == "..") {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "Invalid storage key" })),
        ));
    }
    let storage_error = |e: aegis_core::AegisError| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": format!("Storage error: {}", e) })),
        )
    };

    if !state.storage.exists(&key).await.map_err(storage_error)? {
        return Err((
            StatusCode::NOT_FOUND,
            Json(json!({ "error": format!("File '{}' not found", key) })),
        ));
    }

    let url = state.storage.public_url(&key);
    let references = find_references(&state, &key, &url).await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": format!("Database error: {}", e) })),
        )
    })?;
    if !references.is_empty() && !params.force {
        return Err((
            StatusCode::CONFLICT,
            Json(json!({
                "error": format!(
                    "File is used by {} item(s); pass force=true to delete it anyway",
                    references.len()
                ),
                "references": references,
            })),
        ));
    }

    let delete_event = |outcome: AuditOutcome, extra: serde_json::Value| {
        let mut details = json!({
            "url": url,
            "force": params.force,
            "references": references.len(),
        });
        if let (Some(map), serde_json::Value::Object(extra)) = (details.as_object_mut(), extra) {
            map.extend(extra);
        }
        audit
            .event(AuditAction::StorageDelete, outcome)
            .with_resource("asset", Some(key.clone()))
            .with_details(details)
    };
    if let Err(e) = state.storage.delete(&key).await {
        state
            .audit
            .record(delete_event(
                AuditOutcome::Failure,
                json!({ "error": e.to_string() }),
            ))
            .await;
        return Err(storage_error(e));
    }

    // 強制削除時、アバターとして参照していたユーザーの avatar_key をクリア
    for r in &references {
        if r.content_type == "user" {
            if let Err(e) = aegis_auth::users::set_avatar(state.db.as_ref(), &r.slug, None).await {
                tracing::warn!("Failed to clear avatar_key for user '{}': {}", r.slug, e);
            }
        }
    }

    state
        .audit
        .record(delete_event(AuditOutcome::Success, json!({})))
        .await;
    Ok(StatusCode::NO_CONTENT)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn object(key: &str, last_modified: Option<&str>) -> StorageObject {
        StorageObject {
            key: key.to_string(),
            url: format!("/uploads/{}", key),
            size: 1,
            content_type: aegis_core::guess_content_type(key).to_string(),
            last_modified: last_modified.map(str::to_string),
        }
    }

    fn keys(objects: &[StorageObject]) -> Vec<&str> {
        objects.iter().map(|o| o.key.as_str()).collect()
    }

    #[test]
    fn test_select_objects_sorts_filters_and_pages() {
        let objects = vec![
            object("2026/09/old.png", Some("2026-09-01T00:00:00Z")),
            object("legacy.gif", None),
            object("2026/10/new-Cat.jpg", Some("2026-10-02T00:00:00Z")),
            object("2026/10/report.pdf", Some("2026-10-03T00:00:00Z")),
        ];

        let (all, total) = select_objects(objects.clone(), None, None, 50, 0, false);
        assert_eq!(total, 4);
        assert_eq!(
            keys(&all),
            [
                "2026/10/report.pdf",
                "2026/10/new-Cat.jpg",
                "2026/09/old.png",
                "legacy.gif"
            ]
        );

        let (images, total) = select_objects(objects.clone(), Some("image/"), None, 2, 1, false);
        assert_eq!(total, 3);
        assert_eq!(keys(&images), ["2026/09/old.png", "legacy.gif"]);

        let (found, total) = select_objects(objects, Some("image/"), Some("cat"), 50, 0, false);
        assert_eq!(total, 1);
        assert_eq!(keys(&found), ["2026/10/new-Cat.jpg"]);
    }

    #[test]
    fn test_select_objects_excludes_avatars_by_default() {
        let objects = vec![
            object("avatars/user1-12345678.png", Some("2026-10-25T00:00:00Z")),
            object("2026/10/photo.jpg", Some("2026-10-24T00:00:00Z")),
        ];

        // 既定（include_avatars = false）では avatars/ は除外される
        let (filtered, total) = select_objects(objects.clone(), None, None, 50, 0, false);
        assert_eq!(total, 1);
        assert_eq!(keys(&filtered), ["2026/10/photo.jpg"]);

        // include_avatars = true のときは avatars/ も含まれる
        let (all, total) = select_objects(objects, None, None, 50, 0, true);
        assert_eq!(total, 2);
        assert_eq!(
            keys(&all),
            ["avatars/user1-12345678.png", "2026/10/photo.jpg"]
        );
    }

    use crate::auth::authorize::RequireRole;
    use aegis_db::ContentRecord;

    fn editor() -> EditorSession {
        RequireRole(aegis_auth::SessionData {
            id: "ses_test".to_string(),
            user_id: "editor1".to_string(),
            role: "editor".to_string(),
            created_at: String::new(),
            expires_at: String::new(),
            metadata: Default::default(),
        })
    }

    async fn delete(
        state: &Arc<AppState>,
        key: &str,
        force: bool,
    ) -> Result<StatusCode, (StatusCode, serde_json::Value)> {
        delete_object(
            State(state.clone()),
            editor(),
            Path(key.to_string()),
            Query(DeleteObjectParams { force }),
            AuditContext::default(),
        )
        .await
        .map_err(|(status, Json(body))| (status, body))
    }

    async fn save_article(state: &Arc<AppState>, slug: &str, blocks_json: String) {
        let schema = state.schemas.read().await["article"].clone();
        let now = chrono::Utc::now().to_rfc3339();
        let record = ContentRecord {
            id: Uuid::new_v4().to_string(),
            content_type: "article".to_string(),
            title: format!("Title of {}", slug),
            slug: slug.to_string(),
            status: "published".to_string(),
            blocks_json,
            author_id: Some("editor1".to_string()),
            published_at: Some(now.clone()),
            unpublish_at: None,
            created_at: now.clone(),
            updated_at: now,
            custom_fields: Default::default(),
        };
        state.db.insert_content(&schema, &record).await.unwrap();
    }

    async fn delete_events(state: &Arc<AppState>) -> Vec<aegis_core::AuditEvent> {
        state
            .db
            .query_audit_events(&aegis_core::AuditQuery {
                limit: 50,
                ..Default::default()
            })
            .await
            .unwrap()
            .into_iter()
            .filter(|e| e.action == AuditAction::StorageDelete)
            .collect()
    }

    #[tokio::test]
    async fn test_delete_unused_object() {
        let state = crate::test_support::test_state(aegis_runtime::PluginSupervisor::new()).await;
        let key = format!("tests/{}-unused.png", Uuid::new_v4());
        state.storage.put(&key, b"png", "image/png").await.unwrap();

        assert_eq!(
            delete(&state, &key, false).await,
            Ok(StatusCode::NO_CONTENT)
        );
        assert!(!state.storage.exists(&key).await.unwrap());
        // Deleting again is a 404
        assert_eq!(
            delete(&state, &key, false).await.unwrap_err().0,
            StatusCode::NOT_FOUND
        );

        let events = delete_events(&state).await;
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].outcome, AuditOutcome::Success);
        assert_eq!(events[0].resource_id.as_deref(), Some(key.as_str()));
        assert_eq!(events[0].details["references"], 0);
    }

    #[tokio::test]
    async fn test_delete_object_in_use_needs_force() {
        let state = crate::test_support::test_state(aegis_runtime::PluginSupervisor::new()).await;
        let key = format!("tests/{}-used.png", Uuid::new_v4());
        let meta = state.storage.put(&key, b"png", "image/png").await.unwrap();
        let blocks = json!([{ "name": "core/image", "attributes": { "src": meta.url } }]);
        save_article(&state, "uses-image", blocks.to_string()).await;
        save_article(&state, "other", "[]".to_string()).await;

        let (status, body) = delete(&state, &key, false).await.unwrap_err();
        assert_eq!(status, StatusCode::CONFLICT);
        assert_eq!(body["references"].as_array().unwrap().len(), 1);
        assert_eq!(body["references"][0]["slug"], "uses-image");
        assert_eq!(body["references"][0]["content_type"], "article");
        assert!(state.storage.exists(&key).await.unwrap());
        assert!(delete_events(&state).await.is_empty());

        assert_eq!(delete(&state, &key, true).await, Ok(StatusCode::NO_CONTENT));
        assert!(!state.storage.exists(&key).await.unwrap());
        let events = delete_events(&state).await;
        assert_eq!(events[0].details["force"], true);
        assert_eq!(events[0].details["references"], 1);
    }

    #[tokio::test]
    async fn test_delete_rejects_invalid_keys() {
        let state = crate::test_support::test_state(aegis_runtime::PluginSupervisor::new()).await;
        for key in ["", "  ", "../secret.txt", "a/../../b.png"] {
            assert_eq!(
                delete(&state, key, true).await.unwrap_err().0,
                StatusCode::BAD_REQUEST,
                "{:?}",
                key
            );
        }
    }

    #[tokio::test]
    async fn test_delete_object_used_as_avatar_needs_force() {
        let state = crate::test_support::test_state(aegis_runtime::PluginSupervisor::new()).await;
        let key = format!("avatars/{}-avatar.png", Uuid::new_v4());
        state.storage.put(&key, b"png", "image/png").await.unwrap();

        aegis_auth::users::create_user(
            state.db.as_ref(),
            aegis_auth::users::NewUser {
                username: "alice".to_string(),
                password: "password123".to_string(),
                role: "editor".to_string(),
                email: Some("alice@example.com".to_string()),
                display_name: Some("Alice Smith".to_string()),
            },
        )
        .await
        .unwrap();
        aegis_auth::users::set_avatar(state.db.as_ref(), "alice", Some(key.clone()))
            .await
            .unwrap();

        let (status, body) = delete(&state, &key, false).await.unwrap_err();
        assert_eq!(status, StatusCode::CONFLICT);
        assert_eq!(body["references"].as_array().unwrap().len(), 1);
        assert_eq!(body["references"][0]["slug"], "alice");
        assert_eq!(body["references"][0]["title"], "Alice Smith");
        assert_eq!(body["references"][0]["content_type"], "user");
        assert!(state.storage.exists(&key).await.unwrap());

        assert_eq!(delete(&state, &key, true).await, Ok(StatusCode::NO_CONTENT));
        assert!(!state.storage.exists(&key).await.unwrap());

        let user = state
            .db
            .get_user_by_username("alice")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(user.avatar_key, None);
    }
}
