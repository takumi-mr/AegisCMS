//! アップロードファイルの保存先（`[storage]`: ローカルディスク / S3 互換）

pub mod config;
pub mod local;
pub mod s3;

pub use config::{LocalStorageConfig, S3StorageConfig, StorageBackendKind, StorageConfig};
pub use local::LocalStorageBackend;
pub use s3::S3StorageBackend;

use aegis_core::{Result, StorageBackend};
use std::sync::Arc;

/// 設定のバックエンドを作る。`local_public_url_prefix` はローカルストレージのファイルの公開 URL の
/// 接頭辞（サーバーが `[server].base_path` を付けた `public_url_prefix`）
pub fn create_storage_backend(
    config: &StorageConfig,
    local_public_url_prefix: &str,
) -> Result<Arc<dyn StorageBackend>> {
    match config.backend {
        StorageBackendKind::Local => {
            let public_url_prefix = local_public_url_prefix.to_string();
            tracing::info!(
                "Initializing LocalStorageBackend (dir: {:?}, prefix: {})",
                config.local.upload_dir,
                public_url_prefix
            );
            Ok(Arc::new(LocalStorageBackend::new(
                &config.local.upload_dir,
                public_url_prefix,
            )))
        }
        StorageBackendKind::S3 => {
            tracing::info!(
                "Initializing S3StorageBackend (bucket: {}, region: {}, endpoint: {:?})",
                config.s3.bucket,
                config.s3.region,
                config.s3.endpoint
            );
            Ok(Arc::new(S3StorageBackend::new(config.s3.clone())))
        }
    }
}
