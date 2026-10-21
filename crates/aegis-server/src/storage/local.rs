use aegis_core::{AegisError, Result, StorageBackend, StorageMeta};
use async_trait::async_trait;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct LocalStorageBackend {
    pub upload_dir: PathBuf,
    pub public_url_prefix: String,
}

impl LocalStorageBackend {
    pub fn new(upload_dir: impl AsRef<Path>, public_url_prefix: impl Into<String>) -> Self {
        Self {
            upload_dir: upload_dir.as_ref().to_path_buf(),
            public_url_prefix: public_url_prefix.into(),
        }
    }

    fn resolve_path(&self, key: &str) -> Result<PathBuf> {
        let clean_key = key.trim_start_matches('/').trim_start_matches('\\');
        // パストラバーサル防止
        if clean_key.contains("..") {
            return Err(AegisError::Config(
                "Path traversal detected in storage key".to_string(),
            ));
        }
        Ok(self.upload_dir.join(clean_key))
    }
}

#[async_trait]
impl StorageBackend for LocalStorageBackend {
    async fn put(&self, key: &str, data: &[u8], content_type: &str) -> Result<StorageMeta> {
        let path = self.resolve_path(key)?;
        if let Some(parent) = path.parent() {
            tokio::fs::create_dir_all(parent).await.map_err(|e| {
                AegisError::Database(format!("Failed to create storage dir: {}", e))
            })?;
        }

        tokio::fs::write(&path, data)
            .await
            .map_err(|e| AegisError::Database(format!("Failed to write file to storage: {}", e)))?;

        let url = self.public_url(key);
        Ok(StorageMeta {
            key: key.to_string(),
            url,
            size: data.len(),
            content_type: content_type.to_string(),
        })
    }

    async fn get(&self, key: &str) -> Result<Vec<u8>> {
        let path = self.resolve_path(key)?;
        tokio::fs::read(&path)
            .await
            .map_err(|e| AegisError::Database(format!("Failed to read file from storage: {}", e)))
    }

    async fn delete(&self, key: &str) -> Result<()> {
        let path = self.resolve_path(key)?;
        if path.exists() {
            tokio::fs::remove_file(&path).await.map_err(|e| {
                AegisError::Database(format!("Failed to delete file from storage: {}", e))
            })?;
        }
        Ok(())
    }

    fn public_url(&self, key: &str) -> String {
        let prefix = self.public_url_prefix.trim_end_matches('/');
        let clean_key = key.trim_start_matches('/');
        format!("{}/{}", prefix, clean_key)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_local_storage_lifecycle() {
        let temp_dir =
            std::env::temp_dir().join(format!("aegis_test_storage_{}", uuid::Uuid::new_v4()));
        let storage = LocalStorageBackend::new(&temp_dir, "/uploads");

        let key = "images/sample.txt";
        let content = b"Hello, AegisCMS Storage!";

        // 1. Put
        let meta = storage
            .put(key, content, "text/plain")
            .await
            .expect("put should succeed");
        assert_eq!(meta.key, key);
        assert_eq!(meta.size, content.len());
        assert_eq!(meta.url, "/uploads/images/sample.txt");

        // 2. Get
        let fetched = storage.get(key).await.expect("get should succeed");
        assert_eq!(fetched, content);

        // 3. Delete
        storage.delete(key).await.expect("delete should succeed");
        let get_after_del = storage.get(key).await;
        assert!(get_after_del.is_err());

        let _ = tokio::fs::remove_dir_all(&temp_dir).await;
    }
}
