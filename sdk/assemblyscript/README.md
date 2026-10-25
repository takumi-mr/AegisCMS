# @takumi-mr/plugin-sdk

Official AssemblyScript SDK for AegisCMS WebAssembly (Wasm) plugins, published via GitHub Packages.

[AegisCMS](https://github.com/takumi-mr/AegisCMS) supports writing zero-trust WebAssembly plugins in AssemblyScript, offering TypeScript-like syntax and direct compilation to lightweight WebAssembly binaries.

## Installation (GitHub Packages)

Configure `.npmrc` in your project or home directory:

```text
@takumi-mr:registry=https://npm.pkg.github.com
```

Install the package:

```bash
npm install @takumi-mr/plugin-sdk --save
```

Ensure `assemblyscript` is also installed in your project:

```bash
npm install assemblyscript --save-dev
```

## Quick Start

### 1. `asconfig.json` Configuration

In your plugin's `asconfig.json`, configure the custom abort handler so that runtime traps route safely through the host logging interface:

```json
{
  "targets": {
    "release": {
      "outFile": "plugin.wasm",
      "optimizeLevel": 3,
      "shrinkLevel": 2,
      "converge": true,
      "noExportRuntime": false
    }
  },
  "options": {
    "bindings": "esm",
    "use": [
      "abort=~lib/@takumi-mr/plugin-sdk/assembly/host/abort"
    ]
  }
}
```

### 2. Implement Plugin Hooks (`assembly/index.ts`)

```typescript
import {
  allocate as sdkAllocate,
  readString,
  writeString,
  writeError,
  log,
  net,
  kv,
  parsePostContext,
  serializePostContext,
  PostContext,
} from "@takumi-mr/plugin-sdk/assembly/index";

// Required memory allocator export
export function allocate(len: i32): i32 {
  return sdkAllocate(len);
}

// Hook: on_before_save
export function on_before_save(ptr: i32, len: i32): i64 {
  const inJson = readString(ptr, len);
  const post = parsePostContext(inJson);

  log.info("[AssemblyScript] on_before_save called for: " + post.title);

  // Validate title
  if (post.title.trim().length == 0) {
    return writeError("Post title cannot be empty");
  }

  // Network permission check
  if (net.isAllowed("https://api.example.com/webhook")) {
    log.info("Outbound webhook destination is allowed");
  }

  // State persistence in plugin KV store
  kv.set("last_saved_slug", post.slug);

  const outJson = serializePostContext(post);
  return writeString(outJson);
}
```

### 3. Build WebAssembly Binary

```bash
npx asc assembly/index.ts --target release
```

This compiles your code into `plugin.wasm` ready to be loaded by AegisCMS.

## Supported Lifecycle Hooks

- `on_before_save(ptr: i32, len: i32): i64`
- `transform_content_ast(ptr: i32, len: i32): i64`
- `on_render_page(ptr: i32, len: i32): i64`

## Host APIs

- `log.debug(msg)`, `log.info(msg)`, `log.warn(msg)`, `log.error(msg)`
- `net.isAllowed(url: string): bool`
- `kv.set(key: string, value: string): bool`

## License

Licensed under either of Apache License, Version 2.0 or MIT license at your option.
