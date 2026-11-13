# aegis-core

[English](README.md) | **日本語**

AegisCMS の最下層に位置する共有インターフェース・コアデータ型・契約（Contract）定義クレート。

## 概要

`aegis-core` は、AegisCMS の各ドメインクレート（`aegis-db`, `aegis-storage`, `aegis-auth` 等）および WebAssembly プラグイン実行基盤が共有する抽象インターフェース、共通ドメインモデル、マニフェスト定義、統一エラー型を提供します。

このクレート自身は具体的なストレージやデータベース等の外部依存実装を持たず、純粋な共有契約として機能します。

## 主な責務・提供機能

- **抽象トレイト**:
  - `StorageBackend`: メディアやアセットの保存・取得・削除・公開URL生成の抽象。
  - `CacheBackend`: キャッシュの取得・保存・削除の抽象。
  - `AuthProvider`: 外部認証プロバイダ連携の抽象。
  - `AuditSink`: 監査イベント記録先の抽象。
  - `Notifier`: メール等の通知配信の抽象。
- **プラグイン連携型**:
  - `PostContext`, `PageRenderContext`, `OrderContext`, `CommentSubmission` など、Wasm プラグインとホスト間で受け渡されるコンテキスト型。
- **プラグインマニフェスト**:
  - `PluginManifest`: プラグインのメタデータ、要求権限（ネットワークドメイン、KVSネームスペース）、リソース制限（Fuel/メモリ上限）の定義と検証。
- **パッケージング・署名検証**:
  - プラグインおよびテーマの ZIP 安全展開（パストラバーサル防止）、Ed25519 デジタル署名の検証・署名ユーティリティ。
- **統一エラー**:
  - `AegisError` および `Result<T>` 型エイリアス。

## モジュール構成

| モジュール | 説明 |
| :--- | :--- |
| `audit` | 監査ログのデータモデル (`AuditEvent`, `AuditAction` 等) と `AuditSink` トレイト |
| `auth` | 外部認証プロバイダ抽象 (`AuthProvider`) と `UserAuthContext` |
| `cache` | キャッシュ抽象トレイト (`CacheBackend`) |
| `comments` | コメント審査コンテキスト (`CommentSubmission`, `CommentVerdict`) |
| `commerce` | EC・注文・通貨計算型 (`OrderContext`, `OrderLine`, `format_money`) |
| `entity` | コンテンツモデル (`PostContext`, `PageRenderContext`, `PublishedContent`) |
| `error` | システム統一エラー (`AegisError`) |
| `manifest` | プラグインマニフェスト定義 (`PluginManifest`, `PluginCapabilities`) |
| `notify` | 通知抽象 (`Notifier`, `NotificationMessage`, `Recipient`) |
| `package` | ZIPアーカイブ安全展開・Ed25519署名検証 (`verify_package_signature`) |
| `storage` | オブジェクトストレージ抽象 (`StorageBackend`, `StorageMeta`) |
