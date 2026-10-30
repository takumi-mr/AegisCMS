[← ドキュメント一覧に戻る](README.md)

# サイト内検索

公開中の記事・固定ページを全文検索できるようにします（`[search]`）。検索ページ `/search` と、テーマや外部から使う API `GET /api/v1/search` があります。索引は [tantivy](https://github.com/quickwit-oss/tantivy) で作り、日本語は辞書を使わず 2 文字ずつに分けて索引します（N-gram）。

機能はクレート `aegis-search` にあり、サーバーの Cargo の feature `search`（既定で有効）で組み込みます。

```text
 Admin Studio で公開・更新・非公開・削除 ／ 予約公開・予約非公開
        │  イベントバス（[cache] と同じ。Redis なら全サーバー）に「このコンテンツが変わった」
        ▼
 各サーバーが DB から公開中の版を読み直し、自分の索引に反映（公開中でなければ索引から除く）

 起動時: 公開中のコンテンツをすべて読み直して索引を作り直す（裏で行い、起動は待たせない）
```

---

## 1. 有効にする

既定で有効です。止めるときや、対象の種類を変えるときに書きます。

```toml
[search]
enabled = true
index_dir = "data/search"            # 索引を置くディレクトリ（空ならメモリの中だけ）
content_types = ["article", "page"]  # 検索の対象にするコンテンツの種類
```

- 索引は DB から作り直せるもので、消しても起動時に作り直します。Docker のイメージでは `/app/data/search`（ボリュームの中）に置きます。
- 複数台で動かすときは、索引はサーバーごとに持ちます（同じディレクトリを共有しないでください。使えないときはメモリの索引になります）。変更はイベントバスで全サーバーに届き、Redis に再接続したときは索引を作り直します。
- テーマに固定ページ `search` があっても、`/search` は検索ページになります。

## 2. 検索の仕方

- 空白で区切った語を**すべて**含むものを探します（AND）。`Docker 日本語` は「Docker」と「日本語」の両方を含むもの。
- 語の中は、語順どおりに続いているものだけが一致します。「東京都」は「京都」だけを含む文書には一致しません。
- 英字の大文字・小文字、全角・半角の英数字は区別しません（`ＡＥＧＩＳ` と `aegis` は同じ）。英字は単語単位なので、`cms` は `AegisCMS` に一致しません。
- 1 文字の漢字・かなでも探せます。
- 並びは既定で一致の良い順で、タイトルの一致は本文の一致より上に並びます。`sort=newest` を付けると公開日時の新しい順になります（[§3](#3-検索ページsearch)・[§4](#4-api)）。
- `category` にカテゴリの ID を付けると、そのカテゴリのコンテンツだけを探します（スキーマに `category_id` フィールドがある種類。無い種類は絞り込むと出てきません）。
- 本文として探すのは、段落・見出し・リスト・表のセル・引用・注記・折りたたみ・コードの文字と画像の代替テキスト、抜粋（`excerpt`）です。カスタム HTML・プラグインのブロックの中は対象にしません。
- 検索語は 200 文字まで、語は 10 個までです。

## 3. 検索ページ（`/search`）

`/search?q=検索語&page=2` で（`&category=カテゴリID`、`&sort=newest` も使えます）、テーマの `search.html`（無ければ組み込みのテンプレート）で表示します。1 ページの件数は `[theme].posts_per_page` です。検索エンジンには載せません（`X-Robots-Tag: noindex`）。

テンプレートに渡す値:

| 値 | 内容 |
| --- | --- |
| `fields.query` | 検索語 |
| `fields.category` | 絞り込み中のカテゴリの ID（無ければ空） |
| `fields.sort` | 並び（`relevance` または `newest`。知らない値は `relevance`） |
| `items` | 一致したコンテンツ（一致の良い順）。`item.title`、`item.content_type`、`item.published_at`、`item.fields.url`（公開ページの URL、`base_path` 込み）、`item.fields.snippet_html`（本文の抜粋。検索語を `<mark>` で囲んだエスケープ済みの HTML なので `{{ item.fields.snippet_html \| safe }}` で出す） |
| `pagination` | `total_items`（一致した件数）、`total_pages`、`prev_url` / `next_url` |

```html
<form role="search" action="{{ base_path }}/search">
  <input type="search" name="q" value="{{ fields.query }}">
  <select name="sort">
    <option value="relevance">一致の良い順</option>
    <option value="newest"{% if fields.sort == "newest" %} selected{% endif %}>新しい順</option>
  </select>
</form>
```

テーマのヘッダーなどに置く検索欄は、このフォームを `/search` に送るだけで使えます。

## 4. API

`GET /api/v1/search?q=検索語&type=article,page&category=カテゴリID&sort=newest&limit=10&offset=0`（ログイン不要）

| パラメータ | 内容 |
| --- | --- |
| `q` | 検索語（200 文字まで。超えると `400`） |
| `type` | 絞り込むコンテンツの種類（カンマ区切り。`[search].content_types` にあるものだけ） |
| `category` | 絞り込むカテゴリの ID（空なら絞り込まない） |
| `sort` | 並び。`relevance`（一致の良い順。既定）か `newest`（公開日時の新しい順）。知らない値は `400` |
| `limit` | 件数（既定 10、最大 50） |
| `offset` | 飛ばす件数 |

```json
{
  "query": "キャッシュ",
  "total": 1,
  "items": [
    {
      "content_type": "article",
      "id": "4f2a…",
      "slug": "cdn-cache-tag-purge",
      "title": "CDN の Cache-Tag によるオンデマンドパージに対応しました",
      "url": "/articles/cdn-cache-tag-purge",
      "published_at": "2026-10-29T09:37:43Z",
      "snippet_html": "…サーバー側に HTML の<mark>キャッシュ</mark>を持ちません。…",
      "score": 12.3
    }
  ]
}
```

`sort=newest` のとき `score` は 0 です。

`[search].enabled = false`（または feature `search` なしでビルド）のときは `404` です。
