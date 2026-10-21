pub mod local;
pub mod s3;

pub use local::LocalStorageBackend;
pub use s3::S3StorageBackend;

use aegis_core::{Result, StorageBackend, StorageBackendKind, StorageConfig};
use std::sync::Arc;

pub fn create_storage_backend(config: &StorageConfig) -> Result<Arc<dyn StorageBackend>> {
    match config.backend {
        StorageBackendKind::Local => {
            tracing::info!(
                "Initializing LocalStorageBackend (dir: {:?}, prefix: {})",
                config.local.upload_dir,
                config.local.public_url_prefix
            );
            Ok(Arc::new(LocalStorageBackend::new(
                &config.local.upload_dir,
                &config.local.public_url_prefix,
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
