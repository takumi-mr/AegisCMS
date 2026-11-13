# AegisCMS AssemblyScript WebAssembly Plugin Template

**English** | [日本語](README.ja.md)

The official AegisCMS template for developing AssemblyScript (TypeScript-like syntax) WebAssembly plugins.  
Using your knowledge of the Node.js / npm ecosystem and TypeScript, you can quickly build lightweight and safe Wasm plugins.

---

## 🚀 Quick Start

### 1. Prerequisites

Node.js (v18+) and npm are required.

```bash
# Install the dependency (the assemblyscript compiler)
npm install
```

### 2. Build the WebAssembly Binary

Generate the `.wasm` with the release profile.

```bash
npm run build
```

On success, `plugin.wasm` is written to the directory root.

Do not remove `"use": ["abort=assembly/sdk/host/abort"]` from `asconfig.json`. AssemblyScript imports `env.abort` by default to handle runtime errors, but the host provides only the `host` module, so without this setting the plugin cannot be loaded (it is replaced with the SDK's `abort`, which writes a log and then stops).

### 3. Deploy to AegisCMS

Place the following files in the AegisCMS server's `plugins/<name>/` (`<name>` is the manifest's `name`), add `<name>` to `[plugins].enabled` in the config file, and restart the server to enable it (placing the files alone does not load the plugin).

- `plugin.manifest.json` (permissions, resource limits, hook definitions)
- `plugin.wasm` (the compiled Wasm binary)

---

## 🧩 Plugin Structure

```text
sdk/templates/assemblyscript/
├── package.json               # npm dependencies (references @takumi-mr/plugin-sdk) and build scripts
├── asconfig.json              # AssemblyScript compiler settings
├── plugin.manifest.json       # Plugin manifest (permissions and hook declarations)
├── plugin.wasm                # Built Wasm binary
└── assembly/
    ├── index.ts               # Plugin lifecycle hook implementation
    └── sdk/                   # Backward-compatibility shim (re-exports @takumi-mr/plugin-sdk)
```

Implement against the official npm package [`@takumi-mr/plugin-sdk`](../../assemblyscript).

### Manifest (`plugin.manifest.json`)

Based on the zero-trust execution model, declare the minimum capabilities (permissions) the plugin needs. Any undeclared operation is blocked immediately by the host runtime.

```json
{
  "name": "my-as-plugin",
  "version": "1.0.0",
  "entrypoint": "plugin.wasm",
  "abi_version": 2,
  "capabilities": {
    "network": { "allowed_domains": ["api.example.com"] },
    "storage": { "kv_namespace": "my_as_state" },
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
> To inject a `<script>` in `on-render-page`, do not write a raw `<script>` tag in `extra_head` or `append_content` (the host strips it); use `ctx.scripts.push(InjectedScript.external(...) / .inline(...))` instead. External scripts require declaring `capabilities.csp.script_domains`, and inline scripts require `capabilities.inline_scripts: true`.

### Available Lifecycle Hooks

| Exported function                           | Arguments    | Return | Description                                                          |
| ------------------------------------------- | ------------ | ------ | -------------------------------------------------------------------- |
| `allocate(len: i32)`                        | `len: i32`   | `i32`  | Memory allocator (required)                                          |
| `on_before_save(ptr: i32, len: i32)`        | `(ptr, len)` | `i64`  | Validation, value correction, and rejection before saving an article |
| `transform_content_ast(ptr: i32, len: i32)` | `(ptr, len)` | `i64`  | Transforming the body block AST array                                |
| `on_render_page(ptr: i32, len: i32)`        | `(ptr, len)` | `i64`  | Dynamically updating the public page rendering context               |

### Available Host APIs

| Module | Function                                                             | Description                                                   |
| ------ | -------------------------------------------------------------------- | ------------------------------------------------------------- |
| `log`  | `log.debug(msg)`, `log.info(msg)`, `log.warn(msg)`, `log.error(msg)` | Writes logs safely through the host's logger                  |
| `net`  | `net.isAllowed(url)`                                                 | Checks whether the destination URL is allowed by the manifest |
| `kv`   | `kv.set(key, value)`                                                 | Saves data to the KVS in the plugin's own namespace           |
