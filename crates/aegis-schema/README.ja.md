# aegis-schema

[English](README.md) | **日本語**

宣言的 YAML スキーマ定義、マルチダイアレクト DDL 自動マイグレーション、および TypeScript / OpenAPI 3.1 コード生成エンジン。

## 概要

AegisCMS では、コンテンツタイプ（投稿、商品、イベント、ポートフォリオ等）を YAML ファイルによる宣言的スキーマとして定義します。これによりスキーマを Git でバージョン管理（Code-as-Configuration）できます。

`aegis-schema` はこのスキーマ定義を読み込み、内部 RDBMS（SQLite, PostgreSQL, MySQL）に対する DDL の自動生成およびマイグレーションを実行するほか、フロントエンド用の TypeScript 型定義や OpenAPI 3.1 仕様書を自動生成します。

## 主な責務・提供機能

- **宣言的スキーマ定義 (`definition`)**:
    - `SchemaDefinition`, `FieldDefinition`, `FieldType`（string, text, integer, boolean, datetime, json, relation 等）。
    - 標準共通カラム（`id`, `title`, `slug`, `status`, `created_at`, `updated_at`, `published_at` 等）の透過的統合。
    - リレーション定義 (`RelationTarget`: has_one, has_many, belongs_to)。
- **スキーマローダー (`loader`)**:
    - `schemas/` ディレクトリ配下の YAML/JSON スキーマファイルを走査・パース・整合性検証。
- **マルチダイアレクトマイグレーション (`migration`)**:
    - SQLite, PostgreSQL, MySQL の差分検出および自動 DDL 生成 (`CREATE TABLE`, `ALTER TABLE ADD COLUMN`, インデックス作成)。
- **コード生成 (`codegen`)**:
    - **TypeScript Generator**: Admin Studio や外部フロントエンド用の TypeScript インターフェース定義を生成。
    - **OpenAPI 3.1 Generator**: スキーマ定義に基づく RESTful API エンドポイントおよびスキーマ仕様（JSON/YAML）を自動出力。

## モジュール構成

| モジュール   | 説明                                                                                       |
| :----------- | :----------------------------------------------------------------------------------------- |
| `definition` | スキーマ・フィールド・リレーションのデータ構造定義                                         |
| `loader`     | ファイルシステムからのスキーマ読み込みと構文検証 (`SchemaLoader`)                          |
| `migration`  | 各 SQL 方言へのマイグレーション DDL 生成 (`SchemaMigration`, `SqlDialect`)                 |
| `codegen`    | TypeScript 型定義および OpenAPI 3.1 仕様書生成 (`TypeScriptGenerator`, `OpenApiGenerator`) |

## 使用例

```rust
use aegis_schema::{SchemaLoader, SqlDialect};

// 1. スキーマファイルの読み込み
let loader = SchemaLoader::new();
let schemas = loader.load_from_dir("schemas/")?;

// 2. データベース用 DDL マイグレーションスクリプトの生成
for schema in &schemas {
    let ddl = schema.to_ddl(SqlDialect::Sqlite);
    println!("Migration SQL:\n{}", ddl);
}

// 3. TypeScript 定義の出力
let ts_code = aegis_schema::codegen::TypeScriptGenerator::generate(&schemas)?;
```
