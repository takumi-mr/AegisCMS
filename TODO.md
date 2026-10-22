# AegisCMS 開発進捗 & TODOリスト (TODO.md)

> [!NOTE]
> 作業を再開する際は、まず本ドキュメントの「現在の状況と直近の再開手順」および「TODOタスク一覧」を確認してください。  
> **ルール**: 各修正・実装ステップの終了後には、必ず本ドキュメントを更新してください。

---

## 1. 現在の状況と直近の再開手順

- **現在のステータス**: コンテンツ公開機能 & テンプレートエンジン (`MiniJinja`) 導入、およびプラグインによる動的テーマ書き換え機能の実装完了。
- **動作確認済み**:
  - Rust ワークスペース全体 (`cargo test --workspace`) 100% 成功（全35 tests パス）。
  - コードフォーマット (`cargo fmt --check`) および Clippy ゼロ警告 (`cargo clippy --workspace -- -D warnings`) 達成。
  - フロントエンド本番ビルド (`npm --prefix admin run build`) 成功。
  - `task check` (fmt, clippy, test, admin:build) 全パス。
  - テンプレートエンジン (`crates/aegis-theme`):
    - `MiniJinja` を採用した高速・軽量・堅牢なテンプレート描画エンジン。
    - 美しくモダンなビルトインデフォルトテンプレート（`base.html`, `index.html`, `article.html`, `page.html`, `404.html`）の内蔵。
    - ファイルシステム（`themes/{active}/templates/`）およびプラグイン動的登録テンプレートの 3 層探索フォールバック。
  - 公開フロントエンド配信 (`aegis-server`):
    - `GET /` & `GET /articles`: 最新の公開記事一覧ページ (`index.html`)。
    - `GET /articles/{slug}`: 記事詳細ページ (`article.html`)。`status == "published"` のみ一般公開し、下書きはゼロトラストに 404 を返却。
    - `GET /{slug}`: 固定ページまたは記事へのスマートフォールバック。
    - `GET /theme/assets/*`: テーマ静的アセット（CSS/JS/画像）配信。
    - `GET /admin` & `/admin/*`: Admin Studio SPA 配信の分離共存。
  - プラグインによるテーマ書き換え機構 (`aegis-runtime` & `aegis-plugin-sdk`):
    - `on-render-page` フック: `PageRenderContext`（タイトル、カスタム CSS、`<head>` タグ、テーマ変数、使用テンプレート名）の動的書き換え。
    - `ThemeEngine::register_template`: プラグインからのテンプレート動的完全オーバーライド。

### 🚀 次に作業を再開するときの着手項目
**フェーズ 2 拡張はメディア一覧まで完了。次は「不具合・ドキュメント整備時に判明した課題」の未着手項目**
1. **`on-before-save` フックの戻り値の扱い**: 戻り値 (`PostContext`) を保存データに反映するか、検証専用フックとして仕様を明確化する（SDK で `Err` を返しても保存が拒否されない問題とあわせて ABI を設計する）。
2. （候補）メディアの削除 API / UI、ストレージ API の認証必須化（現状アップロード・一覧ともに認証不要）。

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
- [x] **リファレンスドキュメント整備**
  - [x] `docs/configuration.md`: `config/*.toml` の全設定項目（既定値・省略可否・CLI/環境変数オーバーライド）リファレンス
  - [x] `docs/plugin_hooks.md`: プラグインフック一覧（実行順・エラー時挙動）、マニフェスト、リソース制限、ホスト API リファレンス
- [x] **監査ログ基盤 (`AuditSink` 抽象化)**
  - [x] `aegis-core`: `AuditEvent` / `AuditAction` / `AuditOutcome` / `AuditQuery` 型と `AuditSink` トレイト、`[audit]` 設定 (`enabled`, `sinks`, `trust_forwarded_for`)
  - [x] `aegis-server`: `DatabaseAdapter` に `insert_audit_event` / `query_audit_events` を追加（`_aegis_audit_logs` テーブル、SQLite / PostgreSQL / MySQL）
  - [x] `aegis-server`: 出力先 `database`（DB 永続化）/ `log`（`aegis::audit` 構造化 JSON ログ）/ `prometheus`（`GET /metrics` の `aegis_audit_events_total`）と配送サービス `AuditService`
  - [x] 記録対象: `content.create`, `session.create`, `session.delete`, `storage.upload`, `plugin.error`（セッショントークンは記録せず SHA-256 指紋のみ）
  - [x] 参照 API `GET /api/v1/audit-logs`（`admin` ロール必須、action / outcome / actor / resource / 期間での絞り込み、ページング）
  - [x] `docs/audit.md` の作成、`docs/configuration.md` と `config/*.toml` への `[audit]` 追記
