# AegisCMS Rust WebAssembly Plugin Template

AegisCMS 公式 Rust Wasm プラグイン開発テンプレートです。  
高水準マクロ `#[aegis_plugin]` を利用することで、低レイヤーの Wasm メモリ管理や JSON パッキングを意識することなく、型安全なライフサイクルフックとホスト API を実装できます。

---

## 🚀 クイックスタート

### 1. 前提条件

Rust ツールチェーンおよび `wasm32-unknown-unknown` ターゲットが必要です。

```bash
rustup target add wasm32-unknown-unknown
```

### 2. ローカルテスト (ネイティブ実行)

SDK にはネイティブ環境向けのモックバインディングが内蔵されているため、Wasm にコンパイルすることなく通常の `cargo test` でロジックを単体テストできます。

```bash
cargo test
```

### 3. WebAssembly バイナリのビルド

リリースプロファイルで `.wasm` を生成します。

```bash
cargo build --target wasm32-unknown-unknown --release
```

生成されたバイナリを本ディレクトリ直下に `plugin.wasm` として配置します（リポジトリ直下で `task plugin:build` を実行すると、ビルドと配置をまとめて行います）。

```bash
# Windows (PowerShell)
Copy-Item target/wasm32-unknown-unknown/release/aegis_plugin_template.wasm plugin.wasm

# Linux / macOS
cp target/wasm32-unknown-unknown/release/aegis_plugin_template.wasm plugin.wasm
```

### 4. AegisCMS への配置

AegisCMS サーバーの `plugins/<plugin-name>/` ディレクトリに、以下のファイルを配置するだけでサーバー起動時に自動検出・ロードされます。

- `plugin.manifest.json` (権限・リソース制限・フック定義)
- `plugin.wasm` (コンパイル済み Wasm バイナリ)

---

## 🧩 プラグイン構造

### マニフェスト (`plugin.manifest.json`)
ゼロ・トラスト実行モデルに基づき、プラグインが必要とする最小限のケイパビリティ（権限）を宣言します。未宣言の操作はホストランタイムによって即座にブロックされます。

```json
{
  "name": "my-plugin",
  "version": "1.0.0",
  "entrypoint": "plugin.wasm",
  "capabilities": {
    "network": { "allowed_domains": ["api.example.com"] },
    "storage": { "kv_namespace": "my_plugin_state" },
    "hooks": ["on-before-save", "transform-content-ast"]
  },
  "resource_limits": {
    "max_memory_mb": 16,
    "max_fuel_units": 100000000,
    "execution_timeout_ms": 250
  }
}
```

### ライフサイクルフック (`Plugin` トレイト)

```rust
use aegis_plugin_sdk::prelude::*;

#[derive(Default)]
pub struct MyPlugin;

#[aegis_plugin]
impl Plugin for MyPlugin {
    /// 記事保存前フック
    fn on_before_save(&mut self, mut post: PostContext) -> Result<PostContext> {
        post.title = format!("[Review] {}", post.title);
        Ok(post)
    }

    /// AST 変換フック（保存済みのブロックにも再適用されるため、冪等に書く）
    fn transform_content_ast(&mut self, mut blocks: Vec<BlockNode>) -> Result<Vec<BlockNode>> {
        let text = "Appended by plugin";
        if !blocks.iter().any(|b| b.text.as_deref() == Some(text)) {
            blocks.push(BlockNode::paragraph(text));
        }
        Ok(blocks)
    }
}
```

### 利用可能なホスト API

| モジュール | 関数 | 説明 |
|---|---|---|
| `log` | `log_debug!`, `log_info!`, `log_warn!`, `log_error!` | ホスト側のロガー経由で安全にログを出力 |
| `net` | `net::is_allowed(url)` | アクセス先 URL がマニフェストで許可されているか検証 |
| `kv` | `kv::set(key, val)`, `kv::set_json(key, val)` | プラグイン専用ネームスペースの KVS へデータを保存 |
