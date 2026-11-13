[← Back to the documentation index](README.md)

**English** | [日本語](plugin_sdk_guide.ja.md)

# AegisCMS WebAssembly (Wasm) Plugin Development Official Guide

This document is the official guide for developing AegisCMS's zero-trust WebAssembly plugins.

> [!TIP]
> For details of the available hooks, host API, and manifest items, and current limitations, see the [Plugin hooks & host API reference](./plugin_hooks.md).

---

## 1. Architecture and Execution Model

AegisCMS's plugin system is built on **`Wasmtime`, the WebAssembly runtime from the Bytecode Alliance**.

- **Completely separate memory spaces**: no physical memory is shared between the host and the guest (Wasm); they run strictly isolated inside a sandbox.
- **Resource limits (fuel & timeout)**:
    - **Fuel (instruction quota)**: sets an upper limit on the number of Wasm instructions that can be executed, to prevent infinite loops and DoS attacks.
    - **Asynchronous timeout**: forcibly stops plugin execution that exceeds the specified milliseconds (e.g. 250 ms).
- **Capability-based permission management (zero-trust)**:
    - Anything other than the permissions a plugin declares in `plugin.manifest.json` (network domains, KVS namespace, hooks) is blocked.

---

## 2. Quick Start: The Official Rust SDK

Using the official SDK [`aegis-plugin-sdk`](../crates/aegis-plugin-sdk), you can develop plugins in a type-safe way without worrying about low-level pointer operations or memory management.

### 2.1 Preparing the Template and Generating a Scaffold (`task plugin:new`)

When creating a new plugin, the `task plugin:new` command instantly generates a scaffold that does the renaming and the standalone crate setup (`[workspace]`) automatically.

```bash
# Create an ordinary Rust plugin
task plugin:new -- my-plugin

# Create a body block plugin (based on sdk/templates/rust-block)
task plugin:new -- my-note --type block

# Create an AssemblyScript or TinyGo plugin
task plugin:new -- my-as-plugin --lang as
task plugin:new -- my-go-plugin --lang tinygo
```

If you copy by hand, you can create a new plugin by copying the official template [`sdk/templates/rust`](../sdk/templates/rust). For a plugin that adds blocks that can be placed in the body, start with the block template [`sdk/templates/rust-block`](../sdk/templates/rust-block) and the guide [Creating your first block plugin](plugin_block_tutorial.md).

`Cargo.toml` settings:

```toml
[package]
name = "my-aegis-plugin"
version = "0.1.0"
edition = "2021"

[lib]
crate-type = ["cdylib", "rlib"]

[dependencies]
# When placing in this repository's plugins/<name>/ (same as the template)
aegis-plugin-sdk = { path = "../../crates/aegis-plugin-sdk", version = "0.2.0" }
# When placing outside the repository: write the tag of the version you use in tag (https://github.com/takumi-mr/AegisCMS/releases)
# aegis-plugin-sdk = { git = "https://github.com/takumi-mr/AegisCMS", tag = "v0.2.0" }
serde = { version = "1.0", features = ["derive"] }
```

- The `serde_json` that `#[aegis_plugin]` uses is re-exported by the SDK, so you do not need to write it in `Cargo.toml`. If you use `serde_json::json!` and the like directly in the plugin's code, either add `serde_json = "1.0"` or write `use aegis_plugin_sdk::serde_json;`.
- **Note when creating a new plugin inside this repository**: the root `Cargo.toml` makes `plugins/official-*` and `sdk/templates/rust` members of the workspace. Running `cargo build` in a new folder that is not a member fails with `error: current package believes it's in a workspace when it's not`. Do one of the following.
    - To bring it in as an official plugin, add `"plugins/<name>"` to the root `Cargo.toml`'s `[workspace] members` (it also becomes a target of `cargo test --workspace` / `clippy --workspace`).
    - For a personal plugin, write an empty `[workspace]` at the end of the plugin's `Cargo.toml` to make it a standalone crate. In that case, run the pre-PR checks (`cargo fmt --check`, `cargo clippy -- -D warnings`, `cargo test`) yourself inside the plugin's folder.

