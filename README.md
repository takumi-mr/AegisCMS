# 次世代セキュアCMS「AegisCMS (仮称)」基本設計書

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
