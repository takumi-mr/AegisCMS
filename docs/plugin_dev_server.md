[← Back to the documentation index](README.md)

**English** | [日本語](plugin_dev_server.ja.md)

# AegisCMS Plugin Dev Server (`aegis-server plugin dev`) & Plugin Builder

`aegis-server plugin dev` (or `task dev:plugin -- <dir>`) is a lightweight dev server for instantly testing and debugging an AegisCMS Wasm plugin on its own, without a database or login.

Like `theme dev` (Theme Builder), it watches for plugin changes and reloads automatically, and provides a web UI (Plugin Builder) and a REST API that visualize and let you operate, in real time, the result of hook execution inside the Wasm sandbox, the CPU instruction count (fuel), execution time in milliseconds, plugin logs (`log_info!`, etc.), KVS changes, outgoing HTTP requests, and an HTML preview.

---

## 1. Quick Start

```bash
# Start the dev server with an existing official plugin (default: http://127.0.0.1:8091)
task dev:plugin -- plugins/official-code-enhancer

# If you do not have task, the equivalent command (thanks to the standalone crate aegis-plugin-dev it starts in seconds to tens of seconds even the first time)
cargo run -p aegis-plugin-dev -- plugins/official-code-enhancer

# Start the frontend dev UI (Plugin Builder) (http://localhost:5175)
task dev:plugin-builder
```

---

## 2. API Specification (`/__dev/*`)

The dev server provides the following endpoints.

### 2.1 Getting Plugin Information (`GET /__dev/api/plugin`)

Returns the manifest (`plugin.manifest.json`), the entrypoint, the Wasm file size, and the list of function names the module exports.

```bash
curl http://127.0.0.1:8091/__dev/api/plugin
```

**Example response**:
```json
{
  "manifest": {
    "name": "official-code-enhancer",
    "version": "0.1.0",
    "entrypoint": "plugin.wasm",
    "capabilities": {
      "hooks": ["transform-content-ast", "on-render-page"]
    },
    "resource_limits": {
      "max_memory_mb": 16,
      "max_fuel_units": 100000000,
      "execution_timeout_ms": 250
    }
  },
  "entrypoint": "plugin.wasm",
  "wasm_exists": true,
  "wasm_size_bytes": 1450280,
  "exports": ["memory", "allocate", "transform_content_ast", "on_render_page"],
  "updated_at": 1730424567890
}
```

### 2.2 List of Standard Mock Fixtures (`GET /__dev/api/fixtures`)

Returns the standard input for every hook. The values are exactly the JSON the host actually passes to the hook (no wrapper such as `{"post": {...}}`).

| Hook | Input (Rust type) |
| --- | --- |
| `on-before-save` | `PostContext` |
| `transform-content-ast` | An array of `BlockNode` |
| `render-blocks` | `{"blocks": [BlockNode, ...]}` (the result is `{"html": ["<div>…</div>", null, ...]}`; `null` is a block this plugin does not render) |
| `on-render-page` | `PageRenderContext` |
| `calculate-order` | `OrderContext` |
| `on-order-status-changed` | `OrderStatusChange` |
| `on-content-published` | `PublishedContent` |
| `check-comment` | `CommentSubmission` (the result is `CommentVerdict`) |
| `send-notification` | `NotificationMessage` |
| `on-user-authenticated` | `UserAuthContext` |

The types are in `aegis-core` / `aegis-ast`, and tests check that each fixture can be loaded into its type.

```bash
curl http://127.0.0.1:8091/__dev/api/fixtures
```

### 2.3 Instant Test Execution of a Hook (`POST /__dev/api/invoke`)

Specify the hook name, input data, initial KV, mock HTTP rules, and dynamic settings, and it is executed safely isolated on Wasmtime.
Note: this is a write endpoint, so the header `x-aegis-dev: 1` is required.

```bash
curl -X POST http://127.0.0.1:8091/__dev/api/invoke \
  -H "x-aegis-dev: 1" \
  -H "Content-Type: application/json" \
  -d '{
    "hook": "transform-content-ast",
    "input": [
      {
        "name": "core/code",
        "attributes": { "language": "rust" },
        "text": "fn main() { println!(\"Hello\"); }"
      }
    ],
    "config": {
      "highlight_theme": "github-dark"
    }
  }'
```

**Example response**:
```json
{
  "success": true,
  "output": [
    {
      "name": "core/code",
      "attributes": { "language": "rust", "data-enhanced-code": true },
      "text": "fn main() { println!(\"Hello\"); }",
      "children": []
    }
  ],
  "raw_output": "[{\"name\":\"core/code\",...}]",
  "error": null,
  "fuel_consumed": 15420,
  "duration_ms": 1.45,
  "logs": [
    {
      "level": "info",
      "message": "Highlighted 1 code block using github-dark theme",
      "timestamp": "2026-11-01T01:50:00Z"
    }
  ],
  "kv_mutations": {},
  "http_requests": []
}
```

### 2.4 Mocking and Capturing KVS / HTTP Communication

Plugins that integrate with external APIs (such as Slack notifications) or save state can also be verified without affecting the real environment.

