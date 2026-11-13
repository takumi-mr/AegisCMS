# aegis-db

[English](README.md) | **日本語**

AegisCMS のデータベース抽象化レイヤー (`DatabaseAdapter`) および SQLite / PostgreSQL / MySQL 実装。

## 概要

`aegis-db` は、コンテンツデータ、システム設定、注文・在庫、コメント、掲示板、タグなどの永続化を担当するデータアクセス層クレートです。

特定の方言に依存しない抽象トレイト `DatabaseAdapter` を提供し、Zero-Config な組み込み SQLite から、大規模運用向けの PostgreSQL および MySQL まで、シームレスな切り替えをサポートします。

## 主な責務・提供機能

- **DatabaseAdapter 抽象トレイト**:
    - トランザクション制御、動的クエリ実行、スキーママイグレーション実行。
    - プリペアドステートメントによる SQL インジェクションの完全排除。
- **マルチ RDBMS アダプタ**:
    - `SqliteAdapter`: 組み込み SQLite (rusqlite)。Zero-Config でファイルまたはメモリ上で動作。WAL モードおよび外部キー制約を自動有効化。
    - `PostgresAdapter`: PostgreSQL / TimescaleDB 対応。
    - `MySqlAdapter`: MySQL / MariaDB 対応。
- **各種機能のストレージ実装**:
    - `orders` / `variants`: EC 注文、在庫管理、商品バリアントの永続化。
    - `comments`: コメント投稿、承認待ち、スレッド階層のクエリ。
    - `forum`: 掲示板カテゴリ、スレッド、レス、ピン留め、モデレーション。
    - `tags`: コンテンツ横断タグの紐付け・逆引き。
    - `contact` / `newsletter`: 問い合わせ受信箱、ニュースレター購読者の管理。
    - `analytics`: Cookie レス集計データの日次保存。
    - `notifications`: 通知送信待ちキュー。

## モジュール構成

| モジュール               | 説明                                               |
| :----------------------- | :------------------------------------------------- |
| `sqlite`                 | SQLite アダプタ実装 (`SqliteAdapter`)              |
| `postgres`               | PostgreSQL アダプタ実装 (`PostgresAdapter`)        |
| `mysql`                  | MySQL アダプタ実装 (`MySqlAdapter`)                |
| `orders` / `variants`    | 注文・在庫・バリアントのデータアクセス             |
| `comments`               | コメントデータの保存・検索・モデレーション         |
| `forum`                  | フォーラムデータの保存・検索                       |
| `tags`                   | タグおよびコンテンツ関連付けテーブル操作           |
| `contact` / `newsletter` | お問い合わせフォームおよびメルマガ購読者データ操作 |
| `analytics`              | 日次アクセス解析データの永続化                     |
| `notifications`          | 通知送信キューの永続化                             |
| `config`                 | データベース接続設定 (`DatabaseConfig`)            |