- [x] **コンテンツ公開機能 & テーマエンジン (`MiniJinja`) & プラグイン動的テーマ書き換え**
  - [x] `aegis-core`: `ThemeConfig`、`PageRenderContext`、`ContentSummary` のデータ構造定義
  - [x] `crates/aegis-theme`: `MiniJinja` をコアとしたテーマエンジン実装
  - [x] 洗練されたビルトインモダンテンプレート（`base.html`, `index.html`, `article.html`, `page.html`, `404.html`）の内蔵
  - [x] ファイルシステム（`themes/{active}/templates/`）およびプラグイン動的登録テンプレートの 3 層フォールバック
  - [x] `aegis-runtime` & `aegis-plugin-sdk`: `on-render-page` フックおよび `provide_templates` メソッドの実装
  - [x] `aegis-plugin-macros`: `#[aegis_plugin]` マクロへの `on_render_page` ABI 自動生成の統合
  - [x] `aegis-server`: 公開ページ配信ハンドラー（`GET /`, `GET /articles`, `GET /articles/{slug}`, `GET /{slug}`）の実装
  - [x] ゼロトラストな公開制御（`status == "published"` のみ公開、下書き・存在しない slug は 404 レンダリング）
  - [x] テーマ静的アセット配信ルーティング（`/theme/assets/*`）および Admin Studio SPA（`/admin` & `/assets`）の完全分離共存
  - [x] `config/*.toml` への `[theme]` 宣言的設定セクションの追加
  - [x] `docs/theme_and_publishing_guide.md`: コンテンツ公開 & テーマプラグイン開発公式ガイドの作成
  - [x] 単体テスト & 結合テスト（トップページ公開表示、下書き 404、プラグインによるタイトル・CSS・テンプレート書き換え）の追加・全パス

---

### ⏳ 次に取り組むタスク (Next Up)

#### フェーズ 2 拡張: Admin Studio 連携 & メディア管理
- [x] **Admin Studio: カテゴリ選択 & リレーション編集 UI**
  - [x] 記事作成・編集フォームへのカテゴリ選択ドロップダウンの追加（既存記事を開くと `fields.category_id` から選択を復元、未選択時は `category_id` を送信しない）
  - [x] カテゴリ一覧・新規登録 UI の追加（`CategoryManager` モーダル: 色・説明つき一覧、slug 自動生成、重複 slug の事前チェック、作成したカテゴリを自動選択）
- [x] **Admin Studio: メディアアップローダー UI**
  - [x] 画像ブロックからのファイルアップロード (`/api/v1/storage/upload`) 連携（ドラッグ＆ドロップ / ファイル選択、URL と alt の自動反映、画像以外・上限超過のエラー表示）
  - [x] アップロード上限の設定化（`[storage].max_upload_mb`、既定 10MB。axum 既定の 2MB 制限で 2MB 超の画像が分かりにくい 400 で失敗していた問題を解消し、超過時は 413 を返す）
  - [x] メディア一覧表示: `StorageBackend::list`（ローカル: ディレクトリ走査、S3: ListObjectsV2 + SigV4 クエリ署名）と `GET /api/v1/storage/objects`（新しい順、`prefix` / `content_type` / `q` 絞り込み、`limit` / `offset` ページング）、Admin Studio のメディアライブラリ（画像ブロックから既存画像を選択・再利用、ファイル名検索、追加読み込み）
- [x] **コンテンツ一覧のフィルタリングとリレーション表示**
  - [x] 記事一覧（サイドバー）にカテゴリバッジを表示（カテゴリの色・名前。カテゴリ一覧に無い場合はサーバーが解決した `relations.category_id` の名前を表示）
  - [x] カテゴリ別フィルタリング機能（All / Uncategorized / 各カテゴリ。件数は「表示件数/全件数」で表示。API に絞り込み機能が無いため Admin Studio 内で絞り込み）


