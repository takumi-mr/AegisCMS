[← Back to the documentation index](README.md)

**English** | [日本語](redirects.ja.md)

# Redirects

Forwards visitors who open an old URL to a new URL (301 / 302). Manage the rules in Admin Studio's **Settings › Redirects** (`admin`). When you change the slug of a published page, a rule from the old URL is kept automatically.

```text
 GET /old-page?utm=1
        │
 Look up the page ── found → shown as is (a rule never takes it over)
        │
        └─ 404 → does a rule match? ── yes → 301 / 302  Location: /articles/new?utm=1
                                       └─ no → 404 page
```

## 1. Rules

| Field    | Description                                                                                                                                                                                                                                                     |
| -------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `from`   | The path of the old URL. Start it with `/` (do not include `[server].base_path`). Query (`?`), fragment (`#`), and whitespace cannot be written. `/` itself, and anything under `/api`, the admin screen (`[server].admin_path`), and `/uploads` cannot be used |
| `to`     | The new URL. Either a path on this site starting with `/` (returned with `base_path` added), or an `http://` / `https://` URL (no user info). One starting with `//` is not allowed                                                                             |
| `status` | `301` (permanent; default) or `302` (temporary)                                                                                                                                                                                                                 |

- **How matching works**: URL paths are compared after percent-decoding, and a trailing `/` is ignored (`/old/` and `/old` are the same; `/%E8%A8%98%E4%BA%8B` is `/記事`). Case is distinguished.
- **It is used only when the page is a 404.** Even if you put a rule on the URL of a live page (article, static page, product, category, etc.), that page is displayed. The rule starts working after the page is removed, so adding one beforehand is safe. Only `GET` / `HEAD` are covered.
- **The query string is carried over** (such as `?utm_source=x`). However, if `to` contains `?`, the one in `to` is used. A `#fragment` from `to` is appended last.
- **Loops cannot be created**: a rule that follows rules back to its `from` (A → B, B → A) or redirects to itself is refused (`400 redirect_loop`). There is only one rule per `from` (`409 duplicate_redirect`).
- **The limit is 2,000 rules.**
- **Cache-Control**: 301 gets `public, max-age=3600`, and 302 gets `no-cache`.

## 2. Automatic Rules When a Slug Changes

When a published URL changes, a 301 from the old URL to the new one is added automatically (in Admin Studio it gets an "auto" mark). **The URL visitors use is the published version's slug**, so this works in these cases:

- When you **publish** (`publish: true`) in the edit screen and the published version's slug changes
- When the scheduled publication time arrives and the published version's slug changes

If you only save (the published version is not switched), nothing happens, because the old URL is still published. It is also not created when you revert to draft or archive (because the new URL would be a 404 too).

| Kind                                | Old URL → new URL                                          |
| ----------------------------------- | ---------------------------------------------------------- |
| Article (`article`)                 | `/articles/{old}` → `/articles/{new}`, `/{old}` → `/{new}` |
| Static page (`page`)                | `/{old}` → `/{new}`                                        |
| Product (`[commerce].product_type`) | `/products/{old}` → `/products/{new}`                      |

- **No chains are created**: even if you change the slug repeatedly, all old URLs point to the latest URL (`a → c`, `b → c`, not `a → b`, `b → c`). When you return to the original slug, the rules whose `from` is a URL that is live now are removed.
- When the count is at the limit, the oldest automatic rule is removed to make room (rules made by hand are not removed).
- If you edit an automatic rule in Admin Studio, it becomes a manually managed rule.
- Even if saving a rule fails, saving or publishing the content is not stopped (a warning is logged).

## 3. API

All require `admin` permission. Errors are in English and carry a machine-readable `code`.

| API                                       | Description                                                                                                                                                          |
| ----------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `GET /api/v1/redirects?q=&limit=&offset=` | A list (newest first) and the total `{"redirects": [...], "total": 12}`. `q` is a substring match (case-insensitive) on `from` / `to`. `limit` is 1–200 (default 50) |
| `POST /api/v1/redirects`                  | Add with `{"from": "/old", "to": "/new", "status": 301}` (`201`)                                                                                                     |
| `PUT /api/v1/redirects/{id}`              | Update with the same shape                                                                                                                                           |
| `DELETE /api/v1/redirects/{id}`           | Delete (`204`)                                                                                                                                                       |

Mistakes are `400 {"error", "code": "invalid_redirect", "field": "from" | "to" | "status"}`, `409 duplicate_redirect`, `400 redirect_loop`, `400 too_many_redirects`, and `404 redirect_not_found`. Operations are recorded in the audit log (`redirect.create` / `redirect.update` / `redirect.delete`; automatic rules have `details.auto = true`).

## 4. Storage and Multiple Instances

The rules are in the DB settings (`redirects.rules` in `_aegis_settings`, as JSON) and included in backups. They are loaded into memory at startup, and when changed, the event bus (`[cache]`) notifies the other servers, which reload from the DB.

## 5. CDN

No `Cache-Tag` is added to redirect responses. After you change a rule, a 301 may remain in browsers and CDNs for up to 1 hour (`max-age=3600`). If you want to remove a 301 you added by mistake, first change it to a 302 and watch what happens.
