# AegisCMS 開発進捗 & TODOリスト (TODO.md)

> [!NOTE]
> 作業を再開する際は、まず本ドキュメントの「現在の状況と直近の再開手順」および「TODOタスク一覧」を確認してください。  
> **ルール**: 各修正・実装ステップの終了後には、必ず本ドキュメントを更新してください。

---

## 1. 現在の状況と直近の再開手順

- **現在のステータス**: フェーズ 2 拡張（インフラ抽象化: マルチRDBMS & マルチストレージ対応）完了。
- **動作確認済み**:
  - Rust ワークスペース（`aegis-core`, `aegis-ast`, `aegis-schema`, `aegis-runtime`, `aegis-server`）のビルド・単体テスト (`cargo test --workspace`) 100% 成功（21 tests 全パス）。
  - RDB 抽象化レイヤー (`DatabaseAdapter`): SQLite, PostgreSQL, MySQL への接続プール、スキーマDDL生成、差分カラム自動追加マイグレーション、コンテンツCRUD、リレーション解決のトレイト抽象化完了。
  - ストレージ抽象化レイヤー (`StorageBackend`): ローカルディスク/ブロックストレージ (`LocalStorageBackend`) および AWS S3 / Cloudflare R2 / MinIO 互換ストレージ (`S3StorageBackend` ネイティブ SigV4 署名実装) の完全統合。
  - アセットアップロード API (`POST /api/v1/storage/upload`) およびローカルアセット配信サービス (`/uploads/*`)。
  - Config ベースでの動的切り替え（`config/*.toml` の `[database]` および `[storage]` セクション）。
  - Wasmtime サンドボックスランタイム統合、AST変換、ストリーミング HTML リライター (`lol-html`)、Admin Studio SPA 配信、OpenAPI 3.1 & TypeScript 型定義自動生成。

### 🚀 次に作業を再開するときの着手項目
**フェーズ 2 拡張: Admin Studio リレーション連携 & メディア管理 UI**
1. **Admin Studio でのカテゴリ・リレーション選択 UI 統合**:
   - 記事エディタに `category_id` を選択可能なカテゴリドロップダウン / セレクトボックスを追加。
   - `schemas/categories.yaml` に基づくカテゴリ一覧・作成モーダル/画面の提供。
2. **Admin Studio メディアアップローダー UI**:
   - 画像ブロックやメディア挿入時に `/api/v1/storage/upload` へドラッグ＆ドロップで画像をアップロードし、URL をブロックへ自動反映する UI。

---

## 2. TODOタスク一覧

### ✅ 完了済み (Done)
- [x] プロジェクト全体のモノレポ構成セットアップ (`Cargo.toml`, `.gitignore`)
- [x] `aegis-core`: 共通エンティティ (`Post`, `PostContext`)、設定ローダー、マニフェスト定義、エラーモデル
- [x] `aegis-ast`: Block AST データ構造 (`BlockNode`)、ASTトランスフォーマーパイプライン、決定論的HTMLレンダラー
- [x] `aegis-schema`: 宣言的YAMLスキーマパーサー、SQLite DDL生成、自動マイグレーション機構
- [x] `aegis-runtime`:
  - [x] ケイパビリティ検証ガード (`CapabilityGuard`: ドメインホワイトリスト、フック権限、KVSネームスペース)
  - [x] Wasmtime サンドボックスランタイム統合 (`WasmEngine`, `WasmPluginInstance`)
  - [x] ホストAPIリンカー (`host:log`, `host:http_check_allowed`, `host:kv_set`)
  - [x] タイムアウト多層防御 (`tokio::time::timeout` + Fuelリソースクォータ)
  - [x] ディレクトリ走査型プラグインローダー (`WasmPluginLoader`)
