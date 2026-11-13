[← Back to the documentation index](README.md)

**English** | [日本語](content-api.ja.md)

# Content Posting API (Content Management API)

A REST API guide for automatically posting, editing, and publishing content such as articles and static pages to AegisCMS from external tools, CI/CD pipelines, static-site integrations, and CLI scripts.

Unlike the [headless delivery API](headless.md) (read-only), which fetches only public data without authentication, this one uses an authentication token (Bearer token) to create, save as draft, update, publish immediately, schedule, tag, and delete content.

---

## 1. Authentication & Roles

All content creation and update APIs need the `Authorization: Bearer <token>` header.

### 1.1 Logging In and Getting a Token

First, issue a session with a registered account.

```bash
curl -X POST "http://localhost:8080/api/v1/auth/session" \
  -H "Content-Type: application/json" \
  -d '{
    "username": "editor_user",
    "password": "your-secure-password"
  }'
```

**Success response (200 OK):**

```json
{
  "token": "sess_01HZX...",
  "user": {
    "id": "u-admin-1",
    "username": "editor_user",
    "role": "editor"
  }
}
```

In later requests, put the obtained token in the header:

```http
Authorization: Bearer sess_01HZX...
```

### 1.2 Roles and Permitted Operations

The roles required for each operation are as follows (for details see [Authentication & user management](authentication.md)).

| Operation                                                                      | Required role           | Notes                                                                                              |
| ------------------------------------------------------------------------------ | ----------------------- | -------------------------------------------------------------------------------------------------- |
| Creating a draft (`draft`) / awaiting review (`pending`)                       | `contributor`           | Only content the user is the author of (note: `reviewer` cannot create new content)                |
| Creating new as published (`published`) / scheduled (`scheduled`)              | `editor` and above      | Immediate publication or publication at a specified time                                           |
| Proofreading, approving, and sending back articles awaiting review (`pending`) | `reviewer` and above    | `reviewer` can only touch up `pending`, approve (`published`/`scheduled`), and send back (`draft`) |
| Freely changing existing content and publishing directly                       | `editor` and above      | `contributor` can edit only their own `draft` / `pending`                                          |
| Specifying another user as the author (`author_id`)                            | `admin` only            | If omitted, the logged-in user is the author                                                       |
| Deleting content                                                               | `editor` and above      | `reviewer` and `contributor` cannot delete                                                         |
| Attaching tags (`PUT .../tags`)                                                | `contributor` and above | `reviewer` can adjust the tags of `pending` articles                                               |

> [!TIP]
> **Best practices for LLM multi-agent automatic posting**
>
> - **Writing agent**: use an account with the `contributor` role and submit articles with `status: "pending"` (a safe sandbox where nothing gets published on its own).
> - **Proofreading/auditing agent**: use an account with the `reviewer` role, inspect and proofread the submitted `pending` articles, approve with `status: "published"` if there is no problem, or send back to `status: "draft"` with a `review_note` if corrections are needed. `reviewer` has no permission to create or delete, so even in case of malfunction or prompt injection, the existing site is not destroyed.

---

## 2. Creating an Article (`POST /api/v1/contents/{content_type}`)

Creates an article of the specified content type (such as `article` or `page`).

- **Method**: `POST`
- **Path**: `/api/v1/contents/{content_type}` (e.g. `/api/v1/contents/article`)
- **Headers**:
    - `Authorization: Bearer <token>`
    - `Content-Type: application/json`

### 2.1 Request Body (JSON)

```json
{
  "title": "A Guide to Posting Articles via the AegisCMS API",
  "slug": "posting-via-api-guide",
  "status": "published",
  "blocks": [
    {
      "name": "core/paragraph",
      "attributes": {},
      "text": "This article was posted automatically via the API.",
      "children": []
    }
  ],
  "published_at": "2026-11-10T15:00:00Z",
  "fields": {
    "excerpt": "An explanation of the steps to create and publish articles with the API.",
    "category_id": "cat-tech"
  }
}
```

### 2.2 Parameter Specification

