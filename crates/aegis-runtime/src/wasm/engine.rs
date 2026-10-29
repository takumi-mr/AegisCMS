use super::host_state::PluginHostState;
use aegis_core::{AegisError, PluginManifest, Result};
use std::sync::Arc;
use tokio::sync::{OwnedSemaphorePermit, Semaphore};
use wasmtime::{
    Caller, Config, Engine, InstanceAllocationStrategy, Linker, Module, PoolingAllocationConfig,
};

/// プーリングアロケータの設定（`[runtime].instance_pool_size` / `pool_max_memory_mb`）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InstancePoolConfig {
    /// 同時に存在できるインスタンス数（= 同時に実行できるフック呼び出しの数）
    pub size: u32,
    /// 1 インスタンスの線形メモリの上限（MiB）。これより大きい `max_memory_mb` のプラグインは読み込めない
    pub max_memory_mb: u32,
}

/// ゲストから読み込む文字列（キー・値・HTTP リクエスト）の上限
const MAX_GUEST_INPUT: usize = 4 << 20;

/// ゲストのメモリから UTF-8 文字列を読む（範囲外・大きすぎる・UTF-8 でなければ `None`）
fn read_guest_string(
    caller: &mut Caller<'_, PluginHostState>,
    ptr: i32,
    len: i32,
) -> Option<String> {
    if ptr < 0 || len < 0 || len as usize > MAX_GUEST_INPUT {
        return None;
    }
    let memory = caller.get_export("memory").and_then(|m| m.into_memory())?;
    let mut buffer = vec![0u8; len as usize];
    memory.read(&*caller, ptr as usize, &mut buffer).ok()?;
    String::from_utf8(buffer).ok()
}

/// ゲストの `allocate` で確保した領域に `data` を書き込み、`ptr << 32 | len` を返す（失敗・空なら 0）
async fn write_guest(caller: &mut Caller<'_, PluginHostState>, data: &[u8]) -> i64 {
    if data.is_empty() || data.len() > i32::MAX as usize {
        return 0;
    }
    let Some(allocate) = caller
        .get_export("allocate")
        .and_then(|e| e.into_func())
        .and_then(|f| f.typed::<i32, i32>(&*caller).ok())
    else {
        return 0;
    };
    let Ok(ptr) = allocate.call_async(&mut *caller, data.len() as i32).await else {
        return 0;
    };
    let Some(memory) = caller.get_export("memory").and_then(|m| m.into_memory()) else {
        return 0;
    };
    if ptr <= 0 || memory.write(&mut *caller, ptr as usize, data).is_err() {
        return 0;
    }
    ((ptr as i64) << 32) | data.len() as i64
}

/// KV のキーにプラグインのネームスペースを付ける（`capabilities.storage` が無ければ `None`）
fn namespaced_key(caller: &Caller<'_, PluginHostState>, key: &str) -> Option<String> {
    let state = caller.data();
    let namespace = state
        .manifest
        .capabilities
        .storage
        .as_ref()?
        .kv_namespace
        .clone();
    state.guard().check_storage_access(&namespace).ok()?;
    Some(format!("{}:{}", namespace, key))
}

/// `host.http_fetch` の本体。失敗はプラグインに返すメッセージ
async fn http_fetch(
    caller: &mut Caller<'_, PluginHostState>,
    ptr: i32,
    len: i32,
) -> std::result::Result<crate::services::HttpResponse, String> {
    let raw = read_guest_string(caller, ptr, len).ok_or("Invalid request")?;
    let request: crate::services::HttpRequest =
        serde_json::from_str(&raw).map_err(|e| format!("Invalid request: {}", e))?;
    let state = caller.data_mut();
    state
        .guard()
        .check_network_access(&request.url)
        .map_err(|e| e.to_string())?;
    if state.fetch_count >= super::host_state::MAX_FETCHES_PER_CALL {
        return Err(format!(
            "At most {} HTTP requests per hook call",
            super::host_state::MAX_FETCHES_PER_CALL
        ));
    }
    state.fetch_count += 1;
    let plugin = state.manifest.name.clone();
    let services = state.services.0.clone();
    services
        .http_fetch(&plugin, request)
        .await
        .map_err(|e| e.to_string())
}

/// プール 1 枠あたりのテーブル要素数の上限（Rust / TinyGo / AssemblyScript の関数テーブルに十分な大きさ）
const POOL_TABLE_ELEMENTS: usize = 20_000;