- [x] `aegis-server`: Axum HTTP/REST API Gateway、SQLite接続プール、マイグレーション統合、コンテンツCRUD API
- [x] サンプルプラグイン実バイナリ (`plugins/slack-notifier/plugin.wasm`) の生成およびエンドツーエンド疎通確認
- [x] エージェント運用規約 (`AGENTS.md`) および TODO 管理票 (`TODO.md`) の作成
- [x] `CLAUDE.md` シンボリックリンクの作成 (`CLAUDE.md -> AGENTS.md`)
- [x] エージェント運用規約 (`AGENTS.md`) の拡張（ブランチ戦略、Conventional Commits、PR運用規約・チェックリストの策定）
- [x] Linter & Formatter 設定の追加 (`rustfmt.toml`, `.editorconfig`, `[workspace.lints]`, Clippy ゼロ警告達成)
- [x] モノレポ構造の整合性確保（クレート別ネスト `.git` の解消）および `main` ブランチでのファーストコミット作成
- [x] **Admin Studio SPA セットアップ**
  - [x] `admin/` ディレクトリに Vite + React (TypeScript) プロジェクトを初期化
  - [x] リッチなモダンUIデザインシステム（ダークテーマ、グラスモフィズム、洗練されたタイポグラフィ）の構築
  - [x] API クライアント（`/api/v1/schemas`, `/api/v1/contents`）の実装
- [x] **ブロックエディタ UI 実装**
  - [x] ブロック追加メニュー（見出し、段落、コード、画像、カスタム要素）
  - [x] ブロック並び替え（Up/Down）・削除
  - [x] リアルタイムプレビュー（ASTからレンダリングされたHTMLの表示）
  - [x] 記事保存・更新・公開ステータス切替
  - [x] コアサーバーによる Admin SPA 配信 (`/` -> `admin/dist/index.html`)
- [x] **CI/CD & コンテナ・タスク自動化基盤整備**
  - [x] GitHub Actions 自動テスト・リント・ビルド CI パイプライン構築 (`.github/workflows/ci.yml`)
  - [x] マルチステージ本番 Dockerfile の作成（Node.js + Rust 最新版 + 最小 Debian 実行層、非 root ユーザー実行、サイズ 163MB、HEALTHCHECK 内蔵）
  - [x] Docker 用設定ファイル (`config/docker.toml`) および `.dockerignore` の追加
  - [x] CLI 引数および環境変数による host/port オーバーライド対応（`AEGIS_CONFIG`, `HOST`, `PORT`）
  - [x] `go-task` 構成ファイル (`Taskfile.yml`) の作成（dev, test, lint, fmt, check, build, docker コマンド一元化）
  - [x] Admin Studio ドキュメント刷新・最新化 (`admin/README.md`)
- [x] **OpenAPI / TypeScript 型定義 自動生成基盤**
  - [x] `aegis-schema`: スキーマ定義から TypeScript インターフェースを生成する `TypeScriptGenerator` の実装
  - [x] `aegis-schema`: スキーマ定義および標準エンドポイントから OpenAPI 3.1 仕様書を生成する `OpenApiGenerator` の実装
  - [x] `aegis-server`: CLI サブコマンド `aegis codegen --target <typescript|openapi> --output <path>` の実装
  - [x] `aegis-server`: 最新の OpenAPI 3.1 定義を配信する HTTP エンドポイント `GET /api/v1/openapi.json` の実装
  - [x] `Taskfile.yml`: `task codegen`, `task codegen:ts`, `task codegen:openapi` の一括生成タスク追加
  - [x] Admin Studio: 自動生成された `admin/src/types/schemas.ts` を透過的に利用可能に統合
- [x] **ストリーミング HTML リライター組み込み (`lol-html`)**
  - [x] `crates/aegis-ast`: Cloudflare 製 `lol-html` を使用した `StreamingHtmlRewriter` の実装
  - [x] 外部リンクへの `rel="noopener noreferrer"` 自動付与（既存の rel 属性とのスマートマージ）
  - [x] 内部ドメインリスト（`internal_domains`）による同一組織/自サイトリンクの除外バイパス
  - [x] `<img>` タグへの `loading="lazy"` および `decoding="async"` 強制インジェクション
  - [x] `crates/aegis-core`: `SecurityConfig` の追加および `config/default.toml`, `config/docker.toml` からの宣言的設定対応
  - [x] `crates/aegis-server`: コンテンツ作成 API (`POST /api/v1/contents/{content_type}`) の HTML 生成パイプラインに自動適用
  - [x] 単体テストおよび大規模 HTML（~120KB, 500リンク/500画像）のスループットベンチマーク検証（約33ms）
