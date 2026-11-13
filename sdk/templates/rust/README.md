# AegisCMS Rust WebAssembly Plugin Template

**English** | [日本語](README.ja.md)

The official AegisCMS template for developing Rust Wasm plugins.  
With the high-level `#[aegis_plugin]` macro, you can implement type-safe lifecycle hooks and host APIs without having to think about low-level Wasm memory management or JSON packing.

---

## 🚀 Quick Start

### 1. Prerequisites

The Rust toolchain and the `wasm32-unknown-unknown` target are required.

```bash
rustup target add wasm32-unknown-unknown
```

### 2. Local Testing (Native Execution)

The SDK has built-in mock bindings for native environments, so you can unit-test your logic with plain `cargo test` without compiling to Wasm.

```bash
cargo test
```

### 3. Build the WebAssembly Binary

Generate the `.wasm` with the release profile.

```bash
cargo build --target wasm32-unknown-unknown --release
```

Place the generated binary in this directory as `plugin.wasm` (running `task plugin:build` at the repository root performs the build and placement in one go).

```bash
# Windows (PowerShell)
Copy-Item target/wasm32-unknown-unknown/release/aegis_plugin_template.wasm plugin.wasm

# Linux / macOS
cp target/wasm32-unknown-unknown/release/aegis_plugin_template.wasm plugin.wasm
```

### 4. Deploy to AegisCMS

Place the following files in the AegisCMS server's `plugins/<name>/` (`<name>` is the manifest's `name`), add `<name>` to `[plugins].enabled` in the config file, and restart the server to enable it (placing the files alone does not load the plugin).

- `plugin.manifest.json` (permissions, resource limits, hook definitions)
- `plugin.wasm` (the compiled Wasm binary)

---

## 🧩 Plugin Structure

### Manifest (`plugin.manifest.json`)

Based on the zero-trust execution model, declare the minimum capabilities (permissions) the plugin needs. Any undeclared operation is blocked immediately by the host runtime.

```json
{
  "name": "my-plugin",
  "version": "1.0.0",
  "entrypoint": "plugin.wasm",
  "capabilities": {
    "network": { "allowed_domains": ["api.example.com"] },
    "storage": { "kv_namespace": "my_plugin_state" },
    "hooks": ["on-before-save", "transform-content-ast"]
  },
  "resource_limits": {
    "max_memory_mb": 16,
    "max_fuel_units": 100000000,
    "execution_timeout_ms": 250
  }
}
```

### Lifecycle Hooks (the `Plugin` trait)

```rust
use aegis_plugin_sdk::prelude::*;

#[derive(Default)]
pub struct MyPlugin;

#[aegis_plugin]
impl Plugin for MyPlugin {
    /// Hook before an article is saved
    fn on_before_save(&mut self, mut post: PostContext) -> Result<PostContext> {
        post.title = format!("[Review] {}", post.title);
        Ok(post)
    }

    /// AST transformation hook (it is re-applied to already saved blocks, so write it to be idempotent)
    fn transform_content_ast(&mut self, mut blocks: Vec<BlockNode>) -> Result<Vec<BlockNode>> {
        let text = "Appended by plugin";
        if !blocks.iter().any(|b| b.text.as_deref() == Some(text)) {
            blocks.push(BlockNode::paragraph(text));
        }
        Ok(blocks)
    }
}
```

### Available Host APIs

| Module | Function                                             | Description                                                   |
| ------ | ---------------------------------------------------- | ------------------------------------------------------------- |
| `log`  | `log_debug!`, `log_info!`, `log_warn!`, `log_error!` | Writes logs safely through the host's logger                  |
| `net`  | `net::is_allowed(url)`                               | Checks whether the destination URL is allowed by the manifest |
| `kv`   | `kv::set(key, val)`, `kv::set_json(key, val)`        | Saves data to the KVS in the plugin's own namespace           |
