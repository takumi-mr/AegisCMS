//! Signing in a user that an external IdP (OpenID Connect / SAML) has verified: finds the account
//! linked to the IdP's subject, or creates and links one when `auto_create_users` is set, and keeps
//! the account's role and profile in sync with what the IdP sends.

use aegis_core::{AegisError, Result};
use aegis_db::{DatabaseAdapter, UserRecord};
use std::collections::HashMap;

use super::users::{create_user, NewUser};
use super::{role_level, ROLES};

/// `AegisError::Authentication` reasons (audited, and shown on the login screen)
pub const REASON_IDP_ERROR: &str = "idp_error";
pub const REASON_INVALID_STATE: &str = "invalid_state";
pub const REASON_INVALID_TOKEN: &str = "invalid_token";
pub const REASON_MISSING_USERNAME: &str = "missing_username";
pub const REASON_USERNAME_TAKEN: &str = "username_taken";
pub const REASON_NOT_PROVISIONED: &str = "not_provisioned";
pub const REASON_ACCOUNT_DISABLED: &str = super::local::REASON_ACCOUNT_DISABLED;

pub(crate) fn auth_error(reason: &str) -> AegisError {
    AegisError::Authentication(reason.to_string())
}

/// How users of an IdP become AegisCMS accounts (`[auth.oidc]` / `[auth.saml]`)
pub struct AccountPolicy<'a> {
    /// Provider name of the identity links (`oidc` / `saml`)
    pub provider: &'a str,
    pub auto_create_users: bool,
    pub default_role: &'a str,
    /// `Some` when the IdP decides the role (`role_claim` / `role_attribute` is set)
    pub role_mapping: Option<&'a HashMap<String, String>>,
}

/// What the IdP says about the user
#[derive(Debug, Default)]
pub struct IdpProfile {
    /// Key of the identity link (unique per IdP, e.g. `{issuer}#{sub}`)
    pub subject: String,
    /// User name for a new account
    pub username: Option<String>,
    pub email: Option<String>,
    pub display_name: Option<String>,
    /// Values of the role claim / attribute, mapped with `role_mapping`
    pub role_values: Vec<String>,
}

/// Checks `default_role` and the roles of `role_mapping`
pub fn validate_roles(
    section: &str,
    default_role: &str,
    role_mapping: &HashMap<String, String>,
) -> Result<()> {
    for role in std::iter::once(default_role).chain(role_mapping.values().map(String::as_str)) {
        if !ROLES.contains(&role) {
            return Err(AegisError::Config(format!(
                "[{}] has an unknown role '{}' (expected one of: {})",
                section,
                role,
                ROLES.join(", ")
            )));
        }
    }
    Ok(())
}

/// 256 bits from the OS RNG, base64url encoded
pub(crate) fn random_token() -> String {
    use std::fmt::Write;
    let mut token = String::with_capacity(64);
    for _ in 0..2 {
        for byte in uuid::Uuid::new_v4().as_bytes() {
            let _ = write!(token, "{:02x}", byte);
        }
    }
    token
}

impl AccountPolicy<'_> {
    /// The strongest mapped role, the default role when nothing matches, or `None` when the IdP
    /// does not decide the role
    fn claimed_role(&self, values: &[String]) -> Option<String> {
        let mapping = self.role_mapping?;
        Some(
            values
                .iter()
                .filter_map(|value| mapping.get(value))
                .max_by_key(|role| role_level(role))
                .cloned()
                .unwrap_or_else(|| self.default_role.to_string()),
        )
    }

    /// Finds the account linked to this IdP user, or creates and links one
    async fn resolve_user(
        &self,
        db: &dyn DatabaseAdapter,
        profile: &IdpProfile,
        claimed_role: Option<&str>,
    ) -> Result<UserRecord> {
        if let Some(username) = db
            .get_identity_link(self.provider, &profile.subject)
            .await?
        {
            return db.get_user_by_username(&username).await?.ok_or_else(|| {
                AegisError::Internal(format!(
                    "{} identity is linked to user '{}', which no longer exists",
                    self.provider, username
                ))
            });
        }
        if !self.auto_create_users {
            return Err(auth_error(REASON_NOT_PROVISIONED));
        }
        let username = profile
            .username
            .clone()
            .ok_or_else(|| auth_error(REASON_MISSING_USERNAME))?;
        // Never sign an IdP user into an existing (e.g. local) account that merely shares the name
        if db.get_user_by_username(&username).await?.is_some() {
            return Err(auth_error(REASON_USERNAME_TAKEN));
        }
        let user = create_user(
            db,
            NewUser {
                username,
                // Unusable for password login: nobody knows it
                password: random_token(),
                role: claimed_role.unwrap_or(self.default_role).to_string(),
                email: profile.email.clone(),
                display_name: profile.display_name.clone(),
            },
        )
        .await
        .map_err(|e| match e {
            AegisError::Validation(_) => auth_error(REASON_MISSING_USERNAME),
            other => other,
        })?;
        db.insert_identity_link(self.provider, &profile.subject, &user.username)
            .await?;
        tracing::info!(
            "Created user '{}' from a {} login",
            user.username,
            self.provider
        );
        Ok(user)
    }

    /// The account the IdP user signs in to, with the IdP's role (when it decides the role),
    /// email address and display name copied to it
    pub async fn sign_in(
        &self,
        db: &dyn DatabaseAdapter,
        profile: IdpProfile,
    ) -> Result<UserRecord> {
        let claimed_role = self.claimed_role(&profile.role_values);
        let mut user = self
            .resolve_user(db, &profile, claimed_role.as_deref())
            .await?;
        if user.disabled {
            return Err(auth_error(REASON_ACCOUNT_DISABLED));
        }

        // With a role claim the IdP decides the role; keep the account (and the user list) in sync
        let role = claimed_role.unwrap_or_else(|| user.role.clone());
        let changed = user.role != role
            || (profile.email.is_some() && user.email != profile.email)
            || (profile.display_name.is_some() && user.display_name != profile.display_name);
        if changed {
            user.role = role;
            user.email = profile.email.or(user.email);
            user.display_name = profile.display_name.or(user.display_name);
            user.updated_at = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
            db.update_user(&user).await?;
        }
        Ok(user)
    }
}
