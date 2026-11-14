[← Back to the documentation index](README.md)

**English** | [日本語](plugin_abi_specification.ja.md)

# AegisCMS WebAssembly Plugin Common ABI Specification

This document is the language-independent standard specification that defines the low-level ABI (Application Binary Interface), memory model, host API, lifecycle hooks, and data exchange conventions exchanged between AegisCMS's zero-trust WebAssembly plugins and the host runtime (`aegis-runtime`).

It is the standard to follow when implementing plugins and SDKs not only in Rust but also in any WebAssembly-capable language such as **AssemblyScript**, **TinyGo**, **C/C++**, and **Zig**.

---

## 1. Architecture and Execution Model

AegisCMS's plugin system uses the Bytecode Alliance's WebAssembly runtime `Wasmtime` as the host engine.

- **Sandbox isolation**: a plugin is completely isolated inside the guest's 32-bit linear memory space and cannot directly access the host's physical memory or OS resources.
- **Complete instance separation**: an independent `Store` and `Instance` are created for each hook call, preventing state contamination and memory sharing between plugins (use the host-provided KVS for persistent state).
- **Zero-trust execution control**:
    - **Fuel (CPU quota)**: strictly limits the number of instructions usable in each hook call (against infinite loops and DoS).
    - **Asynchronous timeout**: a hook call exceeding the specified milliseconds (e.g. 250 ms) is forcibly terminated by the host.
    - **Capability guard**: network and storage access not declared in `plugin.manifest.json` is blocked immediately when the host API is called.

---

## 2. Memory Management & Pass-by-Value Conventions

### 2.1 WebAssembly Memory Export

A plugin module must always export its own linear memory under the name `"memory"`.

```wat
(export "memory" (memory 0))
```

### 2.2 Memory Allocator Export (`allocate`)

So the host can secure a region to write strings and JSON data into the plugin, the plugin must always export an allocation function with the following signature.

```text
allocate(len: i32) -> i32
```

- **Argument**:
    - `len` (i32): the byte size to allocate.
- **Return value**:
    - The starting offset address on the guest linear memory of the allocated region (`ptr`: i32).
    - Returns `0` if allocation fails, such as from memory exhaustion.

### 2.3 Steps for Passing Data from Host to Guest

1. The host calls the plugin's exported function `allocate(len)` and has it secure a buffer of the required byte length `len` in guest memory.
2. Starting from the returned starting address `ptr`, the host writes the UTF-8-encoded JSON bytes directly into guest memory.
3. The host calls the target lifecycle hook function (e.g. `on_before_save(ptr, len)`).
4. The guest reads `len` bytes from `ptr` and deserializes the JSON. Releasing the input buffer after use, or leaving it to GC, is the guest's responsibility.

### 2.4 Returning Results from Guest to Host (Fat Pointer Packing)

Lifecycle hook functions pack the transformed data or execution result into a single **64-bit signed integer (`i64`)** and return it to the host.

#### Packing Rule

- **Upper 32 bits**: the starting pointer of the guest memory holding the result data (`ptr` as i32)
- **Lower 32 bits**: the byte length of the result data (`len` as u32)

```text
packed_result (i64) = ((ptr as i64) << 32) | ((len as u32) as i64)
```

#### Packing Implementation Examples by Language

- **Rust**:
    ```rust
    ((ptr as i64) << 32) | ((len as u32) as i64)
    ```
- **AssemblyScript**:
    ```typescript
    ((ptr as i64) << 32) | ((len as i64) & 0xFFFFFFFF)
    ```
- **TinyGo / Go**:
    ```go
    (int64(ptr) << 32) | (int64(uint32(length)) & 0xFFFFFFFF)
    ```

#### The Host's Unpacking

From the received `i64`, the host restores the pointer with `(packed >> 32) as i32` and the byte length with `packed as u32`, and reads the result bytes from guest memory.

---

## 3. Host API Import Specification (the `host` Module)

A plugin can use the functionality the host environment provides through the following import signatures. The module name is `"host"` for all of them.

**The only things that can be imported are the following functions of the `host` module.** A module that imports anything else (`env.abort`, `wasi_snapshot_preview1.*`, etc.) is rejected at load time.

- AssemblyScript: replace the default `env.abort` with the SDK's implementation using `"use": ["abort=assembly/sdk/host/abort"]` in `asconfig.json`.
- TinyGo: build with `-target=wasm-unknown` (the `wasi` target imports WASI).

