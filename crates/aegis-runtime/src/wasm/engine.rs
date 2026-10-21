use super::host_state::PluginHostState;
use aegis_core::{AegisError, Result};
use std::sync::Arc;
use wasmtime::{Caller, Config, Engine, Linker};

#[derive(Clone)]
pub struct WasmEngine {
    engine: Engine,
    linker: Arc<Linker<PluginHostState>>,
}

impl WasmEngine {
    pub fn new() -> Result<Self> {
        let mut config = Config::new();
        config.consume_fuel(true);
        config.async_support(true);
        config.wasm_backtrace(false);

        let engine = Engine::new(&config).map_err(|e| {
            AegisError::PluginRuntime(format!("Failed to create Wasmtime engine: {}", e))
        })?;

        let mut linker = Linker::new(&engine);

        // --- ホストAPI群 (host-api) のバインド ---

        // 1. host::log(level, ptr, len)
        linker
            .func_wrap(
                "host",
                "log",
                |mut caller: Caller<'_, PluginHostState>, level: i32, ptr: i32, len: i32| {
                    let memory = match caller.get_export("memory").and_then(|m| m.into_memory()) {
                        Some(m) => m,
                        None => return,
                    };
                    let mut buffer = vec![0u8; len as usize];
                    if memory.read(&caller, ptr as usize, &mut buffer).is_ok() {
                        let msg = String::from_utf8_lossy(&buffer);
                        let plugin_name = &caller.data().manifest.name;
                        match level {
                            0 => tracing::debug!("[Wasm:{}] {}", plugin_name, msg),
                            1 => tracing::info!("[Wasm:{}] {}", plugin_name, msg),
                            2 => tracing::warn!("[Wasm:{}] {}", plugin_name, msg),
                            _ => tracing::error!("[Wasm:{}] {}", plugin_name, msg),
                        }
                    }
                },
            )
            .map_err(|e| AegisError::PluginRuntime(e.to_string()))?;

        // 2. host::http_check_allowed(url_ptr, url_len) -> i32 (1 if allowed, 0 if denied)
        linker
            .func_wrap(
                "host",
                "http_check_allowed",
                |mut caller: Caller<'_, PluginHostState>, ptr: i32, len: i32| -> i32 {
                    let memory = match caller.get_export("memory").and_then(|m| m.into_memory()) {
                        Some(m) => m,
                        None => return 0,
                    };
                    let mut buffer = vec![0u8; len as usize];
                    if memory.read(&caller, ptr as usize, &mut buffer).is_err() {
                        return 0;
                    }
                    let url = String::from_utf8_lossy(&buffer);
                    if caller.data().guard().check_network_access(&url).is_ok() {
                        1
                    } else {
                        0
                    }
                },
            )
            .map_err(|e| AegisError::PluginRuntime(e.to_string()))?;

        // 3. host::kv_set(key_ptr, key_len, val_ptr, val_len) -> i32 (0: ok, 1: denied, 2: err)
        linker
            .func_wrap(
                "host",
                "kv_set",
                |mut caller: Caller<'_, PluginHostState>,
                 k_ptr: i32,
                 k_len: i32,
                 v_ptr: i32,
                 v_len: i32|
                 -> i32 {
                    let memory = match caller.get_export("memory").and_then(|m| m.into_memory()) {
                        Some(m) => m,
                        None => return 2,
                    };

                    let mut k_buf = vec![0u8; k_len as usize];
                    let mut v_buf = vec![0u8; v_len as usize];
                    if memory.read(&caller, k_ptr as usize, &mut k_buf).is_err()
                        || memory.read(&caller, v_ptr as usize, &mut v_buf).is_err()
                    {
                        return 2;
                    }

                    let key = String::from_utf8_lossy(&k_buf).to_string();
                    let val = String::from_utf8_lossy(&v_buf).to_string();

                    let ns = caller
                        .data()
                        .manifest
                        .capabilities
                        .storage
                        .as_ref()
                        .map(|s| s.kv_namespace.clone())
                        .unwrap_or_default();

                    if caller.data().guard().check_storage_access(&ns).is_err() {
                        return 1;
                    }

                    let full_key = format!("{}:{}", ns, key);
                    if let Ok(mut storage) = caller.data().kv_storage.lock() {
                        storage.insert(full_key, val);
                        0
                    } else {
                        2
                    }
                },
            )
            .map_err(|e| AegisError::PluginRuntime(e.to_string()))?;

        Ok(Self {
            engine,
            linker: Arc::new(linker),
        })
    }

    pub fn engine(&self) -> &Engine {
        &self.engine
    }

    pub fn linker(&self) -> &Linker<PluginHostState> {
        &self.linker
    }
}
