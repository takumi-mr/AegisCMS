use crate::error::Result;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum StorageBackendKind {
    #[default]
    Local,
    S3,
}

impl std::str::FromStr for StorageBackendKind {
    type Err = String;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s.to_ascii_lowercase().as_str() {
            "local" | "disk" | "fs" => Ok(Self::Local),
            "s3" | "r2" | "minio" => Ok(Self::S3),
            other => Err(format!("Unknown storage backend kind: {}", other)),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageMeta {
    pub key: String,
    pub url: String,
    pub size: usize,
    pub content_type: String,
}

#[async_trait]
pub trait StorageBackend: Send + Sync {
    /// Save an object/blob into storage with given key, data, and mime type
    async fn put(&self, key: &str, data: &[u8], content_type: &str) -> Result<StorageMeta>;

    /// Fetch an object's raw bytes
    async fn get(&self, key: &str) -> Result<Vec<u8>>;

    /// Delete an object by key
    async fn delete(&self, key: &str) -> Result<()>;

    /// Determine the public/accessible URL for an object
    fn public_url(&self, key: &str) -> String;
}
