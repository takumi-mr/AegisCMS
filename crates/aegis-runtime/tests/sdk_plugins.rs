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
            assert!(!rendered.extra_css.is_empty(), "{}", name);
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
    assert_eq!(
        plugin.manifest().frame_domains(),
        [
            "www.youtube-nocookie.com",
            "player.vimeo.com",
            "www.openstreetmap.org"
        ]
    );
    // 地図は宣言したホストの iframe として、サニタイズ後も残る
    let map = plugin
        .render_blocks(vec![block(
            "official-blocks/map",
            serde_json::json!({ "latitude": 35.68, "longitude": 139.76 }),
        )])
        .await
        .unwrap();
    let map = aegis_ast::renderer::sanitize_html_fragment_with_frames(
        map[0].as_deref().unwrap(),
        plugin.manifest().frame_domains(),
    );
    assert!(map.contains("<iframe src=\"https://www.openstreetmap.org/export/embed.html?bbox="));
    let video = plugin
        .render_blocks(vec![block(
            "official-blocks/video",
            serde_json::json!({ "url": "https://www.youtube.com/watch?v=dQw4w9WgXcQ" }),
        )])
        .await
        .unwrap();
    assert!(video[0]
        .as_deref()
        .unwrap()
        .contains(r#"<iframe src="https://www.youtube-nocookie.com/embed/dQw4w9WgXcQ""#));
    assert!(html[0]
        .as_deref()
        .unwrap()
        .contains(r#"<a class="ob-button ob-button-primary" href="/start">Get started</a>"#));
    assert_eq!(html[1], None);
}

/// 公式プラグイン `official-page-toc` / `official-back-to-top` のビルド済み `plugin.wasm` で、`on-render-page` が
/// ホストから呼べること（設定が無いときは既定の見た目で、本文・`extra_css`・`extra_head` に足す）
#[tokio::test]
async fn test_official_reading_plugins_add_to_the_page() {
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

    let top = load(&engine, &plugins.join("official-back-to-top"));
    let rendered = top.on_render_page(page("article")).await.unwrap();
    assert!(rendered.append_content.ends_with("</a></div>"));
    assert!(rendered.append_content.contains("href=\"#top\""));
    assert!(rendered.extra_css.contains("a.obt.obt{"));
    let other = top.on_render_page(page("index")).await.unwrap();
    assert_eq!(other.content_html, page("index").content_html);
    assert!(other.extra_css.is_empty() && other.extra_head.is_empty() && other.scripts.is_empty());

    let share = load(&engine, &plugins.join("official-share-buttons"));
    let rendered = share.on_render_page(page("article")).await.unwrap();
    assert!(rendered.append_content.contains("class=\"osb-container\""));
    assert!(rendered.append_content.contains("osb-btn-x"));
    assert!(rendered.append_content.contains("osb-btn-line"));
    assert!(rendered.append_content.contains("osb-btn-copy"));
    assert!(rendered.extra_css.contains(".osb-container"));
    assert!(rendered.head_script_text().contains("osb-btn-copy"));
    let other = share.on_render_page(page("index")).await.unwrap();
    assert_eq!(other.content_html, page("index").content_html);
    assert!(other.extra_css.is_empty() && other.extra_head.is_empty() && other.scripts.is_empty());
}

trait HeadScriptText {
    /// 構造化した注入のうち、インラインのスクリプトの本文を全部つなげたもの
    fn head_script_text(&self) -> String;
}

impl HeadScriptText for PageRenderContext {
    fn head_script_text(&self) -> String {
        self.scripts
            .iter()
            .filter_map(|s| match &s.source {
                aegis_core::ScriptSource::Inline { code } => Some(code.as_str()),
                _ => None,
            })
            .collect()
    }
}

/// 公式プラグイン `official-code-lines` / `official-lightbox` / `official-external-links` のビルド済み `plugin.wasm` で、
/// `on-render-page` がホストから呼べること（本文に対象が無いページには何も足さない）
#[tokio::test]
async fn test_official_content_plugins_add_to_the_page() {
    let engine = WasmEngine::new().unwrap();
    let plugins = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../plugins");
    let page = |html: &str| PageRenderContext {
        template: "article.html".to_string(),
        content_type: "article".to_string(),
        content_html: html.to_string(),
        ..Default::default()
    };

    let cases = [
        (
            "official-code-lines",
            "<pre data-filename=\"a.rs\"><code class=\"language-rust\">fn a() {}</code></pre>",
            "pre.ocl-numbered{",
        ),
        (
            "official-lightbox",
            "<figure><img src=\"/uploads/a.png\" alt=\"A\"></figure>",
            "dialog.olb{",
        ),
        (
            "official-external-links",
            "<p><a href=\"https://example.org/\">x</a></p>",
            "a.oel-external::after",
        ),
    ];
    for (name, html, css) in cases {
        let plugin = load(&engine, &plugins.join(name));
        let rendered = host_render(&plugin, page(html)).await;
        assert_eq!(rendered.content_html, html, "{name} changed the content");
        assert!(
            rendered.extra_css.contains(css),
            "{name}: {}",
            rendered.extra_css
        );
        assert!(rendered.extra_head.starts_with("<script>"), "{name}");
        let plain = page("<p>Nothing to do</p>");
        let other = host_render(&plugin, plain.clone()).await;
        assert_eq!(other.content_html, plain.content_html, "{name}");
        assert!(
            other.extra_css.is_empty() && other.extra_head.is_empty(),
            "{name} added to a page without targets"
        );
    }
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

/// 公式プラグイン `official-ads` のビルド済み `plugin.wasm` で、マニフェスト、`render-blocks`、
/// `on-render-page`、動的 CSP 設定および Cookie 同意連携が正しく動作することを確認する
#[tokio::test]
async fn test_official_ads_plugin_manifest_and_execution() {
    let engine = WasmEngine::new().unwrap();
    let plugins = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../plugins");
    let ads_dir = plugins.join("official-ads");
    let ads = load(&engine, &ads_dir);

    // マニフェスト検査
    let manifest = ads.manifest();
    assert_eq!(manifest.name, "official-ads");
    manifest.validate_blocks().unwrap();
    manifest.validate_csp().unwrap();

    // CSP ケーパビリティの宣言検証
    assert_eq!(
        manifest.csp_script_domains(),
        ["pagead2.googlesyndication.com"]
    );
    assert!(manifest
        .csp_frame_domains()
        .contains(&"googleads.g.doubleclick.net".to_string()));
    assert!(manifest
        .csp_connect_domains()
        .contains(&"pagead2.googlesyndication.com".to_string()));

    // render-blocks フックの動作確認
    let mut node = BlockNode::new("official-ads/display");
    node.attributes = serde_json::json!({
        "custom_html": "<iframe src=\"https://example.com/ad\"></iframe>",
        "align": "center"
    });
    let blocks = vec![node];
    let results = ads.render_blocks(blocks).await.unwrap();
    assert_eq!(results.len(), 1);
    let html = results[0].as_ref().unwrap();
    assert!(html.contains("<div class=\"oa-ad-container oa-align-center\">"));
    // 既定で Cookie 同意連携（require_consent = true）が効いて data-aegis-consent が付くこと
    assert!(html.contains("data-aegis-consent=\"marketing\""));
    assert!(html.contains("data-src=\"https://example.com/ad\""));
}

/// ページと oEmbed の応答を URL で返す、手元のホスト API。通信した URL を覚える
struct FakeWeb {
    requested: Mutex<Vec<String>>,
    extra_domains: &'static str,
}

#[async_trait::async_trait]
impl aegis_runtime::HostServices for FakeWeb {
    async fn kv_get(&self, _key: &str) -> aegis_core::Result<Option<String>> {
        Ok(None)
    }
    async fn kv_set(&self, _key: &str, _value: &str) -> aegis_core::Result<()> {
        Ok(())
    }
    async fn http_fetch(
        &self,
        _plugin: &str,
        request: aegis_runtime::HttpRequest,
    ) -> aegis_core::Result<aegis_runtime::HttpResponse> {
        self.requested.lock().unwrap().push(request.url.clone());
        let (content_type, body) = if request
            .url
            .starts_with("https://publish.twitter.com/oembed")
        {
            (
                "application/json",
                r#"{"provider_name":"X","author_name":"AegisCMS","html":"<blockquote><p>Hello</p></blockquote>"}"#,
            )
        } else if request.url == "https://qiita.com/u/items/1" {
            (
                "text/html; charset=utf-8",
                r#"<html><head><meta property="og:title" content="Qiita &amp; Rust"><meta property="og:image" content="https://cdn.qiita.com/i.png"></head></html>"#,
            )
        } else if request.url == "https://blog.example/post" {
            ("text/html", r#"<head><title>Blog post</title></head>"#)
        } else {
            ("text/plain", "not found")
        };
        Ok(aegis_runtime::HttpResponse {
            status: 200,
            headers: [("content-type".to_string(), content_type.to_string())].into(),
            body: body.to_string(),
        })
    }
    fn plugin_setting(&self, _plugin: &str, key: &str) -> Option<String> {
        (key == "extra_allowed_domains" && !self.extra_domains.is_empty())
            .then(|| self.extra_domains.to_string())
    }
}

/// 公式プラグイン `official-link-embed` のビルド済み `plugin.wasm` で、URL だけの段落が OGP / oEmbed の
/// ブロックになり、許可されていない URL は通信せず、サイトの設定で足したドメインは通信できること
#[tokio::test]
async fn test_official_link_embed_plugin_fetches_in_the_sandbox() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../plugins/official-link-embed");
    let manifest = PluginManifest::from_json_with_defaults(
        &std::fs::read_to_string(dir.join("plugin.manifest.json")).unwrap(),
        &PluginResourceLimits::default(),
    )
    .unwrap();
    manifest.validate_blocks().unwrap();
    let wasm = std::fs::read(dir.join("plugin.wasm")).unwrap();
    let run = |extra_domains: &'static str| {
        let web = Arc::new(FakeWeb {
            requested: Mutex::new(Vec::new()),
            extra_domains,
        });
        let services: Arc<dyn aegis_runtime::HostServices> = web.clone();
        let plugin = WasmPluginInstance::from_bytes(
            WasmEngine::new().unwrap(),
            manifest.clone(),
            &wasm,
            services,
        )
        .unwrap();
        (plugin, web)
    };
    let paragraph = |text: &str| {
        let mut node = BlockNode::new("core/paragraph");
        node.text = Some(text.to_string());
        node
    };
    let blocks = || {
        vec![
            paragraph("https://qiita.com/u/items/1"),
            paragraph("https://x.com/aegiscms/status/1"),
            paragraph("https://blog.example/post"),
            paragraph("Read https://qiita.com/u/items/1 now"),
        ]
    };

    let (plugin, web) = run("");
    let out = plugin.transform_content_ast(blocks()).await.unwrap();
    assert_eq!(out[0].name, "official-link-embed/embed");
    assert_eq!(out[0].attributes["title"], "Qiita & Rust");
    assert_eq!(out[0].attributes["image"], "https://cdn.qiita.com/i.png");
    assert_eq!(out[0].attributes["site"], "qiita.com");
    assert_eq!(out[1].attributes["title"], "AegisCMS / X");
    assert_eq!(out[1].attributes["description"], "Hello");
    // 許可されていないドメインと、文章の中の URL はそのまま
    assert_eq!(out[2].name, "core/paragraph");
    assert_eq!(out[3].name, "core/paragraph");
    assert!(
        web.requested
            .lock()
            .unwrap()
            .iter()
            .all(|u| !u.contains("blog.example")),
        "no request to a domain that is not allowed"
    );

    // サイトの設定で足すと取得できる
    let (plugin, _) = run("blog.example");
    let out = plugin.transform_content_ast(blocks()).await.unwrap();
    assert_eq!(out[2].name, "official-link-embed/embed");
    assert_eq!(out[2].attributes["title"], "Blog post");

    // 表示のときは通信せず、属性のカードを出す
    let (plugin, web) = run("");
    let html = plugin
        .render_blocks(vec![{
            let mut node = BlockNode::new("official-link-embed/embed");
            node.attributes = serde_json::json!({
                "url": "https://qiita.com/u/items/1", "title": "T", "site": "qiita.com"
            });
            node
        }])
        .await
        .unwrap();
    assert!(html[0]
        .as_deref()
        .unwrap()
        .contains(r#"<a class="oe-card" href="https://qiita.com/u/items/1">"#));
    assert!(web.requested.lock().unwrap().is_empty());
}

/// 設定（`plugin_setting`）だけを返す、手元のホスト API
struct SettingsOnly(HashMap<String, String>);

#[async_trait::async_trait]
impl aegis_runtime::HostServices for SettingsOnly {
    async fn kv_get(&self, _key: &str) -> aegis_core::Result<Option<String>> {
        Ok(None)
    }
    async fn kv_set(&self, _key: &str, _value: &str) -> aegis_core::Result<()> {
        Ok(())
    }
    async fn http_fetch(
        &self,
        _plugin: &str,
        _request: aegis_runtime::HttpRequest,
    ) -> aegis_core::Result<aegis_runtime::HttpResponse> {
        Ok(aegis_runtime::HttpResponse::default())
    }
    fn plugin_setting(&self, _plugin: &str, key: &str) -> Option<String> {
        self.0.get(key).cloned()
    }
}

/// 公式プラグイン `official-clarity` のビルド済み `plugin.wasm` で、Clarity の起動コードが同意を待つ形で
/// `<head>` に入り、録画してはいけないページには入らないこと。CSP の宣言も確かめる
#[tokio::test]
async fn test_official_clarity_plugin_adds_the_tag_behind_consent() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../plugins/official-clarity");
    let manifest = PluginManifest::from_json_with_defaults(
        &std::fs::read_to_string(dir.join("plugin.manifest.json")).unwrap(),
        &PluginResourceLimits::default(),
    )
    .unwrap();
    manifest.validate_csp().unwrap();
    manifest.validate_render_page().unwrap();
    // Clarity の公式の案内: タグは www.clarity.ms、送信先は a〜z.clarity.ms と c.bing.com
    assert!(manifest
        .csp_script_domains()
        .contains(&"www.clarity.ms".to_string()));
    for host in [
        "www.clarity.ms",
        "a.clarity.ms",
        "z.clarity.ms",
        "c.bing.com",
    ] {
        assert!(
            manifest.csp_connect_domains().contains(&host.to_string()),
            "{host}"
        );
    }

    let wasm = std::fs::read(dir.join("plugin.wasm")).unwrap();
    let plugin = |settings: &[(&str, &str)]| {
        let map: HashMap<String, String> = settings
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        let services: Arc<dyn aegis_runtime::HostServices> = Arc::new(SettingsOnly(map));
        Arc::new(
            WasmPluginInstance::from_bytes(
                WasmEngine::new().unwrap(),
                manifest.clone(),
                &wasm,
                services,
            )
            .unwrap(),
        )
    };
    let page = |template: &str| PageRenderContext {
        template: template.to_string(),
        content_type: "article".to_string(),
        ..Default::default()
    };

    // プロジェクト ID が無ければ何も足さない
    let off = host_render(&plugin(&[]), page("article.html")).await;
    assert!(off.extra_head.is_empty());

    let on = plugin(&[("project_id", "abc123xyz9")]);
    let head = host_render(&on, page("article.html")).await.extra_head;
    assert!(head.contains(r#"type="text/plain""#), "{head}");
    assert!(head.contains(r#"data-aegis-consent="analytics""#), "{head}");
    assert!(head.contains("https://www.clarity.ms/tag/"), "{head}");
    assert!(head.contains(r#""abc123xyz9""#), "{head}");
    // 会員・買い物のページと、会員レベルを求めるページには足さない
    for template in [
        "account.html",
        "cart.html",
        "order.html",
        "forum_thread.html",
    ] {
        assert!(
            host_render(&on, page(template)).await.extra_head.is_empty(),
            "{template}"
        );
    }
    let mut restricted = page("article.html");
    restricted
        .fields
        .insert("access".to_string(), serde_json::json!("paid"));
    assert!(host_render(&on, restricted).await.extra_head.is_empty());
}
