# AegisCMS Rust ブロックプラグインのテンプレート

本文に置ける**カスタムブロック**を足すプラグインのひな形です（`render-blocks` フック）。ノート（補足・注意の囲み）を 1 つ足す最小の例で、入力欄の宣言（`plugin.manifest.json` の `blocks`）、HTML の作成（`src/lib.rs` の `render_blocks`）、ネイティブの単体テストが入っています。

**手順は [最初のブロックプラグインを作る](../../../docs/plugin_block_tutorial.md) にまとめています**（コピー → 編集 → テスト → ビルド → 開発サーバー → 実サーバーの記事）。

```bash
cargo test                                                    # Wasm なしでテスト
cargo build --target wasm32-unknown-unknown --release         # ビルド
cp ../../../target/wasm32-unknown-unknown/release/aegis_plugin_block_template.wasm plugin.wasm
```

リポジトリのルートで `task plugin:build:block` を実行すると、ビルドと `plugin.wasm` の配置をまとめて行います。

- このフォルダーを `plugins/<name>/` にコピーして使うときは、`aegis-plugin-sdk` の `path` を `../../crates/aegis-plugin-sdk` に直し、独立した crate にするなら `Cargo.toml` の末尾に空の `[workspace]` を足します（チュートリアルの手順 1）。
- マニフェストの `name` を、コピー先のフォルダー名と同じにします。
- 利用者の入力は必ず `escape` してから HTML に入れます。
