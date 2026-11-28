# plugins/

[English](README.md) | **日本語**

手元の AegisCMS に同梱する Wasm プラグインを置くディレクトリです（同梱の設定では `[paths].bundled_plugins_dir`。Admin Studio から入れたものは `[paths].plugins_dir` の `data/plugins` に置かれます）。AegisCMS はプラグインを同梱しないので、空です。自分で作り、手元のコピーやイメージに同梱したいプラグインをここに置きます。

- 1 プラグイン = 1 ディレクトリ（`plugins/<name>/`）。`<name>` は `plugin.manifest.json` の `name` と同じにします。
- 置いただけでは読み込まれません。Admin Studio のプラグイン画面で有効にする（再起動不要）か、設定ファイルの `[plugins].enabled` に名前を追加すると有効になります。
- インストール済みのプラグインと有効・無効は `aegis-server plugin list` で確認できます。

```text
plugins/
└── my-plugin/
    ├── plugin.manifest.json   # "name": "my-plugin"
    └── plugin.wasm
```

プラグインの作り方は [docs/plugin_sdk_guide.ja.md](../docs/plugin_sdk_guide.ja.md)、テンプレートとサンプルは [`sdk/`](../sdk) にあります。本文に置けるブロックを作るなら、[最初のブロックプラグインを作る](../docs/plugin_block_tutorial.ja.md)（テンプレート [`sdk/templates/rust-block`](../sdk/templates/rust-block)）から始めると早いです。

