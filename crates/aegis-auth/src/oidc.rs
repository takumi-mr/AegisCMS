//! OpenID Connect login (authorization code flow with PKCE) as an `AuthProvider`.
//!
//! `initiate_login` stores a one-time state (nonce + PKCE verifier) in the cache and returns the
//! IdP's authorization URL; `authenticate` takes the callback's `code` / `state`, exchanges the
//! code, verifies the ID token (JWKS signature, issuer, audience, expiry, nonce) and maps the
//! claims to an AegisCMS account, creating it on first login when `auto_create_users` is set.

use aegis_core::{
    AegisError, AuthCredentials, AuthIdentity, AuthProvider, AuthRedirect, CacheBackend, Result,
};

use super::config::OidcConfig;
use async_trait::async_trait;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use jsonwebtoken::jwk::JwkSet;
use jsonwebtoken::{Algorithm, DecodingKey, Validation};
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;
use uuid::Uuid;

use super::users::{create_user, NewUser};
use super::ROLES;
use crate::role_level;
use aegis_db::DatabaseAdapter;

use super::{oidc_subject, OIDC_CALLBACK_CREDENTIALS, OIDC_PROVIDER};

/// `AegisError::Authentication` reasons (audited, and shown on the login screen)
pub const REASON_IDP_ERROR: &str = "idp_error";
pub const REASON_INVALID_STATE: &str = "invalid_state";
pub const REASON_INVALID_TOKEN: &str = "invalid_token";
pub const REASON_MISSING_USERNAME: &str = "missing_username";
pub const REASON_USERNAME_TAKEN: &str = "username_taken";
pub const REASON_NOT_PROVISIONED: &str = "not_provisioned";
pub const REASON_ACCOUNT_DISABLED: &str = super::local::REASON_ACCOUNT_DISABLED;

const STATE_KEY_PREFIX: &str = "oidc:state:";
/// How long the user has to finish signing in at the IdP
const STATE_TTL_SECS: u64 = 600;
/// An unknown `kid` refetches the JWKS (key rotation), at most this often
const JWKS_MIN_REFRESH: Duration = Duration::from_secs(60);
/// Clock skew tolerated for `exp` / `iat`
const LEEWAY_SECS: u64 = 60;
/// Asymmetric algorithms only: HS* would let anyone who knows the client secret mint tokens
const ALLOWED_ALGORITHMS: [Algorithm; 8] = [
    Algorithm::RS256,
    Algorithm::RS384,
    Algorithm::RS512,
    Algorithm::PS256,
    Algorithm::PS384,
    Algorithm::PS512,
    Algorithm::ES256,
    Algorithm::ES384,
];

/// The parts of `/.well-known/openid-configuration` the flow needs
#[derive(Debug, Clone, Deserialize)]
struct ProviderMetadata {
    issuer: String,
    authorization_endpoint: String,
    token_endpoint: String,
    jwks_uri: String,
    #[serde(default)]
    token_endpoint_auth_methods_supported: Vec<String>,
}

/// Stored under `oidc:state:{state}` between the redirect to the IdP and the callback
#[derive(Debug, Serialize, Deserialize)]
struct PendingLogin {
    nonce: String,
    code_verifier: String,
}

#[derive(Debug, Deserialize)]
struct TokenResponse {
    id_token: Option<String>,
}

pub struct OidcAuthProvider {
    config: OidcConfig,
    http: reqwest::Client,
    cache: Arc<dyn CacheBackend>,
    db: Arc<dyn DatabaseAdapter>,
    metadata: RwLock<Option<ProviderMetadata>>,
    jwks: RwLock<Option<(JwkSet, Instant)>>,
}

fn auth_error(reason: &str) -> AegisError {
    AegisError::Authentication(reason.to_string())
}

fn invalid_token(detail: impl std::fmt::Display) -> AegisError {
    tracing::warn!("Rejected OIDC ID token: {}", detail);
    auth_error(REASON_INVALID_TOKEN)
}

