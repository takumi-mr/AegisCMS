use axum::{
    extract::{Multipart, Query, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use serde::Deserialize;
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
}

/// 絞り込み（mime type の前方一致・キーの部分一致）後、新しい順に並べて 1 ページ分を返す。
/// 戻り値は (ページ, 絞り込み後の総件数)。
fn select_objects(
    mut objects: Vec<StorageObject>,
    content_type: Option<&str>,
    query: Option<&str>,
    limit: usize,
    offset: usize,
) -> (Vec<StorageObject>, usize) {
    objects.retain(|o| {
        content_type.is_none_or(|ct| o.content_type.starts_with(ct))
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

        let (all, total) = select_objects(objects.clone(), None, None, 50, 0);
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

        let (images, total) = select_objects(objects.clone(), Some("image/"), None, 2, 1);
        assert_eq!(total, 3);
        assert_eq!(keys(&images), ["2026/09/old.png", "legacy.gif"]);

        let (found, total) = select_objects(objects, Some("image/"), Some("cat"), 50, 0);
        assert_eq!(total, 1);
        assert_eq!(keys(&found), ["2026/10/new-Cat.jpg"]);
    }
}
