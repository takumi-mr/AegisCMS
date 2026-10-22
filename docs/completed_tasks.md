# AegisCMS 完了済みタスク・実装実績アーカイブ (completed_tasks.md)

本ドキュメントは、AegisCMS の開発において完了した機能実装、インフラ基盤整備、Admin Studio 拡張、セキュリティ対策、バグ修正などの全記録です。  
現在進行中のタスクおよび未着手のロードマップについては、ルートの [TODO.md](file:///d:/projects/cms/TODO.md) を参照してください。

---

## 目次
- [1. コアアーキテクチャ & モノレポ基盤](#1-コアアーキテクチャ--モノレポ基盤)
- [2. スキーマ駆動開発 & コード生成基盤](#2-スキーマ駆動開発--コード生成基盤)
- [3. Wasm ランタイム & プラグイン SDK](#3-wasm-ランタイム--プラグイン-sdk)
- [4. データベース & ストレージ & キャッシュ基盤](#4-データベース--ストレージ--キャッシュ基盤)
- [5. コンテンツ公開 & テーマエンジン](#5-コンテンツ公開--テーマエンジン)
- [6. 認証・認可 & セキュリティ基盤](#6-認証認可--セキュリティ基盤)
- [7. 監査ログ基盤 (AuditSink)](#7-監査ログ基盤-auditsink)
- [8. Admin Studio SPA & UI/UX 改善](#8-admin-studio-spa--uiux-改善)
- [9. CI/CD & コンテナ・自動化基盤](#9-cicd--コンテナ自動化基盤)
- [10. 不具合修正 & 安定化・リファクタリング](#10-不具合修正--安定化リファクタリング)

---

## 1. コアアーキテクチャ & モノレポ基盤

- [x] **モノレポ構成の初期セットアップ**
  - Cargo ワークスペース構成 (`Cargo.toml`)、Git 設定 (`.gitignore`) の整備。
  - クレート別ネスト `.git` の解消および `main` ブランチでのファーストコミット作成。
  - エージェント運用規約 ([AGENTS.md](file:///d:/projects/cms/AGENTS.md)) および [TODO.md](file:///d:/projects/cms/TODO.md) の策定。
  - `CLAUDE.md` シンボリックリンクの作成 (`CLAUDE.md -> AGENTS.md`)。
  - ブランチ戦略、Conventional Commits 規約、PR 運用規約・チェックリストの策定。
- [x] **静的解析・コードフォーマット基盤**
  - Rust 向けフォーマッタ設定 ([rustfmt.toml](file:///d:/projects/cms/rustfmt.toml))、エディタ設定 ([.editorconfig](file:///d:/projects/cms/.editorconfig))。
  - Clippy ゼロ警告ポリシー (`[workspace.lints]`、`cargo clippy --workspace -- -D warnings`) の達成と維持。
- [x] **`aegis-core` ドメイン共通基盤**
  - 共通エンティティ (`Post`, `PostContext`)、設定ローダー ([AegisConfig](file:///d:/projects/cms/crates/aegis-core/src/config.rs))。
  - プラグインマニフェスト定義 (`PluginManifest`)、統一エラーモデル (`AegisError`)。
- [x] **`aegis-ast` コンテンツ構造化 & レンダリング基盤**
  - Block AST データ構造 (`BlockNode`)、AST トランスフォーマーパイプライン ([AstPipeline](file:///d:/projects/cms/crates/aegis-ast/src/pipeline.rs))。
  - 決定論的 HTML レンダラー ([HtmlRenderer](file:///d:/projects/cms/crates/aegis-ast/src/renderer.rs))。
- [x] **`aegis-server` コア HTTP Gateway**
  - Axum ベースの HTTP/REST API Gateway。
  - コンテンツ CRUD API、SQLite 接続プール統合、マイグレーション統合。

---

## 2. スキーマ駆動開発 & コード生成基盤

- [x] **宣言的スキーマエンジン (`aegis-schema`)**
  - YAML 宣言的スキーマパーサー ([SchemaLoader](file:///d:/projects/cms/crates/aegis-schema/src/loader.rs))。
  - マルチダイアレクト DDL 自動生成 ([SqlDialect](file:///d:/projects/cms/crates/aegis-schema/src/migration.rs): SQLite, PostgreSQL, MySQL)。
  - スキーマ進化マイグレーション（`PRAGMA table_info` / `information_schema` 走査による `ALTER TABLE ... ADD COLUMN` 自動適用）。
- [x] **OpenAPI & TypeScript 型定義 自動生成基盤**
  - スキーマ定義から TypeScript インターフェースを生成する `TypeScriptGenerator` の実装。
  - スキーマ定義および標準エンドポイントから OpenAPI 3.1 仕様書を生成する `OpenApiGenerator` の実装。
  - CLI サブコマンド `aegis codegen --target <typescript|openapi> --output <path>` の実装。
  - 最新の OpenAPI 3.1 定義を配信する HTTP エンドポイント `GET /api/v1/openapi.json` の実装。
  - `Taskfile.yml`: `task codegen`, `task codegen:ts`, `task codegen:openapi` の一括生成タスク追加。
  - Admin Studio: 自動生成された `admin/src/types/schemas.ts` を透過的に利用可能に統合。
- [x] **リレーション解決機能**
  - `schemas/categories.yaml` (カテゴリ管理: title, slug, description, color) の作成。
  - `schemas/pages.yaml` (固定ページ: title, slug, layout, custom_meta) の作成。
  - `schemas/articles.yaml` への `category_id` (type: relation, target: category.id) の定義追加。
  - `aegis-schema`: `RelationTarget` パース機能および `SchemaDefinition::relation_fields()` の実装。
  - 動的カスタムフィールドの永続化およびリレーション自動解決（JOIN / 関連モデル情報の `relations` オブジェクト注入）。

---

## 3. Wasm ランタイム & プラグイン SDK

- [x] **Wasmtime サンドボックスランタイム (`aegis-runtime`)**
  - ケイパビリティ検証ガード ([CapabilityGuard](file:///d:/projects/cms/crates/aegis-runtime/src/guard.rs): ドメインホワイトリスト、フック権限、KVS ネームスペース)。
  - Wasmtime サンドボックスランタイム統合 ([WasmEngine](file:///d:/projects/cms/crates/aegis-runtime/src/engine.rs), `WasmPluginInstance`)。
  - ホスト API リンカー (`host:log`, `host:http_check_allowed`, `host:kv_set`)。
  - タイムアウト多層防御 (`tokio::time::timeout` + Fuel リソースクォータ + MSVC 非同期 Trap 回避)。
  - ディレクトリ走査型プラグインローダー ([WasmPluginLoader](file:///d:/projects/cms/crates/aegis-runtime/src/loader.rs))。
  - サンプルプラグイン実バイナリ (`plugins/slack-notifier/plugin.wasm`) の生成およびエンドツーエンド疎通確認。
- [x] **公式 SDK (`aegis-plugin-sdk`) & 手続型マクロ (`aegis-plugin-macros`)**
  - 手続型マクロ `#[aegis_plugin]`: `allocate` 自動生成、JSON 直列化/逆直列化、packed `(ptr << 32) | len` 隠蔽。
  - 型安全な `Plugin` トレイト、`PostContext`、`BlockNode` 定義とヘルパー群。
  - ホスト API ラッパー（`log_info!`/`log_warn!`、`net::is_allowed`、`kv::set`/`kv::set_json`）。
  - ネイティブ環境（`cargo test`）向けのモック・メモリハンドル内蔵（Wasm ビルド不要でロジックを高速単体テスト可能）。
  - プラグイン公式開発テンプレート ([plugins/template-rust](file:///d:/projects/cms/plugins/template-rust)) の整備。
  - `Taskfile.yml`: `task plugin:build` による Wasm バイナリ最適化ビルドコマンドの追加。
  - 公式ドキュメント ([docs/plugin_sdk_guide.md](file:///d:/projects/cms/docs/plugin_sdk_guide.md), [docs/plugin_hooks.md](file:///d:/projects/cms/docs/plugin_hooks.md)) の作成。
- [x] **フック実行パイプラインの強化**
  - `on-before-save`: プラグインが返した `title` / `slug` / `blocks_json` の保存反映、および `Err` による保存拒否（HTTP 422 / 監査ログ記録）のサポート。
  - `transform-content-ast`: AST 構造書き換え処理の冪等性確保とエラーハンドリング。

---

## 4. データベース & ストレージ & キャッシュ基盤

- [x] **マルチ RDBMS 抽象化 (`DatabaseAdapter`)**
  - `DatabaseBackendKind` (`sqlite`, `postgres`, `mysql`)。
  - [DatabaseAdapter](file:///d:/projects/cms/crates/aegis-server/src/db/mod.rs) トレイト（init, run_migrations, insert_content, get_content, list_contents, resolve_relation）。
  - `SqliteAdapter`: SQLite コネクションプール カプセル化。
  - `PostgresAdapter`: PostgreSQL コネクションプール、型付きバインド (`jsonb` / `BIGINT` / `BOOLEAN`)。
  - `MySqlAdapter`: MySQL コネクションプール、大文字小文字対応、厳格モード対応。
  - [AppState](file:///d:/projects/cms/crates/aegis-server/src/state.rs) のアダプターベースへの全面リファクタリング。
- [x] **マルチストレージ抽象化 (`StorageBackend`) & メディア管理**
  - `StorageBackendKind` (`local`, `s3`)、[StorageBackend](file:///d:/projects/cms/crates/aegis-core/src/storage.rs) トレイト（`put`, `get`, `delete`, `exists`, `list`, `public_url`）、`StorageMeta` 定義。
  - `LocalStorageBackend`: ローカルディスク/ブロックストレージ、ディレクトリ自動作成、パストラバーサル防止、`exists` 実装。
  - `S3StorageBackend`: AWS S3 / Cloudflare R2 / MinIO 互換、AWS SigV4 認証署名ネイティブ実装。
  - ファイルアップロード API (`POST /api/v1/storage/upload`) & 静的配信統合。
  - アップロードサイズ上限の設定化 (`[storage].max_upload_mb`、超過時は 413 返却)。
  - メディア一覧 API (`GET /api/v1/storage/objects`): プレフィックス検索、MIME タイプ推測、走査件数保護（要 `viewer` ロール）。
  - メディア削除 API (`DELETE /api/v1/storage/objects/{key}`):
    - `editor` 以上のロール必須。
    - 保存済みコンテンツ（全スキーマの `blocks_json` およびカスタムフィールド）に削除対象ファイルの公開 URL が含まれるか検査。
    - 参照が存在する場合は HTTP 409 Conflict と参照元情報（`references: [...]`）を返却し意図しないリンク切れを防止。`?force=true` パラメータ指定時のみ強制削除。
    - 監査ログ `storage.delete` の自動記録。
  - メディア仕様書・API ガイド ([docs/media.md](file:///d:/projects/cms/docs/media.md)) の作成。
- [x] **Redis キャッシュ & ステートレスセッション管理**
  - [CacheBackend](file:///d:/projects/cms/crates/aegis-core/src/cache.rs) トレイト (`get`, `set`, `delete`, `exists`, `increment`)。
  - `MemoryCacheBackend`: TTL 有効期限管理、Tokio `RwLock`、期限切れ自動パージ機構。
  - `RedisCacheBackend`: 非同期 `ConnectionManager` による自動再接続、Lua スクリプトによるアトミック操作。
  - ステートレスセッション管理 `SessionService`: `ses_{UUID}` 発行・検証・失効、TTL 連動。
  - セッション REST API (`POST /api/v1/auth/session`, `GET /api/v1/auth/session`, `DELETE /api/v1/auth/session`)。

---

## 5. コンテンツ公開 & テーマエンジン

- [x] **テーマエンジン (`crates/aegis-theme`)**
  - `MiniJinja` をコアとした高速・安全なテンプレート描画エンジン。
  - 洗練されたビルトインモダンテンプレート（`base.html`, `index.html`, `article.html`, `page.html`, `404.html`）の内蔵。
  - 3 層テンプレート探索フォールバック（プラグイン動的登録 `register_template` → ファイルシステム `themes/{active}/templates/` → ビルトイン）。
- [x] **公開ルーティング配信 (`aegis-server`)**
  - `GET /` & `GET /articles`: 最新公開記事一覧 (`index.html`)。
  - `GET /articles/{slug}`: 記事詳細ページ (`article.html`)。
  - `GET /{slug}`: 固定ページまたは記事へのスマートフォールバック。
  - `GET /theme/assets/*`: テーマ静的アセット（CSS/JS/画像）配信ルーティング。
  - Admin Studio SPA（`/admin` & `/assets`）との完全ルーティング分離。
  - ゼロトラストな公開制御（`status == "published"` のみ公開、下書き・非公開は 404 を返却）。
- [x] **プラグイン動的テーマ書き換え**
  - `on-render-page` フック: `PageRenderContext`（タイトル、カスタム CSS、`<head>` タグ、テーマ変数、使用テンプレート名）の動的書き換え。
  - `provide_templates` / `ThemeEngine::register_template`: プラグインからのテンプレート動的完全オーバーライド。
- [x] **テーマ関連ドキュメント**
  - コンテンツ公開 & テーマプラグイン開発公式ガイド ([docs/theme_and_publishing_guide.md](file:///d:/projects/cms/docs/theme_and_publishing_guide.md)) の作成。

---

## 6. 認証・認可 & セキュリティ基盤

- [x] **ストリーミング HTML セキュリティリライター (`lol-html`)**
  - `crates/aegis-ast` 内の `StreamingHtmlRewriter` 実装。
  - 外部リンクへの `rel="noopener noreferrer"` 自動付与（既存 rel 属性とのスマートマージ）。
  - `internal_domains` 設定による自サイト・組織内リンクの除外バイパス。
  - `<img>` タグへの `loading="lazy"` および `decoding="async"` 強制インジェクション。
  - `[security]` 宣言的設定対応および HTML 生成パイプライン自動適用。
- [x] **認証プロバイダ抽象化 (`AuthProvider`)**
  - `aegis-core`: `AuthProvider` トレイト、`AuthIdentity`, `AuthCredentials`, `AuthRedirect` 定義と単体テスト。
  - ローカル認証プロバイダ `LocalAuthProvider`:
    - `_aegis_users` テーブル（SQLite / PostgreSQL / MySQL）。
    - Argon2id によるセキュアなパスワードハッシュ。
    - 存在しないユーザーに対するダミーハッシュ検証（タイミング攻撃防御）。
    - アカウント無効化フラグのサポート。
- [x] **認証済みセッション発行 & CLI 管理**
  - `POST /api/v1/auth/session` をユーザー名/パスワード認証必須に変更（ロールのクライアント指定を禁止）。
  - ユーザー管理 CLI コマンドの実装:
    - `aegis user add <username> [--role <role>]`
    - `aegis user passwd <username>`
    - `aegis user role <username> <role>`
    - `aegis user disable <username>` / `aegis user enable <username>`
    - `aegis user list`
  - 認証設計ドキュメント ([docs/authentication.md](file:///d:/projects/cms/docs/authentication.md)) の作成。
- [x] **API のロールベース認可 (RBAC)**
  - Axum エクストラクター `RequireRole` (`ViewerSession` / `EditorSession` / `AdminSession`) によるロール階層管理（`admin` ⊃ `editor` ⊃ `viewer`）。
  - コンテンツ閲覧は `viewer`、コンテンツ作成・編集・アップロード・メディア削除は `editor`、監査ログ参照は `admin` を要求。
  - `author_id` はログインユーザーを既定とし、他ユーザーの代理指定は `admin` のみ許可。
- [x] **ブルートフォース攻撃対策（レート制限・アカウントロック）**
  - `[auth.login_rate_limit]` によるログイン失敗回数制限。
  - ユーザー名（大文字小文字不問・SHA-256 キー）およびクライアント IP ごとに固定ウィンドウで失敗を集計。
  - 閾値超過時に `lockout_secs` の間 HTTP 429 (`Retry-After` 付き) で即時遮断（ロック中は DB 認証を行わない）。
- [x] **アクセス拒否の監査ログ記録**
  - 401 Unauthorized / 403 Forbidden 発生時に `access.denied` イベントを自動記録。

---

## 7. 監査ログ基盤 (AuditSink)

- [x] **監査イベントモデル & 出力先抽象化**
  - `AuditEvent`, `AuditAction`, `AuditOutcome`, `AuditQuery` データ構造。
  - [AuditSink](file:///d:/projects/cms/crates/aegis-core/src/audit.rs) トレイトおよび `AuditService` ディスパッチャー。
  - 出力先: `database`（DB 永続化テーブル `_aegis_audit_logs`）、`log`（構造化 JSON ログ）、`prometheus`（`aegis_audit_events_total` メトリクス）。
- [x] **セキュアな監査記録**
  - 記録対象: `content.create`, `session.create`, `session.delete`, `storage.upload`, `storage.delete`, `access.denied`, `plugin.error`, `audit.purge`。
  - セッショントークンは記録せず、SHA-256 フィンガープリントのみを保持。
- [x] **監査ログ参照 API & 定期パージ**
  - `GET /api/v1/audit-logs`: `admin` ロール必須、アクション/結果/アクター/期間での絞り込みとページネーション。
  - `[audit].retention_days` / `purge_interval_secs` による期限切れ監査ログの自動削除バックグラウンドタスク。
  - 監査ログ仕様書 ([docs/audit.md](file:///d:/projects/cms/docs/audit.md)) の作成。

---

## 8. Admin Studio SPA & UI/UX 改善

- [x] **SPA 基盤 & デザインシステム構築**
  - Vite + React + TypeScript プロジェクト初期化。
  - ダークテーマ、グラスモフィズム、洗練されたタイポグラフィによるモダン UI デザインシステム。
  - API クライアント（セッショントークン自動付与、エラーハンドリング）の実装。
- [x] **ブロックエディタ UI**
  - ブロック追加メニュー（見出し、段落、コード、画像、カスタム要素）。
  - ブロック並び替え（Up/Down）、削除、リアルタイム編集。
  - 記事保存・更新・公開ステータス切替。
- [x] **リレーション & メディア管理 UI**
  - 記事編集フォームへのカテゴリ選択ドロップダウン。
  - `CategoryManager` モーダル（カラーピッカー、slug 自動生成・重複検証、新規作成）。
  - 記事一覧（サイドバー）のカテゴリバッジ表示およびカテゴリ別フィルタリング。
  - 画像ブロックへのドラッグ＆ドロップ / ファイルアップロード連携。
  - メディアライブラリモーダル（アップロード済み画像のプレビュー・選択・ファイル名検索）。
  - メディア削除 UI:
    - メディアライブラリの各画像タイルへの削除ボタン（`editor` 以上）。
    - 削除確認ダイアログ（使用中の場合は参照元記事一覧を表示し、「Delete anyway」による強制削除をサポート）。
    - 監査ログフィルターへの `storage.delete` 対応。
- [x] **サーバーレンダリング プレビュー統合**
  - `POST /api/v1/preview` エンドポイントの実装（保存前 AST 変換 + lol-html リライター適用）。
  - JavaScript 簡易レンダラーからサーバーサイドプレビューへの置き換え（300ms デバウンス）。
  - 「Themed Page」タブ: テーマテンプレートとプラグイン `on-render-page` が適用された完全なページプレビューを `sandbox` 付き `iframe` 内に表示。
- [x] **ログイン & 認証状態管理 UI**
  - 未認証時の全画面ログインフォーム。
  - セッション復元 (`GET /api/v1/auth/session`)、ヘッダーのユーザー情報・ロール表示、ログアウト機能。
  - HTTP 401 応答時のログイン画面自動フォールバック。
  - ロールに応じた UI 制御（`viewer` での編集ボタン無効化、`admin` 以外の監査ログタブ非表示など）。
  - ログインレート制限時のロック解除残り時間カウントダウン表示。
- [x] **Audit Logs 閲覧画面**
  - イベント絞り込み（アクション、成否、アクター、日時範囲）、詳細 JSON 表示、ページネーション。
- [x] **フロントエンドコード整理 & アクセシビリティ**
  - 未使用テンプレートファイルの削除。
  - 1,389 行の単一 `index.css` をコンポーネント別 CSS Modules へ分割。
  - 共通 UI コンポーネント (`Button`, `Field`, `FieldRow`, `Modal`, `BrandMark`) の整備。
  - モーダルのフォーカストラップ・Escape キー対応・フォーカス復帰。
  - ツールバーやタブのキーボードアクセシビリティ（`role="tablist"` / `tab`）の向上。

---

## 9. CI/CD & コンテナ・自動化基盤

- [x] **CI/CD パイプライン (`.github/workflows/ci.yml`)**
  - GitHub Actions による自動テスト、リント (`clippy`, `oxlint`)、フォーマット (`rustfmt`, `prettier`)、ビルド検証。
- [x] **マルチステージ本番 Dockerfile**
  - `node:22.22-alpine` -> `rust:1.99.1-slim-bookworm` -> `debian:bookworm-slim` による最適化。
  - `cargo-chef` レイヤーキャッシュの活用。
  - 非 root ユーザー (`aegis`, UID 10001) 実行、HEALTHCHECK 内蔵、イメージサイズ約 163MB。
- [x] **設定ファイル & タスクランナー**
  - Docker 用設定 ([config/docker.toml](file:///d:/projects/cms/config/docker.toml)) および [.dockerignore](file:///d:/projects/cms/.dockerignore)。
  - 環境変数・CLI 引数による host/port オーバーライド (`AEGIS_CONFIG`, `HOST`, `PORT`)。
  - [Taskfile.yml](file:///d:/projects/cms/Taskfile.yml) による開発タスクの一元化 (`task dev`, `task check`, `task build:admin`, `task docker:build` 等)。
  - Prettier & oxlint の導入、VS Code 保存時自動整形設定 ([.vscode/settings.json](file:///d:/projects/cms/.vscode/settings.json))。

---

## 10. 不具合修正 & 安定化・リファクタリング

- [x] **PostgreSQL 互換性修正**: カスタムフィールド（`jsonb`）の型付きバインドおよび DDL 既定値（`integer`, `boolean`, `jsonb`）の修正。
- [x] **MySQL 8.4 実機検証 & 修正**: `information_schema` 列名の大文字小文字対応、JSON 列の NULL/デコード処理、インデックス重複エラー対応。
- [x] **設定ファイルパースの堅牢化**: 全設定構造体に `#[serde(default)]` を付与し、同梱設定ファイルパースを単体テストで検証。
- [x] **プラグイン SDK の互換性修正**:
  - `attributes` が null のブロックを空マップとして読み込むよう SDK 側で修正。
  - プラグインエラー時の拒否エンベロープ（`PluginRejected` / 422 返却）の対応。
  - `plugins/template-rust` の `transform-content-ast` 冪等化。
- [x] **Wasmtime リソース制限の適用**: マニフェストで省略されたリソース制限への既定値適用、および `StoreLimits` によるメモリ超過トラップの強制。
- [x] **Redis タイムアウトの適用**: `[cache.redis].timeout_ms` の接続・応答タイムアウト反映と再試行回数制御。
- [x] **レンダリングロジックの集約**: HTML 生成・プレビュー生成を `render.rs` に集約し、公開ページ・プレビュー・API 間でのレンダリング差分を解消。
