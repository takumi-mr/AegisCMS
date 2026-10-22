# 次世代セキュアCMS「AegisCMS」

---

## 1. システム概要と設計思想

### 1.1 背景と課題

従来のモノリシックCMS（WordPress等）は、直感的なコンテンツ編集体験とフック機構による高い拡張性を誇る一方、以下の構造的欠陥を抱えていた。

1. **プロセス・メモリ空間の非分離**: プラグインが本体コアやデータベースと同一権限・同一メモリ空間で動作するため、単一の不具合・脆弱性がシステム全体の侵害・停止に直結する。
2. **EAV（Entity-Attribute-Value）によるクエリ劣化**: メタデータ（`wp_postmeta`等）のkey-value集約により、リレーション・インデックス最適化が阻害される。
3. **構成とデータの結合**: 環境依存設定がDB内にシリアライズ保存され、IaC/GitOpsによるバージョン管理やステージング移行が困難。

### 1.2 コア設計原則

- **Zero-Trust Extensions**: プラグインはすべてWebAssembly（Wasm）サンドボックス内で隔離実行し、リソースアクセスはケイパビリティベースのホワイトリスト制とする。
- **Declarative & Typed Schemas**: コンテンツモデルは宣言的スキーマ（YAML/JSON）で定義し、内部RDBテーブルを自動マイグレーションする。
- **Code-as-Configuration**: 記事本文やメディア等の「コンテンツ」のみをDB管理とし、設定・スキーマ・プラグイン構成はすべてGit管理可能とする。
- **Deterministic Content Manipulation**: 生HTML文字列の直接編集を禁止し、AST（抽象構文木）変換およびShadow DOMによってUI/スコープをカプセル化する。

---

## 2. システムアーキテクチャ

### 2.1 全体構成図

```
┌────────────────────────────────────────────────────────────────────────┐
│ Client Tier                                                            │
│  - Edge Browser (Blocks / Web Components in Shadow DOM)               │
│  - Admin SPA (React / Solid.js Block Editor)                           │
└───────────────────────────────────┬────────────────────────────────────┘
                                    │ HTTPS (JSON-RPC / GraphQL / REST)
┌───────────────────────────────────▼────────────────────────────────────┐
│ Host Engine Tier (Core Binary: Rust)                                   │
│  ┌───────────────────────┐  ┌────────────────────────────────────────┐ │
│  │ HTTP / API Gateway    │  │ Schema & Migration Engine (Declarative)│ │
│  └───────────┬───────────┘  └───────────────────┬────────────────────┘ │
│              │                                  │                      │
│  ┌───────────▼──────────────────────────────────▼────────────────────┐ │
│  │ Content Pipeline (Block AST Parser / Streaming HTMLRewriter)      │ │
│  └───────────┬───────────────────────────────────────────────────────┘ │
│              │                                                         │
│  ┌───────────▼───────────┐         ┌─────────────────────────────────┐ │
│  │ Capability Guard      │         │ Wasm Runtime Engine (Wasmtime)  │ │
│  │ (Permission Manifest) │◄───────►│  ┌───────────────────────────┐  │ │
│  └───────────────────────┘         │  │ Linear Memory Sandbox     │  │ │
│                                    │  │  - Extism / WIT ABI Layer │  │ │
│                                    │  │  - Plugin Guests          │  │ │
│                                    │  │    (AssemblyScript/TinyGo)│  │ │
│                                    │  └───────────────────────────┘  │ │
│                                    └─────────────────────────────────┘ │
└───────────────────────────────────┬────────────────────────────────────┘
                                    │ SQL (Prepared Statements)
┌───────────────────────────────────▼────────────────────────────────────┐
│ Storage Tier                                                           │
│  - SQLite / libSQL (Single-Node Zero-Config)                           │
│  - PostgreSQL / MySQL (Distributed Enterprise)                         │
└────────────────────────────────────────────────────────────────────────┘

```

### 2.2 主要コンポーネント構成

1. **Core Host Engine (Rust)**

- シングルバイナリ配布（Zero-Config）。起動時に設定・スキーマ定義を読み込み、ローカルDB（SQLite/libSQL）または外部DBへ接続。

2. **Admin Studio (TypeScript + React / Solid.js)**

