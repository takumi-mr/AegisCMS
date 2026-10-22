//! Wasm プラグインのフック 1 回あたりの所要時間を、処理の段階ごとに計測する。
//!
//! ```bash
//! cargo run --release -p aegis-runtime --example plugin_profile [plugin_dir ...]
//! ```
//!
//! 引数を省略すると `sdk/templates/*`・`sdk/examples/*`・`plugins/*` を計測する。
//! 段階は `WasmPluginInstance` の呼び出しと同じ順序（Store 作成 → インスタンス化 → `_initialize`
//! → 入力の書き込み → フック → 出力の読み出し）で、最後に実際の `AegisPlugin` 経由の所要時間も測る。
//! 続けて、同時に呼び出した場合のスループット（1 並列と `AEGIS_PROFILE_CONCURRENCY` 並列、既定 32）を測る。
//! 同時実行時だけに現れるコスト（線形メモリの確保・解放のカーネル内の競合など）を見るため。
//! エンジンはサーバーの既定と同じプーリングアロケータ（64 枠・64 MiB）。`AEGIS_PROFILE_POOL=0` で
//! 呼び出しごとに確保する従来の方式、`AEGIS_PROFILE_POOL=<枠数>` で枠数を変えて測る。

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use aegis_ast::BlockNode;
use aegis_core::{PageRenderContext, PluginManifest, PluginResourceLimits, PostContext};
use aegis_runtime::{
    AegisPlugin, InstancePoolConfig, PluginHostState, WasmEngine, WasmPluginInstance,
};
use wasmtime::{Module, Store};

const WARMUP: usize = 20;
const ITERATIONS: usize = 300;
const PHASES: [&str; 6] = [
    "store",
    "instantiate",
    "_initialize",
    "write",
    "call",
    "read",
];

fn sample_blocks() -> Vec<BlockNode> {
    let mut blocks = Vec::new();
    for i in 0..4 {
        blocks.push(BlockNode::heading(2, format!("Section {}", i)));
        blocks.push(BlockNode::paragraph(
            "Lorem ipsum dolor sit amet, consectetur adipiscing elit. ".repeat(3),
        ));
    }
    let mut code = BlockNode::new("core/code");
    code.text = Some("fn main() {\n    println!(\"hello\");\n}".to_string());
    code.attributes = serde_json::json!({ "language": "rust" });
    blocks.push(code);
    blocks
}

fn hook_inputs(blocks: &[BlockNode]) -> Vec<(&'static str, &'static str, Vec<u8>)> {
    let post = PostContext {
        id: "id-1".to_string(),
        title: "Hello".to_string(),
        slug: "hello".to_string(),
        author_id: "editor1".to_string(),
        blocks_json: serde_json::to_string(blocks).unwrap(),
    };
    let page = PageRenderContext {
        site_title: "AegisCMS".to_string(),
        template: "article.html".to_string(),
        content_type: "article".to_string(),
        title: "Hello".to_string(),
        slug: "hello".to_string(),
        content_html: "<h2>Section</h2><p>Body</p>".repeat(8),
        ..Default::default()
    };
    vec![
        (
            "on-before-save",
            "on_before_save",
            serde_json::to_vec(&post).unwrap(),
        ),
        (
            "transform-content-ast",
            "transform_content_ast",
            serde_json::to_vec(blocks).unwrap(),
        ),
        (
            "on-render-page",
            "on_render_page",
            serde_json::to_vec(&page).unwrap(),
        ),
    ]
}

fn plugin_dirs(args: Vec<String>) -> Vec<PathBuf> {
    if !args.is_empty() {
        return args.into_iter().map(PathBuf::from).collect();
    }
    let mut dirs = Vec::new();
    for group in ["sdk/templates", "sdk/examples", "plugins"] {
        let Ok(entries) = std::fs::read_dir(group) else {
            continue;
        };
        for entry in entries.flatten() {
            if entry.path().join("plugin.manifest.json").is_file() {
                dirs.push(entry.path());
            }
        }
    }
    dirs.sort();
    dirs
}

