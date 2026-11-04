# plugins/

AegisCMS にインストールした Wasm プラグインを置くディレクトリです（`[paths].plugins_dir`）。

- 1 プラグイン = 1 ディレクトリ（`plugins/<name>/`）。`<name>` は `plugin.manifest.json` の `name` と同じにします。
- 置いただけでは読み込まれません。Admin Studio のプラグイン画面で有効にする（再起動不要）か、設定ファイルの `[plugins].enabled` に名前を追加すると有効になります。
- インストール済みのプラグインと有効・無効は `aegis-server plugin list` で確認できます。

```text
plugins/
└── my-plugin/
    ├── plugin.manifest.json   # "name": "my-plugin"
    └── plugin.wasm
```

プラグインの作り方は [docs/plugin_sdk_guide.md](../docs/plugin_sdk_guide.md)、テンプレートとサンプルは [`sdk/`](../sdk) にあります。本文に置けるブロックを作るなら、[最初のブロックプラグインを作る](../docs/plugin_block_tutorial.md)（テンプレート [`sdk/templates/rust-block`](../sdk/templates/rust-block)）から始めると早いです。

> [!NOTE]
> このリポジトリの中（`plugins/<name>/`）に Rust のプラグインを新しく作ると、ルートの `Cargo.toml` のワークスペースに含まれていないため、`cargo build` が `current package believes it's in a workspace when it's not` で失敗します。公式プラグインとして取り込むなら、ルートの `[workspace] members` に足します。個人のプラグインなら、プラグインの `Cargo.toml` の末尾に空の `[workspace]` を書いて独立した crate にします（[SDK ガイド 2.1](../docs/plugin_sdk_guide.md#21-テンプレートの準備)）。

## 公式プラグイン

| プラグイン | フック | 内容 |
| --- | --- | --- |
| [`official-code-enhancer`](official-code-enhancer/) | `transform-content-ast`, `on-render-page` | コードのシンタックスハイライト・Mermaid 図・コピー ボタン |
| [`official-shop-rules`](official-shop-rules/) | `calculate-order`, `on-order-status-changed` | EC の送料（送料無料の条件）・クーポン（利用回数の上限）・税（[EC](../docs/commerce.md#71-公式プラグイン)） |
| [`official-order-notifier`](official-order-notifier/) | `on-order-status-changed` | 注文の作成・状態の変化を Slack / Webhook に通知（`http::fetch`） |
| [`official-blocks`](official-blocks/) | `render-blocks` | 本文に置けるボタン・リンクカード・動画（YouTube・Vimeo）のブロック（[本文のブロック](../docs/blocks.md#4-プラグインのブロック)）。カスタムブロックのお手本 |
| [`official-link-embed`](official-link-embed/) | `transform-content-ast`, `render-blocks` | URL だけの段落を、そのページの OGP / oEmbed から作ったリンクカードにする（保存時にサンドボックス内で取得してブロックに保存。表示時は通信しない）。取得先は `extra_allowed_domains` で足せる（[本文のブロック](../docs/blocks.md#5-ogp--oembed)） |
| [`official-page-toc`](official-page-toc/) | `on-render-page` | 記事・固定ページの横に、見出しへのリンク（目次）とスクロールの進捗を出す。位置・色・階層などは `[plugins.settings.official-page-toc]` で変えられる |
| [`official-back-to-top`](official-back-to-top/) | `on-render-page` | 本文の末尾（または画面に固定）に「トップへ戻る」ボタンを出す。形・色・位置などは `[plugins.settings.official-back-to-top]` で変えられる |
| [`official-share-buttons`](official-share-buttons/) | `on-render-page` | 記事・固定ページの末尾（または上部/両方）に SNS 共有ボタン（X・LINE・URLコピーなど）を出す。表示先やカスタム共有先の設定が可能 |
| [`official-reading-time`](official-reading-time/) | `on-render-page` | 記事・固定ページの本文から読了目安時間および文字数/単語数を算出し、先頭や末尾にバッジ表示する。`[plugins.settings.official-reading-time]` で変えられる |
| [`official-post-navigation`](official-post-navigation/) | `on-render-page` | 記事詳細ページの末尾に前後の記事ナビゲーション（Prev/Next）を出す。同一カテゴリ優先または全体日付順の算出設定、デザインなどは `[plugins.settings.official-post-navigation]` で変えられる |
| [`official-code-lines`](official-code-lines/) | `on-render-page` | コードブロックに行番号とファイル名の見出しを出す（ファイル名はコードブロックの `filename`）。最小の行数・色などは `[plugins.settings.official-code-lines]` で変えられる |
| [`official-lightbox`](official-lightbox/) | `on-render-page` | 本文の画像をクリックで拡大表示する（前後の画像・キーボード操作・説明文）。小さい画像やリンクの中の画像は対象外。`[plugins.settings.official-lightbox]` で変えられる |
| [`official-external-links`](official-external-links/) | `on-render-page` | 本文の外部リンクを新しいタブで開き、`rel="noopener nofollow"` と外部リンクのアイコンを付ける（ブラウザーのスクリプトで動く）。`[plugins.settings.official-external-links]` で変えられる |
| [`official-ads`](official-ads/) | `on-render-page`, `render-blocks` | Google AdSense および独自広告・バナー配信。`<head>` へのスクリプト注入、カスタムブロック（`official-ads/display`）、動的 CSP 連携、Cookie 同意バナー（`[consent]`）対応 |

`plugin.wasm` はビルド済みのものを同梱しています。ソースを変えたら `task plugin:build:shop`（`official-blocks` は `task plugin:build:blocks`、`official-link-embed` は `task plugin:build:link-embed`、`official-page-toc` / `official-back-to-top` / `official-share-buttons` / `official-reading-time` / `official-post-navigation` は `task plugin:build:reading`、`official-code-lines` / `official-lightbox` / `official-external-links` は `task plugin:build:content`、`official-ads` は `task plugin:build:ads`）で作り直してください。
