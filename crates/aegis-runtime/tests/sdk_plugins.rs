//! `sdk/templates/*` と `sdk/examples/*` のビルド済み `plugin.wasm` が、実際のホスト（`WasmEngine`）で
//! 読み込めて、マニフェストに宣言したフックを実行できることを確認する。
//! テンプレートのビルド設定の不備（ホストに無い関数のインポートなど）を検出するため

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use aegis_ast::BlockNode;
use aegis_core::{PageRenderContext, PluginManifest, PluginResourceLimits, PostContext};
use aegis_runtime::{AegisPlugin, InstancePoolConfig, WasmEngine, WasmPluginInstance};

fn sdk_plugin_dirs() -> Vec<PathBuf> {
    let sdk = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../sdk");
    let mut dirs = Vec::new();
    for group in ["templates", "examples"] {
        for entry in std::fs::read_dir(sdk.join(group)).unwrap() {
            let path = entry.unwrap().path();
            if path.join("plugin.manifest.json").is_file() {
                dirs.push(path);
            }
        }
    }
    dirs.sort();
    dirs
}

fn load(engine: &WasmEngine, dir: &Path) -> WasmPluginInstance {
    let manifest = PluginManifest::from_json_with_defaults(
        &std::fs::read_to_string(dir.join("plugin.manifest.json")).unwrap(),
        &PluginResourceLimits::default(),
    )
    .unwrap();
    let wasm = std::fs::read(dir.join(&manifest.entrypoint))
        .unwrap_or_else(|e| panic!("{}: plugin.wasm is not built: {}", dir.display(), e));
    WasmPluginInstance::from_bytes(
        engine.clone(),
        manifest,
        &wasm,
        Arc::new(Mutex::new(HashMap::new())),
    )
    .unwrap_or_else(|e| panic!("{}: {}", dir.display(), e))
}

#[tokio::test]
async fn test_sdk_plugins_load_and_run_their_hooks() {
    run_sdk_plugins(&WasmEngine::new().unwrap()).await;
}

/// サーバーと同じプーリングアロケータでも同じように動くこと
#[tokio::test]
async fn test_sdk_plugins_run_on_the_pooling_allocator() {
    let engine = WasmEngine::with_instance_pool(InstancePoolConfig {
        size: 4,
        max_memory_mb: 64,
    })
    .unwrap();
    run_sdk_plugins(&engine).await;
}

async fn run_sdk_plugins(engine: &WasmEngine) {
    let dirs = sdk_plugin_dirs();
    assert!(dirs.len() >= 4, "expected the SDK templates and examples");

    for dir in dirs {
        let plugin = load(engine, &dir);
        let name = dir.display().to_string();
        let hooks = plugin.manifest().capabilities.hooks.clone();
        let blocks = vec![
            BlockNode::heading(2, "Intro"),
            BlockNode::paragraph("Body text"),
        ];

        if hooks.iter().any(|h| h == "on-before-save") {
            let post = PostContext {
                id: "id-1".to_string(),
                title: "Hello".to_string(),
                slug: "hello".to_string(),
                author_id: "editor1".to_string(),
                blocks_json: serde_json::to_string(&blocks).unwrap(),
            };
            let saved = plugin
                .on_before_save(post)
                .await
                .unwrap_or_else(|e| panic!("{}: on_before_save: {}", name, e));
            assert_eq!(saved.slug, "hello", "{}", name);
            assert!(saved.title.contains("Hello"), "{}: {}", name, saved.title);
        }

        if hooks.iter().any(|h| h == "transform-content-ast") {
            let transformed = plugin
                .transform_content_ast(blocks.clone())
                .await
                .unwrap_or_else(|e| panic!("{}: transform_content_ast: {}", name, e));
            // テンプレートは既存のブロックを残す（置き換えない）
            assert!(transformed.len() >= blocks.len(), "{}", name);
            assert_eq!(transformed[0].text.as_deref(), Some("Intro"), "{}", name);
            assert_eq!(
                transformed[1].text.as_deref(),
                Some("Body text"),
                "{}",
                name
            );
        }

        if hooks.iter().any(|h| h == "on-render-page") {
            let ctx = PageRenderContext {
                site_title: "Site".to_string(),
                template: "article.html".to_string(),
                ..Default::default()
            };
            let rendered = plugin
                .on_render_page(ctx)
                .await
                .unwrap_or_else(|e| panic!("{}: on_render_page: {}", name, e));
            assert_eq!(rendered.template, "article.html", "{}", name);
        }
    }
}

/// 公式プラグイン `official-blocks` のビルド済み `plugin.wasm` で、`render-blocks` がホストから
/// 呼べること（入力 `{"blocks": [...]}` と出力 `{"html": [...]}` の形が SDK と合っていること）
#[tokio::test]
async fn test_official_blocks_plugin_renders_its_blocks() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../plugins/official-blocks");
    let plugin = load(&WasmEngine::new().unwrap(), &dir);
    plugin.manifest().validate_blocks().unwrap();
    assert!(plugin.manifest().declares_block("official-blocks/button"));

    let block = |name: &str, attributes: serde_json::Value| {
        let mut node = BlockNode::new(name);
        node.attributes = attributes;
        node
    };
    let html = plugin
        .render_blocks(vec![
            block(
                "official-blocks/button",
                serde_json::json!({ "label": "Get started", "url": "/start" }),
            ),
            block(
                "official-blocks/link-card",
                serde_json::json!({ "url": "javascript:alert(1)", "title": "x" }),
            ),
        ])
        .await
        .unwrap();
    assert_eq!(html.len(), 2);
    assert!(html[0]
        .as_deref()
        .unwrap()
        .contains(r#"<a class="ob-button ob-button-primary" href="/start">Get started</a>"#));
    assert_eq!(html[1], None);
}