- [x] **追加スキーマ定義とリレーション解決**
  - [x] `schemas/categories.yaml` (カテゴリ管理: title, slug, description, color) の作成
  - [x] `schemas/pages.yaml` (固定ページ: title, slug, layout, custom_meta) の作成
  - [x] `schemas/articles.yaml` への `category_id` (type: relation, target: category.id) の定義追加
  - [x] `aegis-schema`: `RelationTarget` パース機能および `SchemaDefinition::relation_fields()` の実装
  - [x] `aegis-server`: `PRAGMA table_info` を走査し、既存テーブルへ差分カラムを自動追加する `ALTER TABLE ... ADD COLUMN` スキーマ進化マイグレーションの実装
  - [x] `aegis-server`: 動的カスタムフィールドの永続化およびリレーション自動解決（JOIN / 関連モデル情報の `relations` オブジェクト注入）
  - [x] TypeScript 型定義および OpenAPI 仕様書の更新 (`task codegen`)
- [x] **インフラ周りの抽象化（RDB & ストレージ）**
  - [x] `aegis-core`: `DatabaseBackendKind` (`sqlite`, `postgres`, `mysql`), `StorageBackendKind` (`local`, `s3`), `StorageConfig`, `StorageBackend` トレイト、`StorageMeta` 定義
  - [x] `aegis-schema`: マルチダイアレクト DDL 生成 (`SqlDialect`: SQLite, Postgres, MySQL) および単体テスト
  - [x] `aegis-server`: `DatabaseAdapter` トレイトの定義（init, run_migrations, insert_content, get_content, list_contents, resolve_relation）
  - [x] `aegis-server`: `SqliteAdapter` 実装（`sqlx::SqlitePool` カプセル化）
  - [x] `aegis-server`: `PostgresAdapter` 実装（`sqlx::PgPool`, `$1` パラメータ, `information_schema` 走査マイグレーション）
  - [x] `aegis-server`: `MySqlAdapter` 実装（`sqlx::MySqlPool`, `?` パラメータ, `information_schema` 走査マイグレーション）
  - [x] `aegis-server`: `LocalStorageBackend` 実装（ローカルディスク/ブロックストレージ、ディレクトリ自動作成、パストラバーサル防止）
  - [x] `aegis-server`: `S3StorageBackend` 実装（AWS S3 / Cloudflare R2 / MinIO 互換, AWS SigV4 認証署名ネイティブ実装）
  - [x] `aegis-server`: アセットアップロード API (`POST /api/v1/storage/upload`) & 静的配信統合
  - [x] `AppState` および `handlers/content.rs` を `DatabaseAdapter` & `StorageBackend` ベースに全面リファクタリング
  - [x] `config/default.toml` および `config/docker.toml` での `[database]` & `[storage]` 設定の体系化