> [!NOTE]
> このリポジトリの中（`plugins/<name>/`）に Rust のプラグインを新しく作ると、ルートの `Cargo.toml` のワークスペースに含まれていないため、`cargo build` が `current package believes it's in a workspace when it's not` で失敗します。公式プラグインとして取り込むなら、ルートの `[workspace] members` に足します。個人のプラグインなら、プラグインの `Cargo.toml` の末尾に空の `[workspace]` を書いて独立した crate にします（[SDK ガイド 2.1](../docs/plugin_sdk_guide.ja.md#21-テンプレートの準備とひな形生成-task-pluginnew)）。

## 公式プラグイン

v0.13.0 から、公式プラグインはすべて同梱せず、[パッケージストア](../docs/store.ja.md)から入れます（Admin Studio › Plugins › ストア）。ソースは [AegisCMS-Plugin](https://github.com/takumi-mr/AegisCMS-Plugin) にあります。コードのハイライト・Mermaid・PlantUML・コンテンツのブロックは、おすすめとして並びます。

| プラグイン                                                                                                            | フック                                       | 内容                                                                                                                                                                                                                                                       |
| --------------------------------------------------------------------------------------------------------------------- | -------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| [`official-ads`](https://github.com/takumi-mr/AegisCMS-Plugin/tree/main/plugins/official-ads)                         | `on-render-page`, `render-blocks`            | Google AdSense および独自広告・バナー配信。`<head>` へのスクリプト注入、カスタムブロック（`official-ads/display`）、動的 CSP 連携、Cookie 同意バナー（`[consent]`）対応                                                                                    |
| [`official-back-to-top`](https://github.com/takumi-mr/AegisCMS-Plugin/tree/main/plugins/official-back-to-top)         | `on-render-page`                             | 本文の末尾（または画面に固定）に「トップへ戻る」ボタンを出す。形・色・位置などは `[plugins.settings.official-back-to-top]` で変えられる                                                                                                                    |
| [`official-blocks`](https://github.com/takumi-mr/AegisCMS-Plugin/tree/main/plugins/official-blocks)                   | `render-blocks`                              | 本文に置けるボタン・リンクカード・ギャラリー・FAQ・タブなどのブロック（[本文のブロック](../docs/blocks.ja.md#4-プラグインのブロック)）。外部のサイトに依存しない。カスタムブロックのお手本                                                                 |
| [`official-charts`](https://github.com/takumi-mr/AegisCMS-Plugin/tree/main/plugins/official-charts)                   | `render-blocks`                              | 本文に置ける SVG グラフブロック（縦棒・積み上げ棒・横棒・折れ線・ドーナツ、複数系列対応）。JavaScript を使わずサーバー側で描画し、Strict CSP に対応                                                                                                        |
| [`official-clarity`](https://github.com/takumi-mr/AegisCMS-Plugin/tree/main/plugins/official-clarity)                 | `on-render-page`                             | Microsoft Clarity（ヒートマップ・画面録画）を公開ページに足す。プロジェクト ID を入れるだけで、Cookie の同意を待ち、CSP のホストを自動で足す。会員・買い物・掲示板のページと、会員限定の記事は録画しない。`[plugins.settings.official-clarity]` で設定する |
| [`official-code-enhancer`](https://github.com/takumi-mr/AegisCMS-Plugin/tree/main/plugins/official-code-enhancer)     | `transform-content-ast`, `on-render-page`    | コードのシンタックスハイライト（Prism.js）・外部テーマ・コピーボタン・インラインコードの整形。テーマは Admin Studio の Settings › Plugins で選ぶ。コードの無いページには何も足さない                                                                       |
| [`official-code-lines`](https://github.com/takumi-mr/AegisCMS-Plugin/tree/main/plugins/official-code-lines)           | `on-render-page`                             | コードブロックに行番号とファイル名の見出しを出す（ファイル名はコードブロックの `filename`）。最小の行数・色などは `[plugins.settings.official-code-lines]` で変えられる                                                                                    |
| [`official-embeds`](https://github.com/takumi-mr/AegisCMS-Plugin/tree/main/plugins/official-embeds)                   | `render-blocks`                              | 外部のサービスを `<iframe>` で埋め込むブロック（YouTube・Vimeo・OpenStreetMap・CodePen・Google マップ・Google カレンダー・Google フォーム）。埋め込み先のホストは、有効な間だけ CSP の `frame-src` に入る                                                  |
| [`official-external-links`](https://github.com/takumi-mr/AegisCMS-Plugin/tree/main/plugins/official-external-links)   | `on-render-page`                             | 本文の外部リンクを新しいタブで開き、`rel="noopener nofollow"` と外部リンクのアイコンを付ける（ブラウザーのスクリプトで動く）。`[plugins.settings.official-external-links]` で変えられる                                                                    |
| [`official-lightbox`](https://github.com/takumi-mr/AegisCMS-Plugin/tree/main/plugins/official-lightbox)               | `on-render-page`                             | 本文の画像をクリックで拡大表示する（前後の画像・キーボード操作・説明文）。小さい画像やリンクの中の画像は対象外。`[plugins.settings.official-lightbox]` で変えられる                                                                                        |
| [`official-link-embed`](https://github.com/takumi-mr/AegisCMS-Plugin/tree/main/plugins/official-link-embed)           | `transform-content-ast`, `render-blocks`     | URL だけの段落を、そのページの OGP / oEmbed から作ったリンクカードにする（保存時にサンドボックス内で取得してブロックに保存。表示時は通信しない）。取得先は `extra_allowed_domains` で足せる（[本文のブロック](../docs/blocks.ja.md#5-ogp--oembed)）        |
| [`official-mermaid`](https://github.com/takumi-mr/AegisCMS-Plugin/tree/main/plugins/official-mermaid)                 | `on-render-page`                             | ` ```mermaid ` のコードブロックを Mermaid の図にする。図のあるページだけ Mermaid を読み込む。テーマ・セキュリティレベル・スクリプトの URL は Admin Studio の Settings › Plugins で変える                                                                   |
| [`official-order-notifier`](https://github.com/takumi-mr/AegisCMS-Plugin/tree/main/plugins/official-order-notifier)   | `on-order-status-changed`                    | 注文の作成・状態の変化を Slack / Webhook に通知（`http::fetch`）                                                                                                                                                                                           |
| [`official-page-toc`](https://github.com/takumi-mr/AegisCMS-Plugin/tree/main/plugins/official-page-toc)               | `on-render-page`                             | 記事・固定ページの横に、見出しへのリンク（目次）とスクロールの進捗を出す。位置・色・階層などは `[plugins.settings.official-page-toc]` で変えられる                                                                                                         |
| [`official-plantuml`](https://github.com/takumi-mr/AegisCMS-Plugin/tree/main/plugins/official-plantuml)               | `on-render-page`                             | ` ```plantuml ` のコードブロックを、PlantUML サーバーが描く図の画像にする（ページにスクリプトを入れない）。サーバーの URL・画像の形式は Admin Studio の Settings › Plugins で変える                                                                        |
| [`official-post-navigation`](https://github.com/takumi-mr/AegisCMS-Plugin/tree/main/plugins/official-post-navigation) | `on-render-page`                             | 記事詳細ページの末尾に前後の記事ナビゲーション（Prev/Next）を出す。同一カテゴリ優先または全体日付順の算出設定、デザインなどは `[plugins.settings.official-post-navigation]` で変えられる                                                                   |
| [`official-reading-time`](https://github.com/takumi-mr/AegisCMS-Plugin/tree/main/plugins/official-reading-time)       | `on-render-page`                             | 記事・固定ページの本文から読了目安時間および文字数/単語数を算出し、先頭や末尾にバッジ表示する。`[plugins.settings.official-reading-time]` で変えられる                                                                                                     |
| [`official-share-buttons`](https://github.com/takumi-mr/AegisCMS-Plugin/tree/main/plugins/official-share-buttons)     | `on-render-page`                             | 記事・固定ページの末尾（または上部/両方）に SNS 共有ボタン（X・LINE・URLコピーなど）を出す。表示先やカスタム共有先の設定が可能                                                                                                                             |
| [`official-shop-rules`](https://github.com/takumi-mr/AegisCMS-Plugin/tree/main/plugins/official-shop-rules)           | `calculate-order`, `on-order-status-changed` | EC の送料（送料無料の条件）・クーポン（利用回数の上限）・税（[EC](../docs/commerce.ja.md#71-公式プラグイン)）                                                                                                                                              |
