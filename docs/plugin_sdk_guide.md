# AegisCMS WebAssembly (Wasm) プラグイン開発公式ガイド

本ドキュメントは、AegisCMS のゼロ・トラスト WebAssembly プラグインを開発するための公式ガイドです。

> [!TIP]
> 利用可能なフック・ホスト API・マニフェスト項目の詳細と現状の制限事項は [プラグインフック & ホスト API リファレンス](./plugin_hooks.md) を参照してください。

---

## 1. アーキテクチャと実行モデル

AegisCMS のプラグインシステムは、**Bytecode Alliance 製 WebAssembly ランタイム `Wasmtime`** をベースに構築されています。

- **完全なメモリ空間の分離**: ホストとゲスト（Wasm）間で物理メモリは共有されず、サンドボックス内で厳格に隔離実行されます。
- **リソース制限 (Fuel & Timeout)**:
  - **Fuel（命令数クォータ）**: 無限ループや DoS 攻撃を防ぐため、Wasm 命令の実行可能上限数を設定。
  - **非同期タイムアウト**: 指定ミリ秒（例: 250ms）を超過したプラグイン実行を強制停止。
- **ケイパビリティベースの権限管理 (Zero-Trust)**:
  - プラグインは `plugin.manifest.json` で宣言した権限（ネットワークドメイン、KVSネームスペース、フック）以外の一切の操作が遮断されます。

---

## 2. クイックスタート: Rust 公式 SDK

