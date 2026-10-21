pub mod abi;
pub mod host;
pub mod prelude;
pub mod types;

pub use aegis_plugin_macros::aegis_plugin;
pub use host::{kv, log, net};
pub use types::{BlockNode, Plugin, PluginError, PostContext, Result};

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
                        return $crate::abi::pack_ptr_len(ptr, len);
                    }
                };

                let mut plugin = <$plugin_type as ::std::default::Default>::default();
                match <$plugin_type as $crate::types::Plugin>::on_before_save(&mut plugin, post) {
                    Ok(updated_post) => match ::serde_json::to_vec(&updated_post) {
                        Ok(out_bytes) => $crate::abi::write_bytes_to_host(out_bytes),
                        Err(e) => {
                            $crate::log_error!("Failed to serialize updated PostContext: {}", e);
                            $crate::abi::pack_ptr_len(ptr, len)
                        }
                    },
                    Err(e) => {
                        $crate::log_error!("Plugin on_before_save error: {}", e);
                        $crate::abi::pack_ptr_len(ptr, len)
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