- [x] **Redis キャッシュ & ステートレスセッション管理 (オプショナル Redis / デフォルトインメモリ)**
  - [x] `aegis-core`: `CacheBackend` トレイト（`get`, `set`, `delete`, `exists`）および `CacheBackendKind` (`Memory`, `Redis`), `CacheConfig`, `RedisCacheConfig` の定義
  - [x] `aegis-server`: `MemoryCacheBackend` の実装（TTL 有効期限管理、Tokio `RwLock`、外部ミドルウェア不要なデフォルト動作）
  - [x] `aegis-server`: `RedisCacheBackend` の実装（非同期 `ConnectionManager` による自動再接続、`GET`/`SETEX`/`DEL`/`EXISTS` コマンド統合）
  - [x] `aegis-server`: ステートレスセッション管理 `SessionService`（Bearer トークン `ses_{UUID}` 発行・検証・失効、メタデータ属性保持、TTL 連動）
  - [x] `aegis-server`: セッション HTTP REST API ハンドラー（`POST /api/v1/auth/session`, `GET /api/v1/auth/session`, `DELETE /api/v1/auth/session`、`Authorization: Bearer <token>` / `X-Session-Id` 両対応）
  - [x] `config/default.toml` および `config/docker.toml` への `[cache]` 宣言的設定ブロック追加
  - [x] 単体テスト追加（メモリキャッシュ TTL CRUD、Redis URL 検証、ステートレスセッションのライフサイクル検証）
- [x] **Wasm プラグイン向け公式 SDK (`aegis-plugin-sdk`) & 開発テンプレート整備**
  - [x] `crates/aegis-plugin-macros`: `#[aegis_plugin]` 手続型マクロの実装（`allocate` アロケータ自動生成、JSON 直列化/逆直列化、packed `(ptr << 32) | len` 隠蔽）
  - [x] `crates/aegis-plugin-sdk`: 型安全な `Plugin` トレイト、`PostContext`、`BlockNode` 定義とヘルパー群
  - [x] `crates/aegis-plugin-sdk`: ホスト API ラッパー（`log_info!`/`log_warn!`、`net::is_allowed`、`kv::set`/`kv::set_json`）
  - [x] `crates/aegis-plugin-sdk`: ネイティブ環境（`cargo test`）向けのモック・メモリハンドル内蔵（Wasm ビルド不要でロジックを高速単体テスト可能）
  - [x] `plugins/template-rust`: 公式開発テンプレートの整備（マニフェスト、フック実装、ローカルテスト）
  - [x] `Taskfile.yml`: `task plugin:build` による Wasm バイナリ最適化ビルドコマンドの追加
  - [x] `docs/plugin_sdk_guide.md`: Wasm プラグイン開発公式ガイドの作成
  - [x] `crates/aegis-runtime`: SDK ビルド済み Wasm バイナリを用いた E2E サンドボックス実行テストの追加・パス

---

### ⏳ 次に取り組むタスク (Next Up)

#### フェーズ 2 拡張: Admin Studio 連携 & メディア管理
- [ ] **Admin Studio: カテゴリ選択 & リレーション編集 UI**
  - [ ] 記事作成・編集フォームへのカテゴリ選択ドロップダウンの追加
  - [ ] カテゴリ一覧・新規登録 UI の追加
- [ ] **Admin Studio: メディアアップローダー UI**
  - [ ] 画像ブロックからのファイルアップロード (`/api/v1/storage/upload`) 連携
  - [ ] アップロード画像のプレビューおよびメディア一覧表示
- [ ] **コンテンツ一覧のフィルタリングとリレーション表示**
  - [ ] 記事一覧テーブルにカテゴリバッジを表示
  - [ ] カテゴリ別フィルタリング機能


---

### 🔮 将来のロードマップ (Future)
- [ ] Wasm プラグイン向け AssemblyScript / TypeScript SDK テンプレート整備
- [ ] PostgreSQL / libSQL 対応（分散ストレージモード）
- [ ] On-demand ISR / Cache-Tag Purge によるエッジキャッシュ制御

---

## 3. 技術メモ・申し送り事項 (Memo & Notes)

- **Wasm プラグイン公式 SDK の設計とメモリ境界**:
  - `#[aegis_plugin]` マクロは、ホストが要求する `allocate(len: i32) -> i32`、`on_before_save(ptr, len) -> i64`、`transform_content_ast(ptr, len) -> i64` を完全に自動生成し、ポインタパッキング（`(ptr << 32) | len`）と JSON 変換を隠蔽します。
  - Wasm 32bit リニアメモリ上では生ポインタ（`*mut u8` as `i32`）として動作し、64bit ネイティブ環境での `cargo test` 時にはポインタ上位 32bit の消失を防ぐため ID ハンドルマッピング機構を内蔵しています。これによりプラグイン開発者は Wasm ターゲットを意識せずローカルで高速にテスト可能です。
