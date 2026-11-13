# aegis-plugin-dev

**English** | [日本語](README.ja.md)

A standalone local development and test server for AegisCMS Wasm plugin developers, plus the API that integrates with Plugin Builder.

## Overview

`aegis-plugin-dev` is a development tool for instantly verifying, testing, and debugging a plugin on its own (`plugin.manifest.json` and `plugin.wasm`), without setting up the main server, a database, or administrator authentication.

Started with the `aegis-server plugin dev <dir>` command (or this crate's standalone binary), it lets you directly run and inspect hook functions in the Wasmtime sandbox, and it integrates seamlessly with the Plugin Builder GUI (`plugin-builder/`).

## Responsibilities and Features

- **Direct hook execution (`POST /__dev/api/invoke`)**:
    - Passes a test payload (JSON) and runs the specified hook (`on-before-save`, `transform-content-ast`, `calculate-order`, etc.) directly.
    - Collects and visualizes in real time the fuel consumed (CPU instruction count), execution time (milliseconds), and Wasm log output (`log_*`).
- **Host environment mocking and tracking**:
    - In-memory simulation of the plugin-specific KVS and tracking of key update history.
    - Mock response settings for external HTTP requests (`http::fetch`) and inspection of the requests actually issued.
    - Dynamic injection of plugin settings (`config::get`).
- **Live HTML preview (`POST /__dev/api/preview`)**:
    - For frontend hooks such as `on-render-page` and `render-blocks`, instantly previews the rendered result using sample HTML or a block AST as input.
- **Automatic recompilation and hot reload**:
    - Detects updates to the Wasm binary and manifest and reloads the instance automatically.
- **Plugin Builder UI integration API**:
    - Provides a REST API for running plugin tests and editing the manifest from Plugin Builder (React + Vite) in the browser.

## Usage

```bash
# Start from the integrated AegisCMS command
aegis-server plugin dev plugins/my-plugin

# Specify a port
aegis-server plugin dev plugins/my-plugin --port 4000
```