/// プールの枠と、空き枠を待つためのセマフォ
struct InstancePool {
    config: InstancePoolConfig,
    slots: Arc<Semaphore>,
}

#[derive(Clone)]
pub struct WasmEngine {
    engine: Engine,
    linker: Arc<Linker<PluginHostState>>,
    pool: Option<Arc<InstancePool>>,
}

impl WasmEngine {
    /// 呼び出しのたびに線形メモリ・スタックを確保して解放するエンジン（テストやツール向け）
    pub fn new() -> Result<Self> {
        Self::build(None)
    }

    /// インスタンスの線形メモリ・テーブル・非同期スタックを事前に確保した枠から割り当てるエンジン
    /// （Wasmtime のプーリングアロケータ）。
    ///
    /// フックは今までどおり呼び出しごとに新しいインスタンスで実行する。枠は返却時に Wasmtime が
    /// ゼロに戻すため、前の呼び出しの状態は残らない。呼び出しごとの `mmap` / `munmap` を避けることで、
    /// 同時実行時のカーネル内の競合（アドレス空間のロック・TLB の無効化）を無くす。
    /// 枠が足りないときは空くまで待つ（`acquire_slot`）。
    pub fn with_instance_pool(pool: InstancePoolConfig) -> Result<Self> {
        Self::build(Some(pool))
    }

    fn build(pool: Option<InstancePoolConfig>) -> Result<Self> {
        let mut config = Config::new();
        config.consume_fuel(true);
        // async_support is enabled by default in wasmtime 50
        config.wasm_backtrace_max_frames(None);
        if let Some(pool) = pool {
            let mut pooling = PoolingAllocationConfig::default();
            pooling
                .total_core_instances(pool.size)
                .total_memories(pool.size)
                .total_tables(pool.size)
                .total_stacks(pool.size)
                .total_component_instances(0)
                .total_gc_heaps(pool.size)
                .max_memories_per_module(1)
                .max_tables_per_module(1)
                .table_elements(POOL_TABLE_ELEMENTS)
                .max_memory_size((pool.max_memory_mb.max(1) as usize) << 20);
            config.allocation_strategy(InstanceAllocationStrategy::Pooling(pooling));
        }

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
            .func_wrap_async(
                "host",
                "kv_set",
                |mut caller: Caller<'_, PluginHostState>,
                 (k_ptr, k_len, v_ptr, v_len): (i32, i32, i32, i32)| {
                    Box::new(async move {
                        let (Some(key), Some(value)) = (
                            read_guest_string(&mut caller, k_ptr, k_len),
                            read_guest_string(&mut caller, v_ptr, v_len),
                        ) else {
                            return 2;
                        };
                        let Some(full_key) = namespaced_key(&caller, &key) else {
                            return 1;
                        };
                        let services = caller.data().services.0.clone();
                        match services.kv_set(&full_key, &value).await {
                            Ok(()) => 0,
                            Err(e) => {
                                tracing::warn!(
                                    "[Wasm:{}] kv_set failed: {}",
                                    caller.data().manifest.name,
                                    e
                                );
                                2
                            }
                        }
                    })
                },
            )
            .map_err(|e| AegisError::PluginRuntime(e.to_string()))?;

        // 4. host::kv_get(key_ptr, key_len) -> i64
        //    (ptr << 32 | len: 値、0: 未設定、-1: 権限なし、-2: エラー)
        linker
            .func_wrap_async(
                "host",
                "kv_get",
                |mut caller: Caller<'_, PluginHostState>, (k_ptr, k_len): (i32, i32)| {
                    Box::new(async move {
                        let Some(key) = read_guest_string(&mut caller, k_ptr, k_len) else {
                            return -2i64;
                        };
                        let Some(full_key) = namespaced_key(&caller, &key) else {
                            return -1;
                        };
                        let services = caller.data().services.0.clone();
                        match services.kv_get(&full_key).await {
                            Ok(Some(value)) => write_guest(&mut caller, value.as_bytes()).await,
                            Ok(None) => 0,
                            Err(e) => {
                                tracing::warn!(
                                    "[Wasm:{}] kv_get failed: {}",
                                    caller.data().manifest.name,
                                    e
                                );
                                -2
                            }
                        }
                    })
                },
            )
            .map_err(|e| AegisError::PluginRuntime(e.to_string()))?;

