# AegisCMS TinyGo Plugin SDK

Official TinyGo / Go module for AegisCMS WebAssembly (Wasm) plugins.

[AegisCMS](https://github.com/takumi-mr/AegisCMS) provides zero-trust WebAssembly plugin capabilities. With TinyGo, you can write type-safe, lightweight, single-binary Wasm plugins using standard Go syntax and idioms.

## Installation

```bash
go get github.com/takumi-mr/AegisCMS/sdk/tinygo@v0.1.0
```

## Quick Start

### 1. `go.mod`

```go
module my-aegis-plugin

go 1.22

require github.com/takumi-mr/AegisCMS/sdk/tinygo v0.1.0
```

### 2. Implement Plugin (`main.go`)

```go
package main

import (
	"encoding/json"
	"strings"

	"github.com/takumi-mr/AegisCMS/sdk/tinygo"
)

//export allocate
func allocate(size int32) uintptr {
	return sdk.Allocate(size)
}

//export on_before_save
func on_before_save(ptr int32, length int32) int64 {
	inBytes := sdk.ReadBytes(uintptr(ptr), length)

	var post sdk.PostContext
	if err := json.Unmarshal(inBytes, &post); err != nil {
		sdk.LogError("[TinyGo] Failed to deserialize PostContext: " + err.Error())
		return sdk.WriteError("Invalid PostContext payload")
	}

	sdk.LogInfo("[TinyGo] Processing: " + post.Title)

	if strings.TrimSpace(post.Title) == "" {
		return sdk.WriteError("Post title cannot be empty")
	}

	// Persist state in plugin KV namespace
	sdk.KVSet("last_saved_slug", post.Slug)

	return sdk.WriteJSON(&post)
}

func main() {}
```

### 3. Build WebAssembly Binary

Compile with TinyGo (v0.31+ recommended):

```bash
tinygo build -target=wasm-unknown -opt=z -no-debug -o plugin.wasm main.go
```

Place `plugin.wasm` alongside `plugin.manifest.json` into `plugins/<name>/` in your AegisCMS installation.

## Supported Lifecycle Hooks

- `on_before_save(ptr int32, length int32) int64`
- `transform_content_ast(ptr int32, length int32) int64`
- `on_render_page(ptr int32, length int32) int64`
- `calculate_order`, `on_order_status_changed`, `send_notification`, `on_content_published`, `check_comment`, `render_blocks` (all `(ptr int32, length int32) int64`)

### `on_render_page` and Structured Script Injection (ABI 2)

Use `sdk.RunHook(ptr, length, "PageRenderContext", func(ctx *sdk.PageRenderContext) error { ... })`:

- **HTML around the article body**: Set `ctx.PrependContent` / `ctx.AppendContent` instead of round-tripping `ctx.ContentHTML`. Check `ctx.ContentLength > 0` to see whether the page has body content even when `content_html` is omitted via `capabilities.render_page.input`.
- **Declarative queries (`ctx.Queries`)**: Read query results declared in `capabilities.render_page.queries` via `ctx.Queries["<query_name>"]` (`map[string]interface{}`).
- **Scripts (`ctx.Scripts`)**: Raw `<script>` tags and `on*` attributes in `ExtraHead`, `PrependContent`, and `AppendContent` are stripped by the host. Append `sdk.InjectedScript` entries to `ctx.Scripts` instead:
  - `sdk.InlineScript("window.init = 1;")` (requires `"inline_scripts": true` in `capabilities`)
  - `sdk.ExternalScript("https://cdn.example.com/lib.js").Async().Deferred().WithCrossorigin().WithConsent("analytics")` (requires `"cdn.example.com"` in `capabilities.csp.script_domains`)

### Commerce, Notification, Comment & Block Hooks

The hooks above after `on_render_page` are declared in the manifest's `capabilities.hooks` in kebab-case (`calculate-order`, ...). Each has a runner that reads the input, calls your function and writes the result in the ABI format:

| Hook | Runner | Your function |
| --- | --- | --- |
| `calculate-order` | `sdk.RunHook(ptr, length, "OrderContext", f)` | `func(*sdk.OrderContext) error` (append to `Adjustments`; the host applies only `Adjustments` and `Metadata`) |
| `on-order-status-changed` | `sdk.RunNotificationHook(ptr, length, "OrderStatusChange", f)` | `func(*sdk.OrderStatusChange) error` |
| `on-content-published` | `sdk.RunNotificationHook(ptr, length, "PublishedContent", f)` | `func(*sdk.PublishedContent) error` |
| `send-notification` | `sdk.RunDeliveryHook(ptr, length, f)` | `func(*sdk.NotificationMessage) (sdk.Delivery, error)` (`DeliverySent` / `DeliverySkipped`) |
| `check-comment` | `sdk.RunCheckCommentHook(ptr, length, f)` | `func(*sdk.CommentSubmission) (sdk.CommentVerdict, error)` (`AllowComment()` / `HoldComment(reason)` / `SpamComment(reason)`) |
| `render-blocks` | `sdk.RunRenderBlocksHook(ptr, length, f)` | `func([]sdk.BlockNode) ([]*string, error)` (`sdk.HTML(...)`; `nil` hides the block) |

`sdk.Reject(message)` refuses the input (the message is shown as is) and, in `send-notification`, marks the failure as permanent so the host does not retry. Any other error is retried. See [`sdk/examples`](../examples) for complete plugins.

## Host APIs

- `sdk.LogDebug(msg)`, `sdk.LogInfo(msg)`, `sdk.LogWarn(msg)`, `sdk.LogError(msg)`
- `sdk.NetIsAllowed(url string) bool`
- `sdk.KVSet(key, value string) bool`
- `sdk.KVGet(key string) (value string, found bool, err error)` (needs `capabilities.storage`)
- `sdk.ConfigGet(key string) (value string, found bool)` / `sdk.ConfigGetOr(key, fallback string) string` (the server's `[plugins.settings.<plugin name>]`; non-string values come back as JSON)
- `sdk.HTTPFetch(sdk.HTTPRequest) (*sdk.HTTPResponse, error)` (only `allowed_domains`; no redirects; 8 calls per hook; 1 MiB bodies; counts toward `execution_timeout_ms`). `sdk.NewHTTPGet(url)` and `sdk.NewHTTPPostJSON(url, body)` build requests

## License

Licensed under either of Apache License, Version 2.0 or MIT license at your option.
