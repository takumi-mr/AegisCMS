//! A minimal OpenID Connect provider for tests: discovery, JWKS (a fresh ES256 key per instance)
//! and a token endpoint that checks the client credentials and the PKCE verifier.

use axum::extract::{Form, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::IntoResponse;
use axum::routing::{get, post};
use axum::{Json, Router};
use base64::engine::general_purpose::{STANDARD, URL_SAFE_NO_PAD};
use base64::Engine;
use jsonwebtoken::{Algorithm, EncodingKey, Header};
use p256::elliptic_curve::sec1::ToSec1Point;
use p256::elliptic_curve::Generate;
use p256::pkcs8::{EncodePrivateKey, LineEnding};
use serde_json::{json, Map, Value};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

pub const CLIENT_ID: &str = "aegis-test";
pub const CLIENT_SECRET: &str = "test-secret";
pub const REDIRECT_URL: &str = "http://127.0.0.1:8080/api/v1/auth/oidc/callback";
const KID: &str = "test-key";

/// What the token endpoint returns for a code
enum Issued {
    /// Claims to sign (merged over the defaults)
    Claims(Map<String, Value>),
    /// A ready-made ID token (to test tokens the server must refuse)
    Raw(String),
}

/// A code the user got at the "authorization endpoint": the PKCE challenge, the nonce and what
/// to issue for it
struct PendingCode {
    challenge: String,
    nonce: String,
    issued: Issued,
}

#[derive(Clone)]
struct IdpState {
    issuer: String,
    key: Arc<EncodingKey>,
    jwk: Value,
    codes: Arc<Mutex<HashMap<String, PendingCode>>>,
}

pub struct MockIdp {
    state: IdpState,
    task: tokio::task::JoinHandle<()>,
}

impl Drop for MockIdp {
    fn drop(&mut self) {
        self.task.abort();
    }
}

fn now() -> i64 {
    chrono::Utc::now().timestamp()
}

impl IdpState {
    fn sign(&self, claims: &Value) -> String {
        let mut header = Header::new(Algorithm::ES256);
        header.kid = Some(KID.to_string());
        jsonwebtoken::encode(&header, claims, &self.key).unwrap()
    }
}

async fn discovery(State(idp): State<IdpState>) -> Json<Value> {
    Json(json!({
        "issuer": idp.issuer,
        "authorization_endpoint": format!("{}/authorize", idp.issuer),
        "token_endpoint": format!("{}/token", idp.issuer),
        "jwks_uri": format!("{}/jwks", idp.issuer),
    }))
}

async fn jwks(State(idp): State<IdpState>) -> Json<Value> {
    Json(json!({ "keys": [idp.jwk] }))
}

async fn token(
    State(idp): State<IdpState>,
    headers: HeaderMap,
    Form(form): Form<HashMap<String, String>>,
) -> impl IntoResponse {
    let expected = format!(
        "Basic {}",
        STANDARD.encode(format!("{}:{}", CLIENT_ID, CLIENT_SECRET))
    );
    let authorized = headers
        .get("authorization")
        .is_some_and(|value| value.to_str().ok() == Some(expected.as_str()));
    let bad = |error: &str| (StatusCode::BAD_REQUEST, Json(json!({ "error": error })));
    if !authorized {
        return bad("invalid_client");
    }
    if form.get("grant_type").map(String::as_str) != Some("authorization_code")
        || form.get("redirect_uri").map(String::as_str) != Some(REDIRECT_URL)
    {
        return bad("invalid_request");
    }
    let code = form.get("code").cloned().unwrap_or_default();
    let Some(PendingCode {
        challenge,
        nonce,
        issued,
    }) = idp.codes.lock().unwrap().remove(&code)
    else {
        return bad("invalid_grant");
    };
    let verifier = form.get("code_verifier").cloned().unwrap_or_default();
    if URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes())) != challenge {
        return bad("invalid_grant");
    }
    let id_token = match issued {
        Issued::Raw(token) => token,
        Issued::Claims(extra) => {
            let mut claims = json!({
                "iss": idp.issuer,
                "aud": CLIENT_ID,
                "iat": now(),
                "exp": now() + 300,
                "nonce": nonce,
            });
            claims.as_object_mut().unwrap().extend(extra);
            idp.sign(&claims)
        }
    };
    (
        StatusCode::OK,
        Json(json!({ "access_token": "at", "token_type": "Bearer", "id_token": id_token })),
    )
}