/// 1 回の呼び出しを段階ごとに計測する（`WasmPluginInstance::call_hook` と同じ手順）
async fn profile_once(
    engine: &WasmEngine,
    module: &Module,
    manifest: &PluginManifest,
    export: &str,
    input: &[u8],
) -> anyhow_free::Result<[Duration; 6]> {
    let mut t = [Duration::ZERO; 6];
    let kv = Arc::new(Mutex::new(HashMap::new()));

    let start = Instant::now();
    let mut store = Store::new(engine.engine(), PluginHostState::new(manifest.clone(), kv));
    store.limiter(|state| &mut state.limits);
    store.set_fuel(manifest.resource_limits.max_fuel_units)?;
    t[0] = start.elapsed();

    let start = Instant::now();
    let instance = engine
        .linker()
        .instantiate_async(&mut store, module)
        .await?;
    t[1] = start.elapsed();

    let start = Instant::now();
    if let Ok(init) = instance.get_typed_func::<(), ()>(&mut store, "_initialize") {
        init.call_async(&mut store, ()).await?;
    }
    t[2] = start.elapsed();

    let start = Instant::now();
    let memory = instance
        .get_memory(&mut store, "memory")
        .ok_or("no memory")?;
    let allocate = instance.get_typed_func::<i32, i32>(&mut store, "allocate")?;
    let ptr = allocate.call_async(&mut store, input.len() as i32).await?;
    memory.write(&mut store, ptr as usize, input)?;
    t[3] = start.elapsed();

    let start = Instant::now();
    let hook = instance.get_typed_func::<(i32, i32), i64>(&mut store, export)?;
    let packed = hook
        .call_async(&mut store, (ptr, input.len() as i32))
        .await?;
    t[4] = start.elapsed();

    let start = Instant::now();
    let (out_ptr, out_len) = ((packed >> 32) as usize, (packed & 0xFFFF_FFFF) as usize);
    let mut out = vec![0u8; out_len];
    memory.read(&store, out_ptr, &mut out)?;
    t[5] = start.elapsed();
    Ok(t)
}

/// `Box<dyn Error>` を `?` で集める最小限の Result（依存を増やさないため）
mod anyhow_free {
    pub type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;
}

fn micros(d: Duration) -> f64 {
    d.as_secs_f64() * 1e6
}

async fn end_to_end(plugin: &WasmPluginInstance, hook: &str, blocks: &[BlockNode]) -> Duration {
    let start = Instant::now();
    let _ = match hook {
        "on-before-save" => plugin
            .on_before_save(PostContext {
                id: "id-1".to_string(),
                title: "Hello".to_string(),
                slug: "hello".to_string(),
                author_id: "editor1".to_string(),
                blocks_json: serde_json::to_string(blocks).unwrap(),
            })
            .await
            .map(|_| ()),
        "transform-content-ast" => plugin
            .transform_content_ast(blocks.to_vec())
            .await
            .map(|_| ()),
        _ => plugin
            .on_render_page(PageRenderContext {
                template: "article.html".to_string(),
                content_html: "<h2>Section</h2><p>Body</p>".repeat(8),
                ..Default::default()
            })
            .await
            .map(|_| ()),
    };
    start.elapsed()
}

