# aegis-storage

**English** | [日本語](README.ja.md)

File and media storage backend implementations for AegisCMS (local disk / AWS S3-compatible).

## Overview

`aegis-storage` is the crate responsible for storing, serving, and deleting uploaded images, documents, and asset files. It implements the `aegis_core::StorageBackend` trait, so you can switch transparently between the local file system and S3-compatible object storage.

## Responsibilities and Features

- **LocalStorageBackend**:
    - Safe storage on the server's local disk (`data/uploads/`, etc.).
    - Path traversal prevention (validation and normalization of invalid paths containing `..`).
    - Automatic directory creation, MIME type inference, and public URL prefixing.
- **S3StorageBackend**:
    - Supports AWS S3 and S3-compatible storage (Cloudflare R2, MinIO, Wasabi, Google Cloud Storage, etc.).
    - Builds AWS Signature Version 4 (SigV4) signed requests itself (no heavy external SDK required).
    - Custom endpoints and path-style / virtual-host-style bucket access.
- **Factory function (`create_storage_backend`)**:
    - Automatically creates the appropriate backend instance from the contents of `StorageConfig`.

## Module Layout

| Module   | Description                                                                 |
| :------- | :-------------------------------------------------------------------------- |
| `local`  | Local file system storage implementation (`LocalStorageBackend`)            |
| `s3`     | AWS S3-compatible object storage implementation (`S3StorageBackend`)        |
| `config` | Storage settings (`StorageConfig`, `LocalStorageConfig`, `S3StorageConfig`) |

## Usage Example

```rust
use aegis_core::StorageBackend;
use aegis_storage::{create_storage_backend, StorageConfig};

// Create a backend from the config
let config = StorageConfig::default(); // local storage is the default
let storage = create_storage_backend(&config).expect("failed to initialize storage");

// Save a file
let bytes = b"sample image bytes".to_vec();
let meta = storage.put("media/avatar.png", bytes, "image/png").await.expect("upload failed");
println!("Uploaded to URL: {}", meta.public_url.unwrap_or_default());
```