- Gutenbergライクなドラッグ＆ドロップ対応ブロックエディタ。UIはAPI経由でコアと完全疎結合に動作。

3. **Wasm Plugin Supervisor**

- Wasmtimeを基盤とし、Fuel消費制限およびエポック割り込みによるタイムアウト・暴走防止機構を常駐。

4. **Content & Rendering Pipeline**

- ASTトランスフォーマー、カスタムブロックレンダラー、エッジキャッシュ制御（On-demand ISR / Cache-Tag Purge）を統合。

### 2.3 クレート構成と各モジュールの責務 (`crates/`)

AegisCMS は Cargo ワークスペースによるモノレポ構成を採用しており、各関心事が独立したクレートとして疎結合に設計・配置されています。

```
crates/
├── aegis-core       # ドメインモデル、システム設定、ストレージ抽象、統一エラー
├── aegis-ast        # Block AST データ構造、HTML決定論的レンダラー、ストリーミングリライター
├── aegis-schema     # 宣言的YAMLスキーマ、マルチダイアレクトDDL生成、TS/OpenAPIコード生成
├── aegis-runtime    # Wasmtimeサンドボックス、ケイパビリティ検証ガード、ホストAPIリンカー
└── aegis-server     # Axum API Gateway、RDB/ストレージアダプタ、CLI、Admin SPA配信
```