- **ステートレスセッション & キャッシュ設計**:
  - セッションデータは `SessionData` として JSON 直列化され、抽象化された `CacheBackend` に格納されます。
  - セッショントークンは `ses_{UUIDv4}` 形式で発行され、HTTP リクエストの `Authorization: Bearer <token>` または `X-Session-Id` ヘッダーから抽出されます。
  - バックエンドとして `backend = "redis"` を指定した場合は Redis クラスター/スタンドアロンでセッションが共有され、サーバープロセス自体は完全なステートレスとして水平スケール可能です。
  - デフォルトは `backend = "memory"` となっており、追加のミドルウェアや環境構築なしに即座にローカル実行・テストが可能です。
- **Admin Studio SPA 配信**:
  - `aegis-server` は起動時に `admin/dist/index.html` の存在をチェックし、存在すれば `tower_http::services::ServeDir` と `ServeFile` による SPA フォールバック配信を行います。
  - フロントエンド更新時は `npm --prefix admin run build` を実行すれば、サーバーの再コンパイル不要で即座に反映されます。
- **コンテンツ API ペイロード仕様**:
  - `POST /api/v1/contents/{content_type}` の JSON ペイロードはトップレベルに `title`, `slug`, `status`, `blocks`, `author_id` を持ちます。
  - `blocks` 配列内の各要素は `BlockNode` (`{ "name": "core/heading", "attributes": { "level": 1 }, "text": "...", "children": [] }`) 形式です。
- **Wasmtime と Windows 環境の非同期実行**:
  - Windows MSVC 環境では、Wasmtime の同期呼び出しで Trap が発生した場合に Rust 1.98 の `extern "C"` non-unwinding 境界チェックと競合して abort するケースがあります。
  - そのため `aegis-runtime` では `config.async_support(true)` を有効化し、`tokio::time::timeout` でタイムアウトを明示的に強制する多層防御設計を採用しています。
- **リニアメモリのデータ受け渡し**:
  - ホストからゲストへ文字列/JSONを渡す際は、必ずゲスト側がエクスポートする `allocate(len: i32) -> i32` を呼び出して安全なメモリ領域を確保してから書き込みます。
  - ゲストからホストへの返却は、上位32bitが `ptr`、下位32bitが `len` の `i64` パック形式で受け渡しを行っています。
- **データベーステーブル命名規則**:
  - スキーマ `content_type: article` の場合、テーブル名は `content_article` となります。
  - ASTブロック配列は `blocks_json TEXT NOT NULL DEFAULT '[]'` カラムに格納されます。
- **Docker コンテナ運用と再現性**:
  - 本番イメージは `Dockerfile` によるマルチステージビルド（`node:22-alpine` -> `rust:latest` -> `debian:bookworm-slim`）で構築され、イメージサイズは約 163MB です。
  - セキュリティのため、UID 10001 の非rootユーザー `aegis` でプロセスを実行します。データ永続化には `/app/data` をボリュームマウント（例: `-v aegis-data:/app/data`）してください。
  - コンテナ内でのリッスン設定は `config/docker.toml`（`host = "0.0.0.0"`, `port = 8080`）で定義され、環境変数 `AEGIS_CONFIG`、`HOST`、`PORT` でも動的に上書き可能です。
- **ストリーミング HTML セキュリティ & 最適化リライター (`lol-html`)**:
  - `crates/aegis-ast` 内の `StreamingHtmlRewriter` は、Cloudflare 製の CSS セレクタベース高速ストリーミングパーサー `lol-html` を使用しています。
  - 外部ドメイン判定された `<a>` タグに対し、`rel="noopener noreferrer"` を自動付与（既存の rel トークンを尊重しスマートマージ）します。
  - 同様に、未指定の `<img>` タグへ `loading="lazy"` および `decoding="async"` を強制インジェクションし、ゼロ・トラストなコンテンツセキュリティと Core Web Vitals パフォーマンス向上を同時に実現します。
  - `config/*.toml` の `[security]` セクション（`internal_domains` や `open_external_links_in_new_tab` など）から柔軟に挙動をカスタマイズ可能です。
