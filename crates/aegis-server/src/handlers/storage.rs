use axum::{
    extract::{Multipart, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use serde_json::json;
use std::sync::Arc;
use uuid::Uuid;

use crate::state::AppState;

pub async fn upload_file(
    State(state): State<Arc<AppState>>,
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
                (
                    StatusCode::BAD_REQUEST,
                    Json(json!({ "error": format!("Failed to read multipart file: {}", e) })),
                )
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

    let meta = state
        .storage
        .put(&storage_key, &data, &ct)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": format!("Storage put error: {}", e) })),
            )
        })?;

    Ok((StatusCode::CREATED, Json(meta)))
}
