# official-blocks

本文に置けるボタンとリンクカードのブロックを足す公式プラグインです。プラグインでカスタムブロックを作るときのお手本でもあります（[`render-blocks` フック](../../docs/plugin_hooks.md#110-render-blocks)）。

| ブロック | 入力欄 | 表示 |
| --- | --- | --- |
| `official-blocks/button` | 文字（必須）・リンク先（必須）・見た目（塗りつぶし / 枠線）・位置（左 / 中央） | ボタンの形のリンク |
| `official-blocks/link-card` | リンク先（必須）・タイトル（必須）・説明・画像 | タイトル・説明・画像と、外部のリンクならホスト名を出すカード |

## 使い方

設定ファイルの `[plugins].enabled` に `"official-blocks"` を足すか、Admin Studio の **Settings › Plugins** で有効にします。エディタのブロックの追加ボタンに「ボタン」「リンクカード」が出ます。

```toml
[plugins]
enabled = ["official-blocks"]
```

- 必須の入力欄が空のブロックや、`http(s)`・`mailto`・相対 URL 以外のリンク先のブロックは表示しません。
- 配置・余白は詳細度 0 の CSS（`:where()`）、リンクの色と下線だけはテーマの本文のリンクの指定に負けないようクラス 2 つの詳細度（`.ob-button-row .ob-button` など）です。色はテーマの `--accent` と、塗りつぶしのボタンの文字の `--accent-contrast`（無ければ白。明るい `--accent` のテーマでは暗い色を指定する）を使います。テーマで変えるときは、それより詳細度の高いセレクタで指定します。
- 外部への通信や KVS は使いません。

## ビルド

```bash
task plugin:build:blocks   # plugin.wasm を作り直す（要 rustup target add wasm32-unknown-unknown）
cargo test -p aegis-plugin-blocks
```
