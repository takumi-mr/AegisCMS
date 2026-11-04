# aegis-storage

AegisCMS のファイル・メディアストレージバックエンド実装（ローカルディスク / AWS S3 互換）。

## 概要

`aegis-storage` は、アップロードされた画像、ドキュメント、アセットファイルの保存・配信・削除を担当するクレートです。`aegis_core::StorageBackend` トレイトを実装し、ローカルファイルシステムと S3 互換オブジェクトストレージを透過的に切り替えて利用できます。

## 主な責務・提供機能

- **LocalStorageBackend**:
  - サーバーのローカルディスク（`data/uploads/` 等）への安全な保存。
  - パストラバーサル防止（`..` を含む不正パスの検証と正規化）。
  - ディレクトリ自動生成、MIME タイプ推測、公開 URL プレフィックスの付与。
- **S3StorageBackend**:
  - AWS S3 および S3 互換ストレージ（Cloudflare R2, MinIO, Wasabi, Google Cloud Storage 等）に対応。
  - AWS Signature Version 4 (SigV4) による署名付きリクエストの自前生成（外部重厚 SDK 不要）。
  - カスタムエンドポイントおよびパススタイル / 仮想ホストスタイルのバケットアクセス。
- **ファクトリ関数 (`create_storage_backend`)**:
  - `StorageConfig` の内容に応じて自動的に適切なバックエンドインスタンスを生成。

## モジュール構成

| モジュール | 説明 |
| :--- | :--- |
| `local` | ローカルファイルシステムストレージ実装 (`LocalStorageBackend`) |
| `s3` | AWS S3 互換オブジェクトストレージ実装 (`S3StorageBackend`) |
| `config` | ストレージ設定 (`StorageConfig`, `LocalStorageConfig`, `S3StorageConfig`) |

## 使用例

```rust
use aegis_core::StorageBackend;
use aegis_storage::{create_storage_backend, StorageConfig};

// 設定からバックエンドを生成
let config = StorageConfig::default(); // ローカルストレージが既定
let storage = create_storage_backend(&config).expect("failed to initialize storage");

// ファイルの保存
let bytes = b"sample image bytes".to_vec();
let meta = storage.put("media/avatar.png", bytes, "image/png").await.expect("upload failed");
println!("Uploaded to URL: {}", meta.public_url.unwrap_or_default());
```
