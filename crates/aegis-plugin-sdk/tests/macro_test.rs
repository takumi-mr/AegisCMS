use aegis_plugin_sdk::prelude::*;

#[derive(Default)]
pub struct SampleMacroPlugin;

#[aegis_plugin]
impl Plugin for SampleMacroPlugin {
    fn on_before_save(&mut self, mut post: PostContext) -> Result<PostContext> {
        post.title = format!("[Verified] {}", post.title);
        Ok(post)
    }

    fn transform_content_ast(&mut self, mut blocks: Vec<BlockNode>) -> Result<Vec<BlockNode>> {
        blocks.push(BlockNode::code("rust", "println!(\"Hello Wasm!\");"));
        Ok(blocks)
    }

    fn check_comment(&mut self, comment: CommentSubmission) -> Result<CommentVerdict> {
        if comment.body.contains("casino") {
            return Ok(CommentVerdict::Spam {
                reason: Some("casino".to_string()),
            });
        }
        if comment.body.is_empty() {
            return Err(PluginError::Rejected("Say something".to_string()));
        }
        Ok(CommentVerdict::Allow)
    }
}

fn write_to_guest_memory(ptr_or_id: i32, data: &[u8]) {
    #[cfg(target_arch = "wasm32")]
    unsafe {
        std::ptr::copy_nonoverlapping(data.as_ptr(), ptr_or_id as *mut u8, data.len());
    }
    #[cfg(not(target_arch = "wasm32"))]
    unsafe {
        let raw = aegis_plugin_sdk::abi::get_native_ptr(ptr_or_id);
        std::ptr::copy_nonoverlapping(data.as_ptr(), raw, data.len());
    }
}

#[test]
fn test_macro_generated_exports() {
    // 1. allocate のテスト
    let ptr = allocate(1024);
    assert!(ptr != 0);
    unsafe {
        aegis_plugin_sdk::abi::deallocate(ptr, 1024);
    }

    // 2. on_before_save のテスト
    let input_post = PostContext::new("My Article", "my-article");
    let json_bytes = serde_json::to_vec(&input_post).unwrap();
    let in_len = json_bytes.len() as i32;

    let in_ptr = allocate(in_len);
    write_to_guest_memory(in_ptr, &json_bytes);

    let packed = on_before_save(in_ptr, in_len);
    let (out_ptr, out_len) = aegis_plugin_sdk::abi::unpack_ptr_len(packed);
    assert!(out_len > 0);

    let out_bytes = unsafe { aegis_plugin_sdk::abi::read_bytes_from_host(out_ptr, out_len) };
    let updated_post: PostContext = serde_json::from_slice(&out_bytes).unwrap();
    assert_eq!(updated_post.title, "[Verified] My Article");

    // 3. transform_content_ast のテスト
    let input_blocks = vec![BlockNode::paragraph("Initial content")];
    let blocks_json = serde_json::to_vec(&input_blocks).unwrap();
    let b_len = blocks_json.len() as i32;
    let b_ptr = allocate(b_len);
    write_to_guest_memory(b_ptr, &blocks_json);

    let packed_blocks = transform_content_ast(b_ptr, b_len);
    let (b_out_ptr, b_out_len) = aegis_plugin_sdk::abi::unpack_ptr_len(packed_blocks);
    assert!(b_out_len > 0);

    let b_out_bytes = unsafe { aegis_plugin_sdk::abi::read_bytes_from_host(b_out_ptr, b_out_len) };
    let updated_blocks: Vec<BlockNode> = serde_json::from_slice(&b_out_bytes).unwrap();
    assert_eq!(updated_blocks.len(), 2);
    assert_eq!(updated_blocks[1].name, "core/code");

    // 4. on_content_published のテスト（既定実装は Ok(()) で 0 を返す）
    let content = PublishedContent::new(
        "id-123",
        "article",
        "Hello",
        "hello",
        "2099-01-01T00:00:00Z",
    );
    let content_json = serde_json::to_vec(&content).unwrap();
    let c_len = content_json.len() as i32;
    let c_ptr = allocate(c_len);
    write_to_guest_memory(c_ptr, &content_json);

    let res = on_content_published(c_ptr, c_len);
    assert_eq!(res, 0);

    // 5. check_comment のテスト（判定をそのまま JSON で返す。断るときは {"error": ...}）
    let call = |body: &str| {
        let comment = CommentSubmission {
            body: body.to_string(),
            status: "approved".to_string(),
            ..Default::default()
        };
        let json = serde_json::to_vec(&comment).unwrap();
        let len = json.len() as i32;
        let ptr = allocate(len);
        write_to_guest_memory(ptr, &json);
        let (out_ptr, out_len) = aegis_plugin_sdk::abi::unpack_ptr_len(check_comment(ptr, len));
        let out = unsafe { aegis_plugin_sdk::abi::read_bytes_from_host(out_ptr, out_len) };
        serde_json::from_slice::<serde_json::Value>(&out).unwrap()
    };
    assert_eq!(
        call("cheap casino"),
        serde_json::json!({ "verdict": "spam", "reason": "casino" })
    );
    assert_eq!(call("Nice post"), serde_json::json!({ "verdict": "allow" }));
    assert_eq!(call(""), serde_json::json!({ "error": "Say something" }));
}
