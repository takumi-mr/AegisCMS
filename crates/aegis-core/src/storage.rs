use crate::error::Result;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageMeta {
    pub key: String,
    pub url: String,
    pub size: usize,
    pub content_type: String,
}

/// An object already stored in a backend, as returned by [`StorageBackend::list`]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StorageObject {
    pub key: String,
    pub url: String,
    pub size: u64,
    /// Guessed from the key's extension (backends do not keep the uploaded mime type)
    pub content_type: String,
    /// RFC 3339 timestamp of the last write, when the backend reports one
    pub last_modified: Option<String>,
}

/// Guess a mime type from a storage key's extension (`application/octet-stream` if unknown)
pub fn guess_content_type(key: &str) -> &'static str {
    let ext = key
        .rsplit_once('.')
        .map(|(_, ext)| ext.to_ascii_lowercase())
        .unwrap_or_default();
    match ext.as_str() {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "avif" => "image/avif",
        "svg" => "image/svg+xml",
        "ico" => "image/x-icon",
        "bmp" => "image/bmp",
        "pdf" => "application/pdf",
        "json" => "application/json",
        "txt" => "text/plain",
        "csv" => "text/csv",
        "css" => "text/css",
        "js" | "mjs" => "application/javascript",
        "woff2" => "font/woff2",
        "woff" => "font/woff",
        "ttf" => "font/ttf",
        "otf" => "font/otf",
        "wasm" => "application/wasm",
        "html" | "htm" => "text/html",
        "mp4" => "video/mp4",
        "webm" => "video/webm",
        "mp3" => "audio/mpeg",
        "zip" => "application/zip",
        _ => "application/octet-stream",
    }
}

/// Objects sharing the key as a prefix that the default `exists` scans
const EXISTS_SCAN_LIMIT: usize = 1000;

#[async_trait]
pub trait StorageBackend: Send + Sync {
    /// Save an object/blob into storage with given key, data, and mime type
    async fn put(&self, key: &str, data: &[u8], content_type: &str) -> Result<StorageMeta>;

    /// Fetch an object's raw bytes
    async fn get(&self, key: &str) -> Result<Vec<u8>>;

    /// Delete an object by key
    async fn delete(&self, key: &str) -> Result<()>;

    /// Whether an object with exactly this key exists. The default looks the key up with
    /// [`list`](Self::list); backends with a cheaper check override it.
    async fn exists(&self, key: &str) -> Result<bool> {
        Ok(self
            .list(key, EXISTS_SCAN_LIMIT)
            .await?
            .iter()
            .any(|o| o.key == key))
    }

    /// List the stored objects whose key starts with `prefix` (`""` lists everything).
    /// At most `max_objects` are returned; the order is backend-specific.
    async fn list(&self, prefix: &str, max_objects: usize) -> Result<Vec<StorageObject>>;

    /// Determine the public/accessible URL for an object
    fn public_url(&self, key: &str) -> String;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_guess_content_type() {
        assert_eq!(guess_content_type("2026/10/abcd-photo.JPG"), "image/jpeg");
        assert_eq!(guess_content_type("logo.svg"), "image/svg+xml");
        assert_eq!(guess_content_type("docs/report.pdf"), "application/pdf");
        assert_eq!(guess_content_type("style.css"), "text/css");
        assert_eq!(
            guess_content_type("theme/assets/app.js"),
            "application/javascript"
        );
        assert_eq!(guess_content_type("fonts/inter.woff2"), "font/woff2");
        assert_eq!(
            guess_content_type("no-extension"),
            "application/octet-stream"
        );
    }
}
