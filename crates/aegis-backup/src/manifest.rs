use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// Current backup archive format version.
pub const FORMAT_VERSION: u32 = 1;

/// Name of the manifest file at the root of the backup ZIP archive.
pub const MANIFEST_FILE_NAME: &str = "manifest.json";

/// Root manifest describing a backup archive and checksums of all contained entries.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BackupManifest {
    pub format_version: u32,
    pub aegis_version: String,
    pub created_at: String,
    pub database: DatabaseSectionManifest,
    pub storage: StorageSectionManifest,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct DatabaseSectionManifest {
    pub included: bool,
    pub tables: Vec<TableManifestEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TableManifestEntry {
    /// Logical table or dataset name (e.g. `_aegis_users`, `content_article`).
    pub name: String,
    /// Relative path inside the ZIP archive (e.g. `database/users.json`).
    pub path: String,
    /// Number of top-level records stored in the file.
    pub count: u64,
    /// Lowercase hex SHA-256 digest of the serialized JSON file.
    pub sha256: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct StorageSectionManifest {
    pub included: bool,
    pub files: Vec<StorageFileEntry>,
    pub total_bytes: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct StorageFileEntry {
    /// Storage backend key (e.g. `uploads/photo.png`).
    pub key: String,
    /// Relative path inside the ZIP archive (e.g. `storage/uploads/photo.png`).
    pub path: String,
    /// File size in bytes.
    pub size_bytes: u64,
    /// Lowercase hex SHA-256 digest of the file contents.
    pub sha256: String,
}

/// Compute lowercase hex SHA-256 of a byte slice.
pub fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hex::encode(hasher.finalize())
}

/// Validate that a storage key or archive relative path is safe (no traversal, root, or backslashes).
pub fn is_safe_relative_path(path: &str) -> bool {
    if path.is_empty() || path.starts_with('/') || path.contains('\\') || path.contains('\0') {
        return false;
    }
    for segment in path.split('/') {
        if segment.is_empty() || segment == "." || segment == ".." {
            return false;
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sha256_hex() {
        let digest = sha256_hex(b"hello aegis");
        assert_eq!(digest.len(), 64);
    }

    #[test]
    fn test_is_safe_relative_path() {
        assert!(is_safe_relative_path("uploads/2026/image.png"));
        assert!(is_safe_relative_path("database/users.json"));
        assert!(!is_safe_relative_path(""));
        assert!(!is_safe_relative_path("/etc/passwd"));
        assert!(!is_safe_relative_path("../secret.txt"));
        assert!(!is_safe_relative_path("uploads/../../secret.txt"));
        assert!(!is_safe_relative_path("uploads\\image.png"));
    }
}