/// 256 bits from the OS RNG, base64url encoded (state, nonce, PKCE verifier)
fn random_token() -> String {
    let mut bytes = Vec::with_capacity(32);
    bytes.extend_from_slice(Uuid::new_v4().as_bytes());
    bytes.extend_from_slice(Uuid::new_v4().as_bytes());
    URL_SAFE_NO_PAD.encode(bytes)
}

fn pkce_challenge(verifier: &str) -> String {
    URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()))
}

/// String values of a claim that may be a string or an array of strings (`groups`, `roles`)
fn claim_values(claims: &Map<String, Value>, name: &str) -> Vec<String> {
    match claims.get(name) {
        Some(Value::String(s)) => vec![s.clone()],
        Some(Value::Array(items)) => items
            .iter()
            .filter_map(|v| v.as_str().map(str::to_string))
            .collect(),
        _ => Vec::new(),
    }
}

fn claim_str<'a>(claims: &'a Map<String, Value>, name: &str) -> Option<&'a str> {
    claims
        .get(name)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
}

impl OidcAuthProvider {
    /// Checks the settings; the IdP itself is contacted on the first login
    pub fn new(
        config: OidcConfig,
        cache: Arc<dyn CacheBackend>,
        db: Arc<dyn DatabaseAdapter>,
    ) -> Result<Self> {
        for (key, value) in [
            ("issuer_url", &config.issuer_url),
            ("client_id", &config.client_id),
            ("redirect_url", &config.redirect_url),
        ] {
            if value.trim().is_empty() {
                return Err(AegisError::Config(format!(
                    "[auth.oidc].{} is required when OIDC is enabled",
                    key
                )));
            }
        }
        let roles = std::iter::once(&config.default_role).chain(config.role_mapping.values());
        for role in roles {
            if !ROLES.contains(&role.as_str()) {
                return Err(AegisError::Config(format!(
                    "[auth.oidc] has an unknown role '{}' (expected one of: {})",
                    role,
                    ROLES.join(", ")
                )));
            }
        }
        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(10))
            .build()
            .map_err(|e| AegisError::Config(format!("Failed to build HTTP client: {}", e)))?;
        Ok(Self {
            config,
            http,
            cache,
            db,
            metadata: RwLock::new(None),
            jwks: RwLock::new(None),
        })
    }

    async fn get_json<T: serde::de::DeserializeOwned>(&self, url: &str) -> Result<T> {
        let res =
            self.http.get(url).send().await.map_err(|e| {
                AegisError::Internal(format!("OIDC request to {} failed: {}", url, e))
            })?;
        if !res.status().is_success() {
            return Err(AegisError::Internal(format!(
                "OIDC request to {} returned {}",
                url,
                res.status()
            )));
        }
        let bytes = res
            .bytes()
            .await
            .map_err(|e| AegisError::Internal(format!("OIDC response from {}: {}", url, e)))?;
        serde_json::from_slice(&bytes)
            .map_err(|e| AegisError::Internal(format!("Invalid OIDC response from {}: {}", url, e)))
    }

    /// Discovery document, fetched once and cached for the process lifetime
    async fn metadata(&self) -> Result<ProviderMetadata> {
        if let Some(metadata) = self.metadata.read().await.as_ref() {
            return Ok(metadata.clone());
        }
        let url = format!(
            "{}/.well-known/openid-configuration",
            self.config.issuer_url.trim_end_matches('/')
        );
        let metadata: ProviderMetadata = self.get_json(&url).await?;
        // The discovery document must describe the configured issuer (OIDC Discovery 4.3)
        if metadata.issuer.trim_end_matches('/') != self.config.issuer_url.trim_end_matches('/') {
            return Err(AegisError::Config(format!(
                "OIDC discovery issuer '{}' does not match [auth.oidc].issuer_url '{}'",
                metadata.issuer, self.config.issuer_url
            )));
        }
        *self.metadata.write().await = Some(metadata.clone());
        Ok(metadata)
    }

    /// The JWKS, refetched when `kid` is not in the cached set (and the last fetch is old enough)
    async fn decoding_key(
        &self,
        metadata: &ProviderMetadata,
        kid: Option<&str>,
        alg: Algorithm,
    ) -> Result<DecodingKey> {
        let pick = |set: &JwkSet| -> Option<jsonwebtoken::jwk::Jwk> {
            match kid {
                Some(kid) => set.find(kid).cloned(),
                // Without a kid the set must hold a single key
                None if set.keys.len() == 1 => set.keys.first().cloned(),
                None => None,
            }
        };
        let cached = self.jwks.read().await.clone();
        let mut jwk = cached.as_ref().and_then(|(set, _)| pick(set));
        let stale = cached
            .as_ref()
            .is_none_or(|(_, fetched)| fetched.elapsed() >= JWKS_MIN_REFRESH);
        if jwk.is_none() && stale {
            let set: JwkSet = self.get_json(&metadata.jwks_uri).await?;
            jwk = pick(&set);
            *self.jwks.write().await = Some((set, Instant::now()));
        }
        let jwk = jwk.ok_or_else(|| invalid_token(format!("no JWKS key for kid {:?}", kid)))?;
        if let Some(key_alg) = jwk.common.key_algorithm {
            if format!("{:?}", key_alg) != format!("{:?}", alg) {
                return Err(invalid_token(format!(
                    "key algorithm {:?} does not match token algorithm {:?}",
                    key_alg, alg
                )));
            }
        }
        DecodingKey::from_jwk(&jwk).map_err(invalid_token)
    }

    /// Verifies the ID token and returns its claims
    async fn verify_id_token(
        &self,
        metadata: &ProviderMetadata,
        id_token: &str,
        nonce: &str,
    ) -> Result<Map<String, Value>> {
        let header = jsonwebtoken::decode_header(id_token).map_err(invalid_token)?;
        if !ALLOWED_ALGORITHMS.contains(&header.alg) {
            return Err(invalid_token(format!(
                "algorithm {:?} is not allowed",
                header.alg
            )));
        }
        let key = self
            .decoding_key(metadata, header.kid.as_deref(), header.alg)
            .await?;

        let mut validation = Validation::new(header.alg);
        validation.set_issuer(&[&metadata.issuer]);
        validation.set_audience(&[&self.config.client_id]);
        validation.set_required_spec_claims(&["exp", "iss", "aud", "sub"]);
        validation.leeway = LEEWAY_SECS;
        let claims = jsonwebtoken::decode::<Map<String, Value>>(id_token, &key, &validation)
            .map_err(invalid_token)?
            .claims;

        if claim_str(&claims, "nonce") != Some(nonce) {
            return Err(invalid_token("nonce mismatch"));
        }
        // With several audiences the token must have been issued to this client (OIDC Core 3.1.3.7)
        if let Some(azp) = claim_str(&claims, "azp") {
            if azp != self.config.client_id {
                return Err(invalid_token(format!("azp '{}' is not this client", azp)));
            }
        }
        Ok(claims)
    }

    async fn exchange_code(
        &self,
        metadata: &ProviderMetadata,
        code: &str,
        code_verifier: &str,
    ) -> Result<String> {
        let mut form = vec![
            ("grant_type", "authorization_code"),
            ("code", code),
            ("redirect_uri", self.config.redirect_url.as_str()),
            ("code_verifier", code_verifier),
        ];
        let mut request = self.http.post(&metadata.token_endpoint);
        // client_secret_basic is the default when the IdP does not list its methods
        let methods = &metadata.token_endpoint_auth_methods_supported;
        if methods.is_empty() || methods.iter().any(|m| m == "client_secret_basic") {
            request = request.basic_auth(&self.config.client_id, Some(&self.config.client_secret));
        } else {
            form.push(("client_id", self.config.client_id.as_str()));
            form.push(("client_secret", self.config.client_secret.as_str()));
        }
        let res = request
            .form(&form)
            .send()
            .await
            .map_err(|e| AegisError::Internal(format!("OIDC token request failed: {}", e)))?;
        let status = res.status();
        let bytes = res
            .bytes()
            .await
            .map_err(|e| AegisError::Internal(format!("OIDC token response: {}", e)))?;
        if !status.is_success() {
            tracing::warn!(
                "OIDC token endpoint returned {}: {}",
                status,
                String::from_utf8_lossy(&bytes)
            );
            return Err(auth_error(REASON_INVALID_TOKEN));
        }
        let token: TokenResponse = serde_json::from_slice(&bytes)
            .map_err(|e| AegisError::Internal(format!("Invalid OIDC token response: {}", e)))?;
        token
            .id_token
            .ok_or_else(|| invalid_token("token response has no id_token"))
    }

    /// The strongest role mapped from `role_claim`, or `None` when nothing matches
    fn mapped_role(&self, claims: &Map<String, Value>) -> Option<String> {
        claim_values(claims, &self.config.role_claim)
            .iter()
            .filter_map(|value| self.config.role_mapping.get(value))
            .max_by_key(|role| role_level(role))
            .cloned()
    }

    /// Finds the account linked to this IdP user, or creates and links one
    async fn resolve_user(
        &self,
        subject: &str,
        claims: &Map<String, Value>,
        claimed_role: Option<&str>,
    ) -> Result<aegis_db::UserRecord> {
        if let Some(username) = self.db.get_identity_link(OIDC_PROVIDER, subject).await? {
            return self
                .db
                .get_user_by_username(&username)
                .await?
                .ok_or_else(|| {
                    AegisError::Internal(format!(
                        "OIDC identity is linked to user '{}', which no longer exists",
                        username
                    ))
                });
        }
        if !self.config.auto_create_users {
            return Err(auth_error(REASON_NOT_PROVISIONED));
        }
        let username = claim_str(claims, &self.config.username_claim)
            .or_else(|| claim_str(claims, "email"))
            .ok_or_else(|| auth_error(REASON_MISSING_USERNAME))?
            .to_string();
        // Never sign an IdP user into an existing (e.g. local) account that merely shares the name
        if self.db.get_user_by_username(&username).await?.is_some() {
            return Err(auth_error(REASON_USERNAME_TAKEN));
        }
        let user = create_user(
            self.db.as_ref(),
            NewUser {
                username: username.clone(),
                // Unusable for password login: nobody knows it
                password: format!("{}{}", random_token(), random_token()),
                role: claimed_role
                    .unwrap_or(&self.config.default_role)
                    .to_string(),
                email: claim_str(claims, "email").map(str::to_string),
                display_name: claim_str(claims, "name").map(str::to_string),
            },
        )
        .await
        .map_err(|e| match e {
            AegisError::Validation(_) => auth_error(REASON_MISSING_USERNAME),
            other => other,
        })?;
        self.db
            .insert_identity_link(OIDC_PROVIDER, subject, &user.username)
            .await?;
        tracing::info!("Created user '{}' from an OIDC login", user.username);
        Ok(user)
    }

    async fn complete_callback(
        &self,
        parameters: &HashMap<String, String>,
    ) -> Result<AuthIdentity> {
        if let Some(error) = parameters.get("error") {
            tracing::warn!(
                "OIDC provider returned an error: {} ({})",
                error,
                parameters
                    .get("error_description")
                    .map(String::as_str)
                    .unwrap_or("")
            );
            return Err(auth_error(REASON_IDP_ERROR));
        }
        let state = parameters
            .get("state")
            .filter(|s| !s.is_empty())
            .ok_or_else(|| auth_error(REASON_INVALID_STATE))?;
        let key = format!("{}{}", STATE_KEY_PREFIX, state);
        let pending = self
            .cache
            .get(&key)
            .await?
            .ok_or_else(|| auth_error(REASON_INVALID_STATE))?;
        // One-time: a replayed callback finds no state
        self.cache.delete(&key).await?;
        let pending: PendingLogin =
            serde_json::from_str(&pending).map_err(|_| auth_error(REASON_INVALID_STATE))?;
        let code = parameters
            .get("code")
            .filter(|c| !c.is_empty())
            .ok_or_else(|| auth_error(REASON_IDP_ERROR))?;

        let metadata = self.metadata().await?;
        let id_token = self
            .exchange_code(&metadata, code, &pending.code_verifier)
            .await?;
        let claims = self
            .verify_id_token(&metadata, &id_token, &pending.nonce)
            .await?;
        let sub = claim_str(&claims, "sub").ok_or_else(|| invalid_token("no sub"))?;
        let subject = oidc_subject(&metadata.issuer, sub);

        let claimed_role = if self.config.role_claim.is_empty() {
            None
        } else {
            Some(
                self.mapped_role(&claims)
                    .unwrap_or_else(|| self.config.default_role.clone()),
            )
        };
        let mut user = self
            .resolve_user(&subject, &claims, claimed_role.as_deref())
            .await?;
        if user.disabled {
            return Err(auth_error(REASON_ACCOUNT_DISABLED));
        }

        // With role_claim the IdP decides the role; keep the account (and the user list) in sync
        let email = claim_str(&claims, "email").map(str::to_string);
        let display_name = claim_str(&claims, "name").map(str::to_string);
        let role = claimed_role.unwrap_or_else(|| user.role.clone());
        let changed = user.role != role
            || (email.is_some() && user.email != email)
            || (display_name.is_some() && user.display_name != display_name);
        if changed {
            user.role = role;
            user.email = email.or(user.email);
            user.display_name = display_name.or(user.display_name);
            user.updated_at = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
            self.db.update_user(&user).await?;
        }

        let mut identity = AuthIdentity::new(user.username, user.email.unwrap_or_default());
        identity.display_name = user.display_name;
        identity.role = user.role;
        identity.metadata = HashMap::from([
            ("issuer".to_string(), json!(metadata.issuer)),
            ("sub".to_string(), json!(sub)),
            ("claims".to_string(), Value::Object(claims)),
        ]);
        Ok(identity)
    }
}