### 2.2 Plugin Manifest (`plugin.manifest.json`)

```json
{
  "name": "my-aegis-plugin",
  "version": "0.1.0",
  "entrypoint": "plugin.wasm",
  "abi_version": 2,
  "capabilities": {
    "network": {
      "allowed_domains": ["api.slack.com", "hooks.slack.com"]
    },
    "storage": {
      "kv_namespace": "my_plugin_state"
    },
    "hooks": [
      "on-before-save",
      "transform-content-ast"
    ]
  },
  "resource_limits": {
    "max_memory_mb": 16,
    "max_fuel_units": 100000000,
    "execution_timeout_ms": 250
  }
}
```

### 2.3 Implementing the Plugin Code (`src/lib.rs`)

```rust
use aegis_plugin_sdk::prelude::*;

#[derive(Default)]
pub struct MyPlugin;

#[aegis_plugin]
impl Plugin for MyPlugin {
    /// Hook before an article is saved
    fn on_before_save(&mut self, mut post: PostContext) -> Result<PostContext> {
        log_info!("Processing post: '{}'", post.title);

        // Process and validate the title
        post.title = post.title.trim().to_string();

        // Verify permission to access an external API
        if net::is_allowed("https://hooks.slack.com/services/xxx") {
            log_info!("Slack webhook is authorized");
        }

        // Validation: returning Err aborts the save, and the API returns the message with 422
        if post.title.is_empty() {
            return Err(PluginError::Rejected("title is required".to_string()));
        }

        // Persist state in the KVS
        kv::set("last_saved_slug", &post.slug)?;

        // The returned title / slug / blocks_json are saved
        Ok(post)
    }

    /// Content AST transformation hook
    fn transform_content_ast(&mut self, mut blocks: Vec<BlockNode>) -> Result<Vec<BlockNode>> {
        // Inject a custom attribute into heading blocks
        for block in blocks.iter_mut() {
            if block.name == "core/heading" {
                block.set_attribute("enhanced", serde_json::json!(true));
            }
        }

        // Add a custom block at the end of the article
        // This hook is re-applied to already saved (transformed) blocks too (re-saving, the editor's preview),
        // so do not add it if already added (make it idempotent)
        let footer = "— Processed by MyPlugin —";
        if !blocks.iter().any(|b| b.text.as_deref() == Some(footer)) {
            blocks.push(BlockNode::paragraph(footer));
        }

        Ok(blocks)
    }
}
```

---

### 2.4 Declaring Settings