---

#### 不具合・ドキュメント整備時に判明した課題
- [x] **PostgreSQL でコンテンツ作成が失敗していた問題を修正**: カスタムフィールドを常に文字列でバインドしていたため、`jsonb` 列（`custom_meta`）への `INSERT` が値の有無にかかわらず失敗していた。`PostgresAdapter` でスキーマの列型に合わせて `jsonb` / `BIGINT` / `BOOLEAN` を型付きでバインド・デコードするよう修正（読み出し時に `jsonb` 列が黙って欠落する問題も解消）。
- [x] DDL で `required: true` かつ `nullable: false` のフィールドに型を問わず `DEFAULT ''` が付与され、PostgreSQL では `integer` / `boolean` / `jsonb` のテーブル作成が失敗していた問題を修正（型ごとの既定値: integer `0`、boolean `0` / `FALSE`、jsonb `'{}'`。MySQL の TEXT / JSON 型は既定値なし）。
- [x] **MySQL 実機検証と修正**（MySQL 8.4 / strict モード）: 起動時のカラム走査で `information_schema` の列名が大文字（`COLUMN_NAME`）で返り `unwrap()` がパニックしてサーバーが起動できなかった問題、`JSON` 列が読み出し時に欠落する問題、`null` の JSON 値が空文字列として保存されエラーになる問題を修正。インデックスの重複エラー（1061）以外を握りつぶしていた処理も改善。コンテンツ CRUD・リレーション解決・アップロード・監査ログの記録と検索を E2E 確認済み。
- [x] **監査ログの保持期間と閲覧画面**: `[audit].retention_days` / `purge_interval_secs` による定期削除（削除自体を `audit.purge` として記録）と、Admin Studio の Audit Logs 画面（絞り込み・ページ送り・詳細表示）を追加。
- [x] **同梱設定ファイルがパースに失敗し既定値で起動していた問題を修正**: 全設定構造体に `#[serde(default)]` を付与して全項目を省略可能にし、`load_or_default` はファイルが存在しない場合のみ既定値（警告ログ付き）、パース失敗時はエラーで起動中止するよう変更。同梱設定ファイルのパースを単体テストで検証。
- [ ] `on-before-save` フックの戻り値 (`PostContext`) が保存データに反映されない。反映するか、検証専用フックとして仕様を明確化する。
- [ ] SDK でトレイトメソッドが `Err` を返しても保存が拒否されない（生成コードが入力をそのまま返す）。拒否を表現できる ABI を設計する。
- [ ] `[runtime].default_*`、マニフェストの `max_memory_mb`、`[cache.redis].timeout_ms` が未使用。ランタイムへ適用する。

---

### 🔮 将来のロードマップ (Future)
- [ ] **認証プロバイダ抽象化 & Wasm 認証後フック (`on_user_authenticated`)**
  - [x] `aegis-core`: `AuthProvider` トレイト（Local, SAML, OIDC などの抽象化）、`AuthIdentity`, `AuthCredentials`, `AuthRedirect` 定義と単体テスト完了
  - [ ] 認証判定・暗号署名検証・通信は Rust Crate（静的コンパイル / Cargo Features）で安全に分離実装
  - [ ] `aegis-runtime` & `aegis-plugin-sdk`: 認証成功後のユーザー属性同期・ロールマッピング・監査通知用 Wasm プラグインフック `on_user_authenticated` の追加
- [x] **テーマエンジン & 静的アセット配信システム**
  - [x] ページ全体のテンプレート描画エンジンの導入（MiniJinja によるレイアウト・スケルトンの定義）
  - [x] テーマ向け静的アセット（CSS/JS/画像）配信ルーティング機構（`/theme/assets/*`）
  - [x] プラグインによる動的テーマ CSS / `<head>` / コンテキスト書き換えおよびテンプレート動的オーバーライド機能の実装
