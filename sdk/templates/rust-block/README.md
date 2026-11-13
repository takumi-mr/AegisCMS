# AegisCMS Rust Block Plugin Template

**English** | [日本語](README.ja.md)

A template for a plugin that adds a **custom block** that can be placed in the body (the `render-blocks` hook). It is a minimal example that adds one note (a callout for supplementary information or warnings), and includes the input field declaration (`blocks` in `plugin.manifest.json`), HTML generation (`render_blocks` in `src/lib.rs`), and native unit tests.

**The steps are collected in [Creating your first block plugin](../../../docs/plugin_block_tutorial.md)** (a Japanese document: copy → edit → test → build → dev server → an article on a real server).

```bash
cargo test                                                    # test without Wasm
cargo build --target wasm32-unknown-unknown --release         # build
cp ../../../target/wasm32-unknown-unknown/release/aegis_plugin_block_template.wasm plugin.wasm
```

Running `task plugin:build:block` at the repository root performs the build and places `plugin.wasm` in one go.

- When you copy this folder to `plugins/<name>/` to use it, change the `path` of `aegis-plugin-sdk` to `../../crates/aegis-plugin-sdk`, and to make it a standalone crate, add an empty `[workspace]` at the end of `Cargo.toml` (step 1 of the tutorial).
- Make the manifest's `name` the same as the destination folder name.
- Always `escape` user input before putting it into HTML.
