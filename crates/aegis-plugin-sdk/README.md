# aegis-plugin-sdk

**English** | [日本語](README.ja.md)

The official Rust SDK for AegisCMS WebAssembly (Wasm) plugins.

AegisCMS uses a zero-trust plugin system built on the Bytecode Alliance's Wasmtime runtime. `aegis-plugin-sdk` provides type-safe, intuitive Rust abstractions so you can build plugins safely and quickly, without writing boilerplate or unsafe code.

## Key Features

- **Macro-generated hooks**: implement lifecycle hooks with `#[aegis_plugin]` (or `register_plugin!`), and the Wasm ABI exports are generated automatically.
- **A variety of lifecycle hooks**:
  - `on_before_save`: validation and automatic processing before an article or content is saved.
  - `transform_content_ast`: traversing and rewriting the block AST (syntax tree).
  - `on_render_page`: inserting custom CSS/JS into the rendered HTML or the `<head>` tag.
  - `render_blocks`: rendering the plugin's own custom blocks to HTML.
  - `on_user_authenticated`: role assignment at user login and syncing external IdP profiles.
  - `calculate_order`: dynamic price calculation, coupon discounts, and tax and shipping calculation in the e-commerce order flow.
  - `on_order_status_changed`: processing in response to order status transitions (payment completed, shipped, refunded, etc.).
  - `check_comment`: automatic comment evaluation (spam score calculation, banned word detection, approval state control).
  - `send_notification`: multi-channel notification handling for email, chat, and so on.
  - `on_content_published`: external notifications and cache warming when content is published.
  - `on_content_deleted`: deleting externally linked resources when content is deleted.
- **Host API**:
  - **Structured logging**: `log_debug!`, `log_info!`, `log_warn!`, `log_error!` write safely to the host's `tracing` log.
  - **Capability-controlled networking**: safe external HTTP communication to the domains allowed by the manifest, via `net::is_allowed` and `http::fetch`.
  - **Isolated KVS**: a key-value store with a namespace separated per plugin (`kv::get`, `kv::set`, `kv::delete`, `kv::get_json`, `kv::set_json`).
  - **Reading settings**: reading the plugin settings configured in Admin Studio (`config::get`).
- **Native unit tests**: with the SDK's built-in mocks, you can verify plugin behavior with plain `cargo test`, without compiling to Wasm.

## Quick Start

The plugin's `Cargo.toml`:

```toml
[package]
name = "my-aegis-plugin"
version = "0.2.0"
edition = "2021"

[lib]
crate-type = ["cdylib", "rlib"]

[dependencies]
aegis-plugin-sdk = "0.2.0"
serde = { version = "1.0", features = ["derive"] }
```

The plugin implementation `src/lib.rs`:

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

    fn on_render_page(&mut self, mut ctx: PageRenderContext) -> Result<PageRenderContext> {
        ctx.head_html.push_str("<style>.aegis-highlight { color: #0066cc; }</style>");
        Ok(ctx)
    }
}
```

Build the Wasm binary:

```bash
cargo build --target wasm32-unknown-unknown --release
```

Place the built `target/wasm32-unknown-unknown/release/my_aegis_plugin.wasm` together with `plugin.manifest.json` in AegisCMS's `plugins/my-plugin/` directory.

## Documentation

- [Plugin development guide](../../docs/plugin_sdk_guide.md)
- [Plugin ABI specification](../../docs/plugin_abi_specification.md)
