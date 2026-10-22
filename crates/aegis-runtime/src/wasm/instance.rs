use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use wasmtime::{Instance, Module, Store, TypedFunc};

use aegis_ast::BlockNode;
use aegis_core::{AegisError, PluginManifest, PostContext, Result};

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
}

/// Reads what `on_before_save` returned: the (possibly updated) `PostContext`, or
/// `{"error": "<message>"}` when the plugin rejects the save (`abi::write_error_to_host` in the SDK)
fn parse_before_save_output(plugin: &str, out_bytes: &[u8]) -> Result<PostContext> {
    let value: serde_json::Value = serde_json::from_slice(out_bytes)
        .map_err(|e| AegisError::Serialization(format!("Invalid PostContext returned: {}", e)))?;
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
    serde_json::from_value(value)
        .map_err(|e| AegisError::Serialization(format!("Invalid PostContext returned: {}", e)))
}

#[async_trait]
impl AegisPlugin for WasmPluginInstance {
    fn manifest(&self) -> &PluginManifest {
        &self.manifest
    }

    async fn on_before_save(&self, post: PostContext) -> Result<PostContext> {
        let host_state = PluginHostState::new(self.manifest.clone(), self.kv_storage.clone());
        host_state.guard().check_hook("on-before-save")?;

        let (mut store, instance) = self.instantiate().await?;

        // プラグインが on_before_save をエクスポートしているか確認
        if let Ok(func) = instance.get_typed_func::<(i32, i32), i64>(&mut store, "on_before_save") {
            let json_bytes =
                serde_json::to_vec(&post).map_err(|e| AegisError::Serialization(e.to_string()))?;

            let (ptr, len) = Self::write_to_guest(&mut store, &instance, &json_bytes).await?;

            let timeout_ms = self.manifest.resource_limits.execution_timeout_ms.max(50);
            let duration = std::time::Duration::from_millis(timeout_ms);

            let packed_res =
                match tokio::time::timeout(duration, func.call_async(&mut store, (ptr, len))).await
                {
                    Ok(res) => res.map_err(|e| {
                        AegisError::PluginRuntime(format!("Hook 'on_before_save' error: {}", e))
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

            if res_len > 0 {
                let out_bytes = Self::read_from_guest(&mut store, &instance, res_ptr, res_len)?;
                return parse_before_save_output(&self.manifest.name, &out_bytes);
            }
        }

        Ok(post)
    }

    async fn transform_content_ast(&self, blocks: Vec<BlockNode>) -> Result<Vec<BlockNode>> {
        let host_state = PluginHostState::new(self.manifest.clone(), self.kv_storage.clone());
        host_state.guard().check_hook("transform-content-ast")?;

        let (mut store, instance) = self.instantiate().await?;

        if let Ok(func) =
            instance.get_typed_func::<(i32, i32), i64>(&mut store, "transform_content_ast")
        {
            let json_bytes = serde_json::to_vec(&blocks)
                .map_err(|e| AegisError::Serialization(e.to_string()))?;

            let (ptr, len) = Self::write_to_guest(&mut store, &instance, &json_bytes).await?;

            let timeout_ms = self.manifest.resource_limits.execution_timeout_ms.max(50);
            let duration = std::time::Duration::from_millis(timeout_ms);

            let packed_res =
                match tokio::time::timeout(duration, func.call_async(&mut store, (ptr, len))).await
                {
                    Ok(res) => res.map_err(|e| {
                        AegisError::PluginRuntime(format!(
                            "Hook 'transform_content_ast' error: {}",
                            e
                        ))
                    })?,
                    Err(_) => {
                        return Err(AegisError::ResourceLimitExceeded(format!(
                            "Plugin '{}' exceeded timeout limit of {}ms",
                            self.manifest.name, timeout_ms
                        )))
                    }
                };

            let res_ptr = (packed_res >> 32) as i32;
            let res_len = (packed_res & 0xFFFF_FFFF) as i32;

            if res_len > 0 {
                let out_bytes = Self::read_from_guest(&mut store, &instance, res_ptr, res_len)?;
                let transformed: Vec<BlockNode> =
                    serde_json::from_slice(&out_bytes).map_err(|e| {
                        AegisError::Serialization(format!(
                            "Invalid BlockNode array returned: {}",
                            e
                        ))
                    })?;
                return Ok(transformed);
            }
        }

        Ok(blocks)
    }

    async fn on_render_page(
        &self,
        ctx: aegis_core::PageRenderContext,
    ) -> Result<aegis_core::PageRenderContext> {
        let host_state = PluginHostState::new(self.manifest.clone(), self.kv_storage.clone());
        host_state.guard().check_hook("on-render-page")?;

        let (mut store, instance) = self.instantiate().await?;

        if let Ok(func) = instance.get_typed_func::<(i32, i32), i64>(&mut store, "on_render_page") {
            let json_bytes =
                serde_json::to_vec(&ctx).map_err(|e| AegisError::Serialization(e.to_string()))?;

            let (ptr, len) = Self::write_to_guest(&mut store, &instance, &json_bytes).await?;

            let timeout_ms = self.manifest.resource_limits.execution_timeout_ms.max(50);
            let duration = std::time::Duration::from_millis(timeout_ms);

            let packed_res =
                match tokio::time::timeout(duration, func.call_async(&mut store, (ptr, len))).await
                {
                    Ok(res) => res.map_err(|e| {
                        AegisError::PluginRuntime(format!("Hook 'on_render_page' error: {}", e))
                    })?,
                    Err(_) => {
                        return Err(AegisError::ResourceLimitExceeded(format!(
                            "Plugin '{}' exceeded timeout limit of {}ms",
                            self.manifest.name, timeout_ms
                        )))
                    }
                };

            let res_ptr = (packed_res >> 32) as i32;
            let res_len = (packed_res & 0xFFFF_FFFF) as i32;

            if res_len > 0 {
                let out_bytes = Self::read_from_guest(&mut store, &instance, res_ptr, res_len)?;
                let updated_ctx: aegis_core::PageRenderContext = serde_json::from_slice(&out_bytes)
                    .map_err(|e| {
                        AegisError::Serialization(format!(
                            "Invalid PageRenderContext returned: {}",
                            e
                        ))
                    })?;
                return Ok(updated_ctx);
            }
        }

        Ok(ctx)
    }
}