        // 5. host::http_fetch(req_ptr, req_len) -> i64
        //    リクエスト `HttpRequest` の JSON を受け取り、応答 `HttpResponse` か `{"error": "..."}` の
        //    JSON を ptr << 32 | len で返す（書き込めなければ 0）
        linker
            .func_wrap_async(
                "host",
                "http_fetch",
                |mut caller: Caller<'_, PluginHostState>, (ptr, len): (i32, i32)| {
                    Box::new(async move {
                        let body = match http_fetch(&mut caller, ptr, len).await {
                            Ok(response) => serde_json::to_vec(&response),
                            Err(message) => {
                                serde_json::to_vec(&serde_json::json!({ "error": message }))
                            }
                        }
                        .unwrap_or_default();
                        write_guest(&mut caller, &body).await
                    })
                },
            )
            .map_err(|e| AegisError::PluginRuntime(e.to_string()))?;

        // 6. host::config_get(key_ptr, key_len) -> i64 (ptr << 32 | len、未設定なら 0)
        //    `[plugins.settings.<プラグイン名>]` の値
        linker
            .func_wrap_async(
                "host",
                "config_get",
                |mut caller: Caller<'_, PluginHostState>, (k_ptr, k_len): (i32, i32)| {
                    Box::new(async move {
                        let Some(key) = read_guest_string(&mut caller, k_ptr, k_len) else {
                            return 0i64;
                        };
                        let value = {
                            let state = caller.data();
                            state.services.0.plugin_setting(&state.manifest.name, &key)
                        };
                        match value {
                            Some(value) => write_guest(&mut caller, value.as_bytes()).await,
                            None => 0,
                        }
                    })
                },
            )
            .map_err(|e| AegisError::PluginRuntime(e.to_string()))?;

        Ok(Self {
            engine,
            linker: Arc::new(linker),
            pool: pool.map(|config| {
                Arc::new(InstancePool {
                    config,
                    slots: Arc::new(Semaphore::new(config.size as usize)),
                })
            }),
        })
    }

    /// プーリングアロケータの設定（`with_instance_pool` で作った場合）
    pub fn instance_pool(&self) -> Option<InstancePoolConfig> {
        self.pool.as_ref().map(|pool| pool.config)
    }

    /// プールの空き枠を 1 つ確保する（空くまで待つ）。インスタンスを捨てるまで保持すること。
    /// プールを使わないエンジンでは `None`
    pub(crate) async fn acquire_slot(&self) -> Option<OwnedSemaphorePermit> {
        let pool = self.pool.as_ref()?;
        pool.slots.clone().acquire_owned().await.ok()
    }

    /// プールの枠に収まるモジュールかを読み込み時に確かめる（実行時のインスタンス化の失敗を避ける）
    pub(crate) fn check_fits_pool(&self, manifest: &PluginManifest, module: &Module) -> Result<()> {
        let Some(pool) = &self.pool else {
            return Ok(());
        };
        let slot_mb = pool.config.max_memory_mb;
        let too_large = |what: String| {
            Err(AegisError::PluginRuntime(format!(
                "{} does not fit the instance pool slot. Raise [runtime].pool_max_memory_mb or \
                 disable pooling with [runtime].instance_pool_size = 0",
                what
            )))
        };
        if manifest.resource_limits.max_memory_mb > slot_mb {
            return too_large(format!(
                "resource_limits.max_memory_mb = {} (slot: {} MiB)",
                manifest.resource_limits.max_memory_mb, slot_mb
            ));
        }
        let required = module.resources_required();
        if required.num_memories > 1 || required.num_tables > 1 {
            return too_large(format!(
                "A module with {} memories and {} tables",
                required.num_memories, required.num_tables
            ));
        }
        // Wasm のページ（64 KiB）単位
        if let Some(pages) = required.max_initial_memory_size {
            if pages.saturating_mul(64 << 10) > (u64::from(slot_mb) << 20) {
                return too_large(format!(
                    "An initial memory of {} pages (64 KiB each)",
                    pages
                ));
            }
        }
        if let Some(elements) = required.max_initial_table_size {
            if elements > POOL_TABLE_ELEMENTS as u64 {
                return too_large(format!("An initial table of {} elements", elements));
            }
        }
        Ok(())
    }

    pub fn engine(&self) -> &Engine {
        &self.engine
    }

    pub fn linker(&self) -> &Linker<PluginHostState> {
        &self.linker
    }
}