- [ ] Wasm プラグイン向け AssemblyScript / TypeScript SDK テンプレート整備
- [ ] PostgreSQL / libSQL 対応（分散ストレージモード）
- [ ] On-demand ISR / Cache-Tag Purge によるエッジキャッシュ制御


---

## 3. 技術メモ・申し送り事項 (Memo & Notes)

- **メディア一覧 API (`GET /api/v1/storage/objects`)**:
  - ストレージ側に並び順・絞り込みの機能が無いため、サーバーは最大 10,000 件（`MAX_LIST_SCAN`）を走査してからメモリ上で絞り込み・新しい順の並び替え・ページングを行います。超過時はレスポンスの `truncated: true` で通知します（`q` / `content_type` は走査後の絞り込みなので、超過分を探すには `prefix` を使う）。
  - バックエンドはアップロード時の mime type を保持していないため、一覧の `content_type` は拡張子から推定します（`aegis_core::guess_content_type`）。ローカルでは隠しファイル・シンボリックリンクを除外、S3 では末尾 `/` の「フォルダ」オブジェクトを除外します。
  - アップロード API と同様に現状は認証不要です。
- **アップロードのサイズ上限**:
  - axum の `Multipart` は既定でリクエストボディを 2MB に制限します。`/api/v1/storage/upload` には `DefaultBodyLimit::max(config.storage.max_upload_bytes())` を個別に設定しているため、上限を変える場合は `[storage].max_upload_mb` を変更してください（リバースプロキシ側の上限、例: nginx の `client_max_body_size` も合わせる必要があります）。
  - アップロード API は現状認証なしで受け付けます（監査ログには記録されます）。認証プロバイダの実装時に保護対象へ含めてください。
- **Admin Studio のコンテンツ型と API レスポンスの差異**:
  - 自動生成の `admin/src/types/schemas.ts`（`ArticleContent` など）はカスタムフィールドをトップレベルに持ちますが、実際のコンテンツ API はカスタムフィールドを `fields`、解決済みリレーションを `relations` に入れて返します。Admin Studio では `types/index.ts` の `ContentItem`（`BaseContentItem` + `fields` / `relations`）を使ってください。作成時のペイロードはトップレベル（`CreateArticlePayload` など）で正しいです。
- **監査ログの保持期間**:
  - `AuditService::spawn_retention_task` が起動直後と `purge_interval_secs` ごとに `purge_expired` を実行し、`DatabaseAdapter::delete_audit_events_before` で `occurred_at < 基準時刻` のレコードを削除します。削除が 1 件以上あれば `audit.purge` を記録します。
  - 既定値は `retention_days = 0`（無期限）です。既存環境でアップデート後に監査ログが意図せず消えないよう、明示的に設定した場合のみ削除します。
- **Admin Studio の配信パスと API 接続先**:
  - Admin Studio は `/admin` で配信されます（`/` はテーマエンジンの公開サイト）。API の接続先は `admin/src/api/client.ts` の `API_BASE`（`http://127.0.0.1:8080/api/v1`）に固定されているため、別ホスト・別ポートで動かす場合は変更が必要です。
- **カスタムフィールドの型付きバインド (PostgreSQL / MySQL)**:
  - `db::CustomFieldValue::from_json` がスキーマのフィールド型に合わせて値を変換し（jsonb → JSON、integer → i64、boolean → bool、その他は文字列）、変換できない値は `AegisError::Validation`（API は 400）を返します。SQLite は従来どおり文字列で保存します。
  - 読み出しも各アダプターの `custom_fields_from_row` でフィールド型ごとにデコードします。新しいフィールド型を追加する際は、`SchemaMigration` の型対応・`CustomFieldValue`・各アダプターのデコードを揃えて更新してください。
  - MySQL は `CREATE INDEX IF NOT EXISTS` に対応していないため、既存インデックスの重複エラー（1061）のみ無視しています。
