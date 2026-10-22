use aegis_plugin_sdk::prelude::*;

/// Maps the `cms-admins` group to the admin role and refuses users outside example.com
#[derive(Default)]
pub struct RoleMappingPlugin;

#[aegis_plugin]
impl Plugin for RoleMappingPlugin {
    fn on_user_authenticated(&mut self, mut ctx: UserAuthContext) -> Result<UserAuthContext> {
        if !ctx.email.ends_with("@example.com") {
            return Err(PluginError::Rejected(
                "only example.com accounts may sign in".to_string(),
            ));
        }
        let groups = ctx.metadata.get("groups").and_then(|g| g.as_array());
        if groups.is_some_and(|g| g.iter().any(|v| v == "cms-admins")) {
            ctx.role = "admin".to_string();
        }
        ctx.metadata
            .insert("department".to_string(), serde_json::json!("docs"));
        Ok(ctx)
    }
}

fn call(ctx: &serde_json::Value) -> serde_json::Value {
    let json_bytes = serde_json::to_vec(ctx).unwrap();
    let in_ptr = allocate(json_bytes.len() as i32);
    unsafe {
        let raw = aegis_plugin_sdk::abi::get_native_ptr(in_ptr);
        std::ptr::copy_nonoverlapping(json_bytes.as_ptr(), raw, json_bytes.len());
    }
    let (out_ptr, out_len) = aegis_plugin_sdk::abi::unpack_ptr_len(on_user_authenticated(
        in_ptr,
        json_bytes.len() as i32,
    ));
    let out_bytes = unsafe { aegis_plugin_sdk::abi::read_bytes_from_host(out_ptr, out_len) };
    serde_json::from_slice(&out_bytes).unwrap()
}

#[test]
fn test_role_mapping_and_rejection() {
    let mapped = call(&serde_json::json!({
        "provider": "oidc",
        "user_id": "alice",
        "email": "alice@example.com",
        "role": "viewer",
        "metadata": { "groups": ["cms-admins"] }
    }));
    assert_eq!(mapped["role"], "admin");
    assert_eq!(mapped["user_id"], "alice");
    assert_eq!(mapped["metadata"]["department"], "docs");

    let rejected = call(&serde_json::json!({
        "provider": "oidc",
        "user_id": "mallory",
        "email": "mallory@evil.test",
        "role": "viewer"
    }));
    assert_eq!(
        rejected,
        serde_json::json!({ "error": "only example.com accounts may sign in" })
    );
}
