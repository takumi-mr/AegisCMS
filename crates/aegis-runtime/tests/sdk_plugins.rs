//! `sdk/templates/*` と `sdk/examples/*` のビルド済み `plugin.wasm` が、実際のホスト（`WasmEngine`）で
//! 読み込めて、マニフェストに宣言したフックを実行できることを確認する。
//! テンプレートのビルド設定の不備（ホストに無い関数のインポートなど）を検出するため。
//! 本体に同梱する公式プラグインのテストもここに置く（同梱をやめたものは AegisCMS-Plugin にテストごと移した）

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

fn load(engine: &WasmEngine, dir: &Path) -> Arc<WasmPluginInstance> {
    let manifest = PluginManifest::from_json_with_defaults(
        &std::fs::read_to_string(dir.join("plugin.manifest.json")).unwrap(),
        &PluginResourceLimits::default(),
    )
    .unwrap();
    let wasm = std::fs::read(dir.join(&manifest.entrypoint))
        .unwrap_or_else(|e| panic!("{}: plugin.wasm is not built: {}", dir.display(), e));
    Arc::new(
        WasmPluginInstance::from_bytes(
            engine.clone(),
            manifest,
            &wasm,
            Arc::new(Mutex::new(HashMap::new())),
        )
        .unwrap_or_else(|e| panic!("{}: {}", dir.display(), e)),
    )
}

/// `on-render-page` を、ホストを通して呼ぶ。プラグインが `scripts` で足したスクリプトは、ホストが検査して
/// `<script>` のタグにし、`extra_head` に入れる（プラグインの出力そのものは `extra_head` に入らない）
async fn host_render(
    plugin: &Arc<WasmPluginInstance>,
    ctx: PageRenderContext,
) -> PageRenderContext {
    let mut supervisor = aegis_runtime::PluginSupervisor::new();
    supervisor.register(plugin.clone());
    supervisor.apply_render_page(ctx).await.unwrap()
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
        assert_eq!(plugin.manifest().abi_version, Some(2), "{}", name);
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
            // 再保存・プレビューで同じブロックに再適用しても重複して増えない（冪等）
            let twice = plugin
                .transform_content_ast(transformed.clone())
                .await
                .unwrap_or_else(|e| panic!("{}: transform_content_ast (2nd): {}", name, e));
            assert_eq!(twice.len(), transformed.len(), "{}", name);
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
            // ブロック専用プラグイン（react-island / vue-island）は該当ブロックがあるページにだけアセットを足す
            if plugin.manifest().blocks.is_empty() {
                assert!(!rendered.extra_css.is_empty(), "{}", name);
            }
        }
    }
}