#[async_trait]
impl AuthProvider for OidcAuthProvider {
    fn provider_id(&self) -> &str {
        OIDC_PROVIDER
    }

    async fn initiate_login(&self, _callback_url: &str) -> Result<Option<AuthRedirect>> {
        let metadata = self.metadata().await?;
        let state = random_token();
        let pending = PendingLogin {
            nonce: random_token(),
            code_verifier: random_token(),
        };
        let pending_json = serde_json::to_string(&pending)
            .map_err(|e| AegisError::Serialization(e.to_string()))?;
        self.cache
            .set(
                &format!("{}{}", STATE_KEY_PREFIX, state),
                &pending_json,
                Some(STATE_TTL_SECS),
            )
            .await?;

        let scopes = if self.config.scopes.iter().any(|s| s == "openid") {
            self.config.scopes.join(" ")
        } else {
            std::iter::once("openid".to_string())
                .chain(self.config.scopes.iter().cloned())
                .collect::<Vec<_>>()
                .join(" ")
        };
        let url = reqwest::Url::parse_with_params(
            &metadata.authorization_endpoint,
            [
                ("response_type", "code"),
                ("client_id", self.config.client_id.as_str()),
                ("redirect_uri", self.config.redirect_url.as_str()),
                ("scope", scopes.as_str()),
                ("state", state.as_str()),
                ("nonce", pending.nonce.as_str()),
                (
                    "code_challenge",
                    pkce_challenge(&pending.code_verifier).as_str(),
                ),
                ("code_challenge_method", "S256"),
            ],
        )
        .map_err(|e| AegisError::Config(format!("Invalid OIDC authorization endpoint: {}", e)))?;
        Ok(Some(AuthRedirect {
            url: url.to_string(),
            method: "GET".to_string(),
            form_fields: HashMap::new(),
        }))
    }