impl MockIdp {
    pub async fn start() -> Self {
        let secret = p256::SecretKey::generate();
        let pem = secret.to_pkcs8_pem(LineEnding::LF).unwrap();
        let key = Arc::new(EncodingKey::from_ec_pem(pem.as_bytes()).unwrap());
        let pt = secret.public_key().to_sec1_point(false);
        let jwk = json!({
            "kty": "EC",
            "crv": "P-256",
            "x": URL_SAFE_NO_PAD.encode(pt.x().expect("x coord")),
            "y": URL_SAFE_NO_PAD.encode(pt.y().expect("y coord")),
            "kid": KID,
            "alg": "ES256",
            "use": "sig",
        });

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let issuer = format!("http://{}", listener.local_addr().unwrap());
        let state = IdpState {
            issuer,
            key,
            jwk,
            codes: Arc::new(Mutex::new(HashMap::new())),
        };
        let app = Router::new()
            .route("/.well-known/openid-configuration", get(discovery))
            .route("/jwks", get(jwks))
            .route("/token", post(token))
            .with_state(state.clone());
        let task = tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });
        Self { state, task }
    }

    pub fn issuer(&self) -> &str {
        &self.state.issuer
    }

    /// `[auth.oidc]` pointing at this provider
    pub fn config(&self) -> crate::OidcConfig {
        crate::OidcConfig {
            enabled: true,
            issuer_url: self.state.issuer.clone(),
            client_id: CLIENT_ID.to_string(),
            client_secret: CLIENT_SECRET.to_string(),
            redirect_url: REDIRECT_URL.to_string(),
            ..Default::default()
        }
    }

    fn register(&self, authorization_url: &str, issued: Issued) -> HashMap<String, String> {
        let url = reqwest::Url::parse(authorization_url).unwrap();
        let query: HashMap<String, String> = url.query_pairs().into_owned().collect();
        assert_eq!(query["client_id"], CLIENT_ID);
        assert_eq!(query["redirect_uri"], REDIRECT_URL);
        assert_eq!(query["code_challenge_method"], "S256");
        let code = uuid::Uuid::new_v4().to_string();
        self.state.codes.lock().unwrap().insert(
            code.clone(),
            PendingCode {
                challenge: query["code_challenge"].clone(),
                nonce: query["nonce"].clone(),
                issued,
            },
        );
        HashMap::from([
            ("code".to_string(), code),
            ("state".to_string(), query["state"].clone()),
        ])
    }

    /// Simulates the user signing in at the IdP: returns the callback query (`code`, `state`)
    /// for the authorization URL; the token endpoint will issue an ID token with `claims`
    /// merged over valid defaults (iss, aud, iat, exp, nonce)
    pub fn sign_in(&self, authorization_url: &str, claims: Value) -> HashMap<String, String> {
        let Value::Object(claims) = claims else {
            panic!("claims must be an object");
        };
        self.register(authorization_url, Issued::Claims(claims))
    }

    /// Like `sign_in`, but the token endpoint returns `id_token` as is
    pub fn sign_in_with_token(
        &self,
        authorization_url: &str,
        id_token: String,
    ) -> HashMap<String, String> {
        self.register(authorization_url, Issued::Raw(id_token))
    }

    /// The nonce the server put in the authorization URL
    pub fn nonce(authorization_url: &str) -> String {
        reqwest::Url::parse(authorization_url)
            .unwrap()
            .query_pairs()
            .find(|(k, _)| k == "nonce")
            .map(|(_, v)| v.into_owned())
            .unwrap()
    }
}
