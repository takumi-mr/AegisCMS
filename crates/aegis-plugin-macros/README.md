# aegis-plugin-macros

Procedural macros for the AegisCMS WebAssembly (Wasm) plugin SDK.

This crate provides the `#[aegis_plugin]` attribute macro, which automatically generates low-level WebAssembly ABI exports (`allocate`, `deallocate`, hook entrypoints) and memory marshaling logic for AegisCMS plugins.

## Usage

This crate is typically consumed indirectly via [`aegis-plugin-sdk`](https://crates.io/crates/aegis-plugin-sdk).

```toml
[dependencies]
aegis-plugin-sdk = "0.1.0"
```

```rust
use aegis_plugin_sdk::prelude::*;

#[derive(Default)]
pub struct MyPlugin;

#[aegis_plugin]
impl Plugin for MyPlugin {
    fn on_before_save(&mut self, post: PostContext) -> Result<PostContext> {
        Ok(post)
    }
}
```

## License

Licensed under either of Apache License, Version 2.0 or MIT license at your option.
