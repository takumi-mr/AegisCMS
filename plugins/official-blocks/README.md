# official-blocks

本文に置けるボタン・リンクカード・動画のブロックを足す公式プラグインです。プラグインでカスタムブロックを作るときのお手本でもあります（[`render-blocks` フック](../../docs/plugin_hooks.md#110-render-blocks)、動画は[外部のページの埋め込み](../../docs/plugin_hooks.md#32-外部のページの埋め込み-capabilitiesembed)）。

| ブロック | 入力欄 | 表示 |
| --- | --- | --- |
| `official-blocks/button` | 文字（必須）・リンク先（必須）・見た目（塗りつぶし / 枠線）・位置（左 / 中央） | ボタンの形のリンク |
| `official-blocks/link-card` | リンク先（必須）・タイトル（必須）・説明・画像 | タイトル・説明・画像と、外部のリンクならホスト名を出すカード |
| `official-blocks/video` | 動画の URL（必須。YouTube・Vimeo）・タイトル（読み上げ用）・形（16:9 / 4:3 / 9:16） | 動画のプレーヤー |

## 動画

- YouTube は `youtube.com/watch?v=`・`youtu.be/`・`youtube.com/shorts/`・`youtube.com/embed/`・`youtube.com/live/` の URL を読み、再生開始位置（`t=90`・`t=1m30s`・`start=90`）も使います。Vimeo は `vimeo.com/<ID>`（限定公開の `vimeo.com/<ID>/<ハッシュ>`）と `player.vimeo.com/video/<ID>`。
- 入力された URL から動画の ID だけを取り出し、プレーヤーの URL はこのプラグインが組み立てます。YouTube はプライバシー強化モード（`www.youtube-nocookie.com`）、Vimeo は `dnt=1`（視聴の記録をしない）です。
- 埋め込めるのはマニフェストの `capabilities.embed.frame_domains`（`www.youtube-nocookie.com`・`player.vimeo.com`）だけで、このホストは有効にしている間 CSP の `frame-src` に自動で足されます。
- 読めない URL のブロックは表示しません。
- 動画を見ると YouTube・Vimeo に閲覧者の IP アドレスなどが送られます。プライバシーポリシーに外部の動画サービスを使うことを書いてください。

## 使い方

設定ファイルの `[plugins].enabled` に `"official-blocks"` を足すか、Admin Studio の **Settings › Plugins** で有効にします。エディタのブロックの追加ボタンに「ボタン」「リンクカード」が出ます。

```toml
[plugins]
enabled = ["official-blocks"]
```

- 必須の入力欄が空のブロックや、`http(s)`・`mailto`・相対 URL 以外のリンク先のブロックは表示しません。
- 配置・余白は詳細度 0 の CSS（`:where()`）、リンクの色と下線だけはテーマの本文のリンクの指定に負けないようクラス 2 つの詳細度（`.ob-button-row .ob-button` など）です。色はテーマの `--accent` と、塗りつぶしのボタンの文字の `--accent-contrast`（無ければ白。明るい `--accent` のテーマでは暗い色を指定する）を使います。テーマで変えるときは、それより詳細度の高いセレクタで指定します。
- サーバーから外部への通信や KVS は使いません（動画は閲覧者のブラウザーがプレーヤーを読み込みます）。

## ビルド

```bash
task plugin:build:blocks   # plugin.wasm を作り直す（要 rustup target add wasm32-unknown-unknown）
cargo test -p aegis-plugin-blocks
```
