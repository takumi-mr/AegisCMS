[← Back to the documentation index](README.md)

**English** | [日本語](notices.ja.md)

# Notice Banners

Shows announcements such as maintenance or sales at the top of public pages for a set period. In Admin Studio's **Settings › Notices**, you edit the message, link, kind, display period, and whether there is a close button (no config file is needed).

```text
 Inserted into public pages by the server (regardless of the theme; also on the 404 page)
   <head> ─ <link>   /_aegis/notices.css    Styles for the built-in notices.html
          ─ <script src="/_aegis/notices.js" defer>
   Right after <body> ─ the notices' HTML   The theme's templates/notices.html (or the built-in one) rendered
```

Nothing is inserted into pages with no active notice (neither CSS nor script is loaded).

---

## 1. Notice Fields

| Field                   | Description                                                                                                                                                                           |
| ----------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Message                 | One line, up to 500 characters (HTML cannot be used; it is escaped on output)                                                                                                         |
| Link / link text        | Optional. A path starting with `/` or `http(s)://`. If the text is omitted, "Learn more" (in the `[theme].locale` language) is used                                                   |
| Kind                    | `info` / `success` / `warning` / `alert`. `warning` and `alert` get `role="alert"`; the others get `role="status"`                                                                    |
| Display period          | Start (inclusive) and end (exclusive). Either can be omitted, and an omitted side has no limit. Input on the screen is in the browser's local time and is passed to the server in UTC |
| Dismissible by visitors | `true` shows a close button                                                                                                                                                           |
| Enabled                 | `false` keeps it hidden even within the period (a draft)                                                                                                                              |

- Up to 20 can be registered, and **the first 3 in creation order are shown at the same time**.
- The status is **Showing / Scheduled / Ended / Disabled**, and the period is evaluated at each display (no restart or action is needed when a period arrives; pages already cached stay stale until the cache expires).
- A notice a visitor closed is remembered with a revision in `localStorage` (`aegis_notices`), and **editing the notice changes the revision so it appears again**. In environments where storage is unavailable, it stays closed only while viewing that page. With JavaScript disabled, no close button appears and the notice stays shown.

## 2. API

All require the `admin` role. The error's `error` is in English, and `code` is `invalid_notice` (with the invalid field in `field`) / `too_many_notices` / `notice_not_found`.

| Method   | Path                   | Description          |
| -------- | ---------------------- | -------------------- |
| `GET`    | `/api/v1/notices`      | List (with `status`) |
| `POST`   | `/api/v1/notices`      | Add (201)            |
| `PUT`    | `/api/v1/notices/{id}` | Update               |
| `DELETE` | `/api/v1/notices/{id}` | Delete (204)         |

They are stored as JSON in the site settings (`notices.items` in `_aegis_settings`), so no DB table is added, and they are included in backups. Changes are recorded in the audit log (`notice.create` / `notice.update` / `notice.delete`) and reflected to the other nodes in the cluster and the CDN cache.

## 3. Building Notices in a Theme (`templates/notices.html`)

If a theme has `templates/notices.html`, it is used instead of the built-in one (it is a fragment that does not extend `base.html`). Each element of `fields.notices` has the following values.

| Value                                 | Description                                                  |
| ------------------------------------- | ------------------------------------------------------------ |
| `id` / `rev`                          | The notice's ID and revision (used for the dismissed record) |
| `message` / `link_url` / `link_label` | The body and link (`link_label` may be empty)                |
| `style`                               | `info` / `success` / `warning` / `alert`                     |
| `dismissible`                         | Whether it can be closed                                     |

The script finds elements by the following attributes. **If you keep these, the structure and look are free.**

| Attribute                                                                            | Where                                                    |
| ------------------------------------------------------------------------------------ | -------------------------------------------------------- |
| `data-aegis-notice="{{ notice.id }}"` and `data-aegis-notice-rev="{{ notice.rev }}"` | The element for one notice                               |
| `data-aegis-notice-close`                                                            | The close button (give it `hidden`; the script shows it) |

Elements without a close button always remain. When the last one is closed, the empty parent element is removed too. You can look up the text with `t("notice.label")` / `t("notice.more")` / `t("notice.close")`. The built-in CSS (zero specificity with `:where()`) is easy to override in the theme's CSS.

The theme dev server (`aegis-server theme dev`) previews `notices.html` with sample notices.
