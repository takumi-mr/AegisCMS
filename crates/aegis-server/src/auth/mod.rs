//! 認証の HTTP 層: セッション ID の取り出し、ロールを要求する extractor、`on-user-authenticated`
//! フックの適用。ユーザー・セッション・プロバイダ本体は `aegis_auth`

pub mod authorize;
pub mod hook;

use axum::http::HeaderMap;

pub use authorize::{AdminSession, EditorSession, ViewerSession};

/// `Authorization: Bearer <token>` または `X-Session-Id` ヘッダーからセッション ID を取り出す
pub fn extract_session_id(headers: &HeaderMap) -> Option<String> {
    if let Some(auth_val) = headers.get("authorization") {
        if let Ok(auth_str) = auth_val.to_str() {
            if let Some(token) = auth_str.strip_prefix("Bearer ") {
                let trimmed = token.trim();
                if !trimmed.is_empty() {
                    return Some(trimmed.to_string());
                }
            }
        }
    }

    if let Some(header_val) = headers.get("x-session-id") {
        if let Ok(val_str) = header_val.to_str() {
            let trimmed = val_str.trim();
            if !trimmed.is_empty() {
                return Some(trimmed.to_string());
            }
        }
    }

    None
}
