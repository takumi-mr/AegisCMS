use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use wasmtime::{Instance, Module, Store, TypedFunc};

use aegis_ast::BlockNode;
use aegis_core::{
    AegisError, PageRenderContext, PluginManifest, PostContext, Result, UserAuthContext,
};

use super::engine::WasmEngine;
use super::host_state::PluginHostState;
use crate::plugin::AegisPlugin;

pub struct WasmPluginInstance {
    pub manifest: PluginManifest,
    pub engine: WasmEngine,
    pub module: Module,
    pub kv_storage: Arc<Mutex<HashMap<String, String>>>,
}

impl WasmPluginInstance {
    pub fn from_bytes(
        engine: WasmEngine,
        manifest: PluginManifest,
        wasm_bytes: &[u8],
        kv_storage: Arc<Mutex<HashMap<String, String>>>,
    ) -> Result<Self> {
        let module = Module::new(engine.engine(), wasm_bytes).map_err(|e| {
            AegisError::PluginRuntime(format!("Failed to compile Wasm module: {}", e))
        })?;

        Ok(Self {
            manifest,
            engine,
            module,
            kv_storage,
        })
    }

    /// 各フック呼び出しごとに独立した Store / Instance を初期化（Zero-Trust & 完全分離）
    async fn instantiate(&self) -> Result<(Store<PluginHostState>, Instance)> {
        let host_state = PluginHostState::new(self.manifest.clone(), self.kv_storage.clone());
        let mut store = Store::new(self.engine.engine(), host_state);
        // メモリ上限（max_memory_mb）。初期メモリが上限を超えるモジュールはインスタンス化で失敗する
        store.limiter(|state| &mut state.limits);

        // Fuel設定（CPU暴走・無限ループ防止）
        store
            .set_fuel(self.manifest.resource_limits.max_fuel_units)
            .map_err(|e| AegisError::ResourceLimitExceeded(e.to_string()))?;

        let instance = self
            .engine
            .linker()
            .instantiate_async(&mut store, &self.module)
            .await
            .map_err(|e| AegisError::PluginRuntime(format!("Instantiation failed: {}", e)))?;

        Ok((store, instance))
    }

    /// ゲストメモリに文字列・データを安全に書き込む
    async fn write_to_guest(
        store: &mut Store<PluginHostState>,
        instance: &Instance,
        data: &[u8],
    ) -> Result<(i32, i32)> {
        let memory = instance.get_memory(&mut *store, "memory").ok_or_else(|| {
            AegisError::PluginRuntime("Plugin missing 'memory' export".to_string())
        })?;

        // ゲスト側のメモリアロケータを呼び出す（allocate(len) -> ptr）
        let alloc_func: TypedFunc<i32, i32> = instance
            .get_typed_func(&mut *store, "allocate")
            .map_err(|_| {
                AegisError::PluginRuntime("Plugin missing 'allocate' export".to_string())
            })?;

        let ptr = alloc_func
            .call_async(&mut *store, data.len() as i32)
            .await
            .map_err(|e| AegisError::PluginRuntime(format!("Guest allocate() failed: {}", e)))?;

        memory
            .write(&mut *store, ptr as usize, data)
            .map_err(|e| AegisError::PluginRuntime(format!("Memory write failed: {}", e)))?;

        Ok((ptr, data.len() as i32))
    }

    /// ゲストメモリからデータを読み取る
    fn read_from_guest(
        store: &mut Store<PluginHostState>,
        instance: &Instance,
        ptr: i32,
        len: i32,
    ) -> Result<Vec<u8>> {
        let memory = instance.get_memory(&mut *store, "memory").ok_or_else(|| {
            AegisError::PluginRuntime("Plugin missing 'memory' export".to_string())
        })?;

        let mut buffer = vec![0u8; len as usize];
        memory
            .read(&mut *store, ptr as usize, &mut buffer)
            .map_err(|e| AegisError::PluginRuntime(format!("Memory read failed: {}", e)))?;

        Ok(buffer)
    }