/// ブロックプラグインのテンプレート `sdk/templates/rust-block` のビルド済み `plugin.wasm` で、マニフェストの
/// `blocks` が検証を通り、`render-blocks` がホストから呼べること
#[tokio::test]
async fn test_block_template_renders_its_block() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../sdk/templates/rust-block");
    let plugin = load(&WasmEngine::new().unwrap(), &dir);
    plugin.manifest().validate_blocks().unwrap();
    let name = format!("{}/note", plugin.manifest().name);
    assert!(plugin.manifest().declares_block(&name));

    let note = |attributes: serde_json::Value| {
        let mut node = BlockNode::new(&name);
        node.attributes = attributes;
        node
    };
    let html = plugin
        .render_blocks(vec![
            note(serde_json::json!({ "text": "Careful <now>", "tone": "warning" })),
            note(serde_json::json!({ "text": "" })),
        ])
        .await
        .unwrap();
    assert_eq!(html.len(), 2);
    let first = html[0].as_deref().unwrap();
    assert!(first
        .contains("<aside class=\"nb-note nb-note--warning\"><p>Careful &lt;now&gt;</p></aside>"));
    assert_eq!(html[1], None);
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
    // 外部のサイトを埋め込むブロックは official-embeds にあり、このプラグインは CSP にホストを足さない
    assert!(plugin.manifest().frame_domains().is_empty());
    assert!(html[0]
        .as_deref()
        .unwrap()
        .contains(r#"<a class="ob-button ob-button-primary" href="/start">Get started</a>"#));
    assert_eq!(html[1], None);
}

/// 公式プラグイン `official-page-toc` のビルド済み `plugin.wasm` で、`on-render-page` が
/// ホストから呼べること（設定が無いときは既定の見た目で、本文・`extra_css`・`extra_head` に足す）
#[tokio::test]
async fn test_official_page_toc_adds_to_the_page() {
    let engine = WasmEngine::new().unwrap();
    let plugins = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../plugins");
    let body = "<p>Intro</p><h2 id=\"one\">One</h2><p>a</p><h2 id=\"two\">Two</h2>";
    let page = |content_type: &str| PageRenderContext {
        template: "article.html".to_string(),
        content_type: content_type.to_string(),
        content_html: body.to_string(),
        content_length: body.len(),
        ..Default::default()
    };

    // 本文に足す HTML は `append_content` に返り、本文（`content_html`）は変わらない。スクリプトは
    // `scripts`（構造化した注入）で返り、ホストを通すと `extra_head` の `<script>` になる
    let toc = load(&engine, &plugins.join("official-page-toc"));
    let rendered = toc.on_render_page(page("article")).await.unwrap();
    assert_eq!(rendered.content_html, body);
    assert!(rendered.append_content.contains("<a href=\"#one\">One</a>"));
    assert!(rendered.append_content.contains("<a href=\"#two\">Two</a>"));
    assert!(rendered.extra_css.contains(".apt{position:fixed"));
    assert!(rendered.extra_head.is_empty());
    assert!(matches!(
        rendered.scripts.as_slice(),
        [aegis_core::InjectedScript {
            source: aegis_core::ScriptSource::Inline { .. },
            ..
        }]
    ));
    let hosted = host_render(&toc, page("article")).await;
    assert!(
        hosted.extra_head.starts_with("<script>"),
        "{}",
        hosted.extra_head
    );
    let other = toc.on_render_page(page("index")).await.unwrap();
    assert_eq!(other.content_html, page("index").content_html);
    assert!(other.extra_css.is_empty() && other.extra_head.is_empty() && other.scripts.is_empty());
}

/// 公式プラグイン `official-code-enhancer`（ハイライト）と `official-mermaid`（図）のビルド済み `plugin.wasm` で、
/// マニフェスト（CSP・設定）の検査と、本文に対象があるページにだけ足すこと（ハイライトは Mermaid を持たず、
/// 図は Mermaid のプラグインだけが扱う）を確かめる
#[tokio::test]
async fn test_official_code_and_diagram_plugins_load_only_when_needed() {
    let engine = WasmEngine::new().unwrap();
    let plugins = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../plugins");
    let page = |html: &str| PageRenderContext {
        template: "article.html".to_string(),
        content_type: "article".to_string(),
        content_html: html.to_string(),
        ..Default::default()
    };
    let code = "<pre><code class=\"language-rust\">fn a() {}</code></pre>";
    let diagram = "<pre><code class=\"language-mermaid\">graph TD; A--&gt;B;</code></pre>";

    let highlight = load(&engine, &plugins.join("official-code-enhancer"));
    let manifest = highlight.manifest();
    manifest.validate_csp().unwrap();
    manifest.validate_settings().unwrap();
    assert_eq!(manifest.csp_script_domains(), ["cdn.jsdelivr.net"]);
    assert!(manifest.capabilities.network.is_none() && manifest.capabilities.storage.is_none());
    let out = host_render(&highlight, page(code)).await;
    assert_eq!(out.content_html, code);
    assert!(out.extra_head.contains("prism.min.js"));
    assert!(!out.extra_head.to_lowercase().contains("mermaid.min.js"));
    let plain = host_render(&highlight, page("<p>No code</p>")).await;
    assert!(plain.extra_css.is_empty() && plain.extra_head.is_empty());

    let mermaid = load(&engine, &plugins.join("official-mermaid"));
    let manifest = mermaid.manifest();
    manifest.validate_csp().unwrap();
    manifest.validate_settings().unwrap();
    assert_eq!(manifest.csp_script_domains(), ["cdn.jsdelivr.net"]);
    let out = host_render(&mermaid, page(diagram)).await;
    assert_eq!(out.content_html, diagram);
    assert!(out
        .extra_head
        .contains("mermaid@10.9.3/dist/mermaid.min.js"));
    assert!(out.extra_css.contains(".omd-diagram"));
    for html in [code, "<p>No diagram</p>"] {
        let other = host_render(&mermaid, page(html)).await;
        assert!(
            other.extra_css.is_empty() && other.extra_head.is_empty(),
            "{html}"
        );
    }
}

/// 公式プラグイン `official-plantuml` のビルド済み `plugin.wasm` で、マニフェスト（CSP・設定）の検査と、
/// ` ```plantuml ` のブロックだけを PlantUML サーバーの画像に置き換える（スクリプトは足さない）ことを確かめる
#[tokio::test]
async fn test_official_plantuml_replaces_only_plantuml_blocks() {
    let engine = WasmEngine::new().unwrap();
    let plugins = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../plugins");
    let page = |html: &str| PageRenderContext {
        template: "article.html".to_string(),
        content_type: "article".to_string(),
        content_html: html.to_string(),
        ..Default::default()
    };
    let plantuml = load(&engine, &plugins.join("official-plantuml"));
    let manifest = plantuml.manifest();
    manifest.validate_csp().unwrap();
    manifest.validate_settings().unwrap();
    assert_eq!(manifest.csp_img_domains(), ["www.plantuml.com"]);
    assert!(manifest.csp_script_domains().is_empty());
    assert!(manifest.capabilities.network.is_none() && manifest.capabilities.storage.is_none());

    // 大きめの図（シーケンス図 200 行）でも、燃料・時間の上限に収まる
    let mut source = String::from("@startuml\n");
    for i in 0..200 {
        source.push_str(&format!(
            "Alice -&gt; Bob : message {i}\nBob --&gt; Alice : reply {i}\n"
        ));
    }
    source.push_str("@enduml");
    let code = "<pre><code class=\"language-rust\">fn a() {}</code></pre>";
    for diagram in [
        "<pre><code class=\"language-plantuml\">Alice -&gt; Bob : hello</code></pre>".to_string(),
        format!("<pre><code class=\"language-plantuml\">{source}</code></pre>"),
    ] {
        let out = host_render(&plantuml, page(&format!("{code}{diagram}"))).await;
        assert!(out.content_html.starts_with(code));
        assert!(out.content_html.contains(
            "<figure class=\"opu-diagram\"><img src=\"https://www.plantuml.com/plantuml/svg/"
        ));
        assert!(!out.content_html.contains("language-plantuml"));
        assert!(out.extra_css.contains(".opu-diagram"));
        assert!(out.extra_head.is_empty());
    }
    for html in [code, "<p>No diagram</p>"] {
        let other = host_render(&plantuml, page(html)).await;
        assert_eq!(other.content_html, html);
        assert!(other.extra_css.is_empty() && other.extra_head.is_empty());
    }
}
