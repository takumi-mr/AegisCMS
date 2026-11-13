# aegis-backup

[English](README.md) | **日本語**

AegisCMS のデータベースおよびアップロードストレージの一括バックアップ・復元（ダンプ / リストア）クレート（`feature = "backup"`）。

## 概要

`aegis-backup` は、サイトの全データ（`_aegis_*` 管理テーブルおよび `content_*` コンテンツテーブル）と、`StorageBackend` に保存されたアップロードファイルを、単一のポータブルな ZIP アーカイブとして一括エクスポートおよびインポートする機能を提供します。

環境移行（開発・ステージング・本番環境間）や災害対策（DR）、定期バックアップの作成・復元に利用されます。

## 主な責務・提供機能

- **一括ダンプ (`dump`)**:
  - 全テーブルのデータを方言非依存な JSON 形式でシリアライズ。
  - アップロードされたメディアファイルを ZIP アーカイブに同梱。
  - 各ファイルの SHA-256 ハッシュ値とファイルサイズを記録した `manifest.json` を生成。
- **整合性検証付きリストア (`restore`)**:
  - `manifest.json` に基づき、展開前に全ファイルの SHA-256 チェックサムを事前検証。
  - パストラバーサル防止（Zip Slip 脆弱性対策）: アーカイブ内の相対パスが安全であることを厳格に検査。
  - トランザクション内でのテーブルデータ復元とメディアファイルのストレージ配置。
- **CLI 統合**:
  - `aegis-server dump -o backup.zip`
  - `aegis-server restore backup.zip`

## モジュール構成

| モジュール | 説明 |
| :--- | :--- |
| `dump` | データベースおよびストレージファイルから ZIP アーカイブを生成 (`dump_to_file`, `dump_to_writer`) |
| `restore` | ZIP アーカイブを検証しデータベースおよびストレージへ復元 |
| `manifest` | バックアップメタデータおよび SHA-256 チェックサム管理 (`BackupManifest`, `TableManifestEntry`) |
