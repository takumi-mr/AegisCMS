use axum::{
    extract::{Multipart, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use serde_json::json;
use std::sync::Arc;
use uuid::Uuid;

use crate::audit::AuditContext;
use crate::state::AppState;
use aegis_core::{AuditAction, AuditOutcome};

pub async fn upload_file(
    State(state): State<Arc<AppState>>,
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