If a module exports `_initialize` (no arguments or return value), the host calls it right after instantiation, before any other export (the WASI reactor-style convention; TinyGo's `wasm-unknown` initializes the runtime and global variables here).

### 3.1 `host.log`

Safely outputs logs through the host's structured logging facility (`tracing`).

```text
(import "host" "log" (func (param i32 i32 i32)))
```

- **Arguments**:
    - `level` (i32): the log level
        - `0`: DEBUG
        - `1`: INFO
        - `2`: WARN
        - `3` (or anything else): ERROR
    - `ptr` (i32): the starting pointer of the log message string (UTF-8)
    - `len` (i32): the byte length of the log message string

### 3.2 `host.http_check_allowed`

The host verifies whether the destination URL is allowed by `capabilities.network.allowed_domains` in the plugin manifest.

```text
(import "host" "http_check_allowed" (func (param i32 i32) (result i32)))
```

- **Arguments**:
    - `ptr` (i32): the starting pointer of the URL string to verify (UTF-8)
    - `len` (i32): the byte length of the URL string
- **Return value** (i32):
    - `1`: allowed (Allowed)
    - `0`: denied (Denied / invalid URL)

### 3.3 `host.kv_set`

Persists and updates data in the Key-Value store under the plugin's own name (the host prefixes the key with the plugin name). The use of KV must be declared by `capabilities.storage.kv_namespace` in the manifest.

```text
(import "host" "kv_set" (func (param i32 i32 i32 i32) (result i32)))
```

- **Arguments**:
    - `k_ptr` (i32): the starting pointer of the key string (UTF-8)
    - `k_len` (i32): the byte length of the key string
    - `v_ptr` (i32): the starting pointer of the value string (UTF-8)
    - `v_len` (i32): the byte length of the value string
- **Return value** (i32):
    - `0`: success (Ok)
    - `1`: insufficient permission (Denied: storage permission not declared in the manifest)
    - `2`: error (Internal Error: memory access failure or lock contention)
- The value is stored in the server's `[cache]` backend (with `redis` it remains after a restart).

### 3.4 Common Convention for Imports That Return Results to Guest Memory

`kv_get` / `http_fetch` / `config_get` do not know the length of the result in advance, so the host calls the guest's `allocate` export to secure a region, writes the UTF-8 result into it, and returns an `i64` of `(ptr << 32) | len`. The guest frees the region when done reading (the Rust SDK's `abi::read_bytes_from_host`). `0` means "no value", and a negative value is an error.

### 3.5 `host.kv_get`

```text
(import "host" "kv_get" (func (param i32 i32) (result i64)))
```

- **Arguments**: the `ptr`, `len` of the key string (UTF-8) (the host adds the namespace)
- **Return value** (i64): the value's `(ptr << 32) | len`; `0` if unset; `-1` for insufficient permission (storage not declared); `-2` for an error

### 3.6 `host.http_fetch`

Sends an HTTP request to a destination permitted by `allowed_domains`.

```text
(import "host" "http_fetch" (func (param i32 i32) (result i64)))
```

- **Arguments**: the `ptr`, `len` of the request JSON
    ```json
    { "method": "POST", "url": "https://hooks.slack.com/services/…",
      "headers": { "Content-Type": "application/json" }, "body": "{\"text\":\"hi\"}" }
    ```
- **Return value** (i64): the `(ptr << 32) | len` of the response JSON `{"status": 200, "headers": {"content-type": "…"}, "body": "…"}`, or on failure `{"error": "<reason>"}` (`0` if it could not be written)
- **Limits**: the methods are GET / POST / PUT / PATCH / DELETE; the body is 1 MiB for both sending and receiving; 10 seconds per call; up to 8 calls per hook invocation. The waiting time is included in the hook's `execution_timeout_ms`
- **Destination restrictions**: only `http` / `https`, and ports only 443 for `https` and 80 or 8080 for `http`. It does not connect to internal IP addresses (loopback, private, link-local, etc., including where host names resolve to). The host follows redirects up to 3 times and checks the allowed domains and the above restrictions **at each destination** (moving to a destination that is not allowed is an error)

### 3.7 `host.config_get`

Reads values from the server's `[plugins.settings.<plugin name>]`.