- **スキーマ駆動コード生成 (Codegen & OpenAPI)**:
  - YAMLスキーマ (`schemas/*.yaml`) を修正・追加した場合、`task codegen`（または `task codegen:ts`, `task codegen:openapi`）を実行することで、フロントエンド用 TypeScript 型定義 (`admin/src/types/schemas.ts`) および OpenAPI 3.1 仕様書 (`docs/openapi.json`) をワンコマンドで自動更新できます。
  - サーバー稼働中は `GET /api/v1/openapi.json` エンドポイントからも最新の OpenAPI 3.1 仕様が JSON 形式で動的に配信され、Swagger UI / Redoc や外部 API クライアント生成ツールと容易に統合できます。
- **宣言的スキーマ進化 (SQLite 自動カラム追加マイグレーション)**:
  - `aegis-server` の起動時マイグレーション (`run_migrations`) は、テーブル作成 (`CREATE TABLE IF NOT EXISTS`) だけでなく、`PRAGMA table_info` を走査して既存テーブルの既存カラム定義を取得します。
  - YAML スキーマに新しく定義されたフィールド（例: `category_id`）が既存テーブルに存在しない場合、自動的に `ALTER TABLE {table_name} ADD COLUMN {col_name} {col_type}` を実行してからインデックスを生成します。
  - これにより、開発中やリリース後のスキーマ変更時でも、手動マイグレーションスクリプトを書くことなく、既存データを保持したまま安全にスキーマを進化させることが可能です。
- **動的リレーション自動解決 (Automatic Relation Resolution)**:
  - スキーマのフィールド定義で `type: relation` および `target: category.id` （`<target_schema>.<target_column>`）が指定されている場合、API レスポンスの `relations` マップに自動解決された関連リソース情報（`id`, `content_type`, `title`, `slug`）が埋め込まれます。
  - フロントエンドは個別に追加 API リクエストを発行することなく、単一のコンテンツ取得クエリでリレーション先のメタデータ（カテゴリ名やスラッグ）を即座に描画・利用可能です。
- **マルチRDBMS 抽象化アーキテクチャ (`DatabaseAdapter`)**:
  - `DatabaseAdapter` トレイトによって SQLite / PostgreSQL / MySQL の SQL 方言・プレースホルダ構文（`?1` vs `$1` vs `?`）・接続プール・カラム情報スキーマ走査（`PRAGMA table_info` vs `information_schema.columns`）の違いを完全に隠蔽。
  - ハンドラ層は `DatabaseAdapter` 経由でコンテンツ CRUD やリレーション解決を行うため、特定の RDBMS に一切ロックインされません。
  - `config/*.toml` の `[database]` セクションで `backend = "sqlite" | "postgres" | "mysql"` と `url` を指定するだけで、サーバー再コンパイル不要でデータベースを切り替え可能です。
- **マルチストレージ抽象化アーキテクチャ (`StorageBackend`)**:
  - `StorageBackend` トレイト（`put`, `get`, `delete`, `public_url`）により、ローカルディスク/ブロックストレージ (`LocalStorageBackend`) と S3 互換オブジェクトストレージ (`S3StorageBackend`) を Config ベースで透過的に切り替え可能。
  - `S3StorageBackend` は AWS SigV4 認証プロトコルをネイティブ実装しているため、重量級の AWS SDK を追加することなく、AWS S3, Cloudflare R2, MinIO, Wasabi と完全互換で高速・軽量に動作します。
  - マルチパートファイルアップロード API (`POST /api/v1/storage/upload`) により、メディアやアセットの柔軟な保存と URL 返却に対応しています。