| Field          | Type    | Required     | Default          | Description                                                                                                                                                      |
| -------------- | ------- | ------------ | ---------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `title`        | string  | **Required** | -                | The article's title.                                                                                                                                             |
| `slug`         | string  | **Required** | -                | URL slug (alphanumerics, hyphens, etc.). Unique within the same `content_type`.                                                                                  |
| `status`       | string  | Optional     | `"draft"`        | Publication status. One of `"draft"`, `"pending"`, `"published"`, `"scheduled"`, `"archived"`.                                                                   |
| `blocks`       | array   | Optional     | `[]`             | An array of body blocks (see [3. Body block specification](#3-body-block-specification-blocks) below).                                                           |
| `author_id`    | string  | Optional     | (logged-in user) | The author's user ID. Only `admin` can specify someone other than the logged-in user.                                                                            |
| `published_at` | string  | Conditional  | `null`           | Publication date and time in RFC 3339 format (e.g. `2026-11-10T12:00:00Z`). Required when `status: "scheduled"`.                                                 |
| `unpublish_at` | string  | Optional     | `null`           | Scheduled unpublish (archive) date and time in RFC 3339 format. Can be specified only with `published` or `scheduled` (must be later than the publication time). |
| `publish`      | boolean | Optional     | `false`          | A flag that explicitly switches the published version pointer to the latest working version.                                                                     |
| `review_note`  | string  | Optional     | `null`           | A note for review requests, approvals, and send-backs. Recorded in the audit log and the version history.                                                        |
| `fields`       | object  | Optional     | `{}`             | Custom fields defined in the schema (`schemas/*.yaml`) (summary, category, member access rights, etc.).                                                          |

> [!IMPORTANT]
> **Note on where to write custom fields**  
> Always nest the custom fields defined in the schema (such as `excerpt` and `category_id`) under `"fields": { ... }`. Writing an unknown key directly at the top level returns `400 Bad Request` (`unknown field`).

### 2.3 Status Kinds

- `"draft"`: a draft. Not shown on the public site, and viewable only in the admin screen and the editing API.
- `"pending"`: awaiting review. The state where a `contributor` has finished writing and asked an editor for approval and publication.
- `"published"`: published. Reflected immediately on the public site and the headless delivery API.
- `"scheduled"`: scheduled publication. At the time specified in `published_at`, the background scheduler switches it to `published` automatically.
- `"archived"`: archived (unpublished). The state where it was published in the past but has now been withdrawn.

---

## 3. Body Block Specification (`blocks`)

AegisCMS manages the article body not as inline HTML but as an array of structured AST blocks (`BlockNode`) (for details see the [Body blocks & Markdown reference](blocks.md)).

```json
{
  "name": "core/paragraph",
  "attributes": {},
  "text": "Paragraph text",
  "children": []
}
```

### Representative Blocks

#### Paragraph (`core/paragraph`)

```json
{
  "name": "core/paragraph",
  "attributes": {},
  "text": "This is a paragraph. Inline formatting such as **bold**, *italic*, and [links](https://example.com) can be used.",
  "children": []
}
```

#### Heading (`core/heading`)

Specify 1–6 with `attributes.level` (default 2). The heading's `id` is assigned automatically on save.

```json
{
  "name": "core/heading",
  "attributes": { "level": 2 },
  "text": "Title of a major heading",
  "children": []
}
```

#### Code Block (`core/code`)

```json
{
  "name": "core/code",
  "attributes": {
    "language": "rust",
    "filename": "src/main.rs"
  },
  "text": "fn main() {\n    println!(\"Hello, AegisCMS!\");\n}",
  "children": []
}
```

#### Unordered List (`core/list`)

The children are `core/list-item`s in a row.

```json
{
  "name": "core/list",
  "attributes": { "ordered": false },
  "children": [
    { "name": "core/list-item", "attributes": {}, "text": "First item", "children": [] },
    { "name": "core/list-item", "attributes": {}, "text": "Second item", "children": [] }
  ]
}
```

#### Image (`core/image`)

```json
{
  "name": "core/image",
  "attributes": {
    "src": "/uploads/media/2026/11/architecture.png",
    "alt": "Architecture diagram"
  },
  "children": []
}
```

#### Note / Callout (`core/callout`)

Choose `variant` from `"note"`, `"tip"`, `"important"`, `"warning"`, and `"caution"`.

```json
{
  "name": "core/callout",
  "attributes": {
    "variant": "tip",
    "title": "Tip"
  },
  "text": "When posting through the API, mind the token's expiry.",
  "children": []
}
```

#### Table (`core/table`)

```json
{
  "name": "core/table",
  "attributes": {
    "header": true,
    "align": ["left", "center", "right"],
    "rows": [
      ["Feature", "Status", "Notes"],
      ["API posting", "Stable", "RESTful"],
      ["Saving drafts", "Stable", "Automatic version management"]
    ]
  },
  "children": []
}
```

#### Members-Only Divider (`core/paywall`)

Blocks after this are shown only to members with the permission ([Member accounts](members.md)).

```json
{
  "name": "core/paywall",
  "attributes": {},
  "children": []
}
```

---

## 4. Custom Fields and Metadata (`fields`)

In the `fields` object, set the fields defined in the target schema (`schemas/{content_type}.yaml`).

```json
"fields": {
  "excerpt": "An excerpt used for article lists and SNS sharing.",
  "category_id": "cat-tech",
  "access": "public",
  "custom_meta": {
    "seo": {
      "canonical_url": "https://example.com/original-article"
    }
  }
}
```

- **`excerpt`**: the article's summary (used in list screens, RSS/Atom feeds, and SEO meta tags).
- **`category_id`**: the ID of the category it belongs to (when a relation to categories is defined).
- **`access`**: the members-only setting (`"public"` (everyone), `"free"` (free members and above), `"paid"` (paid members only)). A value not in the settings gives `400 Bad Request`.
- **`custom_meta`**: arbitrary structured JSONB metadata.

---

## 5. Uploading Images and Media

To insert an image into an article, first call the media upload API to get the storage key and URL (for details see [Media management](media.md)).

### Upload API

- **Method**: `POST`
- **Path**: `/api/v1/storage/upload`
- **Format**: `multipart/form-data`
- **Field name**: `file`

```bash
curl -X POST "http://localhost:8080/api/v1/storage/upload" \
  -H "Authorization: Bearer <token>" \
  -F "file=@./screenshot.png"
```

**Success response (201 Created):**

```json
{
  "key": "media/2026/11/screenshot.png",
  "url": "/uploads/media/2026/11/screenshot.png",
  "size": 1048576,
  "content_type": "image/png"
}
```

Set the obtained `url` in the `core/image` block's `attributes.src`.

---

## 6. Attaching Tags (`PUT /api/v1/contents/{content_type}/{slug}/tags`)

To attach tags to a posted article, call the tag update API (for details see [Tags](tags.md)).

- **Method**: `PUT`
- **Path**: `/api/v1/contents/{content_type}/{slug_or_id}/tags`
- **Request body**:
    ```json
    {
      "tag_ids": [
        "tag-rust-uuid",
        "tag-api-uuid"
      ]
    }
    ```

Note: the list of existing tags can be fetched with `GET /api/v1/tags`. To register a new tag, run `POST /api/v1/tags`.

---

## 7. Updating and Deleting Articles

### 7.1 Updating an Article (`PUT /api/v1/contents/{content_type}/{slug_or_id}`)

Overwrites the article's contents. The body and fields are replaced entirely with the new values (custom fields you did not send are deleted).

- **Method**: `PUT`
- **Path**: `/api/v1/contents/{content_type}/{slug_or_id}`
- **Request body**: the same JSON payload as for creation.

> [!NOTE]
> Every time you update an article, a new revision (version) is saved automatically. You can restore to a past revision from the admin screen.

### 7.1a Getting the Content of a Version (`GET /api/v1/contents/{content_type}/{slug_or_id}/versions/{revision}`)

Returns the content of one saved version (`title`, `slug`, `blocks`, `fields`, and the version's `revision`, `created_at`, `created_by`, `change_note`). It changes nothing. As with the version list (`.../versions`), it can be fetched when logged in. `status`, the publication time, and the author are not returned, because they are not brought back by a restore either (they stay at their current values). A version that does not exist is 404.

The admin screen calls this from "Review" in the version history and shows a preview of that version and a diff against what is in the editor now (title, slug, custom fields, and the body's Markdown line by line).

### 7.2 Deleting an Article (`DELETE /api/v1/contents/{content_type}/{slug_or_id}`)

Deletes the specified article (`editor` or higher permission is required).

- **Method**: `DELETE`
- **Path**: `/api/v1/contents/{content_type}/{slug_or_id}`
- **On success**: `204 No Content` or `200 OK`

---

## 8. Implementation Code Examples

### 8.1 Posting Steps with cURL

```bash
# 1. Log in and get a token
TOKEN=$(curl -s -X POST "http://localhost:8080/api/v1/auth/session" \
  -H "Content-Type: application/json" \
  -d '{"username": "admin", "password": "your-password"}' | jq -r .token)

# 2. Post a new article
curl -i -X POST "http://localhost:8080/api/v1/contents/article" \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "title": "API posting test",
    "slug": "api-post-test",
    "status": "published",
    "blocks": [
      {
        "name": "core/heading",
        "attributes": { "level": 2 },
        "text": "Overview of automatic posting",
        "children": []
      },
      {
        "name": "core/paragraph",
        "attributes": {},
        "text": "This article was posted automatically from a cURL script.",
        "children": []
      }
    ],
    "fields": {
      "excerpt": "A test of posting via the API."
    }
  }'
```

### 8.2 Python Script Example

```python
import requests

BASE_URL = "http://localhost:8080/api/v1"

# 1. Log in and get a session token
login_res = requests.post(f"{BASE_URL}/auth/session", json={
    "username": "editor_user",
    "password": "your-password"
})
login_res.raise_for_status()
token = login_res.json()["token"]

headers = {
    "Authorization": f"Bearer {token}",
    "Content-Type": "application/json"
}

# 2. Article creation request
article_payload = {
    "title": "An article posted from Python",
    "slug": "python-auto-post",
    "status": "published",
    "blocks": [
        {
            "name": "core/heading",
            "attributes": {"level": 2},
            "text": "Introduction",
            "children": []
        },
        {
            "name": "core/paragraph",
            "attributes": {},
            "text": "This was posted using Python's requests library.",
            "children": []
        },
        {
            "name": "core/code",
            "attributes": {"language": "python", "filename": "post.py"},
            "text": "print('Hello, AegisCMS!')",
            "children": []
        }
    ],
    "fields": {
        "excerpt": "A sample of automatic posting with a Python script."
    }
}

res = requests.post(f"{BASE_URL}/contents/article", headers=headers, json=article_payload)
if res.status_code == 201:
    article = res.json()
    print(f"Article created: ID={article['id']}, URL=/{article['slug']}")
else:
    print(f"Creation failed ({res.status_code}): {res.text}")
```

### 8.3 TypeScript / Node.js Script Example

```typescript
interface BlockNode {
  name: string;
  attributes?: Record<string, unknown>;
  text?: string;
  children?: BlockNode[];
}

async function publishArticle() {
  const BASE_URL = 'http://localhost:8080/api/v1';

  // 1. Log in
  const sessionRes = await fetch(`${BASE_URL}/auth/session`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({
      username: 'editor_user',
      password: 'your-password'
    }),
  });

  if (!sessionRes.ok) {
    throw new Error(`Login failed: ${sessionRes.statusText}`);
  }
  const { token } = await sessionRes.json();

  // 2. Create the article
  const payload = {
    title: 'An article auto-posted with TypeScript',
    slug: 'typescript-auto-post',
    status: 'published',
    blocks: [
      {
        name: 'core/heading',
        attributes: { level: 2 },
        text: 'TypeScript integration',
        children: [],
      },
      {
        name: 'core/paragraph',
        attributes: {},
        text: 'You can create articles directly from Node.js / Bun.',
        children: [],
      },
    ] as BlockNode[],
    fields: {
      excerpt: 'A sample posting script in TypeScript.',
    },
  };

  const createRes = await fetch(`${BASE_URL}/contents/article`, {
    method: 'POST',
    headers: {
      'Authorization': `Bearer ${token}`,
      'Content-Type': 'application/json',
    },
    body: JSON.stringify(payload),
  });

  if (createRes.status === 201) {
    const data = await createRes.json();
    console.log('Posted:', data.id, data.slug);
  } else {
    const error = await createRes.json();
    console.error('Post error:', createRes.status, error);
  }
}

publishArticle();
```

---

## 9. Error Handling

AegisCMS's REST API returns standard HTTP status codes and English error messages (JSON) on errors.

```json
{
  "error": "Unpublish date ('unpublish_at') must be after publication date"
}
```

### Main Status Codes

| Status code                 | Meaning                   | Main causes and fixes                                                                                                                                                                                                                                                |
| --------------------------- | ------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `400 Bad Request`           | Malformed request         | - A required field (`title`, `slug`) is missing<br>- An unknown key is at the top level (put custom fields inside `fields`)<br>- The `published_at` or `unpublish_at` date format (RFC 3339) is invalid<br>- An undefined membership level was specified in `access` |
| `401 Unauthorized`          | Authentication error      | - No `Authorization` header, or the token is invalid or expired                                                                                                                                                                                                      |
| `403 Forbidden`             | Insufficient permission   | - A `contributor` tried to create directly as `published` (only `draft` or `pending` allowed)<br>- A user other than `admin` specified someone else's `author_id`<br>- Tried to edit someone else's content                                                          |
| `404 Not Found`             | Resource does not exist   | - No schema is registered for the specified `content_type`<br>- The article to update does not exist                                                                                                                                                                 |
| `409 Conflict`              | Conflict                  | - The specified `slug` is already used within the same `content_type`                                                                                                                                                                                                |
| `422 Unprocessable Entity`  | Validation / hook refusal | - Saving was refused by the `on-before-save` hook of a registered Wasm plugin                                                                                                                                                                                        |
| `500 Internal Server Error` | Internal server error     | - A database error or unexpected failure                                                                                                                                                                                                                             |

---

## 10. Automatic Processing After Publication (Side Effects)

When an article is created or updated with the `published` status, the following is run automatically inside the server.

1. **Cache invalidation**:
    - The CDN / internal cache of the article's page on the public site, list pages, and feeds (`/feed.atom`) is discarded automatically.
2. **Updating the full-text search index (`aegis-search`)**:
    - The tantivy search engine's index queue is notified, and it is reflected in search results immediately ([Site search](search.md)).
3. **Calling plugin hooks**:
    - Wasm hooks such as `on-content-published` that plugins have registered fire automatically ([Plugin hook specification](plugin_hooks.md)).
4. **Recording the operation audit log (`aegis-audit`)**:
    - A `content.create` or `content.update` event is recorded in the audit log with the executing user and IP address ([Audit log](audit.md)).
