pub mod local;
pub mod s3;

pub use local::LocalStorageBackend;
pub use s3::S3StorageBackend;

use aegis_core::{Result, StorageBackend, StorageBackendKind, StorageConfig};
use std::sync::Arc;

/// `base_path` は `[server].base_path`（正規化済み）。ローカルストレージの公開 URL に付ける
/// （配信のルートは `public_url_prefix` のまま。`app::with_base_path` がベースパスを取り除く）
pub fn create_storage_backend(
    config: &StorageConfig,
    base_path: &str,
) -> Result<Arc<dyn StorageBackend>> {
    match config.backend {
        StorageBackendKind::Local => {
            let public_url_prefix =
                crate::app::under_base_path(base_path, &config.local.public_url_prefix);
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