    /// Calls `export` with `input` as JSON in a fresh instance, under the manifest's timeout.
    /// Returns the JSON the guest wrote back, or `None` when the module does not export the
    /// function or returned nothing (length 0), meaning "unchanged".
    async fn call_hook(&self, hook: &str, export: &str, input: &[u8]) -> Result<Option<Vec<u8>>> {
        let host_state = PluginHostState::new(self.manifest.clone(), self.kv_storage.clone());
        host_state.guard().check_hook(hook)?;

        let (mut store, instance) = self.instantiate().await?;
        let Ok(func) = instance.get_typed_func::<(i32, i32), i64>(&mut store, export) else {
            return Ok(None);
        };
        let (ptr, len) = Self::write_to_guest(&mut store, &instance, input).await?;

        let timeout_ms = self.manifest.resource_limits.execution_timeout_ms.max(50);
        let duration = std::time::Duration::from_millis(timeout_ms);
        let packed_res =
            match tokio::time::timeout(duration, func.call_async(&mut store, (ptr, len))).await {
                Ok(res) => res.map_err(|e| {
                    AegisError::PluginRuntime(format!("Hook '{}' error: {}", export, e))
                })?,
                Err(_) => {
                    return Err(AegisError::ResourceLimitExceeded(format!(
                        "Plugin '{}' exceeded timeout limit of {}ms",
                        self.manifest.name, timeout_ms
                    )))
                }
            };

        // 返り値は i64: 上位32bitがptr、下位32bitがlen
        let res_ptr = (packed_res >> 32) as i32;
        let res_len = (packed_res & 0xFFFF_FFFF) as i32;
        if res_len <= 0 {
            return Ok(None);
        }
        Self::read_from_guest(&mut store, &instance, res_ptr, res_len).map(Some)
    }
}

/// Reads what a hook that may refuse (`on_before_save`, `on_user_authenticated`) returned: the
/// (possibly updated) value, or `{"error": "<message>"}` when the plugin refuses
/// (`abi::write_error_to_host` in the SDK)
fn parse_refusable_output<T: serde::de::DeserializeOwned>(
    plugin: &str,
    type_name: &str,
    out_bytes: &[u8],
) -> Result<T> {
    let invalid = |e: serde_json::Error| {
        AegisError::Serialization(format!("Invalid {} returned: {}", type_name, e))
    };
    let value: serde_json::Value = serde_json::from_slice(out_bytes).map_err(invalid)?;
    if let Some(error) = value.get("error") {
        let message = error
            .as_str()
            .map(str::to_string)
            .unwrap_or_else(|| error.to_string());
        return Err(AegisError::PluginRejected(format!(
            "{}: {}",
            plugin, message
        )));
    }
    serde_json::from_value(value).map_err(invalid)
}

fn to_json<T: serde::Serialize>(value: &T) -> Result<Vec<u8>> {
    serde_json::to_vec(value).map_err(|e| AegisError::Serialization(e.to_string()))
}

#[async_trait]
impl AegisPlugin for WasmPluginInstance {
    fn manifest(&self) -> &PluginManifest {
        &self.manifest
    }

    async fn on_before_save(&self, post: PostContext) -> Result<PostContext> {
        match self
            .call_hook("on-before-save", "on_before_save", &to_json(&post)?)
            .await?
        {
            Some(out) => parse_refusable_output(&self.manifest.name, "PostContext", &out),
            None => Ok(post),
        }
    }

    async fn transform_content_ast(&self, blocks: Vec<BlockNode>) -> Result<Vec<BlockNode>> {
        match self
            .call_hook(
                "transform-content-ast",
                "transform_content_ast",
                &to_json(&blocks)?,
            )
            .await?
        {
            Some(out) => serde_json::from_slice(&out).map_err(|e| {
                AegisError::Serialization(format!("Invalid BlockNode array returned: {}", e))
            }),
            None => Ok(blocks),
        }
    }

    async fn on_render_page(&self, ctx: PageRenderContext) -> Result<PageRenderContext> {
        match self
            .call_hook("on-render-page", "on_render_page", &to_json(&ctx)?)
            .await?
        {
            Some(out) => serde_json::from_slice(&out).map_err(|e| {
                AegisError::Serialization(format!("Invalid PageRenderContext returned: {}", e))
            }),
            None => Ok(ctx),
        }
    }

    async fn on_user_authenticated(&self, ctx: UserAuthContext) -> Result<UserAuthContext> {
        match self
            .call_hook(
                "on-user-authenticated",
                "on_user_authenticated",
                &to_json(&ctx)?,
            )
            .await?
        {
            Some(out) => parse_refusable_output(&self.manifest.name, "UserAuthContext", &out),
            None => Ok(ctx),
        }
    }
}
