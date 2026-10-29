# テーマ作成ガイド

公開サイトの見た目（テーマ）の作り方をまとめます。テーマエンジンの仕組み（テンプレートの探索順、プラグインによる書き換え）は [コンテンツ公開 & テーマエンジン開発ガイド](./theme_and_publishing_guide.md) を参照してください。

お手本には `themes/roastery`（サンプルショップ「朝凪珈琲」のテーマ）が一番よくまとまっています。ショップのページまで含めてテーマの色で表示され、スマートフォンの幅にも対応しています。

---

## 1. ディレクトリ構成

テーマは `[paths].themes_dir`（既定は `themes/`）の下に、テーマの ID と同じ名前のディレクトリとして置きます。

```text
themes/my-theme/
├── theme.json          # マニフェスト（必須）
├── preview.svg         # Admin Studio のテーマ一覧に出すプレビュー画像（preview.png / .webp / .jpg でも可）
├── templates/
│   ├── base.html       # 全ページ共通の骨組み（必須）
│   ├── index.html      # 記事一覧（トップページが記事一覧のときはトップも）
│   ├── front.html      # トップページ（[theme].front_page = "template" のとき）
│   ├── category.html   # カテゴリの記事一覧
│   ├── article.html    # 記事
│   ├── page.html       # 固定ページ
│   └── 404.html        # 見つからない・非公開のページ
└── assets/
    └── style.css       # /theme/assets/style.css で配信される
```

- `templates/` に無いテンプレートは組み込みのテンプレートで描画されます。最初は `base.html` だけを作り、他は組み込みのまま確かめていく進め方もできます。
- ただし将来のテーママーケットプレイスでの配布に使う検証（`validate_theme_package`）は、`theme.json` と `templates/base.html` が無いと通りません。
- `assets/` の中身は `{{ base_path }}/theme/assets/<パス>` で配信されます（画像・フォント・JS も置けます）。

## 2. `theme.json`

```json
{
  "id": "my-theme",
  "name": "My Theme",
  "version": "1.0.0",
  "description": "テーマの説明。Admin Studio のテーマ一覧に表示されます。",
  "author": { "name": "Your Name", "url": "https://example.com" },
  "license": "MIT",
  "preview_image": "preview.svg",
  "tags": ["light", "blog"],
  "homepage": "https://example.com/themes/my-theme",
  "min_aegis_version": "0.1.0"
}
```

| キー | 必須 | 内容 |
|---|---|---|
| `id` | ○ | テーマの ID（kebab-case）。ディレクトリ名と同じにする。空ならディレクトリ名を使う |
| `name` | ○ | 表示名 |
| `version` | | セマンティックバージョン（既定 `1.0.0`） |
| `description` | | 説明文 |
| `author` | | 文字列、または `{ "name", "email", "url" }` |
| `license` / `homepage` / `repository` / `min_aegis_version` | | 配布用の情報 |
| `preview_image` | | プレビュー画像のファイル名。無ければ `preview.svg` → `.png` → `.webp` → `.jpg` の順に探す |
| `tags` | | 分類用のタグ |

`theme.json` が無い・JSON として読めない場合は、ディレクトリ名から作った名前（`my-theme` → `My Theme`）と既定の説明で一覧に出ます（読めないときはサーバーのログに警告が出ます）。

## 3. テンプレート

