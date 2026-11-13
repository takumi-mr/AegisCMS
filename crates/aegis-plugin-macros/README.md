# aegis-plugin-macros

**English** | [日本語](README.ja.md)

Procedural macros for the AegisCMS WebAssembly (Wasm) plugin SDK.

## Overview

`aegis-plugin-macros` provides the attribute macro for developing AegisCMS plugins in Rust. It automatically generates the boilerplate for low-level WebAssembly linear memory reads and writes, the memory allocation and deallocation hooks (`allocate`, `deallocate`), and the C-ABI export functions that conform to the AegisCMS host ABI specification.

## Usage

This crate is normally used through [`aegis-plugin-sdk`](../aegis-plugin-sdk).

The plugin's `Cargo.toml`:

```toml
[dependencies]
aegis-plugin-sdk = "0.2.0"
```

The plugin's `src/lib.rs`:

```rust
use aegis_plugin_sdk::prelude::*;

#[derive(Default)]
pub struct MyPlugin;

#[aegis_plugin]
impl Plugin for MyPlugin {
    fn on_before_save(&mut self, post: PostContext) -> Result<PostContext> {
        Ok(post)
    }

    fn on_render_page(&mut self, ctx: PageRenderContext) -> Result<PageRenderContext> {
        Ok(ctx)
    }
}
```

## Automatically Generated ABI Exports

Adding `#[aegis_plugin]` exports the following functions automatically:

- `allocate(len: i32) -> i32`: the entry point for memory allocation from the host.
- `deallocate(ptr: i32, len: i32)`: the entry point for memory deallocation from the host.
- Hook wrapper functions (`on_before_save`, `transform_content_ast`, `on_render_page`, `render_blocks`, `calculate_order`, `on_order_status_changed`, `check_comment`, `send_notification`, `on_user_authenticated`, `on_content_published`, `on_content_deleted`, etc.): transparently handle JSON deserialization and serialization of input and output with host memory, and error propagation.
