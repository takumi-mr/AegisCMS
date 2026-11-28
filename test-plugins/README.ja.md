[English](README.md) | **日本語**

# テスト用のプラグイン

AegisCMS 本体のテストのための、小さな Wasm プラグインです。**同梱しません**（Docker のイメージにもリリースのアーカイブにも入りません）。**ストアにも載せません**。本体のテストは、[パッケージストア](../docs/store.ja.md)から配る公式プラグインの代わりに、これを使います。

| プラグイン    | すること                                                                                                                                                          | 使うテスト                                                                                                         |
| ------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------ |
| `test-page`   | 記事にインラインスクリプトで印を付ける（`on-render-page`。CSP のホストつき）。`test-page/box` を自分のスタイルつきで描く（`render-blocks`）。各型の設定を宣言する | プラグインの切り替えと設定の検証（`handlers/plugin.rs`）、E2E（`plugins.spec.ts`・`csp.spec.ts`）、診断（pentest） |
| `test-shop`   | 送料と 1 つのクーポン（`calculate-order`）。クーポンの利用回数を KV で数える（`on-order-status-changed`）。設定は宣言しない                                       | チェックアウト（`handlers/shop_tests.rs`）、E2E（`plugins.spec.ts`）                                               |
| `test-notify` | 運営者向けの通知（`send-notification`）と、指定した注文の状態（`on-order-status-changed`）を `webhook_url` に送る                                                 | 通知のチャネル（`notify_plugins.rs`）、チェックアウト（`handlers/shop_tests.rs`）                                  |

ビルドした `plugin.wasm` は、公式プラグインと同じくコミットしています。ソースを変えたら作り直します。

```bash
task plugin:build:test   # 要 rustup target add wasm32-unknown-unknown
```

E2E のサーバー（`admin/e2e/aegis.e2e.toml`）と診断（`docker/pentest/pentest.toml`）は、このディレクトリを `bundled_plugins_dir` として読みます。
