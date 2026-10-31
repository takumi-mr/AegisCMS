# plugins/

AegisCMS にインストールした Wasm プラグインを置くディレクトリです（`[paths].plugins_dir`）。

- 1 プラグイン = 1 ディレクトリ（`plugins/<name>/`）。`<name>` は `plugin.manifest.json` の `name` と同じにします。
- 置いただけでは読み込まれません。Admin Studio のプラグイン画面で有効にする（再起動不要）か、設定ファイルの `[plugins].enabled` に名前を追加すると有効になります。
- インストール済みのプラグインと有効・無効は `aegis plugin list` で確認できます。

```text
plugins/
└── my-plugin/
    ├── plugin.manifest.json   # "name": "my-plugin"
    └── plugin.wasm
```

プラグインの作り方は [docs/plugin_sdk_guide.md](../docs/plugin_sdk_guide.md)、テンプレートとサンプルは [`sdk/`](../sdk) にあります。

## 公式プラグイン

| プラグイン | フック | 内容 |
| --- | --- | --- |
| [`official-code-enhancer`](official-code-enhancer/) | `transform-content-ast`, `on-render-page` | コードのシンタックスハイライト・Mermaid 図・コピー ボタン |
| [`official-shop-rules`](official-shop-rules/) | `calculate-order`, `on-order-status-changed` | EC の送料（送料無料の条件）・クーポン（利用回数の上限）・税（[EC](../docs/commerce.md#71-公式プラグイン)） |
| [`official-order-notifier`](official-order-notifier/) | `on-order-status-changed` | 注文の作成・状態の変化を Slack / Webhook に通知（`http::fetch`） |
| [`official-blocks`](official-blocks/) | `render-blocks` | 本文に置けるボタン・リンクカード・動画（YouTube・Vimeo）のブロック（[本文のブロック](../docs/blocks.md#4-プラグインのブロック)）。カスタムブロックのお手本 |
| [`official-page-toc`](official-page-toc/) | `on-render-page` | 記事・固定ページの横に、見出しへのリンク（目次）とスクロールの進捗を出す。位置・色・階層などは `[plugins.settings.official-page-toc]` で変えられる |
| [`official-back-to-top`](official-back-to-top/) | `on-render-page` | 本文の末尾（または画面に固定）に「トップへ戻る」ボタンを出す。形・色・位置などは `[plugins.settings.official-back-to-top]` で変えられる |
| [`official-share-buttons`](official-share-buttons/) | `on-render-page` | 記事・固定ページの末尾（または上部/両方）に SNS 共有ボタン（X・LINE・URLコピーなど）を出す。表示先やカスタム共有先の設定が可能 |

`plugin.wasm` はビルド済みのものを同梱しています。ソースを変えたら `task plugin:build:shop`（`official-blocks` は `task plugin:build:blocks`、`official-page-toc` / `official-back-to-top` / `official-share-buttons` は `task plugin:build:reading`）で作り直してください。
