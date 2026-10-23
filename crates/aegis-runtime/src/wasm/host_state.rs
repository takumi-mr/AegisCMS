use crate::guard::CapabilityGuard;
use crate::services::PluginServices;
use aegis_core::PluginManifest;
use wasmtime::{StoreLimits, StoreLimitsBuilder};

/// 1 回のフック呼び出しで `host.http_fetch` を呼べる回数
pub const MAX_FETCHES_PER_CALL: u32 = 8;

/// WasmtimeのStore内に保持されるホスト状態
pub struct PluginHostState {
    pub manifest: PluginManifest,
    /// KV ストア・HTTP・設定値（サーバーが実装する）
    pub services: PluginServices,
    /// この呼び出しで `host.http_fetch` を呼んだ回数
    pub fetch_count: u32,
    pub last_error: Option<String>,
    /// Linear memory cap from `resource_limits.max_memory_mb` (installed with `Store::limiter`)
    pub limits: StoreLimits,
}

impl PluginHostState {
    pub fn new(manifest: PluginManifest, services: PluginServices) -> Self {
        let max_memory_bytes = (manifest.resource_limits.max_memory_mb.max(1) as usize) << 20;
        let limits = StoreLimitsBuilder::new()
            .memory_size(max_memory_bytes)
            // Growing past the cap traps (reported as a plugin error) instead of quietly
            // returning -1 from memory.grow
            .trap_on_grow_failure(true)
            .build();
        Self {
            manifest,
            services,
            fetch_count: 0,
            last_error: None,
            limits,
        }
    }

    pub fn guard(&self) -> CapabilityGuard<'_> {
        CapabilityGuard::new(&self.manifest)
    }
}
