# official-page-toc

記事・固定ページの横に、**見出しへのリンク（目次）**と**スクロールの進捗**を出す公式プラグインです（[`on-render-page` フック](../../docs/plugin_hooks.md)）。

- 目次はサーバー側で本文の見出し（`<h2 id="...">` など。`id` は組み込みの処理が付けます）から作るので、JavaScript が無くてもリンクとして使えます。
- 進捗のバーと「いま読んでいる見出し」の強調だけが、`extra_head` に入るインラインのスクリプトで動きます。Admin Studio のプレビューは `sandbox` でスクリプトが動かないため、公開ページでだけ動きます。
- 画面が狭いとき（既定は 1200px 未満）と印刷では出しません。`prefers-reduced-motion` のときはなめらかなスクロールにしません。

## 使い方

設定ファイルの `[plugins].enabled` に `"official-page-toc"` を足すか、Admin Studio の **Settings › Plugins** で有効にします。

```toml
[plugins]
enabled = ["official-page-toc"]
```

テーマが `extra_head` と `extra_css` を出力している必要があります（組み込みのテンプレートは出力します）。CSP から `'unsafe-inline'` の `script-src` を外していると、進捗のスクリプトは動きません（目次のリンクは動きます）。

## 設定

Admin Studio の **Settings › Plugins** の「設定」（入力欄が出ます。項目はマニフェストの `settings` で宣言していて、保存時に値を検査します）か、設定ファイルの `[plugins.settings.official-page-toc]` で変えます。省略した項目・不正な値は既定値になります。

| キー | 既定値 | 説明 |
| --- | --- | --- |
| `content_types` | `article,page` | 出すコンテンツの種類（カンマ区切り） |
| `heading_levels` | `2,3` | 目次に入れる見出しの階層（`1`〜`6`） |
| `min_headings` | `2` | 見出しがこれより少ないときは目次を出さない（進捗は出す） |
| `show_toc` | `true` | 目次を出す |
| `show_progress` | `true` | 進捗のバーを出す |
| `position` | `right` | `right` / `left` |
| `accent_color` | テーマの `--accent` | 進捗と現在の見出しの色。`#hex`・`rgb()`・`hsl()`・`var(--名前)`・色名 |
| `top_offset_px` | `96` | 上からの位置。見出しへ移動したときの上の余白（固定ヘッダーの高さ）にも使う（0〜400） |
| `width_px` | `220` | 目次の幅（120〜480） |
| `min_viewport_px` | `1200` | これより画面が狭いと出さない（320〜4000） |
| `smooth_scroll` | `true` | リンクへなめらかにスクロールする |
| `locale` | `en` | 組み込みの文言の言語（`en` / `ja`） |
| `title` | `On this page` / `目次` | 目次の見出し |
| `progress_label` | `Reading progress` / `読了位置` | 進捗バーの読み上げ用の名前 |

```toml
[plugins.settings.official-page-toc]
position = "left"
accent_color = "#e11d48"
heading_levels = "2,3,4"
locale = "ja"
```

記事ごとにやめたいときは、スキーマに真偽値のフィールド `hide_toc` を足して `true` にします。

## 見た目を変える

固定配置の `<nav class="apt">`（`.apt-track` / `.apt-fill` が進捗、`.apt-list` が目次、現在の見出しのリンクに `.apt-active`）です。設定で足りないところは、テーマの CSS で上書きできます。CSS に入るのは検査した値だけです（設定の値をそのまま CSS には入れません）。

## ビルド

```bash
task plugin:build:reading   # plugin.wasm を作り直す（要 rustup target add wasm32-unknown-unknown）
cargo test -p aegis-plugin-page-toc
```
