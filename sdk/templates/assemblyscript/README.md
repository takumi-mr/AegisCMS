# AegisCMS AssemblyScript WebAssembly Plugin Template

AegisCMS 公式 AssemblyScript (TypeScript 風構文) WebAssembly プラグイン開発テンプレートです。  
Node.js / npm エコシステムと TypeScript の知識を活かして、軽量かつ安全な Wasm プラグインを高速に開発できます。

---

## 🚀 クイックスタート

### 1. 前提条件

Node.js (v18+) および npm が必要です。

```bash
# 依存パッケージ (assemblyscript コンパイラ) のインストール
npm install
```

### 2. WebAssembly バイナリのビルド

リリースプロファイルで `.wasm` を生成します。

```bash
npm run build
```

コンパイルに成功すると、ディレクトリ直下に `plugin.wasm` が出力されます。

`asconfig.json` の `"use": ["abort=assembly/sdk/host/abort"]` は削除しないでください。AssemblyScript は既定で実行時エラーの処理に `env.abort` をインポートしますが、ホストは `host` モジュールしか提供しないため、この設定が無いとプラグインを読み込めません（SDK の `abort` に差し替えて、ログを出してから停止します）。

### 3. AegisCMS への配置

AegisCMS サーバーの `plugins/<name>/`（`<name>` はマニフェストの `name`）に以下のファイルを配置し、設定ファイルの `[plugins].enabled` に `<name>` を追加してサーバーを再起動すると有効になります（配置しただけでは読み込まれません）。

- `plugin.manifest.json` (権限・リソース制限・フック定義)
- `plugin.wasm` (コンパイル済み Wasm バイナリ)

---

## 🧩 プラグイン構造

```text
sdk/templates/assemblyscript/
├── package.json               # npm 依存関係 (@takumi-mr/plugin-sdk を参照) およびビルドスクリプト
├── asconfig.json              # AssemblyScript コンパイラ設定
├── plugin.manifest.json       # プラグインマニフェスト (権限・フック宣言)
├── plugin.wasm                # ビルド済み Wasm バイナリ
└── assembly/
    ├── index.ts               # プラグインのライフサイクルフック実装
    └── sdk/                   # 後方互換用シム (@takumi-mr/plugin-sdk の再エクスポート)
```

公式 npm パッケージ [`@takumi-mr/plugin-sdk`](../../assemblyscript) をインポートして実装します。

### マニフェスト (`plugin.manifest.json`)
ゼロ・トラスト実行モデルに基づき、プラグインが必要とする最小限のケイパビリティ（権限）を宣言します。未宣言の操作はホストランタイムによって即座にブロックされます。

```json
{
  "name": "my-as-plugin",
  "version": "1.0.0",
  "entrypoint": "plugin.wasm",
  "capabilities": {
    "network": { "allowed_domains": ["api.example.com"] },
    "storage": { "kv_namespace": "my_as_state" },
    "hooks": ["on-before-save", "transform-content-ast", "on-render-page"]
  },
  "resource_limits": {
    "max_memory_mb": 16,
    "max_fuel_units": 100000000,
    "execution_timeout_ms": 250
  }
}
```

### 利用可能なライフサイクルフック

| エクスポート関数 | 引数 | 戻り値 | 説明 |
|---|---|---|---|
| `allocate(len: i32)` | `len: i32` | `i32` | メモリアロケータ (必須) |
| `on_before_save(ptr: i32, len: i32)` | `(ptr, len)` | `i64` | 記事保存前の検証・値補正・保存拒否 |
| `transform_content_ast(ptr: i32, len: i32)` | `(ptr, len)` | `i64` | 本文ブロック AST 配列の変換 |
| `on_render_page(ptr: i32, len: i32)` | `(ptr, len)` | `i64` | 公開ページ描画コンテキストの動的更新 |

### 利用可能なホスト API

| モジュール | 関数 | 説明 |
|---|---|---|
| `log` | `log.debug(msg)`, `log.info(msg)`, `log.warn(msg)`, `log.error(msg)` | ホスト側のロガー経由で安全にログを出力 |
| `net` | `net.isAllowed(url)` | アクセス先 URL がマニフェストで許可されているか検証 |
| `kv` | `kv.set(key, value)` | プラグイン専用ネームスペースの KVS へデータを保存 |
