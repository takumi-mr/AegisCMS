[← Back to the documentation index](README.md)

**English** | [日本語](headless.ja.md)

# Headless Delivery API

A read-only API that returns **only published content** as JSON, without authentication. Instead of rendering with an AegisCMS theme, it lets other sites (Next.js, Astro, etc.) and apps fetch the content.

The shape of the returned data is **decided by the schema (`schemas/*.yaml`)**. Writing a schema produces the delivery API in the same flow that produces the editing screen, the editing API, types, and OpenAPI.

## Usage

It is disabled by default. Enable it with `[headless]` in the config file (for the list of settings, see [Configuration reference 2.25](configuration.md#225-headless--headless-delivery-api)).

```toml
[headless]
enabled = true
content_types = ["article", "page"]   # kinds to deliver (default)
```

```bash
# A list of published content (newest first; pass next_cursor as cursor for the rest)
curl "http://localhost:8080/api/v1/public/contents/article?limit=10"
# One item (with the body blocks; ?include=html also gives the rendered HTML)
curl "http://localhost:8080/api/v1/public/contents/article/hello-world?include=html"
# OpenAPI for only the delivered kinds and fields
curl "http://localhost:8080/api/v1/public/openapi.json"
```

Fields meant for internal use stop being returned if you write `public: false` in the schema.

```yaml
fields:
  - name: internal_note
    type: string
    public: false
```

---

## 1. Positioning and Scope

- AegisCMS's main axis stays the traditional one of rendering with themes. Headless delivery is an additional entry point that only people who use it enable (disabled by default).
- It is not a general-purpose backend. It is **read-only**; it does not provide an API where end users write arbitrary data, per-row permissions, or realtime. Features that involve writing, such as comments, contact, and orders, use each feature's own entry point as before.
- It is a separate entry point from the editing `/api/v1/contents/...` (which needs a Bearer token and also returns drafts; [Content posting API](content-api.md)). The reason is that reusing it for public use would make "not returning drafts" depend on how the implementation is written.

### What It Does Not Do (Outside the Scope of This Design)

- Fetching drafts and previews (an API key would be needed, so it would be a separate design)
- A search API (the site search `GET /api/v1/search` already exists)
- GraphQL
- Write APIs

---

## 2. Structure

```text
 Client ──▶ Server (unauthenticated route, rate limit, CORS, cache headers)
                    │
                    ▼
              aegis-headless (a new crate)
              Validating filter arguments / shaping per the schema / cursors / ETag
                    │  Only "reading the published version" is available
                    ▼
              aegis-db (get_published_content / list_published_contents_page)
```

### 2.1 The `aegis-headless` Crate

It is built the same way as search (`aegis-search`). It is built in with the server's Cargo feature `headless`.

- **Dependencies**: only `aegis-core`, `aegis-schema`, and the DB's read entry points. It does not depend on the server's internal state (`AppState`).
- **Narrowing the DB entry point**: the crate does not accept a `dyn DatabaseAdapter`; it accepts a small trait that has only fetching of published versions (tentatively `PublishedSource`). The server implements it on top of the adapter. The editing methods are not visible as a type, so code that returns drafts cannot be written.
- **What it holds (pure logic)**:
    - Validating list arguments (`limit`, `cursor`, `category`, `tag`) and clamping to limits
    - Shaping records per the schema (selecting only the public fields)
    - Encoding and decoding cursors (400 if tampered with)
    - Computing the `ETag`
- **The server's responsibilities**: registering routes, rate limiting, CORS, `Cache-Control`, CDN cache tags, and loading settings. The same division as the other crates (commerce, comments).

### 2.2 Reusing Existing Mechanisms (No New Queries Are Added)

| Purpose             | What is used                                              | Notes                                                                                                                                                                                                                                                                                                |
| ------------------- | --------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| One item            | `get_published_content(schema, slug_or_id)`               |                                                                                                                                                                                                                                                                                                      |
| List                | `list_published_contents_page(schema, &ContentListQuery)` | Filtering by `status`, `limit`/`offset`, keyset (`cursor_*`), `tag_id`, and `category_id` is supported. It is left to SQL with the published record's columns and indexes, so it does not depend on the number of articles                                                                           |
| Resolving relations | `resolve_published_relations`                             | Looks up **only published ones** by `id` / `slug` from the published version records (`_aegis_content_publications`). The editing `resolve_relations` reads the content's own table and would also return unpublished titles and drafts, so it is not used. Fields with empty values are not queried |
| Cache clearing      | `cdn::content_invalidation_tags`, `purge_content`         | With the same tags as public pages, they are cleared on update by the same mechanism                                                                                                                                                                                                                 |

The published list uses only what needs no additional queries. **Filtering and partial matching on extra fields are not supported** (for the same reason as items considered and set aside earlier).

---

## 3. Endpoints

All are `GET`, unauthenticated.

| Path                                    | Description                       |
| --------------------------------------- | --------------------------------- |
| `/api/v1/public/contents/{type}`        | A list of published content       |
| `/api/v1/public/contents/{type}/{slug}` | One published item                |
| `/api/v1/public/openapi.json`           | OpenAPI for only the public kinds |

### 3.1 List Query

| Argument   | Description                                                                                |
| ---------- | ------------------------------------------------------------------------------------------ |
| `limit`    | Items per page. Default 20, up to `max_page_size` (default 100)                            |
| `cursor`   | The previous page's `next_cursor`. The contents are opaque (clients do not interpret them) |
| `category` | The category's ID. `none` for uncategorized                                                |
| `tag`      | The tag's slug                                                                             |

### 3.2 Response Shape

List:

```json
{
  "items": [ { "...": "the shape of one item" } ],
  "next_cursor": "opaque-string-or-null",
  "total": 123
}
```

One item (an element of `items`, and a single-item response):

```json
{
  "id": "…",
  "content_type": "article",
  "title": "…",
  "slug": "…",
  "excerpt": "…",
  "published_at": "2026-11-05T00:00:00Z",
  "updated_at": "2026-11-05T00:00:00Z",
  "fields": { "excerpt": "…", "custom_meta": { } },
  "relations": { "category_id": { "id": "…", "title": "…", "slug": "…", "content_type": "category" } },
  "blocks": [ ]
}
```

- List elements are summaries (without `blocks`), and a single item includes `blocks` (the same split as `view=summary` of the editing API).
- **Standard items not returned**: `author_id` (the internal user ID), `status`, `unpublish_at`, `comments_open`, `rendered_html` (§6, item 2).
- The values of `fields` have the same shape as the editing API (jsonb is an object, and relations are resolved in `relations`).

---

## 4. What Is Decided by the Schema

### 4.1 Which Kinds Are Public

Decided by the settings' allowlist. **The default is only articles and static pages.** Kinds with internal values, such as a product's inventory, are not returned unless explicitly specified. A kind not on the allowlist is `404` (not distinguished from a nonexistent kind).

### 4.2 Which Fields Are Returned

A field carries whether it is public.

```yaml
fields:
  - name: internal_note
    type: string
    public: false      # not returned by the delivery API
```

- The default of `public` is "public" (§6, item 1). Write `public: false` only on fields you want to hide.
- Standard items (title, slug, dates, body) follow §3.2 above and cannot be changed by the schema.
- `category_id` (the built-in field) is always public.

### 4.3 OpenAPI and Types

Using the existing `OpenApiGenerator` and `TypeScriptGenerator`, definitions for only the public kinds and fields are delivered at `GET /api/v1/public/openapi.json` (the running settings are reflected).

---

## 5. Safety

- **Only published content is returned**: the DB entry point is limited to published versions (the `published` snapshot). Drafts, awaiting review, scheduled, and archived items cannot be fetched.
- **Expired content is not returned**: as with public pages, `unpublish_at` is checked at request time (without waiting for the scheduler to archive). A single item gives `404`, and in lists it is removed from the results (the cursor does not break even when removed). An unreadable date and time is treated as not published. The list's `total` is the DB count, so until archiving (up to 30 seconds) it includes expired items.
- **Related content is published versions only**: the targets of `relations` are looked up from the published version records, and drafts, titles being edited, and scheduled items do not appear. Whether the target kind is among the public kinds is not checked (only `id`, `content_type`, `title`, and `slug` are returned, which is information visible on the site if published).
- **Body blocks are returned as stored**: blocks such as `core/html` include the stored HTML as is (sanitization happens at render time). When a client inserts it into the DOM, it should sanitize on its own or use the sanitized `?include=html`.
- **Rate limit**: it is unauthenticated, so a per-IP limit is applied (the existing cache backend's method; to be aligned with contact intake and the like at implementation).
- **CORS**: `GET` on `/api/v1/public/` and `/api/v1/search` returns `Access-Control-Allow-Origin: *` for all origins (read-only public data). APIs that require authentication are allowed only for the origins written in `[server].cors_origins` ([configuration.md 4.3.1](configuration.md)).
- **Input**: `limit` is clamped to the limit, `cursor` is an encoding with a signature or validation and gives `400` if tampered with. Kind names and slugs go through the existing identifier validation.
- **Error messages are English only** (convention).

### 5.1 Caching

- `Cache-Control: public, max-age=<seconds>` (setting; default 60), and an `ETag` (computed from `updated_at` and the id). `If-None-Match` gives `304`.
- The same cache tags as public pages (`[cdn]`) are attached, and the existing mechanism clears them on content update, publish, and unpublish.

---

## 6. Decisions Made

1. **The default of `public` is public.** Write `public: false` only for what to hide. The per-kind allowlist (§4.1) is the first line of defense, and the bundled schemas have no problem making all fields public. When publishing products, write `public: false` on internal fields.
2. **The body returns `blocks` (the AST).** Rendered HTML only with `?include=html`. Rendering calls plugins (Wasm), so it is not included in the default cost.
3. **List ordering is the default only** (newest creation date first). Publication date order (`PublishedOrder`) waits until requested.
4. **Products are out of scope.** How to show price and inventory (for example, showing only availability without the stock count) will be designed separately when demand appears.
5. **The setting is `[headless]`.**

```toml
[headless]
enabled = false                         # disabled by default
content_types = ["article", "page"]     # kinds to publish
max_page_size = 100
cache_max_age = 60                      # Cache-Control max-age (seconds)
```

---

## 7. Test Policy

- **No leaks** (top priority): drafts, awaiting review, scheduled, archived, and published items past `unpublish_at` are all not returned in lists or singles. A kind not on the allowlist is `404`.
- **Items not returned**: `author_id`, `status`, and fields with `public: false` are absent from the response.
- **Input**: the `limit` cap, and a tampered cursor gives `400`.
- **Cache**: `ETag` and `304` for `If-None-Match`, and the `ETag` changes after an update.
- **Performance**: the delivery API adds no new queries and uses the published list (the published record's columns and indexes) as is, so it is built not to depend on the number of articles. It has not been measured as a delivery API.
- Checks using real PostgreSQL / MySQL are done in the local quality checks ([Development guide 7](development.md#7-real-database-verification-postgresql--mysql)).
