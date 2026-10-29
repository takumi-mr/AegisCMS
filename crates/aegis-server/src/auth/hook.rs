//! The `on-user-authenticated` plugin hook, run after any provider authenticated a user and
//! before the session is issued

use aegis_core::{AegisError, AuthIdentity, Result, UserAuthContext};

use crate::state::AppState;
use aegis_auth::ROLES;

/// Session metadata keys the server sets itself; plugin metadata cannot override them
pub const RESERVED_METADATA_KEYS: [&str; 3] = ["provider", "display_name", "email"];

/// Passes the identity through every `on-user-authenticated` plugin and returns it with the
/// role, email, display name and metadata they returned. `provider` and `user_id` cannot be
/// changed (changes are ignored). Errors refuse the login: `AegisError::PluginRejected` when a
/// plugin turned the user away, any other error when a plugin failed or returned an unknown role.
pub async fn run_auth_hook(
    state: &AppState,
    provider: &str,
    mut identity: AuthIdentity,
) -> Result<AuthIdentity> {
    let ctx = UserAuthContext {
        provider: provider.to_string(),
        user_id: identity.external_id.clone(),
        email: identity.email.clone(),
        display_name: identity.display_name.clone(),
        role: identity.role.clone(),
        metadata: identity.metadata.clone(),
    };
    let returned = {
        let supervisor = state.supervisor.read().await;
        supervisor.apply_user_authenticated(ctx.clone()).await?
    };
    if returned == ctx {
        return Ok(identity);
    }

    if returned.provider != ctx.provider || returned.user_id != ctx.user_id {
        tracing::warn!(
            "Plugin hook 'on-user-authenticated' changed provider or user_id of '{}'; the change is ignored",
            ctx.user_id
        );
    }
    if !ROLES.contains(&returned.role.as_str()) {
        return Err(AegisError::PluginRuntime(format!(
            "on-user-authenticated returned an unknown role '{}'",
            returned.role
        )));
    }
    identity.role = returned.role;
    identity.email = returned.email;
    identity.display_name = returned.display_name;
    identity.metadata = returned.metadata;
    Ok(identity)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::test_support::test_state;
    use aegis_core::{PluginCapabilities, PluginManifest};
    use aegis_runtime::{AegisPlugin, PluginSupervisor};
    use serde_json::json;
    use std::sync::Arc;

    /// What the test plugin does with the context
    #[derive(Clone, Copy)]
    pub(crate) enum Mode {
        /// Maps the `cms-admins` group to admin and everyone else to editor, fills the display name,
        /// tries to rename the user
        Map,
        Reject,
        Fail,
        UnknownRole,
    }

    struct AuthPlugin {
        manifest: PluginManifest,
        mode: Mode,
    }

    #[async_trait::async_trait]
    impl AegisPlugin for AuthPlugin {
        fn manifest(&self) -> &PluginManifest {
            &self.manifest
        }

        async fn on_user_authenticated(&self, mut ctx: UserAuthContext) -> Result<UserAuthContext> {
            match self.mode {
                Mode::Map => {
                    let groups = ctx.metadata.get("groups").and_then(|g| g.as_array());
                    ctx.role = if groups.is_some_and(|g| g.iter().any(|v| v == "cms-admins")) {
                        "admin".to_string()
                    } else {
                        "editor".to_string()
                    };
                    ctx.display_name = Some("Alice A.".to_string());
                    ctx.metadata.insert("department".to_string(), json!("docs"));
                    ctx.user_id = "root".to_string();
                    Ok(ctx)
                }
                Mode::Reject => Err(AegisError::PluginRejected(
                    "guard: account is suspended".to_string(),
                )),
                Mode::Fail => Err(AegisError::PluginRuntime("trap".to_string())),
                Mode::UnknownRole => {
                    ctx.role = "superuser".to_string();
                    Ok(ctx)
                }
            }
        }
    }

    pub(crate) fn auth_plugin(mode: Mode) -> Arc<dyn AegisPlugin> {
        Arc::new(AuthPlugin {
            manifest: PluginManifest {
                name: "guard".to_string(),
                version: "1.0.0".to_string(),
                entrypoint: "plugin.wasm".to_string(),
                capabilities: PluginCapabilities {
                    embed: None,
                    network: None,
                    storage: None,
                    hooks: vec!["on-user-authenticated".to_string()],
                },
                resource_limits: Default::default(),
                ..Default::default()
            },
            mode,
        })
    }

    fn identity() -> AuthIdentity {
        let mut identity = AuthIdentity::new("alice", "alice@example.com");
        identity.role = "viewer".to_string();
        identity
            .metadata
            .insert("groups".to_string(), json!(["cms-admins"]));
        identity
    }

    async fn run(mode: Option<Mode>) -> Result<AuthIdentity> {
        let mut supervisor = PluginSupervisor::new();
        if let Some(mode) = mode {
            supervisor.register(auth_plugin(mode));
        }
        let state = test_state(supervisor).await;
        run_auth_hook(&state, "local", identity()).await
    }

    #[tokio::test]
    async fn test_without_plugins_the_identity_is_unchanged() {
        assert_eq!(run(None).await.unwrap(), identity());
    }

    #[tokio::test]
    async fn test_plugin_maps_role_and_attributes() {
        let mapped = run(Some(Mode::Map)).await.unwrap();
        assert_eq!(mapped.role, "admin");
        assert_eq!(mapped.display_name.as_deref(), Some("Alice A."));
        assert_eq!(mapped.metadata["department"], json!("docs"));
        // The user id is not the plugin's to change
        assert_eq!(mapped.external_id, "alice");
    }

    #[tokio::test]
    async fn test_rejection_failure_and_unknown_role_refuse_the_login() {
        assert!(matches!(
            run(Some(Mode::Reject)).await,
            Err(AegisError::PluginRejected(_))
        ));
        assert!(matches!(
            run(Some(Mode::Fail)).await,
            Err(AegisError::PluginRuntime(_))
        ));
        assert!(matches!(
            run(Some(Mode::UnknownRole)).await,
            Err(AegisError::PluginRuntime(_))
        ));
    }
}
