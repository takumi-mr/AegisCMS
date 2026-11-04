# aegis-plugin-sdk

AegisCMS 公式 WebAssembly (Wasm) プラグイン Rust SDK。

AegisCMS は Bytecode Alliance の Wasmtime ランタイムを基盤とするゼロトラストなプラグインシステムを採用しています。`aegis-plugin-sdk` は、型安全で直感的な Rust 抽象化を提供し、ボイラープレートや unsafe コードを書くことなく安全かつ高速にプラグインを開発できるようにします。

## 主な機能

- **マクロによるフック自動生成**: `#[aegis_plugin]`（または `register_plugin!`）でライフサイクルフックを実装し、Wasm ABI エクスポートを自動生成。
- **多彩なライフサイクルフック**:
  - `on_before_save`: 記事・コンテンツ保存前のバリデーションや自動加工。
  - `transform_content_ast`: ブロック AST（構文木）の走査・書き換え。
  - `on_render_page`: 描画後の HTML や `<head>` タグへのカスタム CSS/JS の挿入。
  - `render_blocks`: プラグイン独自のカスタムブロックを HTML にレンダリング。
  - `on_user_authenticated`: ユーザーログイン時のロール付与や外部 IdP プロファイル同期。
  - `calculate_order`: EC 注文フローにおける動的価格計算、クーポン割引、税金・送料の算出。
  - `on_order_status_changed`: 注文ステータス遷移（決済完了、発送、返金など）に応じた処理。
  - `check_comment`: コメントの自動判定（スパムスコア算出、NG ワード判定、承認状態制御）。
  - `send_notification`: メールやチャットなどのマルチチャネル通知処理。
  - `on_content_published`: コンテンツ公開時の外部通知・キャッシュウォーミング。
  - `on_content_deleted`: コンテンツ削除時の外部連携リソース削除。
- **ホスト API**:
  - **構造化ロギング**: `log_debug!`, `log_info!`, `log_warn!`, `log_error!` により、ホスト本体の `tracing` ログへ安全に出力。
  - **ケイパビリティ制御付きネットワーク**: `net::is_allowed` および `http::fetch` による、マニフェスト許可ドメインへの安全な外部 HTTP 通信。
  - **隔離 KVS**: プラグインごとに名前空間が分離されたキーバリューストア（`kv::get`, `kv::set`, `kv::delete`, `kv::get_json`, `kv::set_json`）。
  - **設定値の取得**: Admin Studio で設定されたプラグイン設定値の読み取り（`config::get`）。
- **ネイティブ単体テスト**: SDK に内蔵されたモック機能により、Wasm にコンパイルすることなく通常の `cargo test` でプラグインの動作を検証可能。

## クイックスタート

プラグインの `Cargo.toml`:

```toml
[package]
name = "my-aegis-plugin"
version = "0.2.0"
edition = "2021"

[lib]
crate-type = ["cdylib", "rlib"]

[dependencies]
aegis-plugin-sdk = "0.2.0"
serde = { version = "1.0", features = ["derive"] }
```

プラグインの実装 `src/lib.rs`:

```rust
use aegis_plugin_sdk::prelude::*;

#[derive(Default)]
pub struct MyPlugin;

#[aegis_plugin]
impl Plugin for MyPlugin {
    fn on_before_save(&mut self, mut post: PostContext) -> Result<PostContext> {
        log_info!("Processing post: '{}'", post.title);

        if post.title.trim().is_empty() {
            return Err(PluginError::Rejected("Title is required".into()));
        }

        kv::set("last_saved_slug", &post.slug)?;
        Ok(post)
    }

    fn on_render_page(&mut self, mut ctx: PageRenderContext) -> Result<PageRenderContext> {
        ctx.head_html.push_str("<style>.aegis-highlight { color: #0066cc; }</style>");
        Ok(ctx)
    }
}
```

Wasm バイナリのビルド:

```bash
cargo build --target wasm32-unknown-unknown --release
```

ビルドされた `target/wasm32-unknown-unknown/release/my_aegis_plugin.wasm` を `plugin.manifest.json` と共に AegisCMS の `plugins/my-plugin/` ディレクトリに配置します。

## ドキュメント

- [プラグイン開発ガイド](../../docs/plugin_sdk_guide.md)
- [プラグイン ABI 仕様書](../../docs/plugin_abi_specification.md)