#[tokio::main]
async fn main() -> anyhow_free::Result<()> {
    let pool_size: u32 = std::env::var("AEGIS_PROFILE_POOL")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(64);
    let engine = if pool_size == 0 {
        println!("allocator: on demand");
        WasmEngine::new()?
    } else {
        println!("allocator: pooling ({} slots, 64 MiB)", pool_size);
        WasmEngine::with_instance_pool(InstancePoolConfig {
            size: pool_size,
            max_memory_mb: 64,
        })?
    };
    let blocks = sample_blocks();
    let inputs = hook_inputs(&blocks);

    println!(
        "{:<28} {:<22} {:>7} {:>11} {:>11} {:>7} {:>8} {:>6} | {:>8} {:>8}",
        "plugin",
        "hook",
        "store",
        "instantiate",
        "_initialize",
        "write",
        "call",
        "read",
        "sum",
        "e2e"
    );
    for dir in plugin_dirs(std::env::args().skip(1).collect()) {
        let manifest = PluginManifest::from_json_with_defaults(
            &std::fs::read_to_string(dir.join("plugin.manifest.json"))?,
            &PluginResourceLimits::default(),
        )?;
        let wasm = std::fs::read(dir.join(&manifest.entrypoint))?;
        let module = Module::new(engine.engine(), &wasm)?;
        let plugin = WasmPluginInstance::from_bytes(
            engine.clone(),
            manifest.clone(),
            &wasm,
            Arc::new(Mutex::new(HashMap::new())),
        )?;
        let name = display_name(&dir, &manifest);

        for (hook, export, input) in &inputs {
            if !manifest.capabilities.hooks.iter().any(|h| h == hook) {
                continue;
            }
            if module.get_export(export).is_none() {
                // 宣言だけでエクスポートしていないフックは呼ばれない（ホストは「変更なし」として扱う）
                println!("{:<28} {:<22} (not exported)", name, hook);
                continue;
            }
            let mut phase_samples: Vec<Vec<Duration>> = vec![Vec::new(); PHASES.len()];
            let mut e2e_samples = Vec::new();
            for i in 0..WARMUP + ITERATIONS {
                let phases = match profile_once(&engine, &module, &manifest, export, input).await {
                    Ok(phases) => phases,
                    Err(e) => {
                        println!("{:<28} {:<22} error: {}", name, hook, e);
                        break;
                    }
                };
                let e2e = end_to_end(&plugin, hook, &blocks).await;
                if i >= WARMUP {
                    for (samples, d) in phase_samples.iter_mut().zip(phases) {
                        samples.push(d);
                    }
                    e2e_samples.push(e2e);
                }
            }
            if e2e_samples.is_empty() {
                continue;
            }
            let medians: Vec<f64> = phase_samples.iter_mut().map(|s| median(s)).collect();
            println!(
                "{:<28} {:<22} {:>7.1} {:>11.1} {:>11.1} {:>7.1} {:>8.1} {:>6.1} | {:>8.1} {:>8.1}",
                name,
                hook,
                medians[0],
                medians[1],
                medians[2],
                medians[3],
                medians[4],
                medians[5],
                medians.iter().sum::<f64>(),
                median(&mut e2e_samples)
            );
        }
    }
    println!("(median of {} calls, microseconds)", ITERATIONS);

    let concurrency: usize = std::env::var("AEGIS_PROFILE_CONCURRENCY")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(32);
    println!();
    println!(
        "{:<28} {:<22} {:>12} {:>12} {:>8}",
        "plugin",
        "hook",
        "1 task/s",
        format!("{} tasks/s", concurrency),
        "scaling"
    );
    for dir in plugin_dirs(std::env::args().skip(1).collect()) {
        let manifest = PluginManifest::from_json_with_defaults(
            &std::fs::read_to_string(dir.join("plugin.manifest.json"))?,
            &PluginResourceLimits::default(),
        )?;
        let wasm = std::fs::read(dir.join(&manifest.entrypoint))?;
        let module = Module::new(engine.engine(), &wasm)?;
        let plugin = Arc::new(WasmPluginInstance::from_bytes(
            engine.clone(),
            manifest.clone(),
            &wasm,
            Arc::new(Mutex::new(HashMap::new())),
        )?);
        let name = display_name(&dir, &manifest);
        for (hook, export, _) in &inputs {
            if !manifest.capabilities.hooks.iter().any(|h| h == hook)
                || module.get_export(export).is_none()
            {
                continue;
            }
            let single = throughput(&plugin, hook, &blocks, 1).await;
            let many = throughput(&plugin, hook, &blocks, concurrency).await;
            println!(
                "{:<28} {:<22} {:>12.0} {:>12.0} {:>7.1}x",
                name,
                hook,
                single,
                many,
                many / single
            );
        }
    }
    println!(
        "(calls per second over {:?}, {} worker threads)",
        THROUGHPUT_DURATION,
        tokio::runtime::Handle::current().metrics().num_workers()
    );
    Ok(())
}

const THROUGHPUT_DURATION: Duration = Duration::from_secs(2);

/// `tasks` 個のタスクから同じフックを呼び続け、1 秒あたりの完了数を返す
async fn throughput(
    plugin: &Arc<WasmPluginInstance>,
    hook: &'static str,
    blocks: &[BlockNode],
    tasks: usize,
) -> f64 {
    let deadline = Instant::now() + THROUGHPUT_DURATION;
    let handles: Vec<_> = (0..tasks)
        .map(|_| {
            let plugin = plugin.clone();
            let blocks = blocks.to_vec();
            tokio::spawn(async move {
                let mut calls = 0u64;
                while Instant::now() < deadline {
                    end_to_end(&plugin, hook, &blocks).await;
                    calls += 1;
                }
                calls
            })
        })
        .collect();
    let mut total = 0;
    for handle in handles {
        total += handle.await.unwrap_or(0);
    }
    total as f64 / THROUGHPUT_DURATION.as_secs_f64()
}

fn display_name(dir: &Path, manifest: &PluginManifest) -> String {
    let parent = dir
        .parent()
        .and_then(|p| p.file_name())
        .and_then(|n| n.to_str())
        .unwrap_or_default();
    format!("{}/{}", parent, manifest.name)
}

fn median(samples: &mut [Duration]) -> f64 {
    samples.sort();
    micros(samples[samples.len() / 2])
}
