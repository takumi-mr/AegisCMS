use proc_macro::TokenStream;
use quote::quote;
use syn::{parse_macro_input, Item};

/// AegisCMS Wasm プラグインのエントリポイント・アロケータ・ABIバインディングを自動生成するマクロ
///
/// プラグインの構造体、または `impl Plugin for ...` ブロックに付与できます。
///
/// # 使用例
/// ```rust,ignore
/// use aegis_plugin_sdk::prelude::*;
///
/// #[derive(Default)]
/// pub struct MyPlugin;
///
/// #[aegis_plugin]
/// impl Plugin for MyPlugin {
///     fn on_before_save(&mut self, mut post: PostContext) -> Result<PostContext> {
///         post.title = format!("[Published] {}", post.title);
///         Ok(post)
///     }
/// }
/// ```
#[proc_macro_attribute]
pub fn aegis_plugin(_attr: TokenStream, item: TokenStream) -> TokenStream {
    let input = parse_macro_input!(item as Item);

    let (ident_type, original_item) = match &input {
        Item::Impl(item_impl) => {
            let self_ty = &item_impl.self_ty;
            (quote!(#self_ty), quote!(#item_impl))
        }
        Item::Struct(item_struct) => {
            let ident = &item_struct.ident;
            (quote!(#ident), quote!(#item_struct))
        }
        _ => {
            return syn::Error::new_spanned(
                &input,
                "#[aegis_plugin] must be applied to a struct or an 'impl Plugin for ...' block",
            )
            .to_compile_error()
            .into();
        }
    };

    let expanded = quote! {
        #original_item

        // --- 自動生成される Wasm ABI エクスポート群 ---

        #[no_mangle]
        pub extern "C" fn allocate(len: i32) -> i32 {
            ::aegis_plugin_sdk::abi::allocate(len as usize) as i32
        }

        #[no_mangle]
        pub extern "C" fn on_before_save(ptr: i32, len: i32) -> i64 {
            unsafe {
                let in_bytes = ::aegis_plugin_sdk::abi::read_bytes_from_host(ptr, len);
                let post: ::aegis_plugin_sdk::types::PostContext = match ::serde_json::from_slice(&in_bytes) {
                    Ok(p) => p,
                    Err(e) => {
                        ::aegis_plugin_sdk::log_error!("Failed to deserialize PostContext: {}", e);
                        return ::aegis_plugin_sdk::abi::write_error_to_host(&::std::format!("Invalid PostContext: {}", e));
                    }
                };

                let mut plugin = <#ident_type as ::std::default::Default>::default();
                match <#ident_type as ::aegis_plugin_sdk::types::Plugin>::on_before_save(&mut plugin, post) {
                    Ok(updated_post) => match ::serde_json::to_vec(&updated_post) {
                        Ok(out_bytes) => ::aegis_plugin_sdk::abi::write_bytes_to_host(out_bytes),
                        Err(e) => {
                            ::aegis_plugin_sdk::log_error!("Failed to serialize updated PostContext: {}", e);
                            ::aegis_plugin_sdk::abi::write_error_to_host(&::std::format!("Failed to serialize PostContext: {}", e))
                        }
                    },
                    Err(e) => {
                        // ホストは保存を中止する
                        ::aegis_plugin_sdk::abi::write_error_to_host(&e.to_string())
                    }
                }
            }
        }

        #[no_mangle]
        pub extern "C" fn on_user_authenticated(ptr: i32, len: i32) -> i64 {
            unsafe {
                let in_bytes = ::aegis_plugin_sdk::abi::read_bytes_from_host(ptr, len);
                let ctx: ::aegis_plugin_sdk::types::UserAuthContext = match ::serde_json::from_slice(&in_bytes) {
                    Ok(p) => p,
                    Err(e) => {
                        ::aegis_plugin_sdk::log_error!("Failed to deserialize UserAuthContext: {}", e);
                        return ::aegis_plugin_sdk::abi::write_error_to_host(&::std::format!("Invalid UserAuthContext: {}", e));
                    }
                };

                let mut plugin = <#ident_type as ::std::default::Default>::default();
                match <#ident_type as ::aegis_plugin_sdk::types::Plugin>::on_user_authenticated(&mut plugin, ctx) {
                    Ok(updated_ctx) => match ::serde_json::to_vec(&updated_ctx) {
                        Ok(out_bytes) => ::aegis_plugin_sdk::abi::write_bytes_to_host(out_bytes),
                        Err(e) => {
                            ::aegis_plugin_sdk::log_error!("Failed to serialize updated UserAuthContext: {}", e);
                            ::aegis_plugin_sdk::abi::write_error_to_host(&::std::format!("Failed to serialize UserAuthContext: {}", e))
                        }
                    },
                    Err(e) => {
                        // ホストは保存を中止する
                        ::aegis_plugin_sdk::abi::write_error_to_host(&e.to_string())
                    }
                }
            }
        }

        #[no_mangle]
        pub extern "C" fn transform_content_ast(ptr: i32, len: i32) -> i64 {
            unsafe {
                let in_bytes = ::aegis_plugin_sdk::abi::read_bytes_from_host(ptr, len);
                let blocks: ::std::vec::Vec<::aegis_plugin_sdk::types::BlockNode> = match ::serde_json::from_slice(&in_bytes) {
                    Ok(b) => b,
                    Err(e) => {
                        ::aegis_plugin_sdk::log_error!("Failed to deserialize BlockNode list: {}", e);
                        return ::aegis_plugin_sdk::abi::pack_ptr_len(ptr, len);
                    }
                };

                let mut plugin = <#ident_type as ::std::default::Default>::default();
                match <#ident_type as ::aegis_plugin_sdk::types::Plugin>::transform_content_ast(&mut plugin, blocks) {
                    Ok(updated_blocks) => match ::serde_json::to_vec(&updated_blocks) {
                        Ok(out_bytes) => ::aegis_plugin_sdk::abi::write_bytes_to_host(out_bytes),
                        Err(e) => {
                            ::aegis_plugin_sdk::log_error!("Failed to serialize updated BlockNode list: {}", e);
                            ::aegis_plugin_sdk::abi::pack_ptr_len(ptr, len)
                        }
                    },
                    Err(e) => {
                        ::aegis_plugin_sdk::log_error!("Plugin transform_content_ast error: {}", e);
                        ::aegis_plugin_sdk::abi::pack_ptr_len(ptr, len)
                    }
                }
            }
        }

        #[no_mangle]
        pub extern "C" fn on_render_page(ptr: i32, len: i32) -> i64 {
            unsafe {
                let in_bytes = ::aegis_plugin_sdk::abi::read_bytes_from_host(ptr, len);
                let ctx: ::aegis_plugin_sdk::types::PageRenderContext = match ::serde_json::from_slice(&in_bytes) {
                    Ok(c) => c,
                    Err(e) => {
                        ::aegis_plugin_sdk::log_error!("Failed to deserialize PageRenderContext: {}", e);
                        return ::aegis_plugin_sdk::abi::pack_ptr_len(ptr, len);
                    }
                };

                let mut plugin = <#ident_type as ::std::default::Default>::default();
                match <#ident_type as ::aegis_plugin_sdk::types::Plugin>::on_render_page(&mut plugin, ctx) {
                    Ok(updated_ctx) => match ::serde_json::to_vec(&updated_ctx) {
                        Ok(out_bytes) => ::aegis_plugin_sdk::abi::write_bytes_to_host(out_bytes),
                        Err(e) => {
                            ::aegis_plugin_sdk::log_error!("Failed to serialize updated PageRenderContext: {}", e);
                            ::aegis_plugin_sdk::abi::pack_ptr_len(ptr, len)
                        }
                    },
                    Err(e) => {
                        ::aegis_plugin_sdk::log_error!("Plugin on_render_page error: {}", e);
                        ::aegis_plugin_sdk::abi::pack_ptr_len(ptr, len)
                    }
                }
            }
        }
    };

    TokenStream::from(expanded)
}
