[← Back to the documentation index](README.md)

**English** | [日本語](search.ja.md)

# Site Search

Makes published articles and static pages full-text searchable (`[search]`). There is a search page `/search` and an API `GET /api/v1/search` for themes and external use. The index is built with [tantivy](https://github.com/quickwit-oss/tantivy), and Japanese is indexed two characters at a time without a dictionary (N-gram).

The feature lives in the `aegis-search` crate and is built in with the server's Cargo feature `search` (enabled by default).

```text
 Publish / update / unpublish / delete in Admin Studio, or scheduled publish / unpublish
        │  Event bus (same as [cache]; all servers with Redis): "this content changed"
        ▼
 Each server rereads the published version from the DB and reflects it in its own index (removed from the index if not published)

 At startup: reread all published content and rebuild the index (done in the background; startup is not blocked)
```

---

## 1. Enabling It

It is enabled by default. Write this when you want to turn it off or change the target kinds.

```toml
[search]
enabled = true
index_dir = "data/search"            # directory for the index (in memory only if empty)
content_types = ["article", "page"]  # content kinds to search
```

- The index can be rebuilt from the DB, and if you delete it, it is rebuilt at startup. In the Docker image it is placed at `/app/data/search` (inside the volume).
- When running multiple servers, each server has its own index (do not share the same directory; if it cannot be used, an in-memory index is used). Changes reach all servers over the event bus, and when reconnecting to Redis the index is rebuilt.
- Even if the theme has a static page `search`, `/search` is the search page.

## 2. How Searching Works

- It finds documents that contain **all** the words separated by spaces (AND). `Docker 日本語` finds those containing both "Docker" and "日本語" (Japanese).
- Within a word, only text that continues in the same order matches. "東京都" does not match a document containing only "京都".
- Case of Latin letters and full-width vs half-width alphanumerics are not distinguished (`ＡＥＧＩＳ` and `aegis` are the same). Latin letters are word-based, so `cms` does not match `AegisCMS`.
- Single kanji or kana characters can be searched too.
- The order is by relevance by default, and title matches rank above body matches. Adding `sort=newest` orders by publication date, newest first ([§3](#3-the-search-page-search), [§4](#4-api)).
- Adding a category ID as `category` searches only that category's content (for kinds whose schema has a `category_id` field; kinds without one return nothing when filtered).
- Adding a tag slug (or ID) as `tag` searches only content with that tag ([Tags](tags.md)).
- What is searched as body text is the characters of paragraphs, headings, lists, table cells, quotes, notes, collapsibles, and code, the alt text of images, and the excerpt (`excerpt`). The inside of custom HTML and plugin blocks is not searched.
- Search terms are up to 200 characters, and up to 10 words.

## 3. The Search Page (`/search`)

`/search?q=search+terms&page=2` (you can also use `&category=category-ID`, `&tag=tag-slug`, and `&sort=newest`) is displayed with the theme's `search.html` (or the built-in template if absent). The number of items per page is `[theme].posts_per_page`. It is kept out of search engines (`X-Robots-Tag: noindex`).

![Results on the search page (`/search`)](images/search-results.png)

Values passed to the template:

| Value | Description |
| --- | --- |
| `fields.query` | The search terms |
| `fields.category` | The ID of the category being filtered on (empty if none) |
| `fields.tag` | The tag being filtered on (empty if none) |
| `fields.sort` | The order (`relevance` or `newest`; unknown values become `relevance`) |
| `items` | The matching content (best match first). `item.title`, `item.content_type`, `item.published_at`, `item.fields.url` (the public page URL, including `base_path`), `item.fields.snippet_html` (an excerpt of the body; escaped HTML with the search terms wrapped in `<mark>`, so output it as `{{ item.fields.snippet_html \| safe }}`) |
| `pagination` | `total_items` (number of matches), `total_pages`, `prev_url` / `next_url` |

```html
<form role="search" action="{{ base_path }}/search">
  <input type="search" name="q" value="{{ fields.query }}">
  <select name="sort">
    <option value="relevance">Best match</option>
    <option value="newest"{% if fields.sort == "newest" %} selected{% endif %}>Newest</option>
  </select>
</form>
```

A search box placed in the theme's header or elsewhere works by simply submitting this form to `/search`.

### Integration with Analytics
When analytics (`[analytics]`) is enabled, the keywords searched on the search page (`/search?q=...`) are aggregated automatically in a form that does not identify individuals (counted only on access to the first page). Keywords whose search returned 0 results are recorded separately as "searches with no results", and can be used in Admin Studio's Analytics dashboard and CSV export to analyze demand for content (calls to the search API `GET /api/v1/search` are not aggregated).

## 4. API

`GET /api/v1/search?q=search+terms&type=article,page&category=category-ID&tag=tag-slug&sort=newest&limit=10&offset=0` (no login required)

| Parameter | Description |
| --- | --- |
| `q` | The search terms (up to 200 characters; exceeding it returns `400`) |
| `type` | Content kinds to filter on (comma-separated; only those in `[search].content_types`) |
| `category` | The ID of the category to filter on (no filtering if empty) |
| `tag` | The slug or ID of the tag to filter on (no filtering if empty) |
| `sort` | The order. `relevance` (best match first; default) or `newest` (newest publication date first). An unknown value returns `400` |
| `limit` | Number of items (default 10, max 50) |
| `offset` | Number of items to skip |

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

With `sort=newest`, `score` is 0.

When `[search].enabled = false` (or built without the `search` feature), it returns `404`.
