use crate::plugin::PluginSupervisor;
use crate::wasm::engine::WasmEngine;
use crate::wasm::instance::WasmPluginInstance;
use aegis_core::{AegisError, PluginManifest, PluginResourceLimits, Result};
use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, Mutex};

pub struct WasmPluginLoader;

impl WasmPluginLoader {
    /// Loads one plugin directory. `default_limits` (the server's `[runtime].default_*`) fills
    /// the `resource_limits` fields its manifest omits.
    pub fn load_plugin<P: AsRef<Path>>(
        plugin_dir: P,
        engine: WasmEngine,
        kv_storage: Arc<Mutex<HashMap<String, String>>>,
        default_limits: &PluginResourceLimits,
    ) -> Result<Arc<WasmPluginInstance>> {
        let dir = plugin_dir.as_ref();
        let manifest_path = dir.join("plugin.manifest.json");
        if !manifest_path.exists() {
            return Err(AegisError::Config(format!(
                "Manifest not found in plugin directory: {:?}",
                dir
            )));
        }

        let manifest_str = std::fs::read_to_string(&manifest_path)
            .map_err(|e| AegisError::Config(format!("Failed to read manifest: {}", e)))?;
        let manifest = PluginManifest::from_json_with_defaults(&manifest_str, default_limits)
            .map_err(|e| AegisError::Config(format!("Failed to parse manifest: {}", e)))?;

        let wasm_path = dir.join(&manifest.entrypoint);
        if !wasm_path.exists() {
            tracing::warn!(
                "Wasm binary '{:?}' not found for plugin '{}' - skipping binary load",
                wasm_path,
                manifest.name
            );
            return Err(AegisError::NotFound(format!(
                "Wasm binary not found: {:?}",
                wasm_path
            )));
        }

        let wasm_bytes = std::fs::read(&wasm_path).map_err(AegisError::Io)?;

        let instance = WasmPluginInstance::from_bytes(engine, manifest, &wasm_bytes, kv_storage)?;
        Ok(Arc::new(instance))
    }

    pub fn load_all_plugins<P: AsRef<Path>>(
        plugins_dir: P,
        engine: WasmEngine,
        supervisor: &mut PluginSupervisor,
        kv_storage: Arc<Mutex<HashMap<String, String>>>,
        default_limits: &PluginResourceLimits,
    ) -> Result<usize> {
        let path = plugins_dir.as_ref();
        if !path.exists() {
            return Ok(0);
        }

        let mut count = 0;
        let entries = std::fs::read_dir(path).map_err(AegisError::Io)?;
        for entry in entries {
            let entry = entry.map_err(AegisError::Io)?;
            let entry_path = entry.path();
            if entry_path.is_dir() {
                match Self::load_plugin(
                    &entry_path,
                    engine.clone(),
                    kv_storage.clone(),
                    default_limits,
                ) {
                    Ok(instance) => {
                        let limits = &instance.manifest.resource_limits;
                        tracing::info!(
                            "Successfully loaded Wasm plugin: {} (memory {} MiB, fuel {}, timeout {} ms)",
                            instance.manifest.name,
                            limits.max_memory_mb,
                            limits.max_fuel_units,
                            limits.execution_timeout_ms
                        );
                        supervisor.register(instance);
                        count += 1;
                    }
                    Err(e) => {
                        tracing::debug!("Could not load plugin from {:?}: {}", entry_path, e);
                    }
                }
            }
        }

        Ok(count)
    }
}
