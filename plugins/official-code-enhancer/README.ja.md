# AegisCMS 公式 Code Enhancer プラグイン (`official-code-enhancer`)

[English](README.md) | **日本語**

コードの **シンタックスハイライト**（[Prism.js](https://prismjs.com/)）、**コードのテーマ**、**コピーボタン**、**インラインコードの整形**を行う公式 WebAssembly プラグインです。Mermaid の図は、別のプラグイン [`official-mermaid`](../official-mermaid/) になりました。

---

## 動き

- 本文に `<pre>`（コードブロック）か `` `inline code` `` があるページだけ、スクリプトと CSS を入れます。コードの無いページには何も足しません。
- ハイライトは Prism.js と、その autoloader（`cdn.jsdelivr.net`）です。言語は、ページに出てきたものだけを、必要になったときに読み込みます（Prism が対応する言語は、コードブロックの言語名を書くだけで使えます。`golang` のような別名は、Prism の言語名（`go`）で書いてください）。
- ` ```mermaid ` のブロックは、ここでは何もしません（`official-mermaid` が図にします）。
- **外部 CDN から読むので、コードのあるページを開いた人の IP アドレスなどが CDN に送られます**。
- このプラグインを有効にしている間、サーバーはマニフェストの `capabilities.csp`（`script_domains`: `cdn.jsdelivr.net`、`style_domains`: 外部テーマ用の `cdn.jsdelivr.net`・`cdnjs.cloudflare.com`・`unpkg.com`）を CSP に足します。
- コードブロックの右上に、ホバーで「Copy」ボタンが出ます。`` `inline code` `` は、XSS 安全（テキストノードの分割）に `<code>` へ整形します。

---

## コードのテーマ

Admin Studio の **Settings › Plugins** のこのプラグインの設定画面で、`code_theme`・`code_theme_url`・`code_theme_css` を設定します。記事ごと、テーマごとに変えたいときは、記事のカスタムフィールドとテーマ変数（`theme_vars`）でも指定できます。

優先順位は「記事のフィールド › テーマ変数 › プラグインの設定 › 既定（`dracula`）」です。

| テーマ ID        | テーマ名     | 特徴                                                           |
| ---------------- | ------------ | -------------------------------------------------------------- |
| `dracula` (既定) | Dracula      | 上品なダーク背景に鮮やかなピンク・パープル・シアンのアクセント |
| `github-dark`    | GitHub Dark  | GitHub の落ち着いたダークブルーグレー                          |
| `one-dark`       | One Dark Pro | VS Code / Atom の定番のダーク                                  |
| `nord`           | Nord Arctic  | 北欧の青と雪白の配色                                           |
| `github-light`   | GitHub Light | 白基調のライト                                                 |
| `monokai`        | Monokai Pro  | 高コントラストのクラシック                                     |

```json
{ "code_theme": "nord" }
```

### 外部 CSS（CDN）

Prism.js などの任意のテーマの CSS を、URL で読み込みます。

```json
{ "code_theme_url": "https://cdnjs.cloudflare.com/ajax/libs/prism/1.29.0/themes/prism-tomorrow.min.css" }
```

読めるホストは、マニフェストが宣言した `cdn.jsdelivr.net`・`cdnjs.cloudflare.com`・`unpkg.com` です（ほかのホストは、サイトの CSP で許可が要ります）。

### カスタム CSS

```json
{ "code_theme_css": ".prose code.inline-code-enhanced { background: #ffeaa7; color: #d63031; }" }
```

### JavaScript の API

`window.AegisCodeEnhancer.setThemeUrl(url)` で、外部テーマの CSS を差し替えられます（Admin Studio のプレビューなど）。`renderAll()` で、ハイライトなどをやり直せます。

---

## Mermaid の図

`official-mermaid` に分かれました。図を使うサイトは、そのプラグインも有効にしてください（`[plugins].enabled` に `"official-mermaid"` を足す。同梱の `config/default.toml` は両方を有効にしています）。

---

## ビルド

```bash
task plugin:build:code-enhancer
```
