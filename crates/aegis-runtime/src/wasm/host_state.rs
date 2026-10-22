use crate::guard::CapabilityGuard;
use aegis_core::PluginManifest;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use wasmtime::{StoreLimits, StoreLimitsBuilder};

/// WasmtimeのStore内に保持されるホスト状態
pub struct PluginHostState {
    pub manifest: PluginManifest,
    pub kv_storage: Arc<Mutex<HashMap<String, String>>>,
    pub last_error: Option<String>,
    /// Linear memory cap from `resource_limits.max_memory_mb` (installed with `Store::limiter`)
    pub limits: StoreLimits,
}

impl PluginHostState {
    pub fn new(manifest: PluginManifest, kv_storage: Arc<Mutex<HashMap<String, String>>>) -> Self {
        let max_memory_bytes = (manifest.resource_limits.max_memory_mb.max(1) as usize) << 20;
        let limits = StoreLimitsBuilder::new()
            .memory_size(max_memory_bytes)
            // Growing past the cap traps (reported as a plugin error) instead of quietly
            // returning -1 from memory.grow
            .trap_on_grow_failure(true)
            .build();
        Self {
            manifest,
            kv_storage,
            last_error: None,
            limits,
        }
    }

    pub fn guard(&self) -> CapabilityGuard<'_> {
        CapabilityGuard::new(&self.manifest)
    }
}
