use async_trait::async_trait;
use wasmtime::{Instance, InstancePre, Module, Store, TypedFunc};

use aegis_ast::BlockNode;
use aegis_core::{
    AegisError, Delivered, NotificationMessage, OrderContext, OrderStatusChange, PageRenderContext,
    PluginManifest, PostContext, Result, SendError, UserAuthContext,
};

use super::engine::WasmEngine;
use super::host_state::PluginHostState;
use crate::plugin::AegisPlugin;
use crate::services::PluginServices;

pub struct WasmPluginInstance {
    pub manifest: PluginManifest,
    pub engine: WasmEngine,
    pub module: Module,
    /// インポートをホスト API に解決済みのモジュール（呼び出しごとのインスタンス化に使う）
    instance_pre: InstancePre<PluginHostState>,
    services: PluginServices,
}

impl WasmPluginInstance {
    pub fn from_bytes(
        engine: WasmEngine,
        manifest: PluginManifest,
        wasm_bytes: &[u8],
        services: impl Into<PluginServices>,
    ) -> Result<Self> {
        let module = Module::new(engine.engine(), wasm_bytes).map_err(|e| {
            AegisError::PluginRuntime(format!("Failed to compile Wasm module: {}", e))
        })?;
        // ホストに無い関数のインポート（AssemblyScript の `env.abort`、TinyGo の WASI など）は、
        // 呼び出し時ではなく読み込み時にエラーにする
        let instance_pre = engine.linker().instantiate_pre(&module).map_err(|e| {
            AegisError::PluginRuntime(format!(
                "The module imports a function the host does not provide ({}). Plugins may only \
                 import the `host` module (docs/plugin_abi_specification.md)",
                e
            ))
        })?;
        engine.check_fits_pool(&manifest, &module)?;

        Ok(Self {
            manifest,
            engine,
            module,
            instance_pre,
            services: services.into(),
        })
    }

    /// 各フック呼び出しごとに独立した Store / Instance を初期化（Zero-Trust & 完全分離）
    async fn instantiate(&self) -> Result<(Store<PluginHostState>, Instance)> {
        let host_state = PluginHostState::new(self.manifest.clone(), self.services.clone());
        let mut store = Store::new(self.engine.engine(), host_state);
        // メモリ上限（max_memory_mb）。初期メモリが上限を超えるモジュールはインスタンス化で失敗する
        store.limiter(|state| &mut state.limits);

        // Fuel設定（CPU暴走・無限ループ防止）
        store
            .set_fuel(self.manifest.resource_limits.max_fuel_units)
            .map_err(|e| AegisError::ResourceLimitExceeded(e.to_string()))?;

        let instance = self
            .instance_pre
            .instantiate_async(&mut store)
            .await
            .map_err(|e| AegisError::PluginRuntime(format!("Instantiation failed: {}", e)))?;

        // リアクター形式のモジュール（TinyGo の wasm-unknown など）は、ランタイムとグローバル変数の
        // 初期化関数 `_initialize` を他のエクスポートより先に呼ぶ必要がある
        if let Ok(initialize) = instance.get_typed_func::<(), ()>(&mut store, "_initialize") {
            self.with_timeout("_initialize", initialize.call_async(&mut store, ()))
                .await?;
        }

        Ok((store, instance))
    }

    /// `execution_timeout_ms`（最低 50ms）以内に終わらなければ `ResourceLimitExceeded`
    async fn with_timeout<T>(
        &self,
        export: &str,
        call: impl std::future::Future<Output = wasmtime::Result<T>>,
    ) -> Result<T> {
        let timeout_ms = self.manifest.resource_limits.execution_timeout_ms.max(50);
        let duration = std::time::Duration::from_millis(timeout_ms);
        match tokio::time::timeout(duration, call).await {
            Ok(res) => res
                .map_err(|e| AegisError::PluginRuntime(format!("Hook '{}' error: {}", export, e))),
            Err(_) => Err(AegisError::ResourceLimitExceeded(format!(
                "Plugin '{}' exceeded timeout limit of {}ms",
                self.manifest.name, timeout_ms
            ))),
        }
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
        let host_state = PluginHostState::new(self.manifest.clone(), self.services.clone());
        host_state.guard().check_hook(hook)?;

        // プールの枠（プーリングアロケータ使用時）。Store を捨てて枠が返るまで保持するため、
        // Store より先に宣言する（ローカル変数は宣言の逆順に破棄される）
        let _slot = self.engine.acquire_slot().await;
        let (mut store, instance) = self.instantiate().await?;
        let Ok(func) = instance.get_typed_func::<(i32, i32), i64>(&mut store, export) else {
            return Ok(None);
        };
        let (ptr, len) = Self::write_to_guest(&mut store, &instance, input).await?;

        let packed_res = self
            .with_timeout(export, func.call_async(&mut store, (ptr, len)))
            .await?;

        // 返り値は i64: 上位32bitがptr、下位32bitがlen
        let res_ptr = (packed_res >> 32) as i32;
        let res_len = (packed_res & 0xFFFF_FFFF) as i32;
        if res_len <= 0 {
            return Ok(None);
        }
        Self::read_from_guest(&mut store, &instance, res_ptr, res_len).map(Some)
    }
}

