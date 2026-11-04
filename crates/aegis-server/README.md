# aegis-server

AegisCMS の HTTP サーバー、CLI エントリポイント、REST API ルーティング、および全体統合クレート。

## 概要

`aegis-server` は、すべてのドメインクレート（`aegis-db`, `aegis-storage`, `aegis-auth`, `aegis-runtime`, `aegis-theme` 等）を統合し、シングルバイナリとして動作する Axum ベースの HTTP サーバーおよび CLI ツールを提供します。

公開 Web サイトの SSR レンダリング、Admin Studio (React SPA) の静的アセット配信、コンテンツ管理 RESTful API、セキュリティヘッダー付与、認可ミドルウェア、定時タスクスケジューラ（公開予約や定期バックアップ）などを統括します。

## 主な責務・提供機能

- **CLI コマンド (`main.rs`)**:
  - `aegis-server run`: コアサーバーの起動。
  - `aegis-server migrate`: スキーマの自動マイグレーション実行。
  - `aegis-server dump` / `restore`: データベースおよびストレージの一括バックアップと復元。
  - `aegis-server theme dev`: テーマのローカル開発サーバー起動。
  - `aegis-server plugin dev`: Wasm プラグインのローカル開発サーバー起動。
  - 管理者ユーザー作成、パスワードリセットなどの管理ユーティリティ。
- **HTTP / REST API ルーティング (`app.rs`, `handlers/`)**:
  - コンテンツ CRUD、メディアアップロード、スキーマ取得、プラグイン設定、コメント、EC 注文、監査ログ等の API。
- **公開ページレンダリング (`render.rs`)**:
  - `aegis-theme` による MiniJinja SSR レンダリング、メタタグ/SEO 出力、Wasm プラグインフック（`on_render_page`）の適用。
- **Admin Studio 配信**:
  - React ベースの SPA を組み込み配信、HTML5 History API のフォールバック。
- **セキュリティ・ミドルウェア**:
  - セキュリティヘッダー（CSP、HSTS、X-Content-Type-Options、X-Frame-Options）。
  - Cookie / Bearer Token によるセッション認証および RBAC 認可。
  - レートリミット、CORS、監査ログ（`AuditOrigin`）の自動コンテキスト抽出。
- **統合設定 (`config::AegisConfig`)**:
  - `aegis.toml` から全ドメインクレートの設定（DB, ストレージ, キャッシュ, 認証, EC 等）を一元パース・管理。

## モジュール構成

| モジュール | 説明 |
| :--- | :--- |
| `app` | Axum Router の構築、ミドルウェアチェイン設定 |
| `config` | 全体設定 `AegisConfig` の定義と読み込み |
| `handlers` | REST API エンドポイントハンドラ群（コンテンツ、ユーザー、メディア、EC 等） |
| `render` | 公開ページのテンプレート描画・SEO メタ・プラグインフック合成 |
| `auth` | 認証・認可ミドルウェア、セッション検証 |
| `audit` | リクエスト情報から監査ログ用コンテキスト (`AuditOrigin`) を抽出 |
| `scheduler` | 公開予約、ゴミ箱自動パージ等のバックグラウンドタスクスケジューラ |
| `headers` | Content-Security-Policy や各種セキュリティヘッダーの構築 |
| `state` | アプリケーション共有状態 (`AppState`) |
