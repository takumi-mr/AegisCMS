# aegis-runtime

**English** | [日本語](README.ja.md)

A zero-trust WebAssembly (Wasm) plugin execution engine and supervisor built on Wasmtime.

## Overview

AegisCMS does not allow plugins to access the raw memory of the host or the database directly, as traditional monolithic CMSs do. Every plugin runs isolated in its own linear memory space provided by the Bytecode Alliance's `wasmtime`.

`aegis-runtime` provides loading of plugin Wasm binaries, instance pool management, binding of host API functions, timeout and resource exhaustion monitoring through fuel accounting and epoch interruption, and manifest-based capability validation (the Zero-Trust Capability Guard).

## Responsibilities and Features

- **Wasm execution engine (`wasm::WasmEngine`)**:
    - JIT compilation and instantiation with `wasmtime`.
    - CPU-bound DoS protection through fuel (instruction count) limits.
    - Reliable execution timeout control through epoch-based interruption.
    - Enforcement of the maximum memory allocation (linear memory limit).
    - A hook result cache (`ResultCache`) for rendering optimization.
- **Capability guard (`CapabilityGuard`)**:
    - Runtime validation of the permission allowlist written in the plugin's `plugin.manifest.json` (allowed external domains, KVS namespace, allowed hooks, etc.).
    - Immediate rejection of access to unauthorized resources.
- **Host services and API bindings (`services`, `wasm::host_state`)**:
    - Structured logging (`log_*` -> the host's `tracing`).
    - Per-plugin KVS storage access.
    - An HTTP client with domain validation.
    - Reading settings configured in Admin Studio.
- **Plugin supervisor (`PluginSupervisor`)**:
    - Lifecycle management, enabling/disabling, and ordering of all plugins under the `plugins/` directory.
    - Orchestration of hooks (`on_before_save`, `on_render_page`, `calculate_order`, etc.).

## Module Layout

| Module     | Description                                                                   |
| :--------- | :---------------------------------------------------------------------------- |
| `wasm`     | Wasmtime engine, instance pool, host state, loader, cache                     |
| `guard`    | Manifest permission validation guard (`CapabilityGuard`)                      |
| `plugin`   | Plugin lifecycle and supervisor (`PluginSupervisor`, `AegisPlugin`)           |
| `services` | Service interfaces provided by the host (`HostServices`, `LocalHostServices`) |
| `config`   | Plugin runtime settings (`PluginsConfig`, `RuntimeConfig`)                    |