/// Reads the JSON object a hook returned (`on_before_save`, `on_render_page`,
/// `on_user_authenticated`). Its top-level keys are applied over `input`, so a plugin may return
/// only the fields it changed and fields it does not know about stay as they were. A hook that may
/// refuse (`refusable`) returns `{"error": "<message>"}` to refuse (`abi::write_error_to_host` in
/// the SDK).
fn parse_object_output<T: serde::Serialize + serde::de::DeserializeOwned>(
    plugin: &str,
    type_name: &str,
    input: &T,
    out_bytes: &[u8],
    refusable: bool,
) -> Result<T> {
    let invalid = |e: serde_json::Error| {
        AegisError::Serialization(format!("Invalid {} returned: {}", type_name, e))
    };
    let value: serde_json::Value = serde_json::from_slice(out_bytes).map_err(invalid)?;
    if refusable {
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
    }
    let serde_json::Value::Object(changes) = value else {
        return Err(AegisError::Serialization(format!(
            "Invalid {} returned: expected a JSON object",
            type_name
        )));
    };
    let mut merged = serde_json::to_value(input).map_err(invalid)?;
    if let Some(fields) = merged.as_object_mut() {
        fields.extend(changes);
    }
    serde_json::from_value(merged).map_err(invalid)
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
            Some(out) => parse_object_output(&self.manifest.name, "PostContext", &post, &out, true),
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
            Some(out) => {
                parse_object_output(&self.manifest.name, "PageRenderContext", &ctx, &out, false)
            }
            None => Ok(ctx),
        }
    }

    async fn calculate_order(&self, ctx: OrderContext) -> Result<OrderContext> {
        match self
            .call_hook("calculate-order", "calculate_order", &to_json(&ctx)?)
            .await?
        {
            Some(out) => parse_object_output(&self.manifest.name, "OrderContext", &ctx, &out, true),
            None => Ok(ctx),
        }
    }

    async fn on_order_status_changed(&self, change: OrderStatusChange) -> Result<()> {
        let out = self
            .call_hook(
                "on-order-status-changed",
                "on_order_status_changed",
                &to_json(&change)?,
            )
            .await?;
        // 通知の結果は使わない。`{"error": ...}` だけを失敗として扱う
        if let Some(out) = out {
            if let Ok(serde_json::Value::Object(body)) = serde_json::from_slice(&out) {
                if let Some(error) = body.get("error") {
                    return Err(AegisError::PluginRuntime(format!(
                        "{}: {}",
                        self.manifest.name,
                        error
                            .as_str()
                            .map(str::to_string)
                            .unwrap_or_else(|| error.to_string())
                    )));
                }
            }
        }
        Ok(())
    }

    async fn send_notification(
        &self,
        message: NotificationMessage,
    ) -> std::result::Result<Delivered, SendError> {
        let input = to_json(&message).map_err(|e| SendError::permanent(e.to_string()))?;
        let out = self
            .call_hook("send-notification", "send_notification", &input)
            .await
            .map_err(|e| match e {
                // フックを宣言していない: 何度呼んでも同じ
                AegisError::PermissionDenied(_) => SendError::permanent(e.to_string()),
                // タイムアウト・トラップなど: 送り先の一時的な不調かもしれないので再試行する
                other => SendError::temporary(other.to_string()),
            })?;
        parse_delivery_output(&self.manifest.name, out.as_deref())
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
            Some(out) => {
                parse_object_output(&self.manifest.name, "UserAuthContext", &ctx, &out, true)
            }
            None => Ok(ctx),
        }
    }
}

/// `send_notification` の返り値: 何も無い（0）なら送信済み、`{"skipped": true}` なら扱わない通知、
/// `{"error": "...", "permanent": true|false}` なら失敗
fn parse_delivery_output(
    plugin: &str,
    out: Option<&[u8]>,
) -> std::result::Result<Delivered, SendError> {
    let Some(out) = out else {
        return Ok(Delivered::Sent);
    };
    let Ok(serde_json::Value::Object(body)) = serde_json::from_slice(out) else {
        return Ok(Delivered::Sent);
    };
    if let Some(error) = body.get("error") {
        let message = format!(
            "{}: {}",
            plugin,
            error
                .as_str()
                .map(str::to_string)
                .unwrap_or_else(|| error.to_string())
        );
        let permanent = body
            .get("permanent")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        return Err(SendError { message, permanent });
    }
    if body.get("skipped").and_then(|v| v.as_bool()) == Some(true) {
        return Ok(Delivered::Skipped);
    }
    Ok(Delivered::Sent)
}

#[cfg(test)]
mod delivery_output_tests {
    use super::*;

    #[test]
    fn parses_the_send_notification_result() {
        assert_eq!(parse_delivery_output("p", None), Ok(Delivered::Sent));
        assert_eq!(
            parse_delivery_output("p", Some(br#"{"skipped":true}"#)),
            Ok(Delivered::Skipped)
        );
        assert_eq!(
            parse_delivery_output("p", Some(br#"{"error":"429 slow down"}"#)),
            Err(SendError::temporary("p: 429 slow down"))
        );
        assert_eq!(
            parse_delivery_output("p", Some(br#"{"error":"bad token","permanent":true}"#)),
            Err(SendError::permanent("p: bad token"))
        );
    }
}