テンプレートは [MiniJinja](https://docs.rs/minijinja)（Jinja2 互換）で書きます。`.html` のテンプレートでは変数が自動で HTML エスケープされます。

### 3.1 どの URL でどのテンプレートが使われるか

| URL | テンプレート | 主な変数 |
|---|---|---|
| `/`（`[theme].front_page = "posts"`、既定） | `index.html` | `title`（`Latest Stories`）、`items`、`pagination` |
| `/`（`front_page = "template"`） | `front.html` | `content_type`（`front`）、`items`（最新の記事 `posts_per_page` 件）、`pagination`（続きは `/articles?page=2`） |
| `/`（`front_page = "page:<slug>"`） | 固定ページと同じ（`page.html` など） | 固定ページと同じ。canonical などの URL は `/` |
| `/articles` | `index.html` | `title`（`Latest Stories`）、`items`、`pagination` |
| `/category/{slug}` | `category.html` | `title`（カテゴリ名）、`slug`、`fields`（カテゴリのフィールド。`description` など）、`items`、`pagination` |
| `/articles/{slug}` | `article.html` | `title`、`content_html`、`author`、`published_at`、`updated_at`、`fields`、`relations`、`comments`（[コメント](#6-コメント欄)） |
| `/{slug}` | `page.html`（固定ページ）または `article.html` | 同上 |
| `/{slug}`（固定ページの `layout` が `blank`） | `blank.html` | 同上。ヘッダー・フッター・幅の制約の無い白紙のページ（LP 向け）。`layout` にほかの名前（例: `landing`）を入れると、テーマの `landing.html` を使う |
| `/forum` 以下 | `forum_index.html` / `forum_category.html` / `forum_thread.html` | [掲示板](forum.md#2-公開サイト) |
| 見つからない・非公開 | `404.html` | `title`（`Page Not Found`）、`content_html`（理由の一文） |
| `/products` | `products.html` | `title`、`items`、`pagination`、`theme_vars` |
| `/products/{slug}` | `product.html` | `title`、`content_html`、`fields`（`price` / `stock` など）、`relations`、`theme_vars` |
| `/cart` | `cart.html` | `theme_vars` |
| `/orders/{id}` | `order.html` | `slug`（注文 ID）、`theme_vars` |

### 3.1.1 トップページ（`[theme].front_page`）

| 値 | `/` に出すもの | 記事一覧 |
|---|---|---|
| `"posts"`（既定） | 記事一覧（`index.html`） | `/` と `/articles` の両方。テンプレートの中では区別できないので、ヒーローなどトップだけに出したい要素も `/articles` に表示される |
| `"template"` | テーマの `front.html`。ランディングページ向け。最新の記事が `items` に入るので、「お知らせ」の欄などに使える。テーマに無ければ `index.html` と同じ表示 | `/articles` |
| `"page:<slug>"` | 固定ページ（Admin Studio で編集できる。「ブランク LP」のレイアウトも使える）。ページが無い・非公開なら記事一覧 | `/articles` |

`"posts"` 以外にするときは、テーマのナビゲーションの記事一覧へのリンクを `{{ base_path }}/articles` にしてください。

### 3.1.2 カテゴリの記事一覧とフィード

- `/category/{slug}` は、記事のスキーマの `target: category.<フィールド>` の関係フィールド（同梱のスキーマでは `category_id`）がそのカテゴリを指す記事を並べます。テーマに `category.html` が無ければ組み込みのテンプレートを使います。カテゴリへのリンクは `{{ base_path }}/category/{{ item.relations.category_id.slug }}` のように書けます。
- Atom フィードは `/feed.xml`（新しい記事）と `/category/{slug}/feed.xml`（カテゴリの記事）。どちらも最新 20 件で、本文の HTML も含みます。自動検出用の `<link rel="alternate" type="application/atom+xml">` はサーバーが `extra_head` に入れるので、テーマで出力する必要はありません（カテゴリのページにはカテゴリのフィードも入ります）。
- フィードの URL は Admin Studio の **Settings › SEO** の canonical URL を基準にします。未設定ならリクエストの Host（とリバースプロキシの `X-Forwarded-Proto`）から作ります。

ショップのテンプレート（`products.html` 以降）は、テーマに無くても組み込みのテンプレートがテーマの `base.html` の中に描画されます（[5. ショップのページ](#5-ショップのページ)）。`blank.html` と掲示板のテンプレートも組み込みのものがあり、テーマに同じ名前のファイルを置くと置き換えられます。

固定ページのレイアウトは Admin Studio のエディタ（「テーマ標準」/「ブランク LP」）で選び、スキーマ `page` の `layout` フィールドに保存されます。ブランク LP の中身は「カスタム HTML」ブロック（`core/html`）で自由に書けます。このブロックの HTML はエスケープせずにそのまま出力します（`[security]` のリンク・画像の属性の付与だけが掛かり、`<script>` も残ります）。

### 3.2 変数

すべてのページで使える変数:

| 変数 | 内容 |
|---|---|
| `base_path` | サイトを公開しているパス（例: `/cms`。ルートで公開しているときは空文字）。**サイト内のリンクとアセットには必ず先頭に付ける** |
| `site_title` / `site_description` | サイト名（Admin Studio の **Settings › Site** で変更した値、無ければ `[theme].site_title`）/ `[theme].site_description` |
| `title` / `slug` | ページのタイトル・スラッグ |
| `content_type` | `index` / `article` / `page` / `error` など |
| `extra_css` | `[theme].custom_css` とプラグインが足した CSS |
| `extra_head` | `<head>` に足す HTML（サイトの favicon の `<link rel="icon">` と、プラグインが足したもの）。**`<head>` の中で必ず `{{ extra_head \| safe }}` を出力する** |
| `theme_vars` | プラグインやショップが渡す文字列の辞書 |

一覧（`items` の各要素）: `title`、`slug`、`excerpt`（本文の先頭 140 文字）、`published_at`、`created_at`、`content_type`、`fields`（カスタムフィールド）、`relations`（解決済みのリレーション。例: `item.relations.category_id.title`）。

ページ送り（`pagination`）: `page`、`per_page`、`total_items`、`total_pages`、`prev_url` / `next_url`（`base_path` 込み。無ければ空）。

### 3.3 フィルター

| フィルター | 例 | 結果 |
|---|---|---|
| `date_short` | `{{ published_at \| date_short }}` | `2026-10-23`（先頭 10 文字） |
| `datetime_short` | `{{ created_at \| datetime_short }}` | `2026-10-23 14:05`（分まで。UTC のまま、`T` を空白に） |
| `truncate(n)` | `{{ item.excerpt \| truncate(90) }}` | n 文字を超えたら切って `...` を付ける（文字数で数えるので日本語でも安全） |
| `money(通貨)` | `{{ fields.price \| money(theme_vars.currency) }}` | `¥1,780`（金額は最小単位の整数） |
| `safe` | `{{ content_html \| safe }}` | エスケープしない |

`safe` を付けてよいのは `content_html`（サーバーがサニタイズ済み）、`extra_css`、`extra_head` だけです。カスタムフィールドなど、利用者が入力した値には付けないでください。

### 3.4 `base.html` に必ず入れるもの

```html
<!DOCTYPE html>
<html lang="ja">
<head>
  <meta charset="UTF-8">
  <meta name="viewport" content="width=device-width, initial-scale=1.0">
  <title>{% if title %}{{ title }} | {% endif %}{{ site_title }}</title>
  <meta name="description" content="{{ site_description }}">
  <link rel="stylesheet" href="{{ base_path }}/theme/assets/style.css">
  {% if extra_css %}
  <style>{{ extra_css | safe }}</style>
  {% endif %}
  {{ extra_head | safe }}
</head>
<body>
  <header>
    <a href="{{ base_path }}/">{{ site_title }}</a>
    <nav>
      <a href="{{ base_path }}/articles">Articles</a>
    </nav>
  </header>
  <main>
    {% block content %}{% endblock %}
  </main>
</body>
</html>
```

- **`viewport` の meta**: 無いとスマートフォンで PC 幅のまま縮小表示されます。
- **`base_path`**: 付けなくてもルート公開では動きますが、サブパス公開（[設定リファレンス](./configuration.md#42-サブパスで公開する)）でリンクが切れます。
- **`extra_css | safe`**: `safe` を付けないと `"` や `>` がエスケープされ、`[theme].custom_css` の CSS が壊れます。
- **`{% block content %}`**: 他のテンプレート（組み込みのショップのテンプレートを含む）はこのブロックに中身を入れます。名前を変えないでください。

他のテンプレートは `base.html` を継承します。

```html
{% extends "base.html" %}

{% block content %}
<article>
  <h1>{{ title }}</h1>
  {% if published_at %}<time>{{ published_at | date_short }}</time>{% endif %}
  <div class="prose">{{ content_html | safe }}</div>
</article>
{% endblock %}
```

## 4. CSS の書き方

### 4.1 色は CSS 変数にまとめ、`--accent` を定義する

```css
:root {
  --my-bg: #fbf8f3;
  --my-text: #2b211b;
  --my-accent: #a4552c;
  --accent: var(--my-accent); /* ショップのボタン・バッジの色 */
}
```

組み込みのショップのページは、ボタンなどの色に `--accent` を使います（無ければ紫 `#6366f1`）。それ以外の線や背景は半透明のグレーなので、明るいテーマでも暗いテーマでもそのまま読めます。

### 4.2 リセットで消したものは戻す

`* { margin: 0; padding: 0; }` のリセットを使うと、本文のリストの字下げや段落の余白も消えます。記事・固定ページの本文（`content_html` を入れる要素）には、見出し・段落・リスト・引用・コード・画像のスタイルを書いてください。

```css
.prose p { margin-bottom: 1.5rem; }
.prose h2 { margin: 2.5rem 0 1rem; }
.prose ul,
.prose ol { padding-left: 1.4rem; margin-bottom: 1.5rem; }
.prose img { max-width: 100%; height: auto; }
.prose pre { overflow-x: auto; }
```

商品ページの本文（組み込みの `product.html` の `.prose`）には、詳細度 0 の最低限の余白と字下げが組み込まれています。テーマの `.prose` の指定があればそちらが優先されます。

### 4.3 スマートフォン（幅 320〜390px）で崩さない

実際に起きた崩れから、気をつける点をまとめます。

- **ヘッダーは折り返せるようにし、サイト名は折り返さない**。日本語は文字ごとに改行できるので、横並びのメニューに押されるとサイト名が 1 文字ずつ縦に並びます。

  ```css
  .header-inner { display: flex; justify-content: space-between; align-items: center; flex-wrap: wrap; gap: 0.75rem 1.5rem; }
  .brand { white-space: nowrap; }
  .nav { display: flex; flex-wrap: wrap; gap: 0.5rem 1rem; }
  ```

- **グリッドの最小幅は `min(..., 100%)` で包む**。`minmax(320px, 1fr)` は、画面から余白を引いた幅が 320px 未満になると横にはみ出します。

  ```css
  .cards { display: grid; grid-template-columns: repeat(auto-fill, minmax(min(320px, 100%), 1fr)); }
  ```

- **横に並べた入力欄には `min-width: 0` を付ける**。`flex: 1` の `<input>` にはブラウザが決めた最小幅があり、隣のボタンを画面の外へ押し出します。
- **固定の `width` を避ける**。`width: 900px` ではなく `max-width: 900px; width: 100%` にします。装飾用の大きな要素（背景の光など）は `position: fixed` か `overflow: hidden` の親の中に置きます。
- **余白は狭い画面で減らす**。左右 `1.5rem` の余白は 320px 幅では大きいので、`@media (max-width: 600px)` で `1rem` 程度にします。

## 5. ショップのページ

`[commerce].enabled = true` のとき、`/products`・`/products/{slug}`・`/cart`・`/orders/{id}` が使えます。テーマに `products.html` などが無ければ組み込みのテンプレートを使います。組み込みのテンプレートはテーマの `base.html` の `content` ブロックの中に描画され、色は 4.1 の `--accent` に合わせます。多くのテーマはショップのテンプレートを書かなくても足ります。

見た目を変えたいときは、まず CSS で組み込みのクラス（`.aegis-shop`、`.aegis-card`、`.aegis-button`、`.aegis-product` など）を上書きしてください。テンプレートを自分で書く場合は、組み込みのテンプレート（`crates/aegis-theme/src/builtins.rs` の `PRODUCTS_TEMPLATE` など）を写して始めます。その際は次の 2 点を守ってください。

- `{% include "shop_common.html" %}` を残す。カートのスクリプト（`/shop/cart.js`）と、カートの件数を出すナビゲーションがここに入っています。
- カートに入れるボタンは、組み込みと同じ `data-aegis-add-to-cart` などの属性を付ける。カート・注文確認の中身（`#aegis-cart`、`#aegis-order`）はスクリプトが描画します。

ショップで使える `theme_vars`: `shop_enabled`（`"true"` / `"false"`）、`currency`、`products_url`、`cart_url`、`shop_api`、`cart_script`（どれも `base_path` 込み）。

## 6. コメント欄

`[comments].enabled = true` で、`[comments].content_types` の種類のページ（`article.html` / `page.html` / `product.html`）に変数 `comments` が渡ります（[コメント](comments.md#3-公開サイト)）。本文の後（`</article>` の前など）に組み込みのコメント欄を読み込んでください。コメントを受け付けないページでは `comments` が無いので、何も出ません。

```jinja
{% if comments %}{% include "comments.html" %}{% endif %}
```

- 組み込みのコメント欄は、色をテーマの文字色を基準にした半透明と `--accent` で指定しているので、多くのテーマはそのまま使えます。見た目は CSS で `.aegis-comments`、`.aegis-comment`、`.aegis-comment-form` などを上書きします。
- 自分で書く場合は、テーマの `templates/comments.html` に置きます（組み込みの `COMMENTS_TEMPLATE` を写して始める）。一覧は `comments["items"]`（`.items` は辞書のメソッド名と重なる）、各コメントの本文は `{{ c.body_html | safe }}`（エスケープ済み）、返信の字下げは `c.depth` です。フォームは `comments.post_url` に JSON で送ります。ハニーポットの欄（`website`）を残してください。

## 7. 開発の流れ

1. `themes/` に既存のテーマ（`roastery` など）をコピーし、ディレクトリ名と `theme.json` の `id` を変える。
2. 設定で `[theme].reload_templates = true` にする。描画のたびにテンプレートを読み直すので、サーバーを再起動せずに確かめられます（本番では `false`）。CSS などのアセットは毎回ファイルから配信されます。
3. テーマを切り替える。Admin Studio の **Settings → Themes**、`PUT /api/v1/themes/active`（`{"theme_id": "my-theme"}`、admin のみ）、または `[theme].active` で切り替えます。
4. 中身のあるサイトで確かめる。サンプルショップ（`task demo:shop` と、別のターミナルで `task demo:shop:seed`。手順は [examples/coffee-shop](../examples/coffee-shop)）を使うと、記事・固定ページ・商品・カテゴリが揃います。

### 確認するページと幅

| ページ | URL の例（サンプルショップ） |
|---|---|
| トップ・記事一覧 | `/`、`/articles` |
| 記事（見出し・リスト入り） | `/articles/how-to-brew-pour-over` |
| 固定ページ | `/about` |
| 商品一覧・商品 | `/products`、`/products/ethiopia-yirgacheffe` |
| カート（空 / 商品を入れた状態） | `/cart` |
| 404 | `/no-such-page` |

幅 **320 / 360 / 390px**（スマートフォン）と **1280px**（PC）で確かめます。ブラウザの開発者ツールでもよいですし、Playwright などで次の値を比べると、横スクロールが出るかを機械的に調べられます。

```js
document.documentElement.scrollWidth > document.documentElement.clientWidth // true なら横にはみ出している
```

サイト名・タイトルは日本語と英語の両方で確かめてください。日本語は文字ごとに折り返すので、英語では問題ない配置でも崩れることがあります。
