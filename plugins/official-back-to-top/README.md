# official-back-to-top

記事・固定ページに**「トップへ戻る」ボタン**を足す公式プラグインです（[`on-render-page` フック](../../docs/plugin_hooks.md)）。

| `style` | 動き |
| --- | --- |
| `inline`（既定） | 本文の末尾にリンクを置く。JavaScript なしで動く（`href="#top"`）。Admin Studio のプレビューにも出る |
| `fixed` | 画面に固定表示し、`show_after_px` スクロールしたら現れる。スクリプトが動かない環境では常に表示する |

本文の末尾に置くので、テーマのコメント欄やフッターはその下にあります（ページの最下部に置きたいときは `fixed` を使います）。

## 使い方

設定ファイルの `[plugins].enabled` に `"official-back-to-top"` を足すか、Admin Studio の **Settings › Plugins** で有効にします。

```toml
[plugins]
enabled = ["official-back-to-top"]
```

テーマが `extra_head` と `extra_css` を出力している必要があります（組み込みのテンプレートは出力します）。

## 設定

`[plugins.settings.official-back-to-top]`（Admin Studio のプラグインの設定でも、同じキーを足せます）。省略・不正な値は既定値になります。

| キー | 既定値 | 説明 |
| --- | --- | --- |
| `content_types` | `article,page` | 出すコンテンツの種類（カンマ区切り） |
| `style` | `inline` | `inline` / `fixed` |
| `position` | `right` | `right` / `left` / `center` |
| `shape` | `circle` | `circle`（矢印だけの丸）/ `square`（矢印だけの角丸）/ `pill`（矢印と文字） |
| `label` | `Back to top` / `トップへ戻る` | 文字（`pill`）と読み上げ用の名前 |
| `locale` | `en` | `label` の既定の言語（`en` / `ja`） |
| `bg_color` | テーマの `--accent` | 背景色。`#hex`・`rgb()`・`hsl()`・`var(--名前)`・色名 |
| `fg_color` | テーマの `--accent-contrast`（無ければ白） | 矢印と文字の色 |
| `size_px` | `44` | ボタンの大きさ（32〜72） |
| `offset_px` | `24` | `fixed` のときの画面の端からの距離（8〜96） |
| `show_after_px` | `400` | `fixed` のとき、これだけスクロールしたら現れる（0〜5000） |
| `smooth_scroll` | `true` | なめらかにスクロールする（`prefers-reduced-motion` のときはしない） |

```toml
[plugins.settings.official-back-to-top]
style = "fixed"
shape = "pill"
bg_color = "#111827"
fg_color = "#ffffff"
locale = "ja"
```

記事ごとにやめたいときは、スキーマに真偽値のフィールド `hide_back_to_top` を足して `true` にします。

## 見た目を変える

ボタンは `<a class="obt obt-inline|obt-fixed obt-{position} obt-{shape}">` です（`inline` は `<div class="obt-row">` の中）。リンクの色・下線は、テーマの本文のリンクの指定に負けないよう `a.obt.obt` で指定しています。設定で足りないところは、それより詳細度の高いセレクタで上書きしてください。CSS に入るのは検査した値だけです。

## ビルド

```bash
task plugin:build:reading   # plugin.wasm を作り直す（要 rustup target add wasm32-unknown-unknown）
cargo test -p aegis-plugin-back-to-top
```
