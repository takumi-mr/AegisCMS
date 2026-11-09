# official-clarity

公開ページに [Microsoft Clarity](https://clarity.microsoft.com/)（ヒートマップ・画面録画）を足す公式の Wasm プラグインです。Clarity のプロジェクト ID を入れるだけで動きます。

## 何をするか

- **`on-render-page`** で、`<head>` に Clarity の標準の起動コードを入れる。
- **Cookie の同意を待つ**: 既定では `type="text/plain"` と `data-aegis-consent="analytics"` の形で出し、同意バナー（`[consent]`）のスクリプトが、訪問者の同意のあとに実行します（[consent.md](../../docs/consent.md)）。**`[consent].enabled = true` にしていないと、Clarity は動きません**。同意が要らないサイトだけ、`require_consent = false` にしてください。
- **CSP を自動で足す**: プラグインを有効にすると、Clarity の公式の案内（[Clarity Content Security Policy](https://learn.microsoft.com/en-us/clarity/clarity-csp)）にある次のホストが、CSP に加わります。サイト全体の CSP を書き換える必要はありません。
  - `script-src`: `www.clarity.ms`、`scripts.clarity.ms`
  - `connect-src`: `www.clarity.ms`、`scripts.clarity.ms`、`a.clarity.ms` 〜 `z.clarity.ms`、`c.bing.com`
  - `img-src`: `c.clarity.ms`、`c.bing.com`
  - CSP のホストにワイルドカード（`*.clarity.ms`）は書けないため、公式の案内にある個別のホストを並べています。`scripts.clarity.ms` は公式の案内には無く、利用者の報告（[microsoft/clarity#913](https://github.com/microsoft/clarity/issues/913)）に基づきます。Clarity 側のホストが変わったら、`plugin.manifest.json` の `capabilities.csp` を直してください。

## 録画しないページ

録画に入れてはいけないページには、起動コードを出しません。

- **会員・買い物・問い合わせ・掲示板のページ**（テンプレートが `account.html`・`cart.html`・`order.html`・`contact.html`・`forum_index.html`・`forum_category.html`・`forum_thread.html`）。会員だけが読める掲示板のカテゴリがあるためです。
- **会員レベルを求める記事・ページ**（`access` が `public` 以外）。全文が録画に入るためです。`exclude_restricted = false` で外せますが、有料の本文が Microsoft のサーバーに残ることを理解したうえで使ってください。
- **管理画面のプレビュー**（`srcdoc` の iframe）と `file:` で開いたページ。起動コードの先頭で、`http(s)` のページでないときは何もしません（編集中のプレビューが、サイトのデータに混ざらないように）。

Clarity 自身も、入力欄の内容を既定で伏せます。ただし、本文の文字は録画に入ります。

## 設定 (`[plugins.settings.official-clarity]`)

```toml
[plugins]
enabled = ["official-clarity"]

[plugins.settings.official-clarity]
project_id = "abc123xyz9"     # Clarity の Settings > Overview にあるプロジェクト ID（英数字）。空なら何も出さない
require_consent = true        # 同意を待つ（既定 true。[consent].enabled = true が必要）
consent_category = "analytics" # 待つ同意の区分（既定 analytics）
exclude_restricted = true     # 会員レベルを求めるページに付けない（既定 true）
```

管理画面の「プラグイン」からも設定できます。

## プライバシー

Clarity は Microsoft のサービスで、画面録画・クリックの位置・ブラウザーの情報が Microsoft に送られます。Cookie を使います。**プライバシーポリシーに、外部サービスへの送信と目的を書いてください**。必要な通知や同意の要件は、サイトの対象地域によって異なります。

## ビルド

`plugin.wasm` はビルド済みのものを同梱しています。ソースを変えたら `task plugin:build:clarity` で作り直します。
