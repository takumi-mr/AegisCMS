# AegisCMS TinyGo WebAssembly Plugin Template

**English** | [日本語](README.ja.md)

The official AegisCMS template for developing TinyGo (Go language) WebAssembly plugins.  
Taking advantage of Go's intuitive syntax and its ability to produce lightweight Wasm binaries, you can build high-performance and safe Wasm plugins.

---

## 🚀 Quick Start

### 1. Prerequisites

- **Go** (1.21 or later)
- **TinyGo** (0.31 or later)

For how to install TinyGo, see the [official TinyGo documentation](https://tinygo.org/getting-started/install/).

### 2. Build the WebAssembly Binary

Build with the TinyGo compiler for the `wasm-unknown` target (TinyGo 0.30+). The `wasi` target imports WASI functions that the host does not provide, so AegisCMS cannot load it. If the module exports `_initialize`, the host calls it before any hook to initialize the runtime.

```bash
tinygo build -target=wasm-unknown -o plugin.wasm main.go
```

For optimization and size reduction:

```bash
tinygo build -target=wasm-unknown -opt=z -no-debug -o plugin.wasm main.go
```

On success, `plugin.wasm` is written to the directory root.

### 3. Deploy to AegisCMS

Place the following files in the AegisCMS server's `plugins/<name>/` (`<name>` is the manifest's `name`), add `<name>` to `[plugins].enabled` in the config file, and restart the server to enable it (placing the files alone does not load the plugin).

- `plugin.manifest.json` (permissions, resource limits, hook definitions)
- `plugin.wasm` (the compiled Wasm binary)

---

## 🧩 Plugin Structure

```text
sdk/templates/tinygo/
├── go.mod                     # Go module definition (references github.com/takumi-mr/AegisCMS/sdk/tinygo)
├── plugin.manifest.json       # Plugin manifest (permissions and hook declarations)
├── plugin.wasm                # Built Wasm binary
└── main.go                    # Plugin lifecycle hook implementation and exports
```

Implement against the official SDK package [`github.com/takumi-mr/AegisCMS/sdk/tinygo`](../../tinygo).

### Manifest (`plugin.manifest.json`)

Based on the zero-trust execution model, declare the minimum capabilities (permissions) the plugin needs. Any undeclared operation is blocked immediately by the host runtime.

```json
{
  "name": "my-tinygo-plugin",
  "version": "1.0.0",
  "entrypoint": "plugin.wasm",
  "abi_version": 2,
  "capabilities": {
    "network": { "allowed_domains": ["api.example.com"] },
    "storage": { "kv_namespace": "my_tinygo_state" },
    "hooks": ["on-before-save", "transform-content-ast", "on-render-page"]
  },
  "resource_limits": {
    "max_memory_mb": 16,
    "max_fuel_units": 100000000,
    "execution_timeout_ms": 250
  }
}
```

> [!NOTE]
> To inject a `<script>` in `on-render-page`, do not write a raw `<script>` tag in `ExtraHead` or `AppendContent` (the host strips it); add `sdk.ExternalScript(...)` / `sdk.InlineScript(...)` to `ctx.Scripts` instead. External scripts require declaring `capabilities.csp.script_domains`, and inline scripts require `capabilities.inline_scripts: true`.

### Available Lifecycle Hooks

| Exported function                                  | Arguments     | Return    | Description                                                          |
| -------------------------------------------------- | ------------- | --------- | -------------------------------------------------------------------- |
| `allocate(size: int32)`                            | `size: int32` | `uintptr` | Memory allocator (required)                                          |
| `on_before_save(ptr: int32, length: int32)`        | `(ptr, len)`  | `int64`   | Validation, value correction, and rejection before saving an article |
| `transform_content_ast(ptr: int32, length: int32)` | `(ptr, len)`  | `int64`   | Transforming the body block AST array                                |
| `on_render_page(ptr: int32, length: int32)`        | `(ptr, len)`  | `int64`   | Dynamically updating the public page rendering context               |

### Available Host APIs

| Function                                                                         | Description                                                   |
| -------------------------------------------------------------------------------- | ------------------------------------------------------------- |
| `sdk.LogDebug(msg)`, `sdk.LogInfo(msg)`, `sdk.LogWarn(msg)`, `sdk.LogError(msg)` | Writes logs safely through the host's logger                  |
| `sdk.NetIsAllowed(url)`                                                          | Checks whether the destination URL is allowed by the manifest |
| `sdk.KVSet(key, value)`                                                          | Saves data to the KVS in the plugin's own namespace           |