公式 SDK [`aegis-plugin-sdk`](file:///d:/projects/cms/crates/aegis-plugin-sdk) を利用することで、低レイヤーのポインタ操作やメモリ管理を意識することなく、型安全にプラグインを開発できます。

### 2.1 テンプレートの準備

公式テンプレート [`plugins/template-rust`](file:///d:/projects/cms/plugins/template-rust) をコピーして新しいプラグインを作成できます。

`Cargo.toml` の設定:

```toml
[package]
name = "my-aegis-plugin"
version = "0.1.0"
edition = "2021"

[lib]
crate-type = ["cdylib", "rlib"]

[dependencies]
aegis-plugin-sdk = "0.1.0"
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"
```

### 2.2 プラグインマニフェスト (`plugin.manifest.json`)

```json
{
  "name": "my-aegis-plugin",
  "version": "0.1.0",
  "entrypoint": "plugin.wasm",
  "capabilities": {
    "network": {
      "allowed_domains": ["api.slack.com", "hooks.slack.com"]
    },
    "storage": {
      "kv_namespace": "my_plugin_state"
    },
    "hooks": [
      "on-before-save",
      "transform-content-ast"
    ]
  },
  "resource_limits": {
    "max_memory_mb": 16,
    "max_fuel_units": 100000000,
    "execution_timeout_ms": 250
  }
}
```

### 2.3 プラグインコードの実装 (`src/lib.rs`)

```rust
use aegis_plugin_sdk::prelude::*;

#[derive(Default)]
pub struct MyPlugin;

#[aegis_plugin]
impl Plugin for MyPlugin {
    /// 記事保存前フック
    fn on_before_save(&mut self, mut post: PostContext) -> Result<PostContext> {
        log_info!("Processing post: '{}'", post.title);

        // タイトルの加工や検証
        post.title = post.title.trim().to_string();

        // 外部 API へのアクセス権限を検証
        if net::is_allowed("https://hooks.slack.com/services/xxx") {
            log_info!("Slack webhook is authorized");
        }

        // KVS に状態を永続化
        kv::set("last_saved_slug", &post.slug)?;

        Ok(post)
    }

    /// コンテンツ AST 変換フック
    fn transform_content_ast(&mut self, mut blocks: Vec<BlockNode>) -> Result<Vec<BlockNode>> {
        // 見出しブロックにカスタム属性を注入
        for block in blocks.iter_mut() {
            if block.name == "core/heading" {
                block.set_attribute("enhanced", serde_json::json!(true));
            }
        }

        // 記事末尾にカスタムブロックを追加
        // このフックは保存済み（変換済み）のブロックにも再適用される（再保存・エディタのプレビュー）ため、
        // 追加済みなら追加しない（冪等にする）
        let footer = "— Processed by MyPlugin —";
        if !blocks.iter().any(|b| b.text.as_deref() == Some(footer)) {
            blocks.push(BlockNode::paragraph(footer));
        }

        Ok(blocks)
    }
}
```

---

## 3. ホスト API リファレンス

SDK からインポートして使用できる標準 API です。

### 3.1 ロギング API (`aegis_plugin_sdk::log`)

ホスト側の `tracing` 構造化ロガーへ直接メッセージを送信します。プラグイン名が自動的にタグ付けされます。

```rust
log_debug!("Debug message: {}", value);
log_info!("Information: {}", message);
log_warn!("Warning encountered");
log_error!("Critical error occurred");
```

### 3.2 ネットワーク検証 API (`aegis_plugin_sdk::net`)

アクセス先 URL が `plugin.manifest.json` の `capabilities.network.allowed_domains` に含まれているかを事前に検証します。

```rust
if net::is_allowed("https://api.github.com/repos/xxx") {
    // 許可されているドメイン
} else {
    // 権限違反
}
```

### 3.3 Key-Value ストレージ API (`aegis_plugin_sdk::kv`)

プラグイン専用のネームスペース（`capabilities.storage.kv_namespace`）内にデータを永続化します。他のプラグインのデータと物理的に隔離されます。

```rust
// 文字列の保存
kv::set("config_key", "config_value")?;

// 任意のシリアライズ可能構造体 (JSON) の保存
kv::set_json("user_state", &my_state_struct)?;
```

---

## 4. テストとデプロイ

### 4.1 ネイティブ単体テスト (高速・モック内蔵)

SDK にはネイティブ実行用のモックが組み込まれているため、Wasm にビルドすることなく通常の `cargo test` で単体テストを実行できます。

```bash
cargo test
```

### 4.2 WebAssembly バイナリのビルド

```bash
# ターゲットのインストール (初回のみ)
rustup target add wasm32-unknown-unknown

# 最適化ビルド
cargo build --target wasm32-unknown-unknown --release

# バイナリの配置 (Linux / macOS)
cp target/wasm32-unknown-unknown/release/<plugin_name>.wasm plugin.wasm
# バイナリの配置 (Windows)
copy target\wasm32-unknown-unknown\release\<plugin_name>.wasm plugin.wasm
```

### 4.3 AegisCMS へのデプロイ

AegisCMS サーバーの `plugins/<plugin-name>/` ディレクトリを作成し、以下を配置します。

```
plugins/my-plugin/
  ├── plugin.manifest.json
  └── plugin.wasm
```

AegisCMS サーバー起動時、またはリロード時に自動走査・初期化されます。

---

## 5. 低水準 ABI 仕様 (AssemblyScript / 他言語開発者向け)

Rust 以外の言語（AssemblyScript, C/C++, TinyGo 等）でプラグインを開発する場合の標準 ABI 仕様です。

### エクスポート必須関数・シンボル

1. **`memory`**: WebAssembly リニアメモリのエクスポート。
2. **`allocate(len: i32) -> i32`**: ホストがゲストメモリにデータを書き込むためのバッファ確保関数。確保先アドレスのポインタ（`ptr`）を返す。
3. **`on_before_save(ptr: i32, len: i32) -> i64`** (オプション):
   - 引数: `PostContext` の JSON 文字列が配置された `(ptr, len)`。
   - 戻り値: 結果 JSON のポインタと長さをパックした 64bit 整数 `(ptr << 32) | len`。
4. **`transform_content_ast(ptr: i32, len: i32) -> i64`** (オプション):
   - 引数: `BlockNode[]` の JSON 配列が配置された `(ptr, len)`。
   - 戻り値: 変換後 `BlockNode[]` の JSON の `(ptr << 32) | len`。

### ホストインポート関数 (`host` モジュール)

```wat
(import "host" "log" (func $log (param i32 i32 i32))) ;; level, ptr, len
(import "host" "http_check_allowed" (func $check (param i32 i32) (result i32))) ;; ptr, len -> 1/0
(import "host" "kv_set" (func $kv_set (param i32 i32 i32 i32) (result i32))) ;; k_ptr, k_len, v_ptr, v_len -> 0:ok, 1:denied, 2:err
```
