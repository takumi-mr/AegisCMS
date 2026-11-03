//! `on-render-page` の所要時間を `content_html` のサイズ別に計測する。
//!
//! ```bash
//! cargo run --release -p aegis-runtime --example plugin_payload_profile [plugin_dir ...]
//! ```
//!
//! 引数を省略すると `plugins/*` のうち `on-render-page` を持つものを計測する。
//! 段階は host_ser（ホストの JSON 化）→ write → call → read → host_merge（返却 JSON の
//! デシリアライズと差分マージ）。段階の列はコンテキスト全体を渡した場合（`render_page.input` の
//! 絞り込みなし）の値で、`e2e` だけが `WasmPluginInstance::on_render_page` の実測値（絞り込みと
//! 差分マージを含む）。絞り込みの効果は `e2e` で見る。

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use aegis_core::{PageRenderContext, PluginManifest, PluginResourceLimits};
use aegis_runtime::{
    AegisPlugin, InstancePoolConfig, PluginHostState, WasmEngine, WasmPluginInstance,
};
use wasmtime::{Module, Store};

const WARMUP: usize = 10;
const ITERATIONS: usize = 60;
const SIZES_KB: [usize; 5] = [1, 16, 64, 256, 1024];

type Res<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;

/// 見出し・段落・リンク・画像・コードを含む、それらしい本文を `kb` KB 分作る
fn content_html(kb: usize) -> String {
    let chunk = "<h2 id=\"s\">Section heading</h2>\
<p>Lorem ipsum dolor sit amet, <a href=\"https://example.org/x?a=1&amp;b=2\">consectetur</a> \
adipiscing elit, \"sed\" do eiusmod tempor. 日本語の文章もここに入ります。</p>\
<figure><img src=\"/uploads/a.png\" alt=\"A cat\"></figure>\
<pre><code class=\"language-rust\">fn main() {\n    println!(\"hi\");\n}</code></pre>\n";
    let mut s = String::with_capacity(kb * 1024 + chunk.len());
    while s.len() < kb * 1024 {
        s.push_str(chunk);
    }
    s
}

fn page(kb: usize) -> PageRenderContext {
    PageRenderContext {
        site_title: "AegisCMS".to_string(),
        template: "article.html".to_string(),
        content_type: "article".to_string(),
        title: "Hello".to_string(),
        slug: "hello".to_string(),
        content_html: content_html(kb),
        ..Default::default()
    }
}

fn median(v: &mut [Duration]) -> f64 {
    v.sort();
    v[v.len() / 2].as_secs_f64() * 1e6
}

/// 戻り値: [host_ser, write, call, read] と出力サイズ
async fn phases(
    engine: &WasmEngine,
    module: &Module,
    manifest: &PluginManifest,
    ctx: &PageRenderContext,
) -> Res<([Duration; 4], usize)> {
    let mut t = [Duration::ZERO; 4];
    let start = Instant::now();
    let input = serde_json::to_vec(ctx)?;
    t[0] = start.elapsed();

    let kv = Arc::new(Mutex::new(HashMap::new()));
    let mut store = Store::new(
        engine.engine(),
        PluginHostState::new(manifest.clone(), kv.into()),
    );
    store.limiter(|state| &mut state.limits);
    store.set_fuel(manifest.resource_limits.max_fuel_units)?;
    let instance = engine
        .linker()
        .instantiate_async(&mut store, module)
        .await?;
    if let Ok(init) = instance.get_typed_func::<(), ()>(&mut store, "_initialize") {
        init.call_async(&mut store, ()).await?;
    }

    let start = Instant::now();
    let memory = instance
        .get_memory(&mut store, "memory")
        .ok_or("no memory")?;
    let allocate = instance.get_typed_func::<i32, i32>(&mut store, "allocate")?;
    let ptr = allocate.call_async(&mut store, input.len() as i32).await?;
    memory.write(&mut store, ptr as usize, &input)?;
    t[1] = start.elapsed();

    let start = Instant::now();
    let hook = instance.get_typed_func::<(i32, i32), i64>(&mut store, "on_render_page")?;
    let packed = hook
        .call_async(&mut store, (ptr, input.len() as i32))
        .await?;
    t[2] = start.elapsed();

    let start = Instant::now();
    let (out_ptr, out_len) = ((packed >> 32) as usize, (packed & 0xFFFF_FFFF) as usize);
    let mut out = vec![0u8; out_len];
    memory.read(&store, out_ptr, &mut out)?;
    t[3] = start.elapsed();
    Ok((t, out_len))
}

#[tokio::main]
async fn main() -> Res<()> {
    let engine = WasmEngine::with_instance_pool(InstancePoolConfig {
        size: 64,
        max_memory_mb: 64,
    })?;
    let mut dirs: Vec<PathBuf> = std::env::args().skip(1).map(PathBuf::from).collect();
    if dirs.is_empty() {
        for entry in std::fs::read_dir("plugins")?.flatten() {
            if entry.path().join("plugin.manifest.json").is_file() {
                dirs.push(entry.path());
            }
        }
        dirs.sort();
    }

    println!(
        "{:<26} {:>7} {:>9} {:>8} {:>10} {:>8} | {:>10} {:>10}",
        "plugin", "size", "out", "host_ser", "write", "call", "read", "e2e"
    );
    for dir in dirs {
        let manifest = PluginManifest::from_json_with_defaults(
            &std::fs::read_to_string(dir.join("plugin.manifest.json"))?,
            &PluginResourceLimits::default(),
        )?;
        if !manifest
            .capabilities
            .hooks
            .iter()
            .any(|h| h == "on-render-page")
        {
            continue;
        }
        let wasm = std::fs::read(dir.join(&manifest.entrypoint))?;
        let module = Module::new(engine.engine(), &wasm)?;
        if module.get_export("on_render_page").is_none() {
            continue;
        }
        let plugin = WasmPluginInstance::from_bytes(
            engine.clone(),
            manifest.clone(),
            &wasm,
            Arc::new(Mutex::new(HashMap::new())),
        )?;
        for kb in SIZES_KB {
            let ctx = page(kb);
            let mut samples: Vec<Vec<Duration>> = vec![Vec::new(); 4];
            let mut e2e = Vec::new();
            let mut out_len = 0;
            let mut failed = None;
            for i in 0..WARMUP + ITERATIONS {
                match phases(&engine, &module, &manifest, &ctx).await {
                    Ok((t, n)) => {
                        out_len = n;
                        if i >= WARMUP {
                            for (s, d) in samples.iter_mut().zip(t) {
                                s.push(d);
                            }
                        }
                    }
                    Err(e) => {
                        failed = Some(e.to_string());
                        break;
                    }
                }
                let start = Instant::now();
                if let Err(e) = plugin.on_render_page(ctx.clone()).await {
                    failed = Some(e.to_string());
                    break;
                }
                if i >= WARMUP {
                    e2e.push(start.elapsed());
                }
            }
            let name = manifest.name.trim_start_matches("official-");
            if let Some(e) = failed {
                println!("{:<26} {:>5}KB error: {}", name, kb, e);
                continue;
            }
            let m: Vec<f64> = samples.iter_mut().map(|s| median(s)).collect();
            println!(
                "{:<26} {:>5}KB {:>8}KB {:>8.0} {:>10.0} {:>8.0} | {:>10.0} {:>10.0}",
                name,
                kb,
                out_len / 1024,
                m[0],
                m[1],
                m[2],
                m[3],
                median(&mut e2e)
            );
        }
    }
    println!(
        "(median of {} calls, microseconds; e2e includes instantiate + host merge)",
        ITERATIONS
    );
    Ok(())
}
