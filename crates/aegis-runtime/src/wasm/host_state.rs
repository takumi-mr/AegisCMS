use crate::guard::CapabilityGuard;
use aegis_core::PluginManifest;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

/// WasmtimeのStore内に保持されるホスト状態
pub struct PluginHostState {
    pub manifest: PluginManifest,
    pub kv_storage: Arc<Mutex<HashMap<String, String>>>,
    pub last_error: Option<String>,
}

impl PluginHostState {
    pub fn new(manifest: PluginManifest, kv_storage: Arc<Mutex<HashMap<String, String>>>) -> Self {
        Self {
            manifest,
            kv_storage,
            last_error: None,
        }
    }

    pub fn guard(&self) -> CapabilityGuard<'_> {
        CapabilityGuard::new(&self.manifest)
    }
}
