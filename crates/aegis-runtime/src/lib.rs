pub mod config;
pub mod guard;
pub mod plugin;
pub mod services;
pub mod wasm;

pub use config::{PluginsConfig, RuntimeConfig};
pub use guard::CapabilityGuard;
pub use plugin::{AegisPlugin, CommentCheck, PluginSupervisor};
pub use services::{HostServices, HttpRequest, HttpResponse, LocalHostServices, PluginServices};
pub use wasm::{
    HookInvocationResult, InstalledPlugin, InstancePoolConfig, PluginHostState, PluginInfo,
    PluginLogEntry, WasmEngine, WasmPluginInstance, WasmPluginLoader,
};

#[cfg(test)]
mod tests {
    use super::*;
    use aegis_ast::BlockNode;
    use aegis_core::{
        NetworkCapability, PluginCapabilities, PluginManifest, PluginResourceLimits, PostContext,
    };
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex};

    fn make_sample_manifest(hooks: Vec<String>) -> PluginManifest {
        PluginManifest {
            name: "test-plugin".to_string(),
            version: "1.0.0".to_string(),
            entrypoint: "plugin.wasm".to_string(),
            capabilities: PluginCapabilities {
                embed: None,
                network: Some(NetworkCapability {
                    allowed_domains: vec!["hooks.slack.com".to_string()],
                }),
                storage: None,
                hooks,
            },
            resource_limits: PluginResourceLimits {
                max_memory_mb: 16,
                max_fuel_units: 100_000,
                execution_timeout_ms: 250,
            },
            ..Default::default()
        }
    }

    #[test]
    fn test_capability_guard_network() {
        let manifest = make_sample_manifest(vec!["on-before-save".to_string()]);
        let guard = CapabilityGuard::new(&manifest);

        assert!(guard
            .check_network_access("https://hooks.slack.com/services/xxx")
            .is_ok());
        assert!(guard.check_network_access("https://evil.com/leak").is_err());
        assert!(guard
            .check_network_access("https://evil.com#.hooks.slack.com")
            .is_err());
        assert!(guard
            .check_network_access("https://hooks.slack.com@evil.com/")
            .is_err());
        assert!(guard
            .check_network_access("https://evil.com\\.hooks.slack.com")
            .is_err());
        assert!(guard
            .check_network_access("https://hooks.slack.com:443/x")
            .is_ok());
    }

    #[test]
    fn test_capability_guard_hooks() {
        let manifest = make_sample_manifest(vec!["on-before-save".to_string()]);
        let guard = CapabilityGuard::new(&manifest);

        assert!(guard.check_hook("on-before-save").is_ok());
        assert!(guard.check_hook("transform-content-ast").is_err());
    }

    #[tokio::test]
    async fn test_render_page_output_only_overrides_returned_fields() {
        // `{"extra_css":"x"}` だけを返すプラグイン
        let wat_src = r#"
        (module
            (memory (export "memory") 1)
            (data (i32.const 256) "{\"extra_css\":\"x\"}")
            (func (export "allocate") (param $len i32) (result i32) i32.const 1024)
            (func (export "on_render_page") (param $ptr i32) (param $len i32) (result i64)
                (i64.or (i64.shl (i64.const 256) (i64.const 32)) (i64.const 17))))
        "#;
        let instance = WasmPluginInstance::from_bytes(
            WasmEngine::new().unwrap(),
            make_sample_manifest(vec!["on-render-page".to_string()]),
            &wat::parse_str(wat_src).unwrap(),
            Arc::new(Mutex::new(HashMap::new())),
        )
        .unwrap();
        let ctx = aegis_core::PageRenderContext {
            site_title: "Site".to_string(),
            template: "index.html".to_string(),
            items: vec![aegis_core::ContentSummary {
                slug: "first".to_string(),
                ..Default::default()
            }],
            ..Default::default()
        };

        let rendered = instance.on_render_page(ctx).await.unwrap();
        assert_eq!(rendered.extra_css, "x");
        assert_eq!(rendered.site_title, "Site");
        assert_eq!(rendered.template, "index.html");
        assert_eq!(rendered.items.len(), 1);
    }

    #[tokio::test]
    async fn test_wasm_successful_post_transform() {
        let engine = WasmEngine::new().expect("WasmEngine initialization failed");
        let kv = Arc::new(Mutex::new(HashMap::new()));

        // ホストAPIの host::log を呼び出し、入力 post の内容をそのまま返すWasmモジュール
        let wat_src = r#"
        (module
            (import "host" "log" (func $log (param i32 i32 i32)))
            (memory (export "memory") 1)
            (func (export "allocate") (param $len i32) (result i32)
                i32.const 16
            )
            (func (export "on_before_save") (param $ptr i32) (param $len i32) (result i64)
                ;; host::log(1 /* info */, $ptr, $len) を呼び出し
                (call $log (i32.const 1) (local.get $ptr) (local.get $len))

                ;; (ptr << 32) | len を返却（入力データをそのまま返す）
                (i64.or
                    (i64.shl (i64.extend_i32_u (local.get $ptr)) (i64.const 32))
                    (i64.extend_i32_u (local.get $len))
                )
            )
        )
        "#;

        let wasm_bytes = wat::parse_str(wat_src).expect("WAT parse failed");
        let manifest = make_sample_manifest(vec!["on-before-save".to_string()]);

        let instance = WasmPluginInstance::from_bytes(engine, manifest, &wasm_bytes, kv)
            .expect("Instance creation failed");

        let post = PostContext {
            id: "uuid-1234".to_string(),
            title: "Secure Architecture".to_string(),
            slug: "secure-architecture".to_string(),
            author_id: "takumi".to_string(),
            blocks_json: "[]".to_string(),
        };

        let result = instance.on_before_save(post.clone()).await;
        assert!(result.is_ok());
        let output = result.unwrap();
        assert_eq!(output.slug, "secure-architecture");
        assert_eq!(output.author_id, "takumi");
    }

    #[tokio::test]
    async fn test_call_hook_raw_captures_logs_and_metrics() {
        let engine = WasmEngine::new().expect("WasmEngine initialization failed");
        let kv = Arc::new(Mutex::new(HashMap::new()));

        let wat_src = r#"
        (module
            (import "host" "log" (func $log (param i32 i32 i32)))
            (memory (export "memory") 1)
            (data (i32.const 256) "Hello from Wasm logger")
            (func (export "allocate") (param $len i32) (result i32)
                i32.const 16
            )
            (func (export "on_before_save") (param $ptr i32) (param $len i32) (result i64)
                (call $log (i32.const 1) (i32.const 256) (i32.const 22))
                (i64.or
                    (i64.shl (i64.extend_i32_u (local.get $ptr)) (i64.const 32))
                    (i64.extend_i32_u (local.get $len))
                )
            )
        )
        "#;

        let wasm_bytes = wat::parse_str(wat_src).expect("WAT parse failed");
        let manifest = make_sample_manifest(vec!["on-before-save".to_string()]);

        let instance = WasmPluginInstance::from_bytes(engine, manifest, &wasm_bytes, kv)
            .expect("Instance creation failed");

        let exports = instance.exports();
        assert!(exports.contains(&"on_before_save".to_string()));
        assert!(exports.contains(&"allocate".to_string()));

        let raw_res = instance
            .call_hook_raw("on-before-save", "on_before_save", b"{\"title\":\"Test\"}")
            .await
            .expect("Hook raw execution failed");

        assert!(raw_res.fuel_consumed > 0);
        assert!(raw_res.duration_ms >= 0.0);
        assert_eq!(
            raw_res.output.as_deref(),
            Some(b"{\"title\":\"Test\"}".as_slice())
        );
        assert_eq!(raw_res.logs.len(), 1);
        assert_eq!(raw_res.logs[0].level, "info");
        assert_eq!(raw_res.logs[0].message, "Hello from Wasm logger");
        assert!(!raw_res.logs[0].timestamp.is_empty());
    }

    #[tokio::test]
    async fn test_wasm_before_save_rejection() {
        let engine = WasmEngine::new().expect("WasmEngine initialization failed");
        let kv = Arc::new(Mutex::new(HashMap::new()));

        // on_before_save always returns {"error": "title is required"} (what the SDK writes for Err)
        let body = r#"{"error":"title is required"}"#;
        let wat_src = format!(
            r#"
        (module
            (memory (export "memory") 1)
            (data (i32.const 1024) "{data}")
            (func (export "allocate") (param $len i32) (result i32)
                i32.const 16
            )
            (func (export "on_before_save") (param $ptr i32) (param $len i32) (result i64)
                (i64.or (i64.shl (i64.const 1024) (i64.const 32)) (i64.const {len}))
            )
        )
        "#,
            data = body.replace('"', "\\22"),
            len = body.len()
        );
        let wasm_bytes = wat::parse_str(&wat_src).expect("WAT parse failed");
        let manifest = make_sample_manifest(vec!["on-before-save".to_string()]);
        let instance = WasmPluginInstance::from_bytes(engine, manifest, &wasm_bytes, kv)
            .expect("Instance creation failed");

        let post = PostContext {
            id: "uuid-1234".to_string(),
            title: String::new(),
            slug: "untitled".to_string(),
            author_id: "takumi".to_string(),
            blocks_json: "[]".to_string(),
        };
        match instance.on_before_save(post).await {
            Err(aegis_core::AegisError::PluginRejected(message)) => {
                assert!(message.ends_with(": title is required"), "{}", message);
            }
            other => panic!("expected a rejection, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn test_wasm_user_authenticated_hook() {
        // Returns a fixed body from a data segment: a role mapping, or a rejection
        async fn run(body: &str) -> aegis_core::Result<aegis_core::UserAuthContext> {
            let engine = WasmEngine::new().expect("WasmEngine initialization failed");
            let kv = Arc::new(Mutex::new(HashMap::new()));
            let wat_src = format!(
                r#"
            (module
                (memory (export "memory") 1)
                (data (i32.const 1024) "{data}")
                (func (export "allocate") (param $len i32) (result i32)
                    i32.const 16
                )
                (func (export "on_user_authenticated") (param $ptr i32) (param $len i32) (result i64)
                    (i64.or (i64.shl (i64.const 1024) (i64.const 32)) (i64.const {len}))
                )
            )
            "#,
                data = body.replace('"', "\\22"),
                len = body.len()
            );
            let wasm_bytes = wat::parse_str(&wat_src).expect("WAT parse failed");
            let manifest = make_sample_manifest(vec!["on-user-authenticated".to_string()]);
            let instance = WasmPluginInstance::from_bytes(engine, manifest, &wasm_bytes, kv)
                .expect("Instance creation failed");
            instance
                .on_user_authenticated(aegis_core::UserAuthContext {
                    provider: "oidc".to_string(),
                    user_id: "alice".to_string(),
                    email: "alice@example.com".to_string(),
                    display_name: None,
                    role: "viewer".to_string(),
                    metadata: HashMap::new(),
                })
                .await
        }

        let mapped = run(r#"{"provider":"oidc","user_id":"alice","role":"admin"}"#)
            .await
            .unwrap();
        assert_eq!(mapped.role, "admin");
        match run(r#"{"error":"not allowed"}"#).await {
            Err(aegis_core::AegisError::PluginRejected(message)) => {
                assert_eq!(message, "test-plugin: not allowed");
            }
            other => panic!("expected a rejection, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn test_wasm_ast_transform_hook() {
        let engine = WasmEngine::new().expect("WasmEngine initialization failed");
        let kv = Arc::new(Mutex::new(HashMap::new()));

        // AST配列をそのままエコーバックするWasmモジュール
        let wat_src = r#"
        (module
            (memory (export "memory") 1)
            (func (export "allocate") (param $len i32) (result i32)
                i32.const 32
            )
            (func (export "transform_content_ast") (param $ptr i32) (param $len i32) (result i64)
                (i64.or
                    (i64.shl (i64.extend_i32_u (local.get $ptr)) (i64.const 32))
                    (i64.extend_i32_u (local.get $len))
                )
            )
        )
        "#;

        let wasm_bytes = wat::parse_str(wat_src).expect("WAT parse failed");
        let manifest = make_sample_manifest(vec!["transform-content-ast".to_string()]);

        let instance = WasmPluginInstance::from_bytes(engine, manifest, &wasm_bytes, kv)
            .expect("Instance creation failed");

        let blocks = vec![
            BlockNode::heading(2, "Sandboxed Plugin Heading"),
            BlockNode::paragraph("This block is processed inside WebAssembly."),
        ];

        let result = instance.transform_content_ast(blocks.clone()).await;
        assert!(result.is_ok());
        let output = result.unwrap();
        assert_eq!(output.len(), 2);
        assert_eq!(output[0].name, "core/heading");
        assert_eq!(output[1].name, "core/paragraph");
    }

    #[tokio::test]
    async fn test_wasm_compiled_plugin_from_sdk() {
        let wasm_path = std::path::Path::new("../../sdk/templates/rust/plugin.wasm");
        let fallback_path = std::path::Path::new("sdk/templates/rust/plugin.wasm");
        let path = if wasm_path.exists() {
            wasm_path
        } else if fallback_path.exists() {
            fallback_path
        } else {
            return;
        };

        let wasm_bytes = std::fs::read(path).expect("Failed to read plugin.wasm");
        let engine = WasmEngine::new().expect("WasmEngine initialization failed");
        let kv = Arc::new(Mutex::new(HashMap::new()));

        let manifest = make_sample_manifest(vec![
            "on-before-save".to_string(),
            "transform-content-ast".to_string(),
        ]);

        let instance = WasmPluginInstance::from_bytes(engine, manifest, &wasm_bytes, kv)
            .expect("Instance creation failed");

        let post = PostContext {
            id: "uuid-1234".to_string(),
            title: "  Trimmed Post Title  ".to_string(),
            slug: "trimmed-post".to_string(),
            author_id: "author-1".to_string(),
            blocks_json: "[]".to_string(),
        };

        let saved = instance
            .on_before_save(post)
            .await
            .expect("on_before_save failed");
        assert_eq!(saved.title, "Trimmed Post Title");

        let blocks = vec![BlockNode::heading(1, "Test Heading")];
        let transformed = instance
            .transform_content_ast(blocks)
            .await
            .expect("transform_content_ast failed");
        assert_eq!(transformed.len(), 2);
        assert_eq!(transformed[1].name, "core/paragraph");
    }

    #[tokio::test]
    async fn test_wasm_on_render_page_theme_hook() {
        let engine = WasmEngine::new().expect("WasmEngine initialization failed");
        let kv = Arc::new(Mutex::new(HashMap::new()));

        let wat_src = r#"
        (module
            (memory (export "memory") 1)
            (func (export "allocate") (param $len i32) (result i32)
                i32.const 64
            )
            (func (export "on_render_page") (param $ptr i32) (param $len i32) (result i64)
                (i64.or
                    (i64.shl (i64.extend_i32_u (local.get $ptr)) (i64.const 32))
                    (i64.extend_i32_u (local.get $len))
                )
            )
        )
        "#;

        let wasm_bytes = wat::parse_str(wat_src).expect("WAT parse failed");
        let manifest = make_sample_manifest(vec!["on-render-page".to_string()]);

        let instance = WasmPluginInstance::from_bytes(engine, manifest, &wasm_bytes, kv)
            .expect("Instance creation failed");

        let ctx = aegis_core::PageRenderContext {
            site_title: "My Site".to_string(),
            site_description: "A fast site".to_string(),
            template: "article.html".to_string(),
            content_type: "article".to_string(),
            title: "Post".to_string(),
            slug: "post".to_string(),
            content_html: "<p>Text</p>".to_string(),
            author: None,
            published_at: None,
            updated_at: None,
            fields: HashMap::new(),
            relations: HashMap::new(),
            items: vec![],
            extra_css: "".to_string(),
            extra_head: "".to_string(),
            theme_vars: HashMap::new(),
            pagination: None,
            comments: None,
            ..Default::default()
        };

        let result = instance.on_render_page(ctx).await;
        assert!(result.is_ok());
        let res_ctx = result.unwrap();
        assert_eq!(res_ctx.site_title, "My Site");
        assert_eq!(res_ctx.template, "article.html");
    }

    /// Grows its memory by 32 pages (2 MiB) and then echoes the AST back
    const GROWING_PLUGIN_WAT: &str = r#"
    (module
        (memory (export "memory") 1)
        (func (export "allocate") (param $len i32) (result i32)
            i32.const 32
        )
        (func (export "transform_content_ast") (param $ptr i32) (param $len i32) (result i64)
            (drop (memory.grow (i32.const 32)))
            (i64.or
                (i64.shl (i64.extend_i32_u (local.get $ptr)) (i64.const 32))
                (i64.extend_i32_u (local.get $len))
            )
        )
    )
    "#;

    async fn run_growing_plugin(max_memory_mb: u32) -> aegis_core::Result<Vec<BlockNode>> {
        let engine = WasmEngine::new().expect("WasmEngine initialization failed");
        let mut manifest = make_sample_manifest(vec!["transform-content-ast".to_string()]);
        manifest.resource_limits.max_memory_mb = max_memory_mb;
        let wasm_bytes = wat::parse_str(GROWING_PLUGIN_WAT).expect("WAT parse failed");
        let instance = WasmPluginInstance::from_bytes(
            engine,
            manifest,
            &wasm_bytes,
            Arc::new(Mutex::new(HashMap::new())),
        )
        .expect("Instance creation failed");
        instance
            .transform_content_ast(vec![BlockNode::paragraph("memory test")])
            .await
    }

    #[tokio::test]
    #[cfg_attr(
        windows,
        ignore = "Wasmtime synchronous trap panics across non-unwinding boundary on Windows MSVC (see AGENTS.md / TODO.md)"
    )]
    async fn test_wasm_memory_limit_is_enforced() {
        // 1 page + 32 pages (~2.1 MiB) fits in 16 MiB
        assert!(run_growing_plugin(16).await.is_ok());

        // ...but not in 1 MiB: growing past max_memory_mb traps instead of succeeding
        let err = run_growing_plugin(1).await.unwrap_err();
        assert!(
            err.to_string().contains("memory"),
            "unexpected error: {}",
            err
        );
    }

    /// 呼び出しのたびにメモリ上のカウンター（4096 番地）を 1 増やし、その値をタイトルとして返すプラグイン
    const COUNTER_WAT: &str = r#"
    (module
        (memory (export "memory") 1)
        (data (i32.const 256) "{\"title\":\"0\"}")
        (func (export "allocate") (param i32) (result i32) i32.const 1024)
        (func (export "on_before_save") (param i32 i32) (result i64)
            (i32.store (i32.const 4096) (i32.add (i32.load (i32.const 4096)) (i32.const 1)))
            (i32.store8 (i32.const 266) (i32.add (i32.const 48) (i32.load (i32.const 4096))))
            (i64.or (i64.shl (i64.const 256) (i64.const 32)) (i64.const 13))))
    "#;

    fn pooled_engine(size: u32) -> WasmEngine {
        WasmEngine::with_instance_pool(InstancePoolConfig {
            size,
            max_memory_mb: 16,
        })
        .expect("pooled engine")
    }

    fn counter_plugin(engine: WasmEngine) -> Arc<WasmPluginInstance> {
        Arc::new(
            WasmPluginInstance::from_bytes(
                engine,
                make_sample_manifest(vec!["on-before-save".to_string()]),
                &wat::parse_str(COUNTER_WAT).unwrap(),
                Arc::new(Mutex::new(HashMap::new())),
            )
            .unwrap(),
        )
    }

    fn post() -> PostContext {
        PostContext {
            id: "id".to_string(),
            title: "t".to_string(),
            slug: "s".to_string(),
            author_id: "a".to_string(),
            blocks_json: "[]".to_string(),
        }
    }

    #[tokio::test]
    async fn test_pooled_instances_start_from_a_clean_memory() {
        // 1 枠のプールでは毎回同じ枠を使い回すが、返却時にゼロへ戻されるので状態は残らない
        let plugin = counter_plugin(pooled_engine(1));
        for _ in 0..5 {
            assert_eq!(plugin.on_before_save(post()).await.unwrap().title, "1");
        }
    }

    #[tokio::test]
    async fn test_pooled_calls_wait_for_a_free_slot() {
        // 枠より多い同時呼び出しは失敗せず、空きを待って実行される
        let plugin = counter_plugin(pooled_engine(2));
        let calls: Vec<_> = (0..32)
            .map(|_| {
                let plugin = plugin.clone();
                tokio::spawn(async move { plugin.on_before_save(post()).await })
            })
            .collect();
        for call in calls {
            assert_eq!(call.await.unwrap().unwrap().title, "1");
        }
    }

    #[test]
    fn test_plugins_larger_than_a_pool_slot_are_rejected_at_load() {
        let mut manifest = make_sample_manifest(vec!["on-before-save".to_string()]);
        manifest.resource_limits.max_memory_mb = 32;
        let wasm = wat::parse_str(COUNTER_WAT).unwrap();
        let kv = Arc::new(Mutex::new(HashMap::new()));
        let error =
            WasmPluginInstance::from_bytes(pooled_engine(1), manifest.clone(), &wasm, kv.clone())
                .err()
                .expect("max_memory_mb above the slot size")
                .to_string();
        assert!(error.contains("pool_max_memory_mb"), "{}", error);

        // 初期メモリ（32 MiB = 512 ページ）が枠に収まらないモジュール
        manifest.resource_limits.max_memory_mb = 16;
        let large = wat::parse_str(r#"(module (memory (export "memory") 512))"#).unwrap();
        assert!(WasmPluginInstance::from_bytes(
            pooled_engine(1),
            manifest.clone(),
            &large,
            kv.clone()
        )
        .is_err());
        // プールを使わないエンジンでは枠の制限は無い（max_memory_mb は StoreLimits で別に効く）
        assert!(
            WasmPluginInstance::from_bytes(WasmEngine::new().unwrap(), manifest, &large, kv)
                .is_ok()
        );
    }

    /// Records HTTP requests and answers them; KV in memory; one setting
    struct FakeServices {
        kv: Mutex<HashMap<String, String>>,
        requests: Mutex<Vec<HttpRequest>>,
    }

    #[async_trait::async_trait]
    impl HostServices for FakeServices {
        async fn kv_get(&self, key: &str) -> aegis_core::Result<Option<String>> {
            Ok(self.kv.lock().unwrap().get(key).cloned())
        }
        async fn kv_set(&self, key: &str, value: &str) -> aegis_core::Result<()> {
            self.kv
                .lock()
                .unwrap()
                .insert(key.to_string(), value.to_string());
            Ok(())
        }
        async fn http_fetch(
            &self,
            plugin: &str,
            request: HttpRequest,
        ) -> aegis_core::Result<HttpResponse> {
            assert_eq!(plugin, "test-plugin");
            self.requests.lock().unwrap().push(request);
            Ok(HttpResponse {
                status: 201,
                headers: Default::default(),
                body: r#"{"error":"fetched"}"#.to_string(),
            })
        }
        fn plugin_setting(&self, plugin: &str, key: &str) -> Option<String> {
            (plugin == "test-plugin" && key == "greeting")
                .then(|| r#"{"error":"configured"}"#.to_string())
        }
    }

    /// A module whose `on_order_status_changed` runs `body` (WAT instructions leaving a packed
    /// i64 result) with `data` at offset 256, and a bump allocator from offset 4096
    fn host_api_module(data: &str, body: &str) -> Vec<u8> {
        wat::parse_str(format!(
            r#"
            (module
                (import "host" "http_fetch" (func $http_fetch (param i32 i32) (result i64)))
                (import "host" "kv_set" (func $kv_set (param i32 i32 i32 i32) (result i32)))
                (import "host" "kv_get" (func $kv_get (param i32 i32) (result i64)))
                (import "host" "config_get" (func $config_get (param i32 i32) (result i64)))
                (memory (export "memory") 1)
                (global $next (mut i32) (i32.const 4096))
                (data (i32.const 256) "{data}")
                (func (export "allocate") (param $len i32) (result i32)
                    (local $ptr i32)
                    (local.set $ptr (global.get $next))
                    (global.set $next (i32.add (global.get $next) (local.get $len)))
                    (local.get $ptr))
                (func (export "on_order_status_changed") (param $ptr i32) (param $len i32) (result i64)
                    {body}))
            "#,
            data = data.replace('"', "\\22"),
            body = body
        ))
        .unwrap()
    }

    fn host_api_plugin(
        services: Arc<FakeServices>,
        wasm: &[u8],
        allowed: &str,
        kv_namespace: Option<&str>,
    ) -> WasmPluginInstance {
        let mut manifest = make_sample_manifest(vec!["on-order-status-changed".to_string()]);
        manifest.capabilities.network = Some(NetworkCapability {
            allowed_domains: vec![allowed.to_string()],
        });
        manifest.capabilities.storage = kv_namespace.map(|ns| aegis_core::StorageCapability {
            kv_namespace: ns.to_string(),
        });
        let services: Arc<dyn HostServices> = services;
        WasmPluginInstance::from_bytes(WasmEngine::new().unwrap(), manifest, wasm, services)
            .unwrap()
    }

    fn fake_services() -> Arc<FakeServices> {
        Arc::new(FakeServices {
            kv: Mutex::new(HashMap::new()),
            requests: Mutex::new(Vec::new()),
        })
    }

    async fn run_order_hook(plugin: &WasmPluginInstance) -> aegis_core::Result<()> {
        plugin
            .on_order_status_changed(aegis_core::OrderStatusChange::default())
            .await
    }

    #[tokio::test]
    async fn test_http_fetch_checks_the_allowed_domains() {
        let request = r#"{"method":"POST","url":"https://hooks.example.com/notify","body":"hi"}"#;
        let wasm = host_api_module(
            request,
            &format!(
                "(call $http_fetch (i32.const 256) (i32.const {}))",
                request.len()
            ),
        );

        // Allowed: the host answers and the plugin returns the response body's JSON
        let services = fake_services();
        let plugin = host_api_plugin(services.clone(), &wasm, "example.com", None);
        let result = run_order_hook(&plugin).await;
        // The response object has no "error" key at the top level, so the hook succeeds
        assert!(result.is_ok(), "{:?}", result);
        let requests = services.requests.lock().unwrap().clone();
        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0].method, "POST");
        assert_eq!(requests[0].body.as_deref(), Some("hi"));

        // Not in allowed_domains: the host refuses without calling the service
        let services = fake_services();
        let plugin = host_api_plugin(services.clone(), &wasm, "other.org", None);
        let error = run_order_hook(&plugin).await.unwrap_err().to_string();
        assert!(error.contains("not permitted"), "{}", error);
        assert!(services.requests.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn test_kv_round_trip_and_settings() {
        // kv_set("k", "{\"error\":\"stored\"}") then return kv_get("k")
        let value = r#"{"error":"stored"}"#;
        let data = format!("k{}", value);
        let wasm = host_api_module(
            &data,
            &format!(
                "(drop (call $kv_set (i32.const 256) (i32.const 1) (i32.const 257) (i32.const {})))
                 (call $kv_get (i32.const 256) (i32.const 1))",
                value.len()
            ),
        );
        let services = fake_services();
        let plugin = host_api_plugin(services.clone(), &wasm, "example.com", Some("ns"));
        let error = run_order_hook(&plugin).await.unwrap_err().to_string();
        assert!(error.ends_with("stored"), "{}", error);
        assert_eq!(
            services.kv.lock().unwrap().get("ns:k").map(String::as_str),
            Some(value),
            "keys are namespaced by the host"
        );

        // Without capabilities.storage, kv_get returns -1 (denied): the output is "unchanged"
        let plugin = host_api_plugin(fake_services(), &wasm, "example.com", None);
        assert!(run_order_hook(&plugin).await.is_ok());

        let wasm = host_api_module(
            "greeting",
            "(call $config_get (i32.const 256) (i32.const 8))",
        );
        let plugin = host_api_plugin(fake_services(), &wasm, "example.com", None);
        let error = run_order_hook(&plugin).await.unwrap_err().to_string();
        assert!(error.ends_with("configured"), "{}", error);
    }
}
