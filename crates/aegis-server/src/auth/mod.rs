pub mod session;

use axum::http::HeaderMap;
use sha2::{Digest, Sha256};

#[allow(unused_imports)]
pub use session::{SessionData, SessionService};

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

/// セッション ID（Bearer トークン）を監査ログに残すための指紋。
/// トークン自体は認証情報なので記録せず、SHA-256 の先頭 16 桁のみを使って同一セッションを突き合わせる
pub fn session_fingerprint(session_id: &str) -> String {
    let digest = Sha256::digest(session_id.as_bytes());
    hex::encode(digest)[..16].to_string()
}