    async fn authenticate(&self, credentials: &AuthCredentials) -> Result<AuthIdentity> {
        if credentials.kind != OIDC_CALLBACK_CREDENTIALS {
            return Err(auth_error(super::local::REASON_UNSUPPORTED_CREDENTIALS));
        }
        self.complete_callback(&credentials.parameters).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::oidc_mock::{MockIdp, CLIENT_ID};
    use aegis_cache::MemoryCacheBackend;
    use aegis_db::SqliteAdapter;

    struct Setup {
        idp: MockIdp,
        db: Arc<dyn DatabaseAdapter>,
        provider: OidcAuthProvider,
    }

    async fn setup(configure: impl FnOnce(&mut OidcConfig)) -> Setup {
        let idp = MockIdp::start().await;
        let adapter = SqliteAdapter::connect("sqlite::memory:", 1).await.unwrap();
        adapter.init().await.unwrap();
        let db: Arc<dyn DatabaseAdapter> = Arc::new(adapter);
        let mut config = idp.config();
        config.role_claim = "groups".to_string();
        config.role_mapping = HashMap::from([
            ("cms-editors".to_string(), "editor".to_string()),
            ("cms-admins".to_string(), "admin".to_string()),
        ]);
        configure(&mut config);
        let provider =
            OidcAuthProvider::new(config, Arc::new(MemoryCacheBackend::new()), db.clone()).unwrap();
        Setup { idp, db, provider }
    }

    impl Setup {
        async fn authorization_url(&self) -> String {
            self.provider.initiate_login("").await.unwrap().unwrap().url
        }

        async fn callback(&self, parameters: HashMap<String, String>) -> Result<AuthIdentity> {
            self.provider
                .authenticate(&AuthCredentials {
                    kind: OIDC_CALLBACK_CREDENTIALS.to_string(),
                    parameters,
                    ..Default::default()
                })
                .await
        }

        /// A full login: redirect, sign-in at the IdP with `claims`, callback
        async fn login(&self, claims: Value) -> Result<AuthIdentity> {
            let url = self.authorization_url().await;
            let parameters = self.idp.sign_in(&url, claims);
            self.callback(parameters).await
        }
    }

    fn reason(result: Result<AuthIdentity>) -> String {
        match result {
            Err(AegisError::Authentication(reason)) => reason,
            other => panic!("expected an authentication error, got {:?}", other),
        }
    }

    fn alice(groups: Value) -> Value {
        json!({
            "sub": "u-123",
            "preferred_username": "alice",
            "email": "alice@example.com",
            "name": "Alice Liddell",
            "groups": groups,
        })
    }

    #[tokio::test]
    async fn test_authorization_url_uses_pkce_and_one_time_state() {
        let setup = setup(|_| {}).await;
        let url = reqwest::Url::parse(&setup.authorization_url().await).unwrap();
        assert!(url
            .as_str()
            .starts_with(&format!("{}/authorize?", setup.idp.issuer())));
        let query: HashMap<String, String> = url.query_pairs().into_owned().collect();
        assert_eq!(query["response_type"], "code");
        assert_eq!(query["scope"], "openid profile email");
        assert_eq!(query["code_challenge_method"], "S256");
        assert!(query["state"].len() >= 43 && query["nonce"].len() >= 43);
    }

    #[tokio::test]
    async fn test_first_login_creates_and_links_the_user() {
        let setup = setup(|_| {}).await;
        let identity = setup.login(alice(json!(["cms-editors"]))).await.unwrap();
        assert_eq!(identity.external_id, "alice");
        assert_eq!(identity.role, "editor");
        assert_eq!(identity.email, "alice@example.com");
        assert_eq!(identity.display_name.as_deref(), Some("Alice Liddell"));
        assert_eq!(identity.metadata["sub"], json!("u-123"));
        assert_eq!(
            identity.metadata["claims"]["groups"],
            json!(["cms-editors"])
        );

        let user = setup
            .db
            .get_user_by_username("alice")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(user.role, "editor");
        let subject = oidc_subject(setup.idp.issuer(), "u-123");
        assert_eq!(
            setup
                .db
                .get_identity_link(OIDC_PROVIDER, &subject)
                .await
                .unwrap()
                .as_deref(),
            Some("alice")
        );

        // The next login finds the same account; the IdP's groups decide the role again
        let again = setup
            .login(alice(json!(["cms-admins", "cms-editors"])))
            .await
            .unwrap();
        assert_eq!(again.external_id, "alice");
        assert_eq!(again.role, "admin");
        let user = setup
            .db
            .get_user_by_username("alice")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(user.role, "admin");
        // No matching group: the default role
        assert_eq!(setup.login(alice(json!([]))).await.unwrap().role, "viewer");
    }

    #[tokio::test]
    async fn test_without_role_claim_the_account_role_applies() {
        let setup = setup(|config| config.role_claim = String::new()).await;
        assert_eq!(
            setup
                .login(alice(json!(["cms-admins"])))
                .await
                .unwrap()
                .role,
            "viewer"
        );
        crate::users::set_role(setup.db.as_ref(), "alice", "editor")
            .await
            .unwrap();
        assert_eq!(
            setup
                .login(alice(json!(["cms-admins"])))
                .await
                .unwrap()
                .role,
            "editor"
        );
    }

    #[tokio::test]
    async fn test_idp_user_never_takes_over_an_existing_account() {
        let setup = setup(|_| {}).await;
        create_user(
            setup.db.as_ref(),
            NewUser {
                username: "alice".to_string(),
                password: "local-password".to_string(),
                role: "admin".to_string(),
                email: None,
                display_name: None,
            },
        )
        .await
        .unwrap();
        assert_eq!(
            reason(setup.login(alice(json!(["cms-admins"]))).await),
            REASON_USERNAME_TAKEN
        );
        let subject = oidc_subject(setup.idp.issuer(), "u-123");
        assert!(setup
            .db
            .get_identity_link(OIDC_PROVIDER, &subject)
            .await
            .unwrap()
            .is_none());
    }

    #[tokio::test]
    async fn test_provisioning_rules() {
        let setup = setup(|config| config.auto_create_users = false).await;
        assert_eq!(
            reason(setup.login(alice(json!([]))).await),
            REASON_NOT_PROVISIONED
        );

        let setup = self::setup(|_| {}).await;
        assert_eq!(
            reason(setup.login(json!({ "sub": "u-9" })).await),
            REASON_MISSING_USERNAME
        );
        // Falls back to the email claim
        let identity = setup
            .login(json!({ "sub": "u-10", "email": "bob@example.com" }))
            .await
            .unwrap();
        assert_eq!(identity.external_id, "bob@example.com");

        setup.login(alice(json!([]))).await.unwrap();
        crate::users::set_disabled(setup.db.as_ref(), "alice", true)
            .await
            .unwrap();
        assert_eq!(
            reason(setup.login(alice(json!([]))).await),
            REASON_ACCOUNT_DISABLED
        );
    }

    #[tokio::test]
    async fn test_linked_local_user_signs_in_without_auto_create() {
        let setup = setup(|config| config.auto_create_users = false).await;
        create_user(
            setup.db.as_ref(),
            NewUser {
                username: "alice".to_string(),
                password: "local-password".to_string(),
                role: "editor".to_string(),
                email: None,
                display_name: None,
            },
        )
        .await
        .unwrap();
        let subject = oidc_subject(&format!("{}/", setup.idp.issuer()), "u-123");
        crate::users::link_identity(setup.db.as_ref(), "alice", OIDC_PROVIDER, &subject)
            .await
            .unwrap();
        // Linking the same IdP account twice is refused
        assert!(
            crate::users::link_identity(setup.db.as_ref(), "alice", OIDC_PROVIDER, &subject)
                .await
                .is_err()
        );

        let identity = setup.login(alice(json!(["cms-editors"]))).await.unwrap();
        assert_eq!(identity.external_id, "alice");
        assert_eq!(identity.role, "editor");
        // The IdP's profile is copied to the account
        let user = setup
            .db
            .get_user_by_username("alice")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(user.email.as_deref(), Some("alice@example.com"));
    }

    #[tokio::test]
    async fn test_callback_state_is_single_use() {
        let setup = setup(|_| {}).await;
        let url = setup.authorization_url().await;
        let parameters = setup.idp.sign_in(&url, alice(json!([])));
        setup.callback(parameters.clone()).await.unwrap();
        assert_eq!(
            reason(setup.callback(parameters).await),
            REASON_INVALID_STATE
        );
        assert_eq!(
            reason(
                setup
                    .callback(HashMap::from([
                        ("code".to_string(), "x".to_string()),
                        ("state".to_string(), "forged".to_string()),
                    ]))
                    .await
            ),
            REASON_INVALID_STATE
        );
        assert_eq!(
            reason(
                setup
                    .callback(HashMap::from([(
                        "error".to_string(),
                        "access_denied".to_string()
                    )]))
                    .await
            ),
            REASON_IDP_ERROR
        );
    }

    #[tokio::test]
    async fn test_invalid_id_tokens_are_refused() {
        let setup = setup(|_| {}).await;
        let cases = [
            ("wrong audience", json!({ "aud": "someone-else" })),
            ("wrong issuer", json!({ "iss": "https://evil.example" })),
            (
                "expired",
                json!({ "exp": chrono::Utc::now().timestamp() - 3600 }),
            ),
            ("wrong nonce", json!({ "nonce": "replayed" })),
            ("other azp", json!({ "azp": "another-client" })),
        ];
        for (name, overrides) in cases {
            let mut claims = alice(json!([]));
            claims
                .as_object_mut()
                .unwrap()
                .extend(overrides.as_object().unwrap().clone());
            assert_eq!(
                reason(setup.login(claims).await),
                REASON_INVALID_TOKEN,
                "{}",
                name
            );
        }

        // A token signed with HMAC using the client secret must not be accepted
        let url = setup.authorization_url().await;
        let hs256 = jsonwebtoken::encode(
            &jsonwebtoken::Header::new(Algorithm::HS256),
            &json!({
                "iss": setup.idp.issuer(),
                "aud": CLIENT_ID,
                "sub": "u-123",
                "exp": chrono::Utc::now().timestamp() + 300,
                "nonce": MockIdp::nonce(&url),
            }),
            &jsonwebtoken::EncodingKey::from_secret(b"test-secret"),
        )
        .unwrap();
        let parameters = setup.idp.sign_in_with_token(&url, hs256);
        assert_eq!(
            reason(setup.callback(parameters).await),
            REASON_INVALID_TOKEN
        );

        // A wrong PKCE verifier is refused by the token endpoint
        let url = setup.authorization_url().await;
        let mut parameters = setup.idp.sign_in(&url, alice(json!([])));
        let other = setup.authorization_url().await;
        let other_state = reqwest::Url::parse(&other)
            .unwrap()
            .query_pairs()
            .find(|(k, _)| k == "state")
            .unwrap()
            .1
            .into_owned();
        parameters.insert("state".to_string(), other_state);
        assert_eq!(
            reason(setup.callback(parameters).await),
            REASON_INVALID_TOKEN
        );

        assert!(setup.db.list_users().await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn test_config_is_validated() {
        let adapter = SqliteAdapter::connect("sqlite::memory:", 1).await.unwrap();
        let db: Arc<dyn DatabaseAdapter> = Arc::new(adapter);
        let cache: Arc<dyn CacheBackend> = Arc::new(MemoryCacheBackend::new());
        let missing = OidcConfig {
            enabled: true,
            ..Default::default()
        };
        assert!(OidcAuthProvider::new(missing, cache.clone(), db.clone()).is_err());
        let bad_role = OidcConfig {
            enabled: true,
            issuer_url: "https://idp.example".to_string(),
            client_id: "c".to_string(),
            redirect_url: "https://cms.example/cb".to_string(),
            role_mapping: HashMap::from([("g".to_string(), "owner".to_string())]),
            ..Default::default()
        };
        assert!(OidcAuthProvider::new(bad_role, cache, db).is_err());
    }
}
