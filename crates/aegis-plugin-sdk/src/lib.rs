pub mod abi;
pub mod host;
pub mod prelude;
pub mod types;

pub use aegis_plugin_macros::aegis_plugin;
pub use host::{config, http, kv, log, net};
pub use types::{
    BlockNode, Delivery, NotificationMessage, OrderAdjustment, OrderContext, OrderLine,
    OrderStatusChange, Plugin, PluginError, PostContext, Recipient, Result, ShippingAddress,
    UserAuthContext,
};

/// 宣言的マクロによる Wasm プラグイン登録
/// `#[aegis_plugin]` 属性マクロと同等の ABI エクスポートを生成します。
#[macro_export]
macro_rules! register_plugin {
    ($plugin_type:ty) => {
        #[no_mangle]
        pub extern "C" fn allocate(len: i32) -> i32 {
            $crate::abi::allocate(len as usize) as i32
        }

        #[no_mangle]
        pub extern "C" fn on_before_save(ptr: i32, len: i32) -> i64 {
            unsafe {
                let in_bytes = $crate::abi::read_bytes_from_host(ptr, len);
                let post: $crate::types::PostContext = match ::serde_json::from_slice(&in_bytes) {
                    Ok(p) => p,
                    Err(e) => {
                        $crate::log_error!("Failed to deserialize PostContext: {}", e);
                        return $crate::abi::write_error_to_host(&::std::format!(
                            "Invalid PostContext: {}",
                            e
                        ));
                    }
                };

                let mut plugin = <$plugin_type as ::std::default::Default>::default();
                match <$plugin_type as $crate::types::Plugin>::on_before_save(&mut plugin, post) {
                    Ok(updated_post) => match ::serde_json::to_vec(&updated_post) {
                        Ok(out_bytes) => $crate::abi::write_bytes_to_host(out_bytes),
                        Err(e) => {
                            $crate::log_error!("Failed to serialize updated PostContext: {}", e);
                            $crate::abi::write_error_to_host(&::std::format!(
                                "Failed to serialize PostContext: {}",
                                e
                            ))
                        }
                    },
                    Err(e) => {
                        // ホストは保存を中止する
                        $crate::abi::write_error_to_host(&e.to_string())
                    }
                }
            }
        }

        #[no_mangle]
        pub extern "C" fn on_user_authenticated(ptr: i32, len: i32) -> i64 {
            unsafe {
                let in_bytes = $crate::abi::read_bytes_from_host(ptr, len);
                let ctx: $crate::types::UserAuthContext = match ::serde_json::from_slice(&in_bytes)
                {
                    Ok(p) => p,
                    Err(e) => {
                        $crate::log_error!("Failed to deserialize UserAuthContext: {}", e);
                        return $crate::abi::write_error_to_host(&::std::format!(
                            "Invalid UserAuthContext: {}",
                            e
                        ));
                    }
                };

                let mut plugin = <$plugin_type as ::std::default::Default>::default();
                match <$plugin_type as $crate::types::Plugin>::on_user_authenticated(
                    &mut plugin,
                    ctx,
                ) {
                    Ok(updated_ctx) => match ::serde_json::to_vec(&updated_ctx) {
                        Ok(out_bytes) => $crate::abi::write_bytes_to_host(out_bytes),
                        Err(e) => {
                            $crate::log_error!(
                                "Failed to serialize updated UserAuthContext: {}",
                                e
                            );
                            $crate::abi::write_error_to_host(&::std::format!(
                                "Failed to serialize UserAuthContext: {}",
                                e
                            ))
                        }
                    },
                    Err(e) => {
                        // ホストは保存を中止する
                        $crate::abi::write_error_to_host(&e.to_string())
                    }
                }
            }
        }

        #[no_mangle]
        pub extern "C" fn transform_content_ast(ptr: i32, len: i32) -> i64 {
            unsafe {
                let in_bytes = $crate::abi::read_bytes_from_host(ptr, len);
                let blocks: ::std::vec::Vec<$crate::types::BlockNode> =
                    match ::serde_json::from_slice(&in_bytes) {
                        Ok(b) => b,
                        Err(e) => {
                            $crate::log_error!("Failed to deserialize BlockNode list: {}", e);
                            return $crate::abi::pack_ptr_len(ptr, len);
                        }
                    };

                let mut plugin = <$plugin_type as ::std::default::Default>::default();
                match <$plugin_type as $crate::types::Plugin>::transform_content_ast(
                    &mut plugin,
                    blocks,
                ) {
                    Ok(updated_blocks) => match ::serde_json::to_vec(&updated_blocks) {
                        Ok(out_bytes) => $crate::abi::write_bytes_to_host(out_bytes),
                        Err(e) => {
                            $crate::log_error!("Failed to serialize updated BlockNode list: {}", e);
                            $crate::abi::pack_ptr_len(ptr, len)
                        }
                    },
                    Err(e) => {
                        $crate::log_error!("Plugin transform_content_ast error: {}", e);
                        $crate::abi::pack_ptr_len(ptr, len)
                    }
                }
            }
        }

        #[no_mangle]
        pub extern "C" fn calculate_order(ptr: i32, len: i32) -> i64 {
            unsafe {
                $crate::abi::run_object_hook(
                    ptr,
                    len,
                    "OrderContext",
                    |ctx: $crate::types::OrderContext| {
                        let mut plugin = <$plugin_type as ::std::default::Default>::default();
                        <$plugin_type as $crate::types::Plugin>::calculate_order(&mut plugin, ctx)
                    },
                )
            }
        }

        #[no_mangle]
        pub extern "C" fn on_order_status_changed(ptr: i32, len: i32) -> i64 {
            unsafe {
                $crate::abi::run_notification_hook(
                    ptr,
                    len,
                    "OrderStatusChange",
                    |change: $crate::types::OrderStatusChange| {
                        let mut plugin = <$plugin_type as ::std::default::Default>::default();
                        <$plugin_type as $crate::types::Plugin>::on_order_status_changed(
                            &mut plugin,
                            change,
                        )
                    },
                )
            }
        }

        #[no_mangle]
        pub extern "C" fn send_notification(ptr: i32, len: i32) -> i64 {
            unsafe {
                $crate::abi::run_delivery_hook(
                    ptr,
                    len,
                    |message: $crate::types::NotificationMessage| {
                        let mut plugin = <$plugin_type as ::std::default::Default>::default();
                        <$plugin_type as $crate::types::Plugin>::send_notification(
                            &mut plugin,
                            message,
                        )
                    },
                )
            }
        }

        #[no_mangle]
        pub extern "C" fn on_render_page(ptr: i32, len: i32) -> i64 {
            unsafe {
                let in_bytes = $crate::abi::read_bytes_from_host(ptr, len);
                let ctx: $crate::types::PageRenderContext =
                    match ::serde_json::from_slice(&in_bytes) {
                        Ok(c) => c,
                        Err(e) => {
                            $crate::log_error!("Failed to deserialize PageRenderContext: {}", e);
                            return $crate::abi::pack_ptr_len(ptr, len);
                        }
                    };

                let mut plugin = <$plugin_type as ::std::default::Default>::default();
                match <$plugin_type as $crate::types::Plugin>::on_render_page(&mut plugin, ctx) {
                    Ok(updated_ctx) => match ::serde_json::to_vec(&updated_ctx) {
                        Ok(out_bytes) => $crate::abi::write_bytes_to_host(out_bytes),
                        Err(e) => {
                            $crate::log_error!(
                                "Failed to serialize updated PageRenderContext: {}",
                                e
                            );
                            $crate::abi::pack_ptr_len(ptr, len)
                        }
                    },
                    Err(e) => {
                        $crate::log_error!("Plugin on_render_page error: {}", e);
                        $crate::abi::pack_ptr_len(ptr, len)
                    }
                }
            }
        }
    };
}

