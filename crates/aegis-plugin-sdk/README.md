# aegis-plugin-sdk

Official Rust SDK for AegisCMS WebAssembly (Wasm) plugins.

[AegisCMS](https://github.com/takumi-mr/AegisCMS) provides a zero-trust WebAssembly plugin system powered by Bytecode Alliance's Wasmtime runtime. This crate provides safe, idiomatic Rust abstractions for writing high-performance AegisCMS plugins with full type safety and zero unsafe boilerplate.

## Features

- **Macro-based Hook Generation**: Use `#[aegis_plugin]` to implement lifecycle hooks with automatic Wasm ABI exports.
- **Rich Hook Ecosystem**:
  - `on_before_save`: Validate and transform content before saving.
  - `transform_content_ast`: Manipulate block tree AST structures.
  - `on_render_page`: Post-process rendered HTML responses.
  - `calculate_order`: Inject custom pricing, discount rules, or tax calculations in commerce flows.
  - `on_order_status_changed`: React to order lifecycle transitions (paid, shipped, refunded).
  - `check_comment`: Filter or moderate user comments.
  - `send_notification`: Handle or dispatch multi-channel notifications.
- **Host APIs**:
  - **Structured Logging**: `log_debug!`, `log_info!`, `log_warn!`, `log_error!` seamlessly bridging to host `tracing` logs.
  - **Capability-Gated Network**: `net::is_allowed` to verify domain permissions ahead of external requests.
  - **Isolated KVS**: `kv::get`, `kv::set`, `kv::delete`, `kv::get_json`, `kv::set_json` isolated per plugin namespace.
  - **Dynamic Configuration**: `config::get` to access plugin settings configured in Admin Studio.
  - **Host HTTP Client**: `http::fetch` for outbound HTTP requests within capability boundaries.
- **Native Unit Testing**: Native mocks built into the SDK allow testing plugins with standard `cargo test` without compiling to Wasm first.

## Quick Start

Add `aegis-plugin-sdk` and `serde` to your `Cargo.toml`:

```toml
[package]
name = "my-aegis-plugin"
version = "0.1.0"
edition = "2021"

[lib]
crate-type = ["cdylib", "rlib"]

[dependencies]
aegis-plugin-sdk = "0.1.0"
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"
```

Implement your plugin in `src/lib.rs`:

```rust
use aegis_plugin_sdk::prelude::*;

#[derive(Default)]
pub struct MyPlugin;

#[aegis_plugin]
impl Plugin for MyPlugin {
    fn on_before_save(&mut self, mut post: PostContext) -> Result<PostContext> {
        log_info!("Processing post: '{}'", post.title);

        if post.title.trim().is_empty() {
            return Err(PluginError::Rejected("Title is required".into()));
        }

        kv::set("last_saved_slug", &post.slug)?;
        Ok(post)
    }
}
```

Build the Wasm binary:

```bash
cargo build --target wasm32-unknown-unknown --release
```

Place `target/wasm32-unknown-unknown/release/my_aegis_plugin.wasm` alongside `plugin.manifest.json` into `plugins/my-plugin/` in your AegisCMS installation.

## Documentation & Guide

- [AegisCMS Plugin Development Guide](https://github.com/takumi-mr/AegisCMS/blob/main/docs/plugin_sdk_guide.md)
- [Plugin ABI Specification](https://github.com/takumi-mr/AegisCMS/blob/main/docs/plugin_abi_specification.md)

## License

Licensed under either of Apache License, Version 2.0 or MIT license at your option.
