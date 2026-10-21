# AegisCMS 開発進捗 & TODOリスト (TODO.md)

> [!NOTE]
> 作業を再開する際は、まず本ドキュメントの「現在の状況と直近の再開手順」および「TODOタスク一覧」を確認してください。  
> **ルール**: 各修正・実装ステップの終了後には、必ず本ドキュメントを更新してください。

---

## 1. 現在の状況と直近の再開手順

- **現在のステータス**: フェーズ 1（コア基盤・Wasmランタイム PoC）完了。
- **動作確認済み**:
  - Rust ワークスペース（`aegis-core`, `aegis-ast`, `aegis-schema`, `aegis-runtime`, `aegis-server`）のビルド・単体テスト (`cargo test --workspace`) 100% 成功。
  - YAMLスキーマ (`schemas/articles.yaml`) から SQLite テーブルへの自動マイグレーション動作。
  - Wasmtime による Wasm プラグイン（`plugins/slack-notifier`）のサンドボックス隔離実行・ホストAPI連携 (`host:log`)・タイムアウト保護。
  - REST API Gateway による記事作成 (`POST /api/v1/contents/article`)、AST変換、HTML決定論的レンダリング。

### 🚀 次に作業を再開するときの着手項目
**Admin Studio（管理画面フロントエンド: TypeScript + React / Vite）のプロトタイプ構築**
1. `admin/` ディレクトリに Vite + React + TypeScript プロジェクトを立ち上げ。
2. コアサーバーの API Gateway (`http://127.0.0.1:8080`) と疎通（CORS設定済み）。
3. スキーマ一覧 (`GET /api/v1/schemas`) および記事一覧・詳細の表示。
4. ドラッグ＆ドロップ対応のブロックエディタ UI（見出し、段落、コードブロック追加・編集・保存）。

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
- [x] Linter & Formatter 設定の追加 (`rustfmt.toml`, `.editorconfig`, `[workspace.lints]`, Clippy ゼロ警告達成)
- [x] モノレポ構造の整合性確保（クレート別ネスト `.git` の解消）および `main` ブランチでのファーストコミット作成

---

### ⏳ 次に取り組むタスク (Next Up)

#### フェーズ 2: Admin Studio (フロントエンド) & エディタ統合
- [ ] **Admin Studio SPA セットアップ**
  - [ ] `admin/` ディレクトリに Vite + React (TypeScript) プロジェクトを初期化
  - [ ] リッチなモダンUIデザインシステム（ダークテーマ、グラスモフィズム、洗練されたタイポグラフィ）の構築
  - [ ] API クライアント（`/api/v1/schemas`, `/api/v1/contents`）の実装
- [ ] **ブロックエディタ UI 実装**
  - [ ] ブロック追加メニュー（見出し、段落、コード、画像、カスタム要素）
  - [ ] ドラッグ＆ドロップによるブロック並び替え
  - [ ] リアルタイムプレビュー（ASTからレンダリングされたHTMLの表示）
  - [ ] 記事保存・更新・公開ステータス切替

#### フェーズ 2 拡張: コア機能拡張
- [ ] **ストリーミング HTML リライター組み込み**
  - [ ] `lol-html` を使用した出力直前ストリームでの属性インジェクション（`rel="noopener"`, 画像Lazy-load強制）
- [ ] **追加スキーマ定義**
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

- **Wasmtime と Windows 環境の非同期実行**:
  - Windows MSVC 環境では、Wasmtime の同期呼び出しで Trap が発生した場合に Rust 1.98 の `extern "C"` non-unwinding 境界チェックと競合して abort するケースがあります。
  - そのため `aegis-runtime` では `config.async_support(true)` を有効化し、`tokio::time::timeout` でタイムアウトを明示的に強制する多層防御設計を採用しています。
- **リニアメモリのデータ受け渡し**:
  - ホストからゲストへ文字列/JSONを渡す際は、必ずゲスト側がエクスポートする `allocate(len: i32) -> i32` を呼び出して安全なメモリ領域を確保してから書き込みます。
  - ゲストからホストへの返却は、上位32bitが `ptr`、下位32bitが `len` の `i64` パック形式で受け渡しを行っています。
- **データベーステーブル命名規則**:
  - スキーマ `content_type: article` の場合、テーブル名は `content_article` となります。
  - ASTブロック配列は `blocks_json TEXT NOT NULL DEFAULT '[]'` カラムに格納されます。
