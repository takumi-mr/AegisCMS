# AegisCMS 開発進捗 & TODOリスト (TODO.md)

> [!NOTE]
> 作業を再開する際は、まず本ドキュメントの「現在の状況と直近の再開手順」および「TODOタスク一覧」を確認してください。  
> **ルール**: 各修正・実装ステップの終了後には、必ず本ドキュメントを更新してください。

---

## 1. 現在の状況と直近の再開手順

- **現在のステータス**: フェーズ 2（Admin Studio SPA & ブロックエディタ統合）完了。
- **動作確認済み**:
  - Rust ワークスペース（`aegis-core`, `aegis-ast`, `aegis-schema`, `aegis-runtime`, `aegis-server`）のビルド・単体テスト (`cargo test --workspace`) 100% 成功。
  - YAMLスキーマ (`schemas/articles.yaml`) から SQLite テーブルへの自動マイグレーション動作。
  - Wasmtime による Wasm プラグイン（`plugins/slack-notifier`）のサンドボックス隔離実行・ホストAPI連携 (`host:log`)・タイムアウト保護。
  - REST API Gateway による記事作成 (`POST /api/v1/contents/article`)、AST変換、HTML決定論的レンダリング。
  - Vite + React + TypeScript による Admin Studio SPA の構築（ダークモード＆グラスモフィズム、ブロックエディタ、リアルタイムプレビュー、スキーマ・記事一覧同期）。
  - `aegis-server` からの Admin Studio 静的アセット SPA 配信 (`fallback_service(ServeDir)`) と E2E ブラウザ疎通完了。

### 🚀 次に作業を再開するときの着手項目
**フェーズ 2 拡張: コア機能拡張**
1. **ストリーミング HTML リライター組み込み (`lol-html`)**:
   - `crates/aegis-ast` または `crates/aegis-server` に `lol-html` を導入。
   - 出力直前ストリームにおいて、外部リンクへの `rel="noopener noreferrer"` 自動付与や `<img>` タグへの `loading="lazy"` / `decoding="async"` 強制インジェクションを実装。
2. **追加スキーマ定義とリレーション解決**:
   - `schemas/categories.yaml` (カテゴリ管理) および `schemas/pages.yaml` (固定ページ) の作成。
   - 記事（`article`）の `tech_category` 等のリレーションフィールドの自動解決・JOIN取得。
3. **OpenAPI / TypeScript 型定義 自動生成**:
   - YAMLスキーマ定義からフロントエンド向けの TypeScript 型定義（`admin/src/types/schemas.d.ts` 等）を自動生成する CLI サブコマンド (`aegis-server codegen-types`)。

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
- [x] **CI/CD & コンテナ基盤整備**
  - [x] GitHub Actions 自動テスト・リント・ビルド CI パイプライン構築 (`.github/workflows/ci.yml`)
  - [x] マルチステージ本番 Dockerfile の作成（Node.js + Rust 最新版 + 最小 Debian 実行層、非 root ユーザー実行、サイズ 163MB、HEALTHCHECK 内蔵）
  - [x] Docker 用設定ファイル (`config/docker.toml`) および `.dockerignore` の追加
  - [x] CLI 引数および環境変数による host/port オーバーライド対応（`AEGIS_CONFIG`, `HOST`, `PORT`）

---

### ⏳ 次に取り組むタスク (Next Up)

#### フェーズ 2 拡張: コア機能拡張
- [ ] **ストリーミング HTML リライター組み込み**
  - [ ] `lol-html` を使用した出力直前ストリームでの属性インジェクション（`rel="noopener noreferrer"`, 画像 `loading="lazy"` 強制）
  - [ ] 単体テストおよびベンチマーク検証
- [ ] **追加スキーマ定義とリレーション解決**
  - [ ] `schemas/categories.yaml` (カテゴリ管理)
  - [ ] `schemas/pages.yaml` (固定ページ)
  - [ ] リレーションフィールドの解決ロジック
- [ ] **OpenAPI / TypeScript型定義 自動生成**
  - [ ] スキーマ定義から TypeScript 型ファイルを出力するコマンド

---

### 🔮 将来のロードマップ (Future)
- [ ] Wasm プラグイン向け公式 SDK（AssemblyScript / Rust）テンプレートリポジトリ整備
- [ ] PostgreSQL / libSQL 対応（分散ストレージモード）
- [ ] On-demand ISR / Cache-Tag Purge によるエッジキャッシュ制御

---

## 3. 技術メモ・申し送り事項 (Memo & Notes)

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