#[cfg(test)]
mod tests {
    use super::prelude::*;

    #[derive(Default)]
    struct SampleTestPlugin;

    impl Plugin for SampleTestPlugin {
        fn on_before_save(&mut self, mut post: PostContext) -> Result<PostContext> {
            post.title = format!("[Prefix] {}", post.title);
            log_info!("Modified title to: {}", post.title);
            Ok(post)
        }

        fn transform_content_ast(&mut self, mut blocks: Vec<BlockNode>) -> Result<Vec<BlockNode>> {
            blocks.push(BlockNode::paragraph("Appended text"));
            Ok(blocks)
        }

        fn on_render_page(&mut self, mut ctx: PageRenderContext) -> Result<PageRenderContext> {
            ctx.site_title = format!("{} (Themed)", ctx.site_title);
            ctx.extra_css.push_str("body { color: #00ffcc; }");
            Ok(ctx)
        }
    }

    #[test]
    fn test_plugin_lifecycle_methods() {
        let mut plugin = SampleTestPlugin::default();

        let post = PostContext::new("Original Title", "orig-slug");
        let result = plugin.on_before_save(post).unwrap();
        assert_eq!(result.title, "[Prefix] Original Title");

        let blocks = vec![BlockNode::heading(1, "Heading 1")];
        let transformed = plugin.transform_content_ast(blocks).unwrap();
        assert_eq!(transformed.len(), 2);
        assert_eq!(transformed[0].name, "core/heading");
        assert_eq!(transformed[1].name, "core/paragraph");
        assert_eq!(transformed[1].text.as_deref(), Some("Appended text"));

        let ctx = PageRenderContext {
            site_title: "My Site".to_string(),
            ..Default::default()
        };
        let updated_ctx = plugin.on_render_page(ctx).unwrap();
        assert_eq!(updated_ctx.site_title, "My Site (Themed)");
        assert!(updated_ctx.extra_css.contains("#00ffcc"));
    }

    #[test]
    fn test_block_attributes_may_be_null() {
        // What the host sends for a block saved without attributes
        let blocks: Vec<BlockNode> = serde_json::from_str(
            r#"[{"name":"core/paragraph","attributes":null,"children":[],"text":"Body"}]"#,
        )
        .unwrap();
        assert!(blocks[0].attributes.is_empty());
        assert_eq!(blocks[0].text.as_deref(), Some("Body"));
    }

    #[test]
    fn test_abi_packing() {
        let ptr = 0x1234;
        let len = 0x5678;
        let packed = crate::abi::pack_ptr_len(ptr, len);
        let (out_ptr, out_len) = crate::abi::unpack_ptr_len(packed);
        assert_eq!(out_ptr, ptr);
        assert_eq!(out_len, len);
    }

    #[test]
    fn test_host_logging_and_kv_mock() {
        log_debug!("Debug message");
        log_info!("Info message");
        log_warn!("Warn message");
        log_error!("Error message");

        assert!(net::is_allowed("https://example.com"));
        assert!(kv::set("test_key", "test_val").is_ok());
    }
}
