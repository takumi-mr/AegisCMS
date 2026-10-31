# official-lightbox

本文の画像を、**クリックで拡大して見られる**ようにする公式プラグインです（[`on-render-page` フック](../../docs/plugin_hooks.md)）。

- 画像をクリック（またはフォーカスして Enter / Space）すると、画面いっぱいの表示（`<dialog>`）で開きます。**Esc**・背景のクリック・閉じるボタンで閉じ、閉じるとフォーカスが元の画像に戻ります。
- 本文に画像が複数あれば、**前後の画像へ**（ボタン・← → キー。端でぐるっと回ります）。`alt` を説明文として出せます。
- **リンクの中の画像**（クリックでリンク先へ行くもの）と、**小さな画像**（`min_width_px` 未満。アイコンなど）は対象にしません。
- 画像は表示中のもの（`currentSrc`）をそのまま拡大します。より大きな元画像への差し替えはしません。
- 本文に `<img` が無いページには何も足しません。スクリプトが動かなくても、画像はそのまま見えます。

## 使い方

設定ファイルの `[plugins].enabled` に `"official-lightbox"` を足すか、Admin Studio の **Settings › Plugins** で有効にします。

```toml
[plugins]
enabled = ["official-lightbox"]
```

テーマが `extra_head` と `extra_css` を出力している必要があります（組み込みのテンプレートは出力します）。

## 設定

Admin Studio の **Settings › Plugins** の「設定」か、設定ファイルの `[plugins.settings.official-lightbox]` で変えます。省略した項目・不正な値は既定値になります。

| キー | 既定値 | 説明 |
| --- | --- | --- |
| `content_types` | `article,page` | 動かすコンテンツの種類（カンマ区切り） |
| `selector` | `.prose` | 本文の範囲（CSS セレクター。カンマ区切りで複数）。この中の画像だけを対象にする |
| `min_width_px` | `120` | 対象にする画像の最小の幅（0〜2000） |
| `show_caption` | `true` | `alt` を説明文として出す |
| `animate` | `true` | ふわっと表示する（`prefers-reduced-motion` のときはしない） |
| `bg_color` | `rgba(0,0,0,.88)` | 背景色。`#hex`・`rgb()`・`hsl()`・`var(--名前)`・色名 |
| `locale` | `en` | 読み上げ用の名前の言語（`en` / `ja`） |

## 見た目を変える

拡大表示は `<dialog class="olb">`（画像は `.olb-full`、説明は `.olb-caption`、ボタンは `.olb-btn`）、対象の画像は `img.olb-img` です。設定で足りないところは、テーマの CSS で上書きしてください。

## ビルド

```bash
task plugin:build:content   # plugin.wasm を作り直す（要 rustup target add wasm32-unknown-unknown）
cargo test -p aegis-plugin-lightbox
```