```text
(import "host" "config_get" (func (param i32 i32) (result i64)))
```

- **Arguments**: the `ptr`, `len` of the key string
- **Return value** (i64): the value's `(ptr << 32) | len` (strings as is, anything else as JSON); `0` if unset

---

## 4. Lifecycle Hook Specification (Guest Export Functions)

A plugin exports the following functions corresponding to the hooks it wants to run. They are called by the host only when the corresponding hook name (kebab-case) is declared in the manifest's `capabilities.hooks`.

| Manifest hook name        | Wasm export function name | Arguments              | Return | Role                                                                            |
| ------------------------- | ------------------------- | ---------------------- | ------ | ------------------------------------------------------------------------------- |
| `on-before-save`          | `on_before_save`          | `(ptr: i32, len: i32)` | `i64`  | Validation, value correction, and save refusal right before an article is saved |
| `transform-content-ast`   | `transform_content_ast`   | `(ptr: i32, len: i32)` | `i64`  | Transforming and decorating the block AST array                                 |
| `on-render-page`          | `on_render_page`          | `(ptr: i32, len: i32)` | `i64`  | Dynamic changes to the public page rendering context                            |
| `on-user-authenticated`   | `on_user_authenticated`   | `(ptr: i32, len: i32)` | `i64`  | Role mapping after a successful login, supplementing attributes, refusing login |
| `calculate-order`         | `calculate_order`         | `(ptr: i32, len: i32)` | `i64`  | Adding shipping, tax, discounts; refusing an order                              |
| `on-order-status-changed` | `on_order_status_changed` | `(ptr: i32, len: i32)` | `i64`  | Notification of order creation and status changes                               |
| `send-notification`       | `send_notification`       | `(ptr: i32, len: i32)` | `i64`  | A notification delivery channel (Slack, SMS, etc.)                              |
| `on-content-published`    | `on_content_published`    | `(ptr: i32, len: i32)` | `i64`  | Notification when content is published, such as by scheduled publication        |
| `check-comment`           | `check_comment`           | `(ptr: i32, len: i32)` | `i64`  | Comment moderation ([4.8.1](#481-check_comment))                                |
| `render-blocks`           | `render_blocks`           | `(ptr: i32, len: i32)` | `i64`  | HTML for custom body blocks ([4.9](#49-render_blocks))                          |

### 4.1 `on_before_save`

Called right before an article is saved to the database.

- **Input**: a UTF-8 JSON-encoded `PostContext`
- **Return value**:
    - On success: the modified `PostContext` as UTF-8 JSON, packed into an `i64`
    - To refuse saving: the error envelope `{"error": "<refusal reason message>"}` as UTF-8 JSON, packed into an `i64`
- **Behavior on refusal**:
    - When an error envelope is returned, the host immediately aborts saving the article, and the API returns `422 Unprocessable Entity`.

### 4.2 `transform_content_ast`

Transforms the article body's block AST array. It runs on save and on the admin screen's real-time preview.

- **Input**: a UTF-8 JSON-encoded `BlockNode[]` array
- **Return value**: the transformed `BlockNode[]` array as UTF-8 JSON, packed into an `i64`
- **Notes**:
    - If you do not change the block array, you can pack and return the `(ptr, len)` passed in as is.
    - It runs on every preview, so do not give it external side effects (HTTP requests or KVS writes).

### 4.3 `on_render_page`

Called right before rendering a public page (`GET /`, `GET /articles/{slug}`) or the whole preview.

- **Input**: a UTF-8 JSON-encoded `PageRenderContext`
- **Return value**: the modified `PageRenderContext` as UTF-8 JSON, packed into an `i64`
- **Notes**:
    - You can dynamically update the site title (`site_title`), extra CSS (`extra_css`), extra header (`extra_head`), and template variables (`variables`).

### 4.4 `on_user_authenticated`

Called after credentials are verified at login (password, OpenID Connect) and before the session is issued.

- **Input**: a UTF-8 JSON-encoded `UserAuthContext` (`provider`, `user_id`, `email`, `display_name`, `role`, `metadata`; for details see [Plugin hooks](plugin_hooks.md#23-userauthcontext))
- **Return value**:
    - On success: the modified `UserAuthContext` as UTF-8 JSON, packed into an `i64` (`role` / `display_name` / `email` / `metadata` are reflected in the session; changes to `provider` / `user_id` are ignored)
    - To refuse login: the error envelope `{"error": "<refusal reason message>"}`. The API returns `403 Forbidden`
- **Notes**:
    - `role` is one of `admin` / `editor` / `viewer`. Anything else, and traps and timeouts, also make the login fail (erring on the safe side).

### 4.5 `calculate_order`

Called when the e-commerce cart is quoted and when an order is confirmed ([Plugin hooks 1.5](plugin_hooks.md#15-calculate-order)).

- **Input**: the JSON of `OrderContext` ([Plugin hooks 2.5](plugin_hooks.md#24-ordercontext--orderstatuschange))
- **Return value**: the modified `OrderContext` (only `adjustments` and `metadata` are reflected; returning only `{"adjustments": [...]}` is fine), or an error envelope that refuses the order `{"error": "<reason to show the customer>"}`

### 4.6 `on_order_status_changed`

Called after an order is created or its status changes.

- **Input**: the JSON of `OrderStatusChange`
- **Return value**: `0` (success), or `{"error": "..."}` (it only remains in the log and audit log and does not affect the order)

### 4.7 `send_notification`

Called on every notification for a plugin that has `"plugin:<name>"` written in `[notifications].channels` ([Plugin hooks 1.7](plugin_hooks.md#17-send-notification)).

- **Input**: the JSON of `NotificationMessage`

```json
{
  "kind": "audit.session.create",
  "recipient": { "type": "staff" },
  "subject": "[Shop] Login locked (root)",
  "body": "Login locked (root)\n\nTime: ...",
  "html": "<!DOCTYPE html>...",
  "data": { "audit": { "action": "session.create", "outcome": "failure", "details": { "locked": "username" } }, "site_name": "Shop" }
}
```

`recipient` is `{"type": "staff"}` or `{"type": "person", "email": "hanako@example.com", "name": "Hanako"}`. `html` is included only when `[notifications].html = true` (make sure you can treat it as absent).

- **Return value**:

| Return value                           | Meaning                                                                                       |
| -------------------------------------- | --------------------------------------------------------------------------------------------- |
| `0`                                    | Sent                                                                                          |
| `{"skipped": true}`                    | A notification it does not handle (not a failure; not left in the audit log either)           |
| `{"error": "...", "permanent": false}` | A transient failure. The host retries at intervals (the same if `permanent` is omitted)       |
| `{"error": "...", "permanent": true}`  | A failure that would not change on retry (a configuration defect, the other side's 4xx, etc.) |

### 4.8 `on_content_published`

Called right after content becomes published, such as by scheduled publication.

- **Input**: the JSON of `PublishedContent` ([5.5](#55-publishedcontent))
- **Return value**: `0` (success), or `{"error": "..."}` (it only remains in the log and the audit log `plugin.error` and does not affect the publishing process)

### 4.8.1 `check_comment`

Called before a comment posted from the public site is saved ([Plugin hooks 1.9](plugin_hooks.md#19-check-comment)).

- **Input**: the JSON of `CommentSubmission` (`content_type`, `content_id`, `content_title`, `content_slug`, `parent_id`, `author_name`, `author_email`, `body`, `ip`, `user_agent`, `status`)
- **Return value**: `{"verdict": "allow" | "hold" | "spam", "reason": "..."}` (returning nothing, or no `verdict`, means `allow`), or an error envelope that refuses the post `{"error": "<reason to show the poster>"}`

---

### 4.9 `render_blocks`

Turns the custom blocks declared in the manifest's `blocks` into HTML. It is called once per plugin on each render, such as of a public page.

- **Input**: `{"blocks": [BlockNode, ...]}` (this plugin's blocks, in document order)
- **Return value**: `{"html": [string | null, ...]}` (in the same order as the input; `null` and missing entries are not shown). Length 0 (returning nothing) shows none of them
- **Failure**: the error envelope `{"error": "..."}`. That plugin's blocks are not shown
- The HTML returned is sanitized by the host (removing `<script>`, `on*` attributes, `javascript:` URLs, etc.).

## 5. Data Structure Specification (JSON Schemas)

All data types are serialized as JSON and passed through linear memory.

### 5.1 `PostContext`

```json
{
  "id": "post_01hgbc3...",
  "title": "Article title",
  "slug": "article-slug",
  "content_type": "article",
  "status": "draft",
  "author_id": "usr_01hgbc...",
  "blocks_json": "[{\"name\":\"core/paragraph\",\"attributes\":{},\"children\":[],\"text\":\"Hello\"}]",
  "created_at": 1729600000,
  "updated_at": 1729600000
}
```

### 5.2 `BlockNode`

```json
{
  "name": "core/paragraph",
  "attributes": {
    "level": 1
  },
  "children": [],
  "text": "Text content"
}
```

- `attributes`: a key-value object of attributes (`{}` or `null` if unspecified).
- `children`: an array of nested child `BlockNode`s.
- `text`: the block's text body (`""` or `null` if unspecified).

### 5.3 `PageRenderContext`

```json
{
  "site_title": "AegisCMS Site",
  "site_description": "Site description",
  "template": "article.html",
  "content_type": "article",
  "title": "Article title",
  "slug": "hello-world",
  "content_html": "<p>Body</p>",
  "content_length": 12,
  "author": "admin",
  "published_at": "2026-10-22T00:00:00Z",
  "updated_at": null,
  "fields": {},
  "relations": {},
  "items": [],
  "extra_css": "body { background: #fafafa; }",
  "extra_head": "<meta name=\"author\" content=\"AegisCMS\">",
  "theme_vars": { "accent": "#10b981" },
  "pagination": {
    "page": 1, "per_page": 10, "total_items": 42, "total_pages": 5,
    "prev_url": null, "next_url": "/?page=2"
  }
}
```

### 5.3.1 Partial Updates for Hooks That Return Objects

The return values (JSON objects) of `on_before_save`, `on_render_page`, and `on_user_authenticated` **overwrite the input per top-level key**. You may return only the fields you changed, and fields you did not return, or fields the plugin does not know about, keep the input's values (for example, returning only `{"extra_css": "..."}`). Returning length 0 means "no change".

### 5.3.2 Adding Before and After the Body in `on_render_page` (`prepend_content` / `append_content`)

A plugin that **only adds** HTML to the body (`content_html`) does not rewrite and return `content_html`; it returns only the HTML to add in `prepend_content` (before the body) and `append_content` (after the body) (for example, `{"append_content": "<nav>…</nav>"}`). Since the body no longer round-trips as JSON, pages with larger bodies get faster (for a 64KB body, about 6µs/KB of JSON reading and writing per plugin is eliminated).

- **Write-only**: it is always passed to plugins empty. The host accumulates the values returned per plugin and **once, after all plugins**, joins them as "prefix + `content_html` + suffix" (after joining, both are empty). Templates and other hooks see only the joined `content_html`.
- **The order** is the same as when rewriting the body directly. A later-running plugin's prefix goes further outside, and its suffix goes further outside the inner ones (that is, further back).
- **The body a plugin that reads the body sees** does not include HTML other plugins added to the frame (for example, the share buttons' text does not enter the reading time's character count). The result of a plugin that rewrites `content_html` directly remains visible to later plugins as before.
- The traditional method of rewriting `content_html` directly can still be used (compatibility).
- **`content_length`**: the byte count of the body (excluding the frame). The host computes it before each plugin call. It is used by plugins that do not include `content_html` in `render_page.input` (below) to check "whether there is a body".

### 5.3.3 Narrowing the Fields Passed to `on_render_page` (`capabilities.render_page.input`)

The input JSON of `on_render_page` includes the body (`content_html`), the article list (`items`), the custom fields (`fields`), and so on, and even if a plugin does not use them, it takes time for copying into Wasm memory and JSON reading and writing. If you write in the manifest's `capabilities.render_page.input` **only the heavy fields you need**, the others are passed as empty values (`""`, `{}`, `[]`, omitted).

```json
"capabilities": {
  "hooks": ["on-render-page"],
  "render_page": { "input": ["fields", "prev_article", "next_article"] }
}
```

- The names you can write are `content_html`, `fields`, `relations`, `items`, `related_articles`, `prev_article`, `next_article`, and `comments`. Everything else (`title`, `template`, `extra_head`, `theme_vars`, etc.) is always passed.
- If `render_page` is omitted (or `input` is omitted), all fields are passed as before. `"input": []` declares that no heavy fields are needed.
- Fields that were not passed are **not reflected** even if the plugin rewrites and returns them (the host restores the original value). To add to the body, use the frame described above.
- A plugin that declares a name that cannot be written cannot be loaded.

#### Results of Declarative Queries (`capabilities.render_page.queries`)

To receive the results of aggregations and lists declared in `queries/<name>.yaml` ([queries.md](queries.md)), write the **names** in the manifest.

```json
"render_page": { "queries": ["by_brand", "case_count"] }
```

- Only the results of the names written go into `PageRenderContext.queries` (name → result). If not written, it is empty. It is independent of `input`.
- **Read-only**: returned values are not reflected and are not visible to the next plugin. Plugins touch neither the DB nor the query definitions.
- Names are lowercase letters, digits, and `_` (starting with a lowercase letter), up to 32. A nonexistent name silently becomes empty. When a result changes, the input JSON changes, so the `on-render-page` result cache (architecture.md 10.8) naturally follows.

### 5.3.3a Adding Scripts in `on_render_page` (`scripts`; ABI 2)

**Scripts can be added only through the structured `scripts`.** `<script>` and `on*` attributes (`onclick`, `onload`, etc.) inside `extra_head`, `prepend_content`, `append_content` (and a rewritten `content_html`) are **removed by the host** (`<meta>`, `<link>`, `<style>`, etc. are as before). For the design, see [csp.md 4](csp.md#4-the-plugin-abi-breaking-change).

```json
"scripts": [
  { "kind": "external", "url": "https://cdn.example.com/lib.js", "async_load": true, "crossorigin": false },
  { "kind": "inline", "code": "window.x = 1;", "consent": "analytics" }
]
```

- **Write-only**: it is always passed to plugins empty. The host validates it, makes `<script>` tags, and adds them after `extra_head` once, after all plugins. **The host decides the attributes** (`onload` and the like cannot be written). The order is the order returned (the plugins' order).
- **`external`**: `url` is either this site's path (`/assets/x.js`) or an `https://` URL whose host is in the manifest's `capabilities.csp.script_domains` (one containing user info, a port, quotes, or whitespace is not allowed). `async_load`, `defer`, and `crossorigin` (`crossorigin="anonymous"`) are optional.
- **`inline`**: the manifest needs the declaration **`capabilities.inline_scripts: true`** (without it, it is dropped). The body is up to 64 KB and must not contain NUL. The host turns `</script` and `<!--` into a form that does not change the JavaScript's meaning.
- **`consent`**: the consent category (1–32 characters of `[a-z0-9_-]`). If present, the host outputs it as `type="text/plain" data-aegis-consent="category"`, and the banner's script runs it after consent ([consent.md](consent.md)). The plugin does not need to assemble HTML.
- **Scripts that violate this are dropped** (the page is not broken). The reason is left in the warning log. The `aegis-plugin-dev` preview shows the reason for dropping.
- ABI 1 plugins can still be loaded as before, but a `<script>` written in `extra_head` is removed (move it to `scripts`).

### 5.3.4 Caching the Result of `on_render_page`

For a call that does not call `kv_get` / `kv_set` / `http_fetch` and whose `config_get` setting values have not changed, the output is determined by the input JSON alone, so the host remembers the result and does not call Wasm for the same input (no declaration is needed; the host checks on each call). No action is needed on the plugin side, but note the following.

- **Give `on_render_page` no side effects**: when a remembered result is returned the function is not called, so `host::log` does not appear either. If KV or HTTP is used, that call is not remembered (it is called every time).
- Read settings values with `config_get` (when a value read changes, the result is not reused and the function is called again).
- The budget is `[runtime].render_cache_mb` (default 32, 0 disables). For the mechanism, see [architecture.md 10.8](architecture.md#108-caching-the-result-of-on-render-page-aegis-runtimesrcwasmresult_cachers).

### 5.4 Error Envelope

```json
{
  "error": "Title is a required field"
}
```

When a hook wants to refuse processing, it returns this JSON to the host.

### 5.5 `PublishedContent`

```json
{
  "id": "c0a80101-0000-0000-0000-000000000001",
  "content_type": "articles",
  "title": "How AegisCMS scheduled publishing works",
  "slug": "scheduled-publishing-deep-dive",
  "published_at": "2026-10-24T12:00:00Z",
  "author_id": "editor1",
  "fields": {
    "category": "Tech",
    "featured": true
  }
}
```

Information about published content passed to the `on_content_published` hook.

---

## 6. Plugin Manifest Specification (`plugin.manifest.json`)

```json
{
  "name": "sample-plugin",
  "version": "1.0.0",
  "entrypoint": "plugin.wasm",
  "abi_version": 2,
  "capabilities": {
    "network": {
      "allowed_domains": ["api.example.com"]
    },
    "storage": {
      "kv_namespace": "sample_state"
    },
    "hooks": [
      "on-before-save",
      "transform-content-ast",
      "on-render-page"
    ]
  },
  "resource_limits": {
    "max_memory_mb": 16,
    "max_fuel_units": 100000000,
    "execution_timeout_ms": 250
  }
}
```

- **`capabilities.hooks`**: the list of hooks the plugin receives (kebab-case). Hooks not listed here are not called even if the Wasm side exports the function.
- **`name`**: the plugin's identifier (lowercase letters, digits, `-`, `_`). It is installed at `plugins/<name>/`, and writing it in `[plugins].enabled` enables it.
- **`display_name` / `description` / `author` / `license` / `tags` / `homepage` / `repository`**: metadata for stores and list display (optional).
- **`min_aegis_version`**: the minimum AegisCMS core version it can run on (SemVer; optional). If the core is older, it is not loaded.
- **`abi_version`**: the version of this ABI the plugin assumes (an integer; currently `2`; optional, but official plugins and templates always write it. `2` is for plugins that use `scripts` (structured script injection; [5.3.3a](#533a-adding-scripts-in-on_render_page-scripts-abi-2)) in `on-render-page`. `1` plugins can still be loaded, but a `<script>` written in `extra_head` and the like is removed). A value newer than the version the host supports, or `0`, is not loaded. The host raises it only when a change that breaks compatibility is introduced ([versioning.md](versioning.md#2-plugin-compatibility)).
- **`resource_limits`**: the memory limit (in MB), the fuel limit, and the timeout (in milliseconds).
- **`capabilities.embed.frame_domains`**: the source hosts of `<iframe>`s that may remain in custom blocks' HTML (also added to the CSP `frame-src`). For the format, see [plugin_hooks.md 3.2](plugin_hooks.md#32-embedding-external-pages-capabilitiesembed).
- **`capabilities.inline_scripts`**: the declaration for adding inline scripts with `scripts` of `on-render-page` (boolean; default `false`). For the format, see [5.3.3a](#533a-adding-scripts-in-on_render_page-scripts-abi-2). The hosts of external scripts are in `capabilities.csp.script_domains`.
- **`capabilities.render_page.input`**: narrowing the heavy fields passed to `on-render-page`. For the format, see [5.3.3](#533-narrowing-the-fields-passed-to-on_render_page-capabilitiesrender_pageinput).
- **`blocks`**: the declaration of custom blocks that can be placed in the body (name, display name, input fields). The `render-blocks` hook is required. For the format, see [plugin_hooks.md 3.1](plugin_hooks.md#31-custom-blocks-blocks).

---

## 7. Implementation Guidelines by Language

| Item                  | Rust                                                          | AssemblyScript                                 | TinyGo                                                     |
| --------------------- | ------------------------------------------------------------- | ---------------------------------------------- | ---------------------------------------------------------- |
| **Official SDK**      | `crates/aegis-plugin-sdk`                                     | `sdk/assemblyscript` (`@takumi-mr/plugin-sdk`) | `sdk/tinygo` (`github.com/takumi-mr/AegisCMS/sdk/tinygo`)  |
| **Official template** | `sdk/templates/rust` (for blocks: `sdk/templates/rust-block`) | `sdk/templates/assemblyscript`                 | `sdk/templates/tinygo`                                     |
| **Build command**     | `cargo build --target wasm32-unknown-unknown --release`       | `npm run asbuild`                              | `tinygo build -target=wasm-unknown -o plugin.wasm main.go` |
| **Host imports**      | `#[link(wasm_import_module = "host")] extern "C"`             | `@external("host", "...")`                     | `//go:wasmimport host ...`                                 |
| **Memory allocator**  | `std::alloc::alloc`                                           | `heap.alloc`                                   | `unsafe.Pointer(&buf[0])` / `malloc`                       |
| **JSON handling**     | `serde_json`                                                  | A lightweight JSON builder / parser            | `encoding/json`                                            |