To let the plugin's settings be entered in Admin Studio, declare the items (key, type, name, default, range) in the manifest's `settings`. The types are string, multi-line, URL, number, on/off, choices, and color, and the server validates the values on save. For the declaration format, see [Hooks & host API 3.3](./plugin_hooks.md#33-settings-items-settings); a working example is in [`plugins/official-page-toc`](../plugins/official-page-toc). Values are read as strings with `config::get("key")`.

### 2.5 Adding Custom Body Blocks

By declaring blocks and input fields in the manifest's `blocks`, adding `render-blocks` to `capabilities.hooks`, and implementing `Plugin::render_blocks`, you can add your own blocks to the Admin Studio editor. For the declaration format and the hook's rules, see [Hooks & host API 1.10 / 3.1](./plugin_hooks.md#110-render-blocks); a working example is in the official plugin [`plugins/official-blocks`](../plugins/official-blocks).

## 3. Host API Reference

The standard APIs that can be imported from the SDK.

### 3.1 Logging API (`aegis_plugin_sdk::log`)

Sends messages directly to the host's `tracing` structured logger. The plugin name is tagged automatically.

```rust
log_debug!("Debug message: {}", value);
log_info!("Information: {}", message);
log_warn!("Warning encountered");
log_error!("Critical error occurred");
```

### 3.2 Network Verification API (`aegis_plugin_sdk::net`)

Checks beforehand whether the destination URL is included in `capabilities.network.allowed_domains` in `plugin.manifest.json`.

```rust
if net::is_allowed("https://api.github.com/repos/xxx") {
    // an allowed domain
} else {
    // a permission violation
}
```

### 3.3 Key-Value Storage API (`aegis_plugin_sdk::kv`)

Persists data in the plugin's own namespace (`capabilities.storage.kv_namespace`). It is physically isolated from other plugins' data.

```rust
// Save a string
kv::set("config_key", "config_value")?;

// Save any serializable struct (JSON)
kv::set_json("user_state", &my_state_struct)?;
```

---

## 4. Testing and Deployment

### 4.1 Native Unit Tests (Fast, Mocks Built In)

The SDK has built-in mocks for native execution, so you can run unit tests with plain `cargo test` without building to Wasm.

```bash
cargo test
```

### 4.2 Building the WebAssembly Binary

```bash
# Install the target (first time only)
rustup target add wasm32-unknown-unknown

# Optimized build
cargo build --target wasm32-unknown-unknown --release

# Place the binary (Linux / macOS)
cp target/wasm32-unknown-unknown/release/<plugin_name>.wasm plugin.wasm
# Place the binary (Windows)
copy target\wasm32-unknown-unknown\release\<plugin_name>.wasm plugin.wasm
```

### 4.3 Instant Verification with the Plugin Dev Server (`task dev:plugin`)

The compiled plugin.wasm and plugin.manifest.json can be verified instantly on the dev server for hook behavior, CPU instruction count (fuel), execution time, logs, and KVS/HTTP behavior, without starting a real server or DB (for details see [Plugin dev server (plugin_dev_server.md)](plugin_dev_server.md)).

```bash
# Start the plugin dev server (port 8091)
task dev:plugin -- plugins/my-plugin
# If you do not have task, the equivalent command
cargo run -p aegis-server -- plugin dev plugins/my-plugin
```

The dev server is split into the standalone crate `aegis-plugin-dev`, so it starts quickly, in seconds to tens of seconds, even the first time.

```bash
# A direct start command that does not go through the whole core server
cargo run -p aegis-plugin-dev -- plugins/my-plugin
```

### 4.4 Deploying to AegisCMS

Create the `plugins/<name>/` directory (`<name>` is the manifest's `name`) on the AegisCMS server and place the following.

![Admin Studio plugin management (Settings → Plugins)](images/plugins-admin.png)

```
plugins/my-plugin/
  ├── plugin.manifest.json   # "name": "my-plugin"
  └── plugin.wasm
```

Placing it alone does not load it. It is enabled by turning it on in Admin Studio's plugins screen (no restart needed), or by adding its name to `[plugins].enabled` in the config file and restarting the server (hooks run in the order of the enabled list).

```toml
[plugins]
enabled = ["my-plugin"]
```

`aegis-server plugin list` shows the installed plugins, whether they are enabled, and the reason one cannot be loaded. If an enabled plugin cannot be loaded, the server does not start ([Plugin hooks reference](plugin_hooks.md#7-installation-and-enabling)).

---

## 5. Multi-Language Support and Official Templates

AegisCMS provides official SDKs and development templates not only for Rust but also for **AssemblyScript**, which is friendly to frontend/web developers, and **TinyGo** for Go developers.

| Language           | Official package                           | Official template                                                 | Package source                                          | Distribution ecosystem (GitHub-centered)                                |
| ------------------ | ------------------------------------------ | ----------------------------------------------------------------- | ------------------------------------------------------- | ----------------------------------------------------------------------- |
| **Rust**           | `aegis-plugin-sdk`                         | [`sdk/templates/rust`](../sdk/templates/rust)                     | [`crates/aegis-plugin-sdk`](../crates/aegis-plugin-sdk) | GitHub (Git dependency `git = "https://github.com/takumi-mr/AegisCMS"`) |
| **AssemblyScript** | `@takumi-mr/plugin-sdk`                    | [`sdk/templates/assemblyscript`](../sdk/templates/assemblyscript) | [`sdk/assemblyscript`](../sdk/assemblyscript)           | GitHub Packages (`npm.pkg.github.com`)                                  |
| **TinyGo**         | `github.com/takumi-mr/AegisCMS/sdk/tinygo` | [`sdk/templates/tinygo`](../sdk/templates/tinygo)                 | [`sdk/tinygo`](../sdk/tinygo)                           | GitHub (Go Modules `go get github.com/...`)                             |

---

### 5.1 AssemblyScript Plugin Development

AssemblyScript is a language that outputs WebAssembly directly from a subset of TypeScript syntax. You can build plugins using the official npm package `@takumi-mr/plugin-sdk` hosted on GitHub Packages.

```bash
# Move to the template directory
cd sdk/templates/assemblyscript

# Install the dependencies (@takumi-mr/plugin-sdk, assemblyscript)
npm install

# Wasm build (outputs plugin.wasm)
npm run build
```

A lifecycle hook implementation example (`assembly/index.ts`):

```typescript
import {
  allocate as sdkAllocate,
  readString,
  writeString,
  writeError,
  log,
  parsePostContext,
  serializePostContext,
  PostContext
} from "@takumi-mr/plugin-sdk/assembly/index";

export function allocate(len: i32): i32 {
  return sdkAllocate(len);
}

export function on_before_save(ptr: i32, len: i32): i64 {
  const post = parsePostContext(readString(ptr, len));

  if (post.title.trim().length === 0) {
    return writeError("Post title cannot be empty");
  }

  log.info("Processing post: " + post.title);
  post.title = "[AS-Plugin] " + post.title;

  return writeString(serializePostContext(post));
}
```

Do not remove `"use": ["abort=~lib/@takumi-mr/plugin-sdk/assembly/host/abort"]` from `asconfig.json`. AssemblyScript imports `env.abort` by default to handle runtime errors, but the host provides only the `host` module, so without this setting the plugin cannot be loaded (it is replaced with the SDK's `abort`, which writes a log and then stops).

For details, see [`sdk/templates/assemblyscript/README.md`](../sdk/templates/assemblyscript/README.md).

---

### 5.2 TinyGo Plugin Development

TinyGo is a compiler that produces extremely small Wasm binaries from Go syntax. You can call the host API type-safely using the official Go module `github.com/takumi-mr/AegisCMS/sdk/tinygo`.

```bash
# Move to the template directory
cd sdk/templates/tinygo

# Wasm build (outputs plugin.wasm)
tinygo build -target=wasm-unknown -opt=z -no-debug -o plugin.wasm main.go
```

A lifecycle hook implementation example (`main.go`):

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
		return sdk.WriteError("Invalid PostContext payload")
	}

	if strings.TrimSpace(post.Title) == "" {
		return sdk.WriteError("Post title cannot be empty")
	}

	sdk.LogInfo("[TinyGo] Processing: " + post.Title)
	post.Title = "[TinyGo] " + post.Title

	return sdk.WriteJSON(&post)
}

func main() {}
```

Use the `wasm-unknown` target (TinyGo 0.30+). The `wasi` target imports WASI functions that the host does not provide, so it cannot be loaded. The `_initialize` that a `wasm-unknown` module exports (initialization of the runtime and global variables) is called by the host before any hook.

For details, see [`sdk/templates/tinygo/README.md`](../sdk/templates/tinygo/README.md).

---

### 5.3 Hook & Host API Support Matrix by Language

AegisCMS's official SDKs let you use all lifecycle hooks and host APIs type-safely not only in Rust but also in AssemblyScript and TinyGo.

| Hook                      | When / purpose                                               | Rust (`aegis-plugin-sdk`)                  | AssemblyScript (`@takumi-mr/plugin-sdk`)                                 | TinyGo (`sdk/tinygo`)                                            |
| ------------------------- | ------------------------------------------------------------ | ------------------------------------------ | ------------------------------------------------------------------------ | ---------------------------------------------------------------- |
| `on-before-save`          | Validation before saving an article, title processing        | `Plugin::on_before_save`                   | `parsePostContext`, `serializePostContext`                               | `RunHook`, `PostContext`                                         |
| `transform-content-ast`   | Traversing and transforming the article AST                  | `Plugin::transform_content_ast`            | `readHookJson`, `writeJson`                                              | `RunHook`, `[]BlockNode`                                         |
| `on-render-page`          | Injecting CSS/JS/ads/table of contents into public HTML      | `Plugin::on_render_page`, `InjectedScript` | `parsePageRenderContext`, `serializePageRenderContext`, `InjectedScript` | `RunHook`, `PageRenderContext`, `InlineScript`, `ExternalScript` |
| `calculate-order`         | Cart and order amount calculation (shipping, discounts)      | `Plugin::calculate_order`                  | `OrderContext`, `writeOrderResult`                                       | `RunHook`, `OrderContext`, `OrderAdjustment`                     |
| `on-order-status-changed` | Order status change notification (webhook integration, etc.) | `Plugin::on_order_status_changed`          | `OrderStatusChange`, `writeOk`                                           | `RunNotificationHook`, `OrderStatusChange`                       |
| `send-notification`       | Notification delivery (Slack/email/external integration)     | `Plugin::send_notification`                | `NotificationMessage`, `writeDelivery`, `writeDeliveryError`             | `RunDeliveryHook`, `NotificationMessage`, `Delivery`             |
| `on-content-published`    | Article publication notification (external delivery, SNS)    | `Plugin::on_content_published`             | `PublishedContent`, `writeOk`                                            | `RunNotificationHook`, `PublishedContent`                        |
| `check-comment`           | Spam check before a comment is posted                        | `Plugin::check_comment`                    | `CommentSubmission`, `CommentVerdict`, `writeVerdict`                    | `RunCheckCommentHook`, `CommentSubmission`, `CommentVerdict`     |
| `render-blocks`           | HTML rendering of custom blocks                              | `Plugin::render_blocks`                    | `readBlocks`, `writeBlocksHtml`, `BlockNode`                             | `RunRenderBlocksHook`, `BlockNode`, `HTML`                       |

#### Host API Support List

| Host API                        | Purpose                               | Rust                                   | AssemblyScript                       | TinyGo                           |
| ------------------------------- | ------------------------------------- | -------------------------------------- | ------------------------------------ | -------------------------------- |
| Logging (`host.log`)            | Debug logs from the sandbox           | `log_info!`, `log_warn!`, `log_error!` | `log.info`, `log.warn`, `log.error`  | `LogInfo`, `LogWarn`, `LogError` |
| Reading settings (`config_get`) | Reading `[plugins.settings.<plugin>]` | `config_get`, `config_get_or`          | `config.get`, `config.getOr`         | `ConfigGet`, `ConfigGetOr`       |
| KVS (`kv_get`, `kv_set`)        | Persistent key-value store            | `kv_get`, `kv_set`                     | `kv.get`, `kv.set`                   | `KVGet`, `KVSet`                 |
| HTTP (`http_fetch`)             | External requests to allowed domains  | `http_fetch`, `http_post_json`         | `http.fetch`, `HttpRequest.postJson` | `HTTPFetch`, `NewHTTPPostJSON`   |

---

### 5.4 Implementing Custom Blocks (`render-blocks`)

A hook that renders custom blocks in Admin Studio and on the public site. Multiple blocks are passed together, so return an HTML array of the same order and length as the input blocks (blocks you do not render are `null` / `nil`).

#### AssemblyScript Example (`assembly/index.ts`)

```typescript
import {
  allocate as sdkAllocate,
  readBlocks,
  writeBlocksHtml,
  writeError,
  BlockNode,
} from "@takumi-mr/plugin-sdk/assembly/index";

export function allocate(len: i32): i32 {
  return sdkAllocate(len);
}

export function render_blocks(ptr: i32, len: i32): i64 {
  const blocks = readBlocks(ptr, len);
  if (blocks === null) return writeError("Invalid render-blocks payload");

  const htmlList = new Array<string | null>(blocks.length);
  for (let i = 0; i < blocks.length; ++i) {
    const block = blocks[i];
    if (block.name === "my-plugin/note") {
      const text = block.attributes.get("text").asString();
      htmlList[i] = `<aside class="my-note"><p>${text}</p></aside>`;
    } else {
      htmlList[i] = null;
    }
  }
  return writeBlocksHtml(htmlList);
}
```

#### TinyGo Example (`main.go`)

```go
package main

import (
	"strings"
	"github.com/takumi-mr/AegisCMS/sdk/tinygo"
)

//export allocate
func allocate(size int32) uintptr {
	return sdk.Allocate(size)
}

//export render_blocks
func render_blocks(ptr int32, length int32) int64 {
	return sdk.RunRenderBlocksHook(ptr, length, func(blocks []sdk.BlockNode) ([]*string, error) {
		results := make([]*string, len(blocks))
		for i, block := range blocks {
			if block.Name == "my-plugin/note" {
				text, _ := block.Attributes["text"].(string)
				html := "<aside class=\"my-note\"><p>" + strings.TrimSpace(text) + "</p></aside>"
				results[i] = sdk.HTML(html)
			} else {
				results[i] = nil
			}
		}
		return results, nil
	})
}

func main() {}
```

> [!TIP]
> For sample code that includes implementations of all hooks (commerce, notifications, comments, blocks, page rendering), see the following.
>
> - AssemblyScript: [`sdk/examples/assemblyscript-hooks`](../sdk/examples/assemblyscript-hooks)
> - TinyGo: [`sdk/tinygo/examples/hooks`](../sdk/tinygo/examples/hooks)

---

### 5.5 Page Rendering (`on-render-page`) and Structured Script Injection (ABI 2)

To add a `<script>` to a public page's `<head>`, do not write a raw `<script>` tag in `extra_head` or `append_content` (the host strips it); add to `scripts` (`InjectedScript`) instead. Declare the host of an external script in the manifest's `capabilities.csp.script_domains`, and inline scripts with `capabilities.inline_scripts: true` (for details see [CSP design document 4](csp.md#4-the-plugin-abi-breaking-change)).

#### AssemblyScript Example (`assembly/index.ts`)

```typescript
import {
  readString,
  writeString,
  parsePageRenderContext,
  serializePageRenderContext,
  InjectedScript,
} from "@takumi-mr/plugin-sdk/assembly/index";

export function on_render_page(ptr: i32, len: i32): i64 {
  const ctx = parsePageRenderContext(readString(ptr, len));
  ctx.extra_css += ".my-banner { padding: 8px; }";
  if (ctx.content_length > 0) {
    ctx.append_content = "<aside class=\"my-banner\">Thank you for reading</aside>";
  }
  ctx.scripts.push(InjectedScript.inline("window.__myPlugin = true;"));
  ctx.scripts.push(
    InjectedScript.external("https://cdn.example.com/widget.js")
      .asyncLoad()
      .deferred()
      .withConsent("analytics"),
  );
  return writeString(serializePageRenderContext(ctx));
}
```

#### TinyGo Example (`main.go`)

```go
//export on_render_page
func on_render_page(ptr int32, length int32) int64 {
	return sdk.RunHook(ptr, length, "PageRenderContext", func(ctx *sdk.PageRenderContext) error {
		ctx.ExtraCSS += ".my-banner { padding: 8px; }"
		if ctx.ContentLength > 0 {
			ctx.AppendContent = "<aside class=\"my-banner\">Thank you for reading</aside>"
		}
		ctx.Scripts = append(
			ctx.Scripts,
			sdk.InlineScript("window.__myPlugin = true;"),
			sdk.ExternalScript("https://cdn.example.com/widget.js").
				Async().
				Deferred().
				WithConsent("analytics"),
		)
		return nil
	})
}
```

---

## 6. Low-Level ABI Specification (Common Specification)

For the WebAssembly memory layout, pointer packing conventions, host API import and export specification, and JSON schemas common to all languages, see the following specification.

👉 **[AegisCMS WebAssembly Plugin Common ABI Specification (`docs/plugin_abi_specification.md`)](./plugin_abi_specification.md)**

---

## 7. Releasing and Distributing SDK Packages on GitHub (Publishing Guide)

The SDKs for all languages are distributed and released centered on GitHub.

### 7.1 Automatic Releases with release-please (Recommended)

Raising versions, tags, and the CHANGELOG are done automatically by release-please from Conventional Commits (for details see [versioning.md](versioning.md)).

- The **Rust SDK** (`crates/aegis-plugin-sdk`) has the same version as the core. The tag is `v<version>`.
- The **AssemblyScript SDK** and **TinyGo SDK** have individual versions. The tags are `sdk-assemblyscript-v<version>` and `sdk/tinygo/v<version>` (Go module convention) respectively.

Merging the **release PR** that appears on `main` creates tags and GitHub Releases. The AssemblyScript SDK is then published to `npm.pkg.github.com` (GitHub Packages) by [`.github/workflows/release-sdk.yml`](../.github/workflows/release-sdk.yml), and for the TinyGo SDK the tag itself is the distribution.

If publishing fails, you can rerun the **Release Plugin SDKs** workflow in Actions with **Run workflow** (specifying the tag in `ref` publishes that version).

---

### 7.2 Manual Release Steps per Language

Steps for releasing manually from local, without using the CI workflow.

#### 1. Rust SDK (`GitHub Git dependency`)

Cargo can resolve dependencies by specifying a GitHub repository directly, so uploading to crates.io is not needed.

1. **Create and push a Git tag for the version**:
    ```bash
    git tag vX.Y.Z   # e.g. v0.2.0
    git push origin vX.Y.Z
    ```
2. **Consumer-side settings**:
   Users can install it just by writing the following in `Cargo.toml`. Cargo resolves the internal macro crate (`aegis-plugin-macros`) from the same repository automatically.
    ```toml
    [dependencies]
    aegis-plugin-sdk = { git = "https://github.com/takumi-mr/AegisCMS", tag = "v0.2.0" }
    ```

#### 2. AssemblyScript SDK (`GitHub Packages`)

1. **Pre-verification**:
    ```bash
    task sdk:pack:dry-run
    ```
2. **Publish to GitHub Packages**:
   Publish with a Personal Access Token (PAT, `write:packages` permission).
    ```bash
    cd sdk/assemblyscript
    npm publish
    ```
3. **Consumer-side settings**:
   Set the registry in the user's project root or `~/.npmrc` and `npm install`.
    ```text
    @takumi-mr:registry=https://npm.pkg.github.com
    ```
    ```bash
    npm install @takumi-mr/plugin-sdk
    ```

#### 3. TinyGo SDK (`Go Modules`)

With Go Modules, a module placed in a subdirectory is published and resolved with a Git tag of the form `<directory path>/<version>`.

1. **Static verification**:
    ```bash
    task sdk:check
    ```
2. **Create and push a Git tag**:
    ```bash
    git tag sdk/tinygo/v0.1.0
    git push origin sdk/tinygo/v0.1.0
    ```
3. **Consumer-side settings**:
   Users can install it with the standard `go get`.
    ```bash
    go get github.com/takumi-mr/AegisCMS/sdk/tinygo@v0.1.0
    ```