- **監査ログ (`AuditSink`) の設計**:
  - 出力先は `aegis_core::AuditSink` トレイトで抽象化し、`AuditService` が `[audit].sinks` に列挙された全出力先へ順に配送します。出力先の失敗はエラーログに留め、リクエストは失敗させません。
  - `occurred_at` は UTC・マイクロ秒精度・`Z` 終端の RFC 3339 に正規化して保存するため、DB 上の文字列比較で期間検索できます。API の `since` / `until` も同じ形式に正規化してから比較します。
  - 操作主体はリクエストのセッションから解決する `AuditContext` エクストラクターで取得します（`FromRequestParts`、失敗しない）。クライアント IP 取得のため、サーバーは `into_make_service_with_connect_info::<SocketAddr>()` で起動しています。
  - セッショントークンは Bearer 認証情報のため保存せず、`auth::session_fingerprint`（SHA-256 先頭 16 桁）で突き合わせます。
- **設定ファイルの読み込み方針**:
  - `AegisConfig` 配下の全構造体は `#[serde(default)]` 付きで、任意のセクション・項目を省略しても `Default` 実装の値で補完されます。新しい設定項目を追加する際は、必ず `Default` 実装に既定値を追加してください。
  - `AegisConfig::load_or_default` はファイル不在時のみ既定値へフォールバックし、パースエラーは `Err` を返してサーバー起動を中止します（設定ミスに気付かず既定値で動き続けることを防ぐため）。
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
  - 本番イメージは `Dockerfile` によるマルチステージビルド（`node:22.22-alpine` -> `rust:1.99.1-slim-bookworm` -> `debian:bookworm-slim`）で構築され、イメージサイズは約 163MB です。
  - ベースイメージはバージョンを固定しています（`latest` は Rust のリリースごとに変わってレイヤーキャッシュが無効になり、実行環境の bookworm と OS もずれるため）。Rust を更新する際は `Dockerfile` のタグを上げてください。
  - Rust のビルドは cargo-chef で「依存クレートのビルド（`cargo chef cook`）」と「ワークスペースのビルド」を分けており、`Cargo.toml` / `Cargo.lock` が変わらない限り依存のレイヤー（約 1.7GB）は CI の GitHub Actions キャッシュ（`type=gha`）から再利用されます。残りの所要時間の大半は release プロファイルの `lto = true` / `codegen-units = 1` によるリンク時最適化（約 3 分）です。
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
- **認証拡張方針（Crate 抽象化と Wasm プラグインフックの役割分担）**:
  - 認証・認可におけるセキュリティリスク（認証バイパス、暗号脆弱性、トークン漏洩）を防ぐため、認証判定や暗号処理（SAML XML-DSIG、OIDC JWT/JWKS 検証など）をサードパーティ Wasm プラグインには直接開放しません。
  - 代わりに `aegis-core` の `AuthProvider` トレイトによる Rust Crate（静的コンパイル / Cargo Features）として安全に実装・分離します。
  - 一方で、認証成功後のメタデータ付加、部署属性によるロールマッピング、Slack/Teams へのログイン監査通知などの安全な拡張ロジックのみを Wasm プラグイン（`on_user_authenticated` フック）に委譲する設計を採用します。
- **コンテンツ公開 & テーマエンジン・プラグイン書き換えアーキテクチャ**:
  - `MiniJinja` をコアエンジンとした `aegis-theme` クレートを導入。ビルトインのデフォルトテンプレート（`base.html`, `index.html`, `article.html`, `page.html`, `404.html`）をバイナリ内にコンパイル時埋め込みしているため、追加のテーマファイルなしでも即座に洗練されたモダン Web サイトを配信可能です。
  - テンプレートの探索は「プラグイン/インメモリ動的登録 (`register_template`)」→「ファイルシステムテーマ (`themes/{active}/templates/`)」→「ビルトインデフォルト」の 3 層フォールバック機構を採用。
  - プラグインフック `on_render_page` により、ページレンダリング直前に `PageRenderContext` をプログラマブルに書き換え可能（サイトタイトル修飾、カスタム CSS 注入、`<head>` タグ追加、テーマ変数の動的更新、テンプレート差し替え）。
  - 一般公開ルート（`GET /`, `GET /articles`, `GET /articles/{slug}`, `GET /{slug}`）と管理画面（`GET /admin` & `/assets`）を同一サーバー上でルーティング分離し、下書き記事はゼロトラストに 404 で遮断。記事本文は `StreamingHtmlRewriter` を透過しセキュリティ・パフォーマンス最適化（rel 属性付与、画像遅延読み込み等）が自動適用されます。


