use aegis_plugin_sdk::prelude::*;

/// Rejects posts without a title (the generated exports are per test binary, hence a separate file)
#[derive(Default)]
pub struct RequireTitlePlugin;

#[aegis_plugin]
impl Plugin for RequireTitlePlugin {
    fn on_before_save(&mut self, post: PostContext) -> Result<PostContext> {
        if post.title.trim().is_empty() {
            return Err(PluginError::Rejected("title is required".to_string()));
        }
        Ok(post)
    }
}

fn call_on_before_save(post: &PostContext) -> serde_json::Value {
    let json_bytes = serde_json::to_vec(post).unwrap();
    let in_len = json_bytes.len() as i32;
    let in_ptr = allocate(in_len);
    unsafe {
        let raw = aegis_plugin_sdk::abi::get_native_ptr(in_ptr);
        std::ptr::copy_nonoverlapping(json_bytes.as_ptr(), raw, json_bytes.len());
    }

    let (out_ptr, out_len) = aegis_plugin_sdk::abi::unpack_ptr_len(on_before_save(in_ptr, in_len));
    assert!(out_len > 0);
    let out_bytes = unsafe { aegis_plugin_sdk::abi::read_bytes_from_host(out_ptr, out_len) };
    serde_json::from_slice(&out_bytes).unwrap()
}

#[test]
fn test_err_is_returned_to_the_host_as_an_error() {
    let rejected = call_on_before_save(&PostContext::new("  ", "untitled"));
    assert_eq!(
        rejected,
        serde_json::json!({ "error": "title is required" })
    );

    let accepted = call_on_before_save(&PostContext::new("Hello", "hello"));
    assert_eq!(accepted["title"], "Hello");
    assert!(accepted.get("error").is_none());
}

#[test]
fn test_unreadable_input_is_an_error_rather_than_a_pass() {
    let garbage = b"not json";
    let in_ptr = allocate(garbage.len() as i32);
    unsafe {
        let raw = aegis_plugin_sdk::abi::get_native_ptr(in_ptr);
        std::ptr::copy_nonoverlapping(garbage.as_ptr(), raw, garbage.len());
    }
    let (out_ptr, out_len) =
        aegis_plugin_sdk::abi::unpack_ptr_len(on_before_save(in_ptr, garbage.len() as i32));
    let out_bytes = unsafe { aegis_plugin_sdk::abi::read_bytes_from_host(out_ptr, out_len) };
    let value: serde_json::Value = serde_json::from_slice(&out_bytes).unwrap();
    assert!(value["error"]
        .as_str()
        .unwrap()
        .starts_with("Invalid PostContext"));
}