| クレート名                                                    | 役割・責務                                               | 主な構成モジュール・対応技術                                                                                                                                           |
| :------------------------------------------------------------ | :------------------------------------------------------- | :--------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| [aegis-core](file:///d:/projects/cms/crates/aegis-core)       | システム共通ドメイン基盤・設定・抽象トレイト             | `config` (TOML設定ローダー), `storage` (`StorageBackend` トレイト), `entity` (`Post`, `PostContext`), `manifest` (`PluginManifest`), `error` (`AegisError`)            |
| [aegis-ast](file:///d:/projects/cms/crates/aegis-ast)         | コンテンツ構造化表現・レンダリング・セキュリティ属性付与 | `ast` (`BlockNode`), `pipeline` (`AstPipeline`), `renderer` (`HtmlRenderer`), `rewriter` (`StreamingHtmlRewriter`, Cloudflare `lol-html`)                              |
| [aegis-schema](file:///d:/projects/cms/crates/aegis-schema)   | 宣言的スキーマ定義・マルチDB DDL生成・型定義生成         | `definition` (`SchemaDefinition`, `RelationTarget`), `loader` (`SchemaLoader`), `migration` (`SchemaMigration`, `SqlDialect`), `codegen` (TypeScript, OpenAPI 3.1)     |
| [aegis-runtime](file:///d:/projects/cms/crates/aegis-runtime) | WebAssembly サンドボックス隔離実行・権限制御             | `engine` (Wasmtime, Fuel制限, タイムアウト), `guard` (`CapabilityGuard`), `host_api` (ホストAPI), `instance` (`WasmPluginInstance`), `supervisor` (`PluginSupervisor`) |
| [aegis-server](file:///d:/projects/cms/crates/aegis-server)   | コア HTTP サーバー・インフラアダプタ・SPA配信            | `db` (`DatabaseAdapter`, SQLite/Postgres/MySQL), `storage` (`LocalStorageBackend`, S3 SigV4 `S3StorageBackend`), `handlers` (CRUD, アップロード), `main` (CLI)         |

---

#### 1. `crates/aegis-core` (ドメインモデル & 共通基盤)

システム全体で共有されるデータモデル、エラー型、設定構造体、抽象インターフェースを集中管理する最下層クレートです。

- **`config`**: `AegisConfig`（サーバー、データベース、ストレージ、キャッシュ、パス、ランタイム、セキュリティ設定）。`config/*.toml` からの読み込みおよび環境変数オーバーライドに対応。
- **`storage`**: ストレージ抽象化トレイト `StorageBackend`（`put`, `get`, `delete`, `public_url`）およびメタデータ `StorageMeta` の定義。
- **`cache`**: キャッシュ抽象化トレイト `CacheBackend`（`get`, `set`, `delete`, `exists`）およびバックエンド種別 `CacheBackendKind`（`Memory`, `Redis`）の定義。
- **`entity`**: 記事・コンテンツエンティティの共通構造（`Post`, `PostContext`, `PostStatus`, `PostId`）。
- **`manifest`**: プラグインが要求する権限（ネットワークドメイン、KVSネームスペース、利用フック、メモリ/Fuel制限）を定義する `PluginManifest`。
- **`error`**: システム統一エラー列挙体 `AegisError` および `Result` 型エイリアス。

#### 2. `crates/aegis-ast` (コンテンツ AST & レンダリングパイプライン)

コンテンツを生 HTML ではなく構造化された AST（抽象構文木）として扱い、安全なレンダリングと高速な HTML 改変を担当します。

- **`ast`**: 見出し、段落、コード、画像、カスタム要素を表現する木構造 `BlockNode`。
- **`pipeline`**: AST 変換処理を直列実行する `AstPipeline` およびビルトイントランスフォーマー `HeadingIdTransformer`。
- **`renderer`**: AST を決定論的かつ安全に HTML 文字列へ展開する `HtmlRenderer`。
- **`rewriter`**: Cloudflare 製 CSS セレクタストリーミングパーサー `lol-html` を用いた `StreamingHtmlRewriter`。外部リンクへの `rel="noopener noreferrer"` 自動付与（自サイト除外バイパス対応）や、`<img>` タグへの `loading="lazy"` / `decoding="async"` 強制インジェクションを実行。

#### 3. `crates/aegis-schema` (宣言的スキーマ & コード生成エンジン)

YAML で記述されたコンテンツスキーマを解析し、マルチ RDBMS 向けのマイグレーション DDL やクライアント向けの型定義を自動生成します。

- **`definition`**: スキーマ定義構造体 `SchemaDefinition`、フィールド型 `FieldType`、およびリレーション先参照 `RelationTarget`（例: `category.id`）。
- **`loader`**: 指定ディレクトリ（`schemas/`）から YAML ファイルを自動走査・読み込む `SchemaLoader`。
- **`migration`**: `SqlDialect`（`Sqlite`, `Postgres`, `MySql`）に対応した DDL 自動生成エンジン `SchemaMigration`。型マッピング、ユニーク制約、インデックス定義を各 RDBMS 向けに出力。
- **`codegen`**: スキーマ定義からフロントエンド用 TypeScript 型定義を出力する `TypeScriptGenerator`、および REST API 仕様を出力する `OpenApiGenerator`（OpenAPI 3.1 仕様）。

#### 4. `crates/aegis-runtime` (ゼロ・トラスト Wasm プラグインランタイム)

WebAssembly（Wasm）サンドボックスによるサードパーティ拡張の安全な隔離実行を提供します。

- **`engine`**: Wasmtime をカプセル化した `WasmEngine`。Fuel（命令数クォータ）計測と非同期タイムアウトによる二重の暴走・DoS 防御。
- **`guard`**: プラグインの `plugin.manifest.json` に基づき、未許可のネットワーク通信・KVSアクセス・未登録フック実行を遮断する `CapabilityGuard`。
- **`host_api`**: サンドボックス内からホスト側の機能（ロギング `host:log`、HTTP通信 `host:http_fetch`、Key-Value操作 `host:kv_get`/`host:kv_set`）を安全に呼び出すリンカー。
- **`instance`**: ゲスト Wasm バイナリの実行インスタンス `WasmPluginInstance`。Fat Pointer 形式（`ptr:i32, len:i32`）およびゲスト側アロケータ（`allocate`）を用いた安全なリニアメモリデータ受け渡し。
- **`supervisor` / `loader`**: ディレクトリ内の全プラグインを自動発見・初期化し、ライフサイクルフック（`on-before-save`, `transform-content-ast`）を実行する `PluginSupervisor`。

#### 5. `crates/aegis-server` (HTTP Gateway, インフラアダプタ & Admin SPA 配信)

Axum を基盤とした REST API Gateway サーバーであり、すべてのインフラ接続と HTTP ハンドラーを統合します。

- **`db`**: `DatabaseAdapter` トレイトに基づくマルチ RDBMS 具象実装（`SqliteAdapter`, `PostgresAdapter`, `MySqlAdapter`）。`PRAGMA table_info` や `information_schema.columns` を走査し、既存テーブルへの未反映カラムを自動追加するスキーマ進化マイグレーション（`ALTER TABLE ... ADD COLUMN`）を内蔵。
- **`storage`**: `StorageBackend` トレイトに基づくマルチストレージ具象実装。ローカルディスク・ブロックストレージ対応の `LocalStorageBackend` と、AWS S3 / Cloudflare R2 / MinIO 対応の `S3StorageBackend`（ネイティブ AWS SigV4 認証署名エンジン内蔵）。
- **`cache`**: `CacheBackend` トレイトに基づくキャッシュ具象実装。TTL 有効期限管理を備えたデフォルトのインメモリキャッシュ `MemoryCacheBackend` と、分散環境・マルチサーバー構成対応の `RedisCacheBackend`（オプショナル）。
- **`auth`**: `CacheBackend` をバックストアとして活用したステートレスセッション管理 `SessionService`。Bearer トークン / `X-Session-Id` による認証状態の永続化と検証。
- **`handlers`**: RESTful API エンドポイント群（コンテンツ CRUD・リレーション自動解決、スキーマ情報、動的 OpenAPI 仕様配信、マルチパートファイルアップロード `POST /api/v1/storage/upload`、アップロード済みファイル一覧 `GET /api/v1/storage/objects`、セッション認証 `POST/GET/DELETE /api/v1/auth/session`（ユーザー名 / パスワードでログイン、[docs/authentication.md](docs/authentication.md)）、ヘルスチェック）。
- **`state`**: 抽象化された `DatabaseAdapter`、`StorageBackend`、`CacheBackend`、`SessionService`、スキーマ定義、プラグインスーパーバイザーを保持する共有状態 `AppState`。
- **`main`**: CLI サブコマンド（サーバー起動 `run`、マイグレーション実行 `migrate`、型生成 `codegen`、ユーザー管理 `user add|passwd|role|disable|enable|list`）および Admin Studio SPA の静的アセット配信（`fallback_service`）。

#### 6. `crates/aegis-plugin-sdk` & `crates/aegis-plugin-macros` (公式 Wasm プラグイン SDK)

サードパーティ開発者が安全・高速に WebAssembly プラグインを開発するための公式 SDK です。

- **`#[aegis_plugin]`**: メモリアロケータ（`allocate`）、ポインタパッキング、JSON シリアライズ/デシリアライズを完全自動隠蔽する高水準手続型マクロ。
- **`Plugin` トレイト**: ライフサイクルフック（記事保存前 `on_before_save`、コンテンツ AST 変換 `transform_content_ast`）の型安全な実装インターフェース。
- **`host` API**: ホスト側ロガー（`log_info!`, `log_warn!`）、ネットワークアクセス事前検証（`net::is_allowed`）、ネームスペース分離 KVS 操作（`kv::set`）をラップした型安全関数群。
- **ネイティブテストモック内蔵**: Wasm 環境にビルドせずとも通常の `cargo test` でプラグインのビジネスロジックを単体テスト可能。
- **公式テンプレート**: 即座に開発を開始できる [`plugins/template-rust`](file:///d:/projects/cms/plugins/template-rust) を同梱（詳細は [Wasm プラグイン開発ガイド](file:///d:/projects/cms/docs/plugin_sdk_guide.md) を参照）。

> [!TIP]
> 関連リファレンス: [設定リファレンス (`docs/configuration.md`)](docs/configuration.md) / [プラグインフック & ホスト API リファレンス (`docs/plugin_hooks.md`)](docs/plugin_hooks.md) / [監査ログリファレンス (`docs/audit.md`)](docs/audit.md)

---

## 3. プラグイン・サンドボックスアーキテクチャ

### 3.1 実行分離とメモリ境界

- ホストとプラグイン（Guest）の物理メモリ空間は完全分離。
- メモリ共有はWasmリニアメモリ上の特定バッファ領域へのポインタ（`ptr`）およびデータ長（`len`）による受け渡しに限定。
- ホスト側からWasmメモリを書き換える際は、プラグインがエクスポートするメモリアロケータ（`allocate`）を事前に呼び出して安全領域を確保。

### 3.2 抽象ABI定義（Wasm Interface Type / WIT）

```wit
package aegis:cms-plugin;

interface types {
    enum log-level { debug, info, warn, error }
    enum http-method { get, post, put, delete }

    record http-request {
        url: string,
        method: http-method,
        headers: list<tuple<string, string>>,
        body: option<list<u8>>,
    }

    record http-response {
        status: u16,
        headers: list<tuple<string, string>>,
        body: list<u8>,
    }

    record block-node {
        name: string,
        attributes-json: string,
        children: list<block-node>,
    }

    record post-context {
        id: string,
        slug: string,
        author-id: string,
        blocks: list<block-node>,
    }
}

interface host-api {
    use types.{log-level, http-request, http-response};

    // ログ出力（コンソール・コアロガー統合）
    log: func(level: log-level, message: string);

    // 制限付きネットワーク通信（マニフェストによる検証必須）
    http-fetch: func(req: http-request) -> result<http-response, string>;

    // スコープ制限付き読み込み専用KVS/クエリ
    kv-get: func(key: string) -> option<string>;
    kv-set: func(key: string, value: string) -> result<_, string>;
}

world plugin-runtime {
    import host-api;
    use types.{post-context, block-node};

    // ライフサイクルフック
    export init: func() -> result<_, string>;
    export on-before-save: func(post: post-context) -> result<post-context, string>;
    export transform-content-ast: func(blocks: list<block-node>) -> result<list<block-node>, string>;
}

```

### 3.3 ケイパビリティベースの権限モデル

プラグインはルートディレクトリに `plugin.manifest.json` を同梱し、利用リソースを明示宣言する。ホストエンジンは呼び出し時にこれらを厳格に照合する。

```json
{
    "name": "aegis-plugin-slack-notifier",
    "version": "1.0.0",
    "entrypoint": "plugin.wasm",
    "capabilities": {
        "network": {
            "allowed_domains": ["hooks.slack.com"]
        },
        "storage": {
            "kv_namespace": "slack_notifier_state"
        },
        "hooks": ["on-before-save", "transform-content-ast"]
    },
    "resource_limits": {
        "max_memory_mb": 16,
        "max_fuel_units": 100000000,
        "execution_timeout_ms": 250
    }
}
```

- **リソースクォータ**: WasmtimeのFuelインストルメンテーションにより命令数を計数。無限ループや過負荷時はホスト側で即座にトラップを発生させ、本体プロセスを保護。
- **ドメインホワイトリスト**: `allowed_domains` 外へのHTTPリクエスト試行時は `PermissionDenied` エラーを返却。

---

## 4. コンテンツ・DOM操作パイプライン

生HTMLの正規表現置換や直接挿入によるXSS・DOM破損リスクを排除するため、操作対象に応じた3層の改変モデルを提供する。

```
              ┌──────────────────────────────────────────────┐
              │             Content Raw Storage              │
              └──────────────────────┬───────────────────────┘
                                     │
                        Parse to Block AST (JSON)
                                     │
                                     ▼
┌────────────────────────────────────────────────────────────────────────────┐
│ 1. AST Transformation Layer (Wasm)                                         │
│   - 見出しID自動生成、目次（TOC）ツリー構築、コンテンツ検閲                  │
└────────────────────────────────────┬───────────────────────────────────────┘
                                     │
                          Render Base HTML Stream
                                     │
                                     ▼
┌────────────────────────────────────────────────────────────────────────────┐
│ 2. Streaming Rewriter Layer (Host Engine: lol-html)                       │
│   - セレクタベースの属性付与 (`rel="noopener"`, 画像Lazy-load属性強制)       │
└────────────────────────────────────┬───────────────────────────────────────┘
                                     │
                         Output to Client Browser
                                     │
                                     ▼
┌────────────────────────────────────────────────────────────────────────────┐
│ 3. Client Isolation Layer (Browser DOM)                                    │
│   - Web Components + Shadow DOM による動的UI（フォーム、スライダー）       │
└────────────────────────────────────────────────────────────────────────────┘

```

1. **AST（抽象構文木）変換**:

- コンテンツ構造の変更（ノード追加、ブロック属性書き換え）はWasmサンドボックス内でASTに対してのみ実行。不正なタグ閉じ忘れやスクリプト直埋め込みを根本から防止。

2. **ストリーミングHTMLリライター**:

- ホストエンジンの出力直前ストリームに対し、Rust製 `lol-html` 経由でCSSセレクタに基づく属性注入を適用。メモリフットプリントを最小化。

3. **Web Components によるフロントエンド隔離**:

- プラグインが提供するブラウザUIはShadow DOMを適用したカスタム要素（例: `<plugin-poll-widget>`）として出力。テーマやグローバルCSS/JSとの衝突をゼロ化。

---

## 5. データモデルと構成管理

### 5.1 宣言的スキーマエンジン

EAVパターンを完全廃止し、コンテンツタイプはYAML形式で定義する。

```yaml
# schemas/articles.yaml
content_type: article
label: "技術記事"
fields:
    - name: title
      type: string
      required: true
      max_length: 255
      index: true
    - name: slug
      type: string
      required: true
      unique: true
    - name: published_at
      type: timestamp
      nullable: true
      index: true
    - name: tech_category
      type: relation
      target: category.id
    - name: custom_meta
      type: jsonb
      indexed_paths:
          - "seo.canonical_url"
```

- **自動マイグレーション**: コアエンジン起動時またはスキーマ同期コマンド実行時に、スキーマ差分を検出し内部テーブル（PostgreSQL/SQLite）へ自動マイグレーションを実行。
- **型定義自動生成**: スキーマ定義からTypeScript型定義およびGraphQL/OpenAPI仕様書をコンパイル時に出力。

### 5.2 GitOps・環境分離構成

- **リポジトリ管理対象（Code-as-Configuration）**:
    - スキーマ定義ファイル (`/schemas/*.yaml`)
    - サイト設定・権限ロール設定 (`/config/*.json`)
    - プラグイン構成・有効化リスト (`/plugins/manifest.lock`)
    - テーマ・UIテンプレート (`/themes/*`)

- **データベース管理対象**:
    - 記事エンティティ・ブロック本文データ
    - アップロードメディアメタデータ
    - ユーザーアカウント・セッション情報

---

## 6. 技術スタック選定基準

| レイヤー               | 採用技術                            | 選定理由・技術的トレードオフ                                                                                                 |
| ---------------------- | ----------------------------------- | ---------------------------------------------------------------------------------------------------------------------------- |
| **コアエンジン**       | **Rust**                            | メモリ安全性、ゼロコスト抽象化、起動速度。Wasmtimeランタイムとのネイティブ親和性。単一バイナリ配布が可能。                   |
| **ストレージ**         | **SQLite (libSQL) / PostgreSQL**    | エッジ/単一サーバー運用時はZero-Configで動くSQLiteを使用。クラスタリング時はPostgreSQL/JSONBインデックスへ透過的に切替可能。 |
| **サンドボックス**     | **Wasmtime + Extism**               | Bytecode Alliance準拠の安定したランタイム。Extismをラッパーに採用することでマルチ言語SDKの生成コストを圧縮。                 |
| **プラグイン開発言語** | **Tier 1: AssemblyScript / TinyGo** | **Tier 1**: 軽量GC・型安全・数十〜数百KBのコンパクトバイナリでWeb開発者の参入障壁を低減。                                    |
| **プラグイン開発言語** | **Tier 2: Rust**                    | **Tier 2**: 極限のパフォーマンスとメモリ制御を要する処理用。                                                                 |
| **管理画面**           | **TypeScript + Solid.js / React**   | 細粒度リアクティビティまたは巨大なReactコンポーネント資産を活用したブロック編集UIの高速動作。                                |

---

## 7. 開発・運用ロードマップ

- **フェーズ 1 (PoC・コア基盤確立)**
    - Rust製コアエンジンとWasmtime組み込みのプロトタイプ実装。
    - AST変換フックおよびメモリ受け渡し（Fat Pointer方式）のベンチマーク検証。

- **フェーズ 2 (スキーマエンジンとエディタ統合)**
    - YAMLスキーマからのDB自動マイグレーションエンジン実装。
    - 基本ブロックエディタUIとREST/GraphQL APIの疎結合結合。

- **フェーズ 3 (SDK整備とエコシステム公開)**
    - AssemblyScript / TinyGo向け公式プラグインSDKの提供。
    - ケイパビリティ検証ガードおよびマニフェストパーサーのセキュリティ監査。
