pub mod guard;
pub mod plugin;
pub mod wasm;

pub use guard::CapabilityGuard;
pub use plugin::{AegisPlugin, PluginSupervisor};
pub use wasm::{PluginHostState, WasmEngine, WasmPluginInstance, WasmPluginLoader};

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
    }

    #[test]
    fn test_capability_guard_hooks() {
        let manifest = make_sample_manifest(vec!["on-before-save".to_string()]);
        let guard = CapabilityGuard::new(&manifest);

        assert!(guard.check_hook("on-before-save").is_ok());
        assert!(guard.check_hook("transform-content-ast").is_err());
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
        let wasm_path = std::path::Path::new("../../plugins/template-rust/plugin.wasm");
        let fallback_path = std::path::Path::new("plugins/template-rust/plugin.wasm");
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
}
