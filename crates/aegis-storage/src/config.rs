//! `[storage]` の設定

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

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
#[serde(default)]
pub struct StorageConfig {
    #[serde(default)]
    pub backend: StorageBackendKind,
    #[serde(default)]
    pub local: LocalStorageConfig,
    #[serde(default)]
    pub s3: S3StorageConfig,
    /// アップロード 1 回あたりの最大リクエストサイズ（MB）
    #[serde(default = "default_max_upload_mb")]
    pub max_upload_mb: u64,
}

fn default_max_upload_mb() -> u64 {
    10
}

impl StorageConfig {
    /// アップロード API に適用するリクエストボディの上限（バイト）
    pub fn max_upload_bytes(&self) -> usize {
        usize::try_from(self.max_upload_mb.saturating_mul(1024 * 1024)).unwrap_or(usize::MAX)
    }
}

impl Default for StorageConfig {
    fn default() -> Self {
        Self {
            backend: StorageBackendKind::Local,
            local: LocalStorageConfig::default(),
            s3: S3StorageConfig::default(),
            max_upload_mb: default_max_upload_mb(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct LocalStorageConfig {
    pub upload_dir: PathBuf,
    pub public_url_prefix: String,
}

impl Default for LocalStorageConfig {
    fn default() -> Self {
        Self {
            upload_dir: PathBuf::from("data/uploads"),
            public_url_prefix: "/uploads".to_string(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct S3StorageConfig {
    pub bucket: String,
    pub region: String,
    pub endpoint: Option<String>,
    pub access_key_id: String,
    pub secret_access_key: String,
    pub force_path_style: bool,
    pub public_url_prefix: Option<String>,
}

impl Default for S3StorageConfig {
    fn default() -> Self {
        Self {
            bucket: "aegis-bucket".to_string(),
            region: "auto".to_string(),
            endpoint: None,
            access_key_id: String::new(),
            secret_access_key: String::new(),
            force_path_style: false,
            public_url_prefix: None,
        }
    }
}