```json
{
  "hook": "on-order-status-changed",
  "input": {
    "order": { "order_id": "ord-123", "currency": "JPY", "lines": [], "subtotal": 0, "total": 0 },
    "from_status": "pending_payment",
    "to_status": "paid",
    "occurred_at": "2026-11-01T09:00:00Z"
  },
  "kv": {
    "notify_count": "5"
  },
  "mock_http": [
    {
      "url_pattern": "https://hooks.slack.com/*",
      "status": 200,
      "body": "{\"ok\":true}"
    }
  ]
}
```

After execution, the rewritten keys and values are returned in `kv_mutations`, and the HTTP methods, URLs, and bodies that were sent are recorded in `http_requests`.

### 2.5 Long Polling for Hot Reload (`GET /__dev/version?since=<version>`)

When it detects a file update in the plugin folder (such as a recompile of `plugin.wasm`), it immediately returns a new timestamp. It is used for automatic re-execution in the frontend (Plugin Builder).

### 2.6 HTML Live Preview API (`POST /__dev/api/preview`)

Takes the output of a frontend-type hook (`on-render-page`, `render-blocks`, `transform-content-ast`, etc.) and dynamically composes and returns a safe standalone HTML document with the CSS and JS embedded.

```bash
curl -X POST http://127.0.0.1:8091/__dev/api/preview \
  -H "x-aegis-dev: 1" \
  -H "Content-Type: application/json" \
  -d '{
    "hook": "render-blocks",
    "output": ["<div class=\"cta-banner\">Special Offer!</div>"]
  }'
```

**Example response**:
```json
{
  "html": "<!DOCTYPE html><html lang=\"en\"><head><meta charset=\"utf-8\"><title>Plugin Preview</title><style>...</style></head><body><div class=\"preview-wrapper\"><div class=\"rendered-blocks\"><div class=\"block-item\"><div class=\"cta-banner\">Special Offer!</div></div></div></div></body></html>"
}
```

### 2.7 Build Trigger API (`POST /__dev/api/build`)

Runs the plugin folder's build command and returns the result, the output log (stdout/stderr), and the time taken (ms).
The build command is decided in this order.

| Priority | Specification | Behavior |
| --- | --- | --- |
| 1 | `--build-command "<command>"` | Runs that command. **It is the command's job to place the artifact at the `entrypoint` (`plugin.wasm`)** (e.g. `cargo build --target wasm32-unknown-unknown --release && cp ... plugin.wasm`) |
| 2 | `"dev": { "build_command": "<command>" }` in the manifest | Same as above (when you want to fix it per plugin) |
| 3 | `Cargo.toml` (Rust) | Runs `cargo build --target wasm32-unknown-unknown --release`, and on success **automatically copies** the cdylib artifact (under `target/`) to the `entrypoint`. Works for both workspace members and standalone crates (it finds the artifact with `cargo metadata`). A release build, so fuel is measured under the same conditions as production |
| 4 | `package.json` (AssemblyScript) | `npm run asbuild`, or `npm run build` if absent |

When you save a source file (`.rs`, `.ts`, etc.), this command runs automatically, and on success the page is reloaded. The dev server reads only the `entrypoint` file, so for anything other than 3, make the command one that updates the `entrypoint` as above (if it is not updated, the build succeeds but the display does not change).

```bash
curl -X POST http://127.0.0.1:8091/__dev/api/build \
  -H "x-aegis-dev: 1"
```

---

## 3. Security Design

The dev server is specialized for local development, but it has the following defenses to prevent unauthorized operation from other browser tabs and malicious external scripts.

1. **DNS rebinding prevention**: For loopback connections, it validates that the `Host` header is `localhost` / `127.0.0.1` / `::1`.
2. **CSRF / unauthorized fetch prevention**: The `x-aegis-dev` custom header is required on all mutation requests other than `GET` / `HEAD`.
3. **Path traversal prevention**: The file API blocks paths that escape from under the plugin directory (`..` and symbolic links).
4. **Preview sandbox**: Plugin Builder's HTML preview iframe is isolated with the attribute `sandbox="allow-scripts allow-forms allow-popups allow-modals"`, preventing same-origin access to the parent screen.

---

## 4. Plugin Builder Web UI (`plugin-builder/`)

A development SPA built with React 19 + TypeScript + Vite + CodeMirror 6.
Start it with `task dev:plugin-builder` (default port: `5175`).

![Plugin Builder: input (Payload) on the left, the HTML preview of the execution result on the right](images/plugin-builder.png)

Edit the input on the left (payload, settings, initial KV, HTTP mocks) and run it, and the result, metrics, and preview appear on the right. The result tab (Output) shows the JSON the hook returned as is.

![Plugin Builder: the Output tab (the JSON the hook returned)](images/plugin-builder-output.png)

- **Hook switching & preset fixtures**: Choosing a hook defined in the manifest immediately sets the corresponding standard test data.
- **Code editor (CodeMirror 6)**: Payload (JSON), Settings (Config), Initial KV Store, and Mock HTTP Rules can be switched between with tabs and edited. Ctrl+Enter runs immediately.
- **Metrics bar**: Visualizes fuel consumption (as a percentage of the maximum resource limit) and execution time (ms) with progress bars.
- **HTML live preview**: The result of `on-render-page` and others is rendered in real time in an iframe, switchable between Desktop / Tablet / Mobile sizes. It also supports showing in another tab and reloading.
- **One-click Rebuild**: Run a Wasm rebuild from the "Rebuild" button on screen. On a build error, you can check the log immediately.
