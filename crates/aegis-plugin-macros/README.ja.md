# aegis-plugin-macros

[English](README.md) | **日本語**

AegisCMS WebAssembly (Wasm) プラグイン SDK 向け手続き型マクロ。

## 概要

`aegis-plugin-macros` は、Rust で AegisCMS プラグインを開発するための属性マクロを提供します。低レベルな WebAssembly リニアメモリの読み書き、メモリ確保・解放フック（`allocate`, `deallocate`）、および AegisCMS ホスト ABI 仕様に準拠した C-ABI エクスポート関数のボイラープレートを自動生成します。

## 使い方

本クレートは通常、[`aegis-plugin-sdk`](../aegis-plugin-sdk) を通じて利用します。

プラグインの `Cargo.toml`:

```toml
[dependencies]
aegis-plugin-sdk = "0.2.0"
```

プラグインの `src/lib.rs`:

```rust
use aegis_plugin_sdk::prelude::*;

#[derive(Default)]
pub struct MyPlugin;

#[aegis_plugin]
impl Plugin for MyPlugin {
    fn on_before_save(&mut self, post: PostContext) -> Result<PostContext> {
        Ok(post)
    }

    fn on_render_page(&mut self, ctx: PageRenderContext) -> Result<PageRenderContext> {
        Ok(ctx)
    }
}
```

## 自動生成される ABI エクスポート

`#[aegis_plugin]` を付与することで、以下の関数が自動的にエクスポートされます：

- `allocate(len: i32) -> i32`: ホストからのメモリ割り当てエントリポイント。
- `deallocate(ptr: i32, len: i32)`: ホストからのメモリ解放エントリポイント。
- 各フックラッパー関数（`on_before_save`, `transform_content_ast`, `on_render_page`, `render_blocks`, `calculate_order`, `on_order_status_changed`, `check_comment`, `send_notification`, `on_user_authenticated`, `on_content_published`, `on_content_deleted` 等）: ホストメモリとの入出力 JSON デシリアライズ・シリアライズおよびエラー伝播を透過的に処理。
