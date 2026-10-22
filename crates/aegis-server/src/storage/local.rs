use aegis_core::{
    guess_content_type, AegisError, Result, StorageBackend, StorageMeta, StorageObject,
};
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

    async fn list(&self, prefix: &str, max_objects: usize) -> Result<Vec<StorageObject>> {
        let mut objects = Vec::new();
        if !self.upload_dir.exists() {
            return Ok(objects);
        }

        // 再帰ではなくスタックで走査する（隠しファイル・シンボリックリンクは対象外）
        let mut dirs = vec![(self.upload_dir.clone(), String::new())];
        while let Some((dir, dir_key)) = dirs.pop() {
            let mut entries = tokio::fs::read_dir(&dir)
                .await
                .map_err(|e| AegisError::Database(format!("Failed to read storage dir: {}", e)))?;
            while let Some(entry) = entries.next_entry().await.map_err(|e| {
                AegisError::Database(format!("Failed to read storage dir entry: {}", e))
            })? {
                let Some(name) = entry.file_name().to_str().map(str::to_string) else {
                    continue;
                };
                if name.starts_with('.') {
                    continue;
                }
                let key = format!("{}{}", dir_key, name);
                let file_type = entry.file_type().await.map_err(|e| {
                    AegisError::Database(format!("Failed to stat storage entry: {}", e))
                })?;
                if file_type.is_dir() {
                    // プレフィックスに一致し得ないディレクトリは降りない
                    let dir_prefix = format!("{}/", key);
                    if dir_prefix.starts_with(prefix) || prefix.starts_with(&dir_prefix) {
                        dirs.push((entry.path(), dir_prefix));
                    }
                } else if file_type.is_file() && key.starts_with(prefix) {
                    let metadata = entry.metadata().await.map_err(|e| {
                        AegisError::Database(format!("Failed to stat storage entry: {}", e))
                    })?;
                    let last_modified = metadata.modified().ok().map(|t| {
                        chrono::DateTime::<chrono::Utc>::from(t)
                            .to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
                    });
                    objects.push(StorageObject {
                        url: self.public_url(&key),
                        size: metadata.len(),
                        content_type: guess_content_type(&key).to_string(),
                        last_modified,
                        key,
                    });
                    if objects.len() >= max_objects {
                        return Ok(objects);
                    }
                }
            }
        }
        Ok(objects)
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

    #[tokio::test]
    async fn test_local_storage_list() {
        let temp_dir =
            std::env::temp_dir().join(format!("aegis_test_storage_{}", uuid::Uuid::new_v4()));
        let storage = LocalStorageBackend::new(&temp_dir, "/uploads/");

        // アップロードディレクトリが未作成でも空一覧を返す
        assert!(storage.list("", 100).await.unwrap().is_empty());

        storage
            .put("2026/09/a-old.png", b"old", "image/png")
            .await
            .unwrap();
        storage
            .put("2026/10/b-new.jpg", b"newer", "image/jpeg")
            .await
            .unwrap();
        storage
            .put("docs/report.pdf", b"pdf", "application/pdf")
            .await
            .unwrap();
        tokio::fs::write(temp_dir.join("2026/10/.DS_Store"), b"x")
            .await
            .unwrap();

        let mut all = storage.list("", 100).await.unwrap();
        all.sort_by(|a, b| a.key.cmp(&b.key));
        let keys: Vec<&str> = all.iter().map(|o| o.key.as_str()).collect();
        assert_eq!(
            keys,
            ["2026/09/a-old.png", "2026/10/b-new.jpg", "docs/report.pdf"]
        );
        assert_eq!(all[1].url, "/uploads/2026/10/b-new.jpg");
        assert_eq!(all[1].size, 5);
        assert_eq!(all[1].content_type, "image/jpeg");
        assert!(all[1].last_modified.is_some());

        let in_2026 = storage.list("2026/", 100).await.unwrap();
        assert_eq!(in_2026.len(), 2);
        let october = storage.list("2026/10", 100).await.unwrap();
        assert_eq!(october.len(), 1);
        assert_eq!(october[0].key, "2026/10/b-new.jpg");

        assert_eq!(storage.list("", 2).await.unwrap().len(), 2);

        let _ = tokio::fs::remove_dir_all(&temp_dir).await;
    }
}
