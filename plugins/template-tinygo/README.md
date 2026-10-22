# AegisCMS TinyGo WebAssembly Plugin Template

AegisCMS 公式 TinyGo (Go 言語) WebAssembly プラグイン開発テンプレートです。  
Go の直感的な構文と軽量 Wasm バイナリ生成能力を活かして、高性能かつ安全な Wasm プラグインを開発できます。

---

## 🚀 クイックスタート

### 1. 前提条件

- **Go** (1.21 以上)
- **TinyGo** (0.31 以上)

TinyGo のインストール手順は [TinyGo 公式ドキュメント](https://tinygo.org/getting-started/install/) を参照してください。

### 2. WebAssembly バイナリのビルド

TinyGo コンパイラを使用して、`wasi` または `wasm` ターゲットでビルドします。

```bash
tinygo build -target=wasi -o plugin.wasm main.go
```

最適化とサイズ削減を行う場合：

```bash
tinygo build -target=wasi -opt=z -no-debug -o plugin.wasm main.go
```

コンパイルに成功すると、ディレクトリ直下に `plugin.wasm` が出力されます。

### 3. AegisCMS への配置

AegisCMS サーバーの `plugins/<plugin-name>/` ディレクトリに、以下のファイルを配置するだけでサーバー起動時に自動検出・ロードされます。

- `plugin.manifest.json` (権限・リソース制限・フック定義)
- `plugin.wasm` (コンパイル済み Wasm バイナリ)

---

## 🧩 プラグイン構造

```text
plugins/template-tinygo/
├── go.mod                     # Go モジュール定義
├── plugin.manifest.json       # プラグインマニフェスト (権限・フック宣言)
├── plugin.wasm                # ビルド済み Wasm バイナリ
├── main.go                    # プラグインのライフサイクルフック実装とエクスポート
└── sdk/                       # AegisCMS 公式 TinyGo SDK
    ├── types.go               # PostContext, BlockNode, PageRenderContext 型定義
    ├── host.go                # Log, Net, KV などのホスト API ラッパー (//go:wasmimport)
    └── abi.go                 # メモリアロケーション・64bit パッキング・JSON 操作
```

### マニフェスト (`plugin.manifest.json`)
ゼロ・トラスト実行モデルに基づき、プラグインが必要とする最小限のケイパビリティ（権限）を宣言します。未宣言の操作はホストランタイムによって即座にブロックされます。

```json
{
  "name": "my-tinygo-plugin",
  "version": "1.0.0",
  "entrypoint": "plugin.wasm",
  "capabilities": {
    "network": { "allowed_domains": ["api.example.com"] },
    "storage": { "kv_namespace": "my_tinygo_state" },
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
| `allocate(size: int32)` | `size: int32` | `uintptr` | メモリアロケータ (必須) |
| `on_before_save(ptr: int32, length: int32)` | `(ptr, len)` | `int64` | 記事保存前の検証・値補正・保存拒否 |
| `transform_content_ast(ptr: int32, length: int32)` | `(ptr, len)` | `int64` | 本文ブロック AST 配列の変換 |
| `on_render_page(ptr: int32, length: int32)` | `(ptr, len)` | `int64` | 公開ページ描画コンテキストの動的更新 |

### 利用可能なホスト API

| 関数 | 説明 |
|---|---|
| `sdk.LogDebug(msg)`, `sdk.LogInfo(msg)`, `sdk.LogWarn(msg)`, `sdk.LogError(msg)` | ホスト側のロガー経由で安全にログを出力 |
| `sdk.NetIsAllowed(url)` | アクセス先 URL がマニフェストで許可されているか検証 |
| `sdk.KVSet(key, value)` | プラグイン専用ネームスペースの KVS へデータを保存 |
