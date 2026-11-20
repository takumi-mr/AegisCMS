//! MCP クライアント向けの OAuth 2.1 認可サーバー（AegisCMS が「トークンを発行する側」。
//! 外部 IdP にログインを任せる `[auth.oidc]` とは逆向き）。
//!
//! - メタデータ: `/.well-known/oauth-protected-resource`・`/.well-known/oauth-authorization-server`（RFC 9728・RFC 8414）
//! - 動的クライアント登録: `POST /oauth/register`（RFC 7591）。公開クライアントだけ。登録した内容（名前・リダイレクト先）は
//!   サーバーの鍵で署名した `client_id` の中に入れるので、DB に何も保存しない（登録を濫用されても溜まらない）
//! - 認可: `GET/POST /oauth/authorize`。JavaScript なしの 1 枚の画面で、ログイン（パスワード、2FA ならコード）と許可を兼ねる。
//!   ログインは REST のログインをそのまま呼ぶので、ロック・2FA・監査ログも同じ。PKCE（S256）必須
//! - トークン: `POST /oauth/token`。認可コードを API トークン（`aegis_pat_...`）に引き換える。
//!   発行したトークンは「設定 › プロフィール › API トークン」に並び、そこで失効できる
//!
//! 許す範囲（scope）はロールの上限に対応する: `content:read` は viewer、`content:draft` は contributor
//! （どちらも持ち主のロールを超えない）。公開・削除はどのスコープにも含めない。

use aegis_core::time::TimestampExt;
use std::net::SocketAddr;
use std::sync::Arc;

use aegis_auth::api_tokens::{issue_api_token, NewApiToken};
use aegis_core::{AuditAction, AuditOutcome};
use std::collections::HashMap;

use axum::{
    body::Body,
    extract::{ConnectInfo, Extension, Form, FromRequest, Request, State},
    http::{header, HeaderMap, HeaderValue, Method, StatusCode},
    response::{IntoResponse, Response},
    Json, Router,
};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use tower::ServiceExt;

use crate::audit::AuditContext;
use crate::handlers::auth::complete_login;
use crate::handlers::feed::public_base_url;
use crate::state::{AppState, SsoLogin};

/// 署名鍵を置く設定の名前（隠しの設定。管理画面には出ない）
const SIGNING_KEY_SETTING: &str = "oauth.signing_key";
const CLIENT_ID_PREFIX: &str = "aegis_oc_";
/// 認可コードの有効秒数
const CODE_TTL_SECS: u64 = 600;
/// 発行するアクセストークンの有効日数。切れたらリフレッシュトークンで取り直す
const ACCESS_TOKEN_DAYS: u32 = 1;
/// リフレッシュトークンの有効日数（使うたびに新しく発行して延びる）
const REFRESH_TOKEN_DAYS: u32 = 90;
const REFRESH_TOKEN_PREFIX: &str = "aegis_ort_";
const SCOPE_READ: &str = "content:read";
const SCOPE_DRAFT: &str = "content:draft";
/// 公開・非公開・削除。求められても既定ではチェックを外した状態で出し、人が選んだときだけ渡す
const SCOPE_PUBLISH: &str = "content:publish";
/// コメントとレビューのモデレーション、注文と会員の参照。顧客の個人情報に触れるので、公開とは別にして既定はオフ
const SCOPE_MANAGE: &str = "site:manage";
const MAX_REDIRECT_URIS: usize = 5;
const MAX_REDIRECT_URI_CHARS: usize = 400;
const MAX_CLIENT_NAME_CHARS: usize = 100;

// ---------------------------------------------------------------- クライアント（署名付き client_id）

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Client {
    /// 名前（同意画面に出る）
    n: String,
    /// リダイレクト先
    r: Vec<String>,
}

type HmacSha256 = Hmac<Sha256>;

fn sign(key: &[u8], payload: &str) -> HmacSha256 {
    let mut mac = HmacSha256::new_from_slice(key).expect("HMAC accepts any key length");
    mac.update(payload.as_bytes());
    mac
}

fn encode_client_id(key: &[u8], client: &Client) -> String {
    let payload = URL_SAFE_NO_PAD.encode(serde_json::to_vec(client).expect("serializable"));
    let signature = URL_SAFE_NO_PAD.encode(sign(key, &payload).finalize().into_bytes());
    format!("{CLIENT_ID_PREFIX}{payload}.{signature}")
}

fn decode_client_id(key: &[u8], client_id: &str) -> Option<Client> {
    let rest = client_id.strip_prefix(CLIENT_ID_PREFIX)?;
    let (payload, signature) = rest.split_once('.')?;
    let signature = URL_SAFE_NO_PAD.decode(signature).ok()?;
    sign(key, payload).verify_slice(&signature).ok()?;
    serde_json::from_slice(&URL_SAFE_NO_PAD.decode(payload).ok()?).ok()
}

/// 署名鍵。まだ無ければ作って保存する（複数のサーバーで同じ DB を使っても、保存された 1 つに揃う）
async fn signing_key(state: &AppState) -> Result<Vec<u8>, String> {
    let read = || async {
        state
            .db
            .list_settings()
            .await
            .map(|s| s.get(SIGNING_KEY_SETTING).cloned())
            .map_err(|e| e.to_string())
    };
    if let Some(key) = read().await? {
        return Ok(key.into_bytes());
    }
    let fresh = format!(
        "{}{}",
        uuid::Uuid::new_v4().simple(),
        uuid::Uuid::new_v4().simple()
    );
    state
        .db
        .set_setting(SIGNING_KEY_SETTING, &fresh)
        .await
        .map_err(|e| e.to_string())?;
    Ok(read().await?.unwrap_or(fresh).into_bytes())
}

fn is_loopback(url: &url::Url) -> bool {
    matches!(
        url.host_str(),
        Some("localhost") | Some("127.0.0.1") | Some("[::1]")
    )
}

/// 登録できるリダイレクト先か: https、ループバックの http、アプリ用のカスタムスキーム
fn valid_redirect_uri(value: &str) -> bool {
    if value.chars().count() > MAX_REDIRECT_URI_CHARS {
        return false;
    }
    let Ok(url) = url::Url::parse(value) else {
        return false;
    };
    if url.fragment().is_some() || !url.username().is_empty() || url.password().is_some() {
        return false;
    }
    match url.scheme() {
        "https" => url.host_str().is_some(),
        "http" => is_loopback(&url),
        "javascript" | "data" | "file" | "vbscript" | "blob" | "about" | "ftp" | "ws" | "wss" => {
            false
        }
        _ => true,
    }
}

/// 登録済みのリダイレクト先と同じか。ループバックの http はポートだけ違ってよい（RFC 8252）
fn redirect_uri_matches(registered: &str, given: &str) -> bool {
    if registered == given {
        return true;
    }
    let (Ok(mut a), Ok(mut b)) = (url::Url::parse(registered), url::Url::parse(given)) else {
        return false;
    };
    if a.scheme() == "http" && is_loopback(&a) && is_loopback(&b) {
        let _ = a.set_port(None);
        let _ = b.set_port(None);
        return a == b;
    }
    false
}

// ---------------------------------------------------------------- 応答の部品

fn with_cors(mut response: Response) -> Response {
    let h = response.headers_mut();
    h.insert(
        header::ACCESS_CONTROL_ALLOW_ORIGIN,
        HeaderValue::from_static("*"),
    );
    h.insert(
        header::ACCESS_CONTROL_ALLOW_METHODS,
        HeaderValue::from_static("GET, POST, OPTIONS"),
    );
    h.insert(
        header::ACCESS_CONTROL_ALLOW_HEADERS,
        HeaderValue::from_static("content-type, authorization, mcp-protocol-version"),
    );
    response
}

fn json_no_store(status: StatusCode, value: Value) -> Response {
    with_cors(
        (
            status,
            [
                (header::CACHE_CONTROL, "no-store"),
                (header::PRAGMA, "no-cache"),
            ],
            Json(value),
        )
            .into_response(),
    )
}

fn oauth_error(status: StatusCode, error: &str, description: &str) -> Response {
    json_no_store(
        status,
        json!({ "error": error, "error_description": description }),
    )
}

/// プリフライト（ブラウザ上のクライアント用）
pub async fn preflight() -> Response {
    with_cors(StatusCode::NO_CONTENT.into_response())
}

fn html_escape(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for c in value.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            c => out.push(c),
        }
    }
    out
}

fn html_page(status: StatusCode, title: &str, body: &str) -> Response {
    html_document(status, "", title, body)
}

/// `head_extra` は `head` にそのまま入れる（`meta refresh` など）
fn html_document(status: StatusCode, head_extra: &str, title: &str, body: &str) -> Response {
    let html = format!(
        "<!DOCTYPE html><html lang=\"en\"><head><meta charset=\"utf-8\"><meta name=\"viewport\" content=\"width=device-width, initial-scale=1\"><meta name=\"robots\" content=\"noindex\">{}<title>{}</title><link rel=\"stylesheet\" href=\"authorize.css\"></head><body><main class=\"card\">{}</main></body></html>",
        head_extra,
        html_escape(title),
        body
    );
    (
        status,
        [
            (header::CONTENT_TYPE, "text/html; charset=utf-8"),
            (header::CACHE_CONTROL, "no-store"),
        ],
        html,
    )
        .into_response()
}

fn error_page(message: &str) -> Response {
    html_page(
        StatusCode::BAD_REQUEST,
        "Authorization error",
        &format!(
            "<h1>Cannot continue</h1><p>{}</p><p class=\"muted\">Close this window and try again from the app.</p>",
            html_escape(message)
        ),
    )
}

/// クライアントのリダイレクト先へ戻す。GET への応答は普通のリダイレクト。POST への応答は、
/// フォームの `form-action 'self'` の CSP に止められないよう、`meta refresh` の画面にする
fn send_back(url: &str, via_page: bool) -> Response {
    if !via_page {
        return (
            StatusCode::FOUND,
            [(header::LOCATION, url.to_string())],
            Body::empty(),
        )
            .into_response();
    }
    let u = html_escape(url);
    html_document(
        StatusCode::OK,
        &format!("<meta http-equiv=\"refresh\" content=\"0;url={u}\">"),
        "Returning to the app",
        &format!("<p>Returning to the app…</p><p><a href=\"{u}\">Continue</a></p>"),
    )
}

fn redirect_with(redirect_uri: &str, params: &[(&str, &str)]) -> String {
    let mut url = url::Url::parse(redirect_uri).expect("validated redirect uri");
    {
        let mut query = url.query_pairs_mut();
        for (k, v) in params {
            if !v.is_empty() {
                query.append_pair(k, v);
            }
        }
    }
    url.to_string()
}

/// `GET /oauth/authorize.css`
pub async fn authorize_css() -> Response {
    (
        [
            (header::CONTENT_TYPE, "text/css; charset=utf-8"),
            (header::CACHE_CONTROL, "public, max-age=3600"),
        ],
        AUTHORIZE_CSS,
    )
        .into_response()
}

const AUTHORIZE_CSS: &str = r#"
:root { color-scheme: light dark; --bg:#f6f7f9; --card:#fff; --text:#1b1f24; --muted:#5b6672; --line:#d6dbe1; --accent:#2457d6; --danger:#c2273d; }
@media (prefers-color-scheme: dark) { :root { --bg:#0f1318; --card:#181d24; --text:#e8ecf1; --muted:#9aa6b2; --line:#2b333d; --accent:#6c93ff; --danger:#ff7b8f; } }
* { box-sizing: border-box; }
body { margin:0; min-height:100vh; display:flex; align-items:center; justify-content:center; padding:1rem; background:var(--bg); color:var(--text); font:16px/1.5 system-ui,-apple-system,"Segoe UI",sans-serif; }
.card { width:100%; max-width:26rem; background:var(--card); border:1px solid var(--line); border-radius:12px; padding:1.5rem; }
h1 { font-size:1.25rem; margin:0 0 .75rem; }
.muted { color:var(--muted); font-size:.875rem; }
.error { color:var(--danger); font-weight:600; }
ul { padding-left:1.25rem; margin:.5rem 0 1rem; }
label { display:block; font-size:.875rem; margin:.75rem 0 .25rem; }
input[type=text], input[type=password] { width:100%; padding:.55rem .65rem; border:1px solid var(--line); border-radius:8px; background:transparent; color:inherit; font:inherit; }
.actions { display:flex; gap:.75rem; margin-top:1.25rem; }
button { flex:1; padding:.6rem; border-radius:8px; border:1px solid var(--accent); background:var(--accent); color:#fff; font:inherit; font-weight:600; cursor:pointer; }
button.secondary { background:transparent; color:var(--text); border-color:var(--line); }
a { color:var(--accent); }
li { margin:.35rem 0; }
label.warn { color:var(--danger); font-weight:600; }
ul label { display:block; margin:0; font-size:1rem; }
"#;

// ---------------------------------------------------------------- メタデータ

fn scopes_supported() -> Value {
    json!([SCOPE_READ, SCOPE_DRAFT, SCOPE_PUBLISH, SCOPE_MANAGE])
}

/// `GET /.well-known/oauth-protected-resource`（`/mcp` 付きも同じ）
pub async fn protected_resource_metadata(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Response {
    let base = public_base_url(&state, &headers);
    json_cacheable(json!({
        "resource": format!("{base}/mcp"),
        "authorization_servers": [base],
        "bearer_methods_supported": ["header"],
        "scopes_supported": scopes_supported(),
    }))
}

/// `GET /.well-known/oauth-authorization-server`
pub async fn authorization_server_metadata(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Response {
    let base = public_base_url(&state, &headers);
    json_cacheable(json!({
        "issuer": base,
        "authorization_endpoint": format!("{base}/oauth/authorize"),
        "token_endpoint": format!("{base}/oauth/token"),
        "registration_endpoint": format!("{base}/oauth/register"),
        "response_types_supported": ["code"],
        "grant_types_supported": ["authorization_code", "refresh_token"],
        "code_challenge_methods_supported": ["S256"],
        "token_endpoint_auth_methods_supported": ["none"],
        "scopes_supported": scopes_supported(),
    }))
}

fn json_cacheable(value: Value) -> Response {
    with_cors(
        (
            StatusCode::OK,
            [(header::CACHE_CONTROL, "no-cache")],
            Json(value),
        )
            .into_response(),
    )
}

/// `/mcp` が 401 のときの `WWW-Authenticate` の値（RFC 9728）
pub fn www_authenticate(state: &AppState, headers: &HeaderMap) -> String {
    format!(
        "Bearer resource_metadata=\"{}/.well-known/oauth-protected-resource\"",
        public_base_url(state, headers)
    )
}

// ---------------------------------------------------------------- 動的クライアント登録

#[derive(Debug, Deserialize)]
pub struct RegisterRequest {
    #[serde(default)]
    client_name: Option<String>,
    #[serde(default)]
    redirect_uris: Vec<String>,
    #[serde(default)]
    token_endpoint_auth_method: Option<String>,
}

/// `POST /oauth/register`
pub async fn register(
    State(state): State<Arc<AppState>>,
    Json(request): Json<RegisterRequest>,
) -> Response {
    let bad = |description: &str| {
        oauth_error(
            StatusCode::BAD_REQUEST,
            "invalid_client_metadata",
            description,
        )
    };
    if request.redirect_uris.is_empty() || request.redirect_uris.len() > MAX_REDIRECT_URIS {
        return bad(&format!(
            "redirect_uris must have 1 to {MAX_REDIRECT_URIS} entries"
        ));
    }
    if let Some(uri) = request
        .redirect_uris
        .iter()
        .find(|u| !valid_redirect_uri(u))
    {
        return oauth_error(
            StatusCode::BAD_REQUEST,
            "invalid_redirect_uri",
            &format!("Not allowed as a redirect URI: {uri}"),
        );
    }
    if request
        .token_endpoint_auth_method
        .as_deref()
        .is_some_and(|m| m != "none")
    {
        return bad("Only public clients are supported (token_endpoint_auth_method: none)");
    }
    let name: String = request
        .client_name
        .unwrap_or_default()
        .chars()
        .filter(|c| !c.is_control())
        .take(MAX_CLIENT_NAME_CHARS)
        .collect();
    let name = if name.trim().is_empty() {
        "An application".to_string()
    } else {
        name.trim().to_string()
    };
    let key = match signing_key(&state).await {
        Ok(key) => key,
        Err(e) => {
            tracing::error!("OAuth signing key unavailable: {e}");
            return oauth_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "server_error",
                "Could not register the client",
            );
        }
    };
    let client = Client {
        n: name.clone(),
        r: request.redirect_uris.clone(),
    };
    json_no_store(
        StatusCode::CREATED,
        json!({
            "client_id": encode_client_id(&key, &client),
            "client_name": name,
            "redirect_uris": request.redirect_uris,
            "grant_types": ["authorization_code", "refresh_token"],
            "response_types": ["code"],
            "token_endpoint_auth_method": "none",
            "client_id_issued_at": jiff::Timestamp::now().as_second(),
        }),
    )
}

// ---------------------------------------------------------------- 認可

#[derive(Debug, Default, Clone, Deserialize, Serialize)]
pub struct AuthParams {
    #[serde(default)]
    response_type: String,
    #[serde(default)]
    client_id: String,
    #[serde(default)]
    redirect_uri: String,
    #[serde(default)]
    state: String,
    #[serde(default)]
    code_challenge: String,
    #[serde(default)]
    code_challenge_method: String,
    #[serde(default)]
    scope: String,
}

#[derive(Debug, Default, Deserialize)]
pub struct AuthForm {
    #[serde(flatten)]
    params: AuthParams,
    #[serde(default)]
    action: String,
    #[serde(default)]
    username: String,
    #[serde(default)]
    password: String,
    #[serde(default)]
    challenge_token: String,
    #[serde(default)]
    code: String,
    /// 許可する範囲のチェックを画面で選んだ印（無ければ既定: 下書きまで）
    #[serde(default)]
    grants: String,
    #[serde(default)]
    grant_draft: String,
    #[serde(default)]
    grant_publish: String,
    #[serde(default)]
    grant_manage: String,
}

/// 画面で選んだ、読み取り以外の範囲
#[derive(Debug, Clone, Copy)]
struct Chosen {
    draft: bool,
    publish: bool,
    manage: bool,
}

impl Chosen {
    /// 画面を開いたときの既定: 下書きはオン、公開はオフ
    fn default_for(prepared: &Prepared) -> Self {
        Chosen {
            draft: prepared.scopes.contains(&SCOPE_DRAFT),
            publish: false,
            manage: false,
        }
    }

    fn from_form(form: &AuthForm, prepared: &Prepared) -> Self {
        if form.grants.is_empty() {
            return Self::default_for(prepared);
        }
        Chosen {
            draft: prepared.scopes.contains(&SCOPE_DRAFT) && form.grant_draft == "on",
            publish: prepared.scopes.contains(&SCOPE_PUBLISH) && form.grant_publish == "on",
            manage: prepared.scopes.contains(&SCOPE_MANAGE) && form.grant_manage == "on",
        }
    }

    /// 渡す範囲（読み取りは常に含む）
    fn scopes(self) -> Vec<&'static str> {
        let mut scopes = vec![SCOPE_READ];
        if self.draft {
            scopes.push(SCOPE_DRAFT);
        }
        if self.publish {
            scopes.push(SCOPE_PUBLISH);
        }
        if self.manage {
            scopes.push(SCOPE_MANAGE);
        }
        scopes
    }
}

struct Prepared {
    client: Client,
    scopes: Vec<&'static str>,
}

enum Invalid {
    /// クライアントかリダイレクト先が信用できない: エラーを画面に出し、どこにも戻さない
    Page(&'static str),
    /// リダイレクト先に `error` を付けて戻す
    Back(&'static str, &'static str),
}

async fn prepare(state: &AppState, params: &AuthParams) -> Result<Prepared, Invalid> {
    let key = signing_key(state)
        .await
        .map_err(|_| Invalid::Page("The server could not check this request"))?;
    let client = decode_client_id(&key, &params.client_id)
        .ok_or(Invalid::Page("The application is not registered"))?;
    if !client
        .r
        .iter()
        .any(|registered| redirect_uri_matches(registered, &params.redirect_uri))
    {
        return Err(Invalid::Page(
            "The redirect address does not match the registration",
        ));
    }
    if params.response_type != "code" {
        return Err(Invalid::Back(
            "unsupported_response_type",
            "Only response_type=code is supported",
        ));
    }
    if params.code_challenge.len() < 43
        || params.code_challenge.len() > 128
        || params.code_challenge_method != "S256"
    {
        return Err(Invalid::Back(
            "invalid_request",
            "PKCE with code_challenge_method=S256 is required",
        ));
    }
    let mut scopes: Vec<&'static str> = Vec::new();
    for requested in params.scope.split_whitespace() {
        for known in [SCOPE_READ, SCOPE_DRAFT, SCOPE_PUBLISH, SCOPE_MANAGE] {
            if requested == known && !scopes.contains(&known) {
                scopes.push(known);
            }
        }
    }
    if scopes.is_empty() {
        scopes = vec![SCOPE_READ, SCOPE_DRAFT];
    }
    Ok(Prepared { client, scopes })
}

fn invalid_response(params: &AuthParams, invalid: Invalid, via_page: bool) -> Response {
    match invalid {
        Invalid::Page(message) => error_page(message),
        Invalid::Back(error, description) => send_back(
            &redirect_with(
                &params.redirect_uri,
                &[
                    ("error", error),
                    ("error_description", description),
                    ("state", &params.state),
                ],
            ),
            via_page,
        ),
    }
}

fn scope_description(scope: &str) -> &'static str {
    match scope {
        SCOPE_READ => "Read content on this site, as far as your role allows",
        SCOPE_DRAFT => {
            "Create and edit drafts, upload media, set tags and send content to review. Nothing becomes visible to visitors"
        }
        SCOPE_MANAGE => {
            "Moderate and reply to comments and reviews, and read orders, sales and members. This includes customers' personal information, as far as your role allows"
        }
        _ => {
            "Publish and unpublish content, and delete it. This changes what visitors see, as far as your role allows"
        }
    }
}

fn hidden(name: &str, value: &str) -> String {
    format!(
        "<input type=\"hidden\" name=\"{}\" value=\"{}\">",
        name,
        html_escape(value)
    )
}

/// 許可の画面。`challenge` があれば 2 段階認証のコードの入力、`error` があれば先頭に出す
#[derive(Clone, Copy)]
struct View<'a> {
    username: &'a str,
    chosen: Chosen,
    challenge: Option<&'a str>,
    error: Option<&'a str>,
    status: StatusCode,
}

fn consent_page(
    state: &AppState,
    prepared: &Prepared,
    params: &AuthParams,
    view: &View,
) -> Response {
    let View {
        username,
        chosen,
        challenge,
        error,
        status,
    } = *view;
    let site = state.site().title;
    let host = url::Url::parse(&params.redirect_uri)
        .ok()
        .map(|u| {
            u.host_str()
                .map(str::to_string)
                .unwrap_or_else(|| format!("{}://", u.scheme()))
        })
        .unwrap_or_default();
    // 2 段階認証の入力のときは、さっき選んだ内容を文章で見せ、選択は隠し項目で持ち越す
    let in_second_step = challenge.is_some();
    let mut scopes = format!("<li>{}</li>", scope_description(SCOPE_READ));
    for (scope, requested, on) in [
        (
            SCOPE_DRAFT,
            prepared.scopes.contains(&SCOPE_DRAFT),
            chosen.draft,
        ),
        (
            SCOPE_PUBLISH,
            prepared.scopes.contains(&SCOPE_PUBLISH),
            chosen.publish,
        ),
        (
            SCOPE_MANAGE,
            prepared.scopes.contains(&SCOPE_MANAGE),
            chosen.manage,
        ),
    ] {
        if !requested {
            continue;
        }
        let field = match scope {
            SCOPE_DRAFT => "grant_draft",
            SCOPE_PUBLISH => "grant_publish",
            _ => "grant_manage",
        };
        let class = if scope != SCOPE_DRAFT {
            " class=\"warn\""
        } else {
            ""
        };
        if in_second_step {
            if on {
                scopes.push_str(&format!("<li>{}</li>", scope_description(scope)));
            }
        } else {
            scopes.push_str(&format!(
                "<li><label{class}><input type=\"checkbox\" name=\"{field}\" value=\"on\"{}> {}</label></li>",
                if on { " checked" } else { "" },
                scope_description(scope)
            ));
        }
    }
    let mut body = format!(
        "<h1>Allow access?</h1><p><strong>{}</strong> wants to use <strong>{}</strong> on your behalf.</p><ul>{}</ul>\
<p class=\"muted\">After you decide, you are sent back to {}. You can revoke the access at any time in Admin Studio under My profile › API tokens.</p>",
        html_escape(&prepared.client.n),
        html_escape(&site),
        scopes,
        html_escape(&host),
    );
    if let Some(error) = error {
        body.push_str(&format!(
            "<p class=\"error\" role=\"alert\">{}</p>",
            html_escape(error)
        ));
    }
    body.push_str("<form method=\"post\" action=\"authorize\">");
    for (name, value) in [
        ("response_type", params.response_type.as_str()),
        ("client_id", &params.client_id),
        ("redirect_uri", &params.redirect_uri),
        ("state", &params.state),
        ("code_challenge", &params.code_challenge),
        ("code_challenge_method", &params.code_challenge_method),
        ("scope", &params.scope),
    ] {
        body.push_str(&hidden(name, value));
    }
    body.push_str(&hidden("grants", "1"));
    match challenge {
        Some(token) => {
            if chosen.draft {
                body.push_str(&hidden("grant_draft", "on"));
            }
            if chosen.publish {
                body.push_str(&hidden("grant_publish", "on"));
            }
            if chosen.manage {
                body.push_str(&hidden("grant_manage", "on"));
            }
            body.push_str(&hidden("challenge_token", token));
            body.push_str(&hidden("username", username));
            body.push_str(
                "<label for=\"code\">Authentication code (or a backup code)</label>\
<input id=\"code\" name=\"code\" type=\"text\" inputmode=\"numeric\" autocomplete=\"one-time-code\" required autofocus>",
            );
        }
        None => body.push_str(&format!(
            "<label for=\"username\">Username</label>\
<input id=\"username\" name=\"username\" type=\"text\" autocomplete=\"username\" value=\"{}\" required>\
<label for=\"password\">Password</label>\
<input id=\"password\" name=\"password\" type=\"password\" autocomplete=\"current-password\" required>",
            html_escape(username)
        )),
    }
    body.push_str(
        "<div class=\"actions\"><button type=\"submit\" name=\"action\" value=\"allow\">Allow</button>\
<button type=\"submit\" name=\"action\" value=\"deny\" class=\"secondary\" formnovalidate>Deny</button></div>",
    );
    // シングルサインオンのアカウント用: IdP でログインして、いまのチェックのとおり許可する
    if let (false, Some(sso)) = (in_second_step, state.oidc.as_ref()) {
        body.push_str(&format!(
            "<p class=\"muted\">Or use single sign-on instead of a password:</p>\
<div class=\"actions\"><button type=\"submit\" name=\"action\" value=\"sso\" class=\"secondary\" formnovalidate>Allow with {}</button></div>",
            html_escape(&sso.display_name)
        ));
    }
    body.push_str("</form>");
    html_page(status, "Allow access", &body)
}

/// `GET /oauth/authorize`
pub async fn authorize_get(
    State(state): State<Arc<AppState>>,
    axum::extract::Query(params): axum::extract::Query<AuthParams>,
) -> Response {
    match prepare(&state, &params).await {
        Ok(prepared) => {
            let chosen = Chosen::default_for(&prepared);
            let view = View {
                username: "",
                chosen,
                challenge: None,
                error: None,
                status: StatusCode::OK,
            };
            consent_page(&state, &prepared, &params, &view)
        }
        Err(invalid) => invalid_response(&params, invalid, false),
    }
}

/// 内部で REST API を呼ぶ（ログインをそのまま使う）。接続元は元のリクエストのものを引き継ぐ
async fn call_api(
    api: &Router,
    path: &str,
    body: &Value,
    peer: Option<ConnectInfo<SocketAddr>>,
    headers: &HeaderMap,
) -> (StatusCode, Value) {
    let mut builder = Request::builder()
        .method(Method::POST)
        .uri(path)
        .header(header::CONTENT_TYPE, "application/json");
    for name in ["x-forwarded-for", "user-agent"] {
        if let Some(value) = headers.get(name) {
            builder = builder.header(name, value);
        }
    }
    let mut request = builder
        .body(Body::from(body.to_string()))
        .expect("internal request");
    if let Some(peer) = peer {
        request.extensions_mut().insert(peer);
    }
    let response = match api.clone().oneshot(request).await {
        Ok(response) => response,
        Err(never) => match never {},
    };
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), 1024 * 1024)
        .await
        .unwrap_or_default();
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    )
}

/// ユーザーのロールを超えない、スコープに見合うロール
fn role_for(scopes: &[&str], user_role: &str) -> String {
    let wanted = if scopes.contains(&SCOPE_PUBLISH) || scopes.contains(&SCOPE_MANAGE) {
        "editor"
    } else if scopes.contains(&SCOPE_DRAFT) {
        "contributor"
    } else {
        "viewer"
    };
    if aegis_auth::role_level(wanted) <= aegis_auth::role_level(user_role) {
        wanted.to_string()
    } else {
        user_role.to_string()
    }
}

fn code_key(code: &str) -> String {
    format!(
        "oauth_code:{}",
        hex::encode(Sha256::digest(code.as_bytes()))
    )
}

/// `POST /oauth/authorize`: ログインと、許可（Allow）か拒否（Deny）
pub async fn authorize_post(
    State(state): State<Arc<AppState>>,
    Extension(api): Extension<Router>,
    audit: AuditContext,
    request: Request,
) -> Response {
    let peer = request
        .extensions()
        .get::<ConnectInfo<SocketAddr>>()
        .cloned();
    let headers = request.headers().clone();
    let Ok(Form(form)) = Form::<AuthForm>::from_request(request, &()).await else {
        return error_page("The request could not be read");
    };
    let params = form.params.clone();
    let prepared = match prepare(&state, &params).await {
        Ok(p) => p,
        Err(invalid) => return invalid_response(&params, invalid, true),
    };

    if form.action == "deny" {
        return invalid_response(
            &params,
            Invalid::Back("access_denied", "The user denied the request"),
            true,
        );
    }

    let username = form.username.trim().to_string();
    let chosen = Chosen::from_form(&form, &prepared);
    if form.action == "sso" {
        return start_sso(&state, &headers, &params, chosen).await;
    }
    let page = |challenge: Option<&str>, error: Option<&str>, status: StatusCode| {
        let view = View {
            username: &username,
            chosen,
            challenge,
            error,
            status,
        };
        consent_page(&state, &prepared, &params, &view)
    };

    let (status, reply) = if !form.challenge_token.is_empty() {
        call_api(
            &api,
            "/api/v1/auth/session/verify-2fa",
            &json!({ "challenge_token": form.challenge_token, "code": form.code.trim() }),
            peer,
            &headers,
        )
        .await
    } else {
        if username.is_empty() || form.password.is_empty() {
            return page(
                None,
                Some("Enter your username and password."),
                StatusCode::BAD_REQUEST,
            );
        }
        call_api(
            &api,
            "/api/v1/auth/session",
            &json!({ "username": username, "password": form.password, "ttl_secs": 60 }),
            peer,
            &headers,
        )
        .await
    };

    if reply["mfa_required"].as_bool() == Some(true) {
        let token = reply["challenge_token"]
            .as_str()
            .unwrap_or_default()
            .to_string();
        return page(Some(&token), None, StatusCode::OK);
    }
    let session_id = reply["id"].as_str().unwrap_or_default().to_string();
    if !status.is_success() || session_id.is_empty() {
        let in_second_step = !form.challenge_token.is_empty();
        let message = match status {
            StatusCode::TOO_MANY_REQUESTS => {
                "Too many attempts. Wait a while and try again.".to_string()
            }
            StatusCode::UNAUTHORIZED if in_second_step => "That code is not valid.".to_string(),
            StatusCode::UNAUTHORIZED => "Invalid username or password.".to_string(),
            StatusCode::FORBIDDEN => reply["error"]
                .as_str()
                .unwrap_or("This account cannot sign in here.")
                .to_string(),
            _ => "Sign-in failed. Try again.".to_string(),
        };
        // 2 段階目の失敗でも、チャレンジが生きていれば同じ入力欄を出す
        let challenge = in_second_step.then_some(form.challenge_token.as_str());
        let shown_status = if status == StatusCode::TOO_MANY_REQUESTS {
            status
        } else {
            StatusCode::UNAUTHORIZED
        };
        return page(challenge, Some(&message), shown_status);
    }

    // 許可のためのログインなので、セッションはすぐ捨てる（ここから先は発行するトークンだけが残る）
    let who = reply["user_id"].as_str().unwrap_or_default().to_string();
    let user_role = reply["role"].as_str().unwrap_or("viewer").to_string();
    let _ = state.sessions.destroy_session(&session_id).await;

    finish_authorization(
        &state,
        &audit,
        &prepared,
        &params,
        chosen,
        (&who, &user_role),
    )
    .await
}

/// ログインできた持ち主に、認可コードを発行してクライアントへ戻す（パスワードでも SSO でも共通）
async fn finish_authorization(
    state: &AppState,
    audit: &AuditContext,
    prepared: &Prepared,
    params: &AuthParams,
    chosen: Chosen,
    (who, user_role): (&str, &str),
) -> Response {
    let granted = chosen.scopes();
    let role = role_for(&granted, user_role);
    let code = format!(
        "{}{}",
        uuid::Uuid::new_v4().simple(),
        uuid::Uuid::new_v4().simple()
    );
    let grant = json!({
        "client_id": params.client_id,
        "client_name": prepared.client.n,
        "redirect_uri": params.redirect_uri,
        "code_challenge": params.code_challenge,
        "username": who,
        "role": role,
        "scope": granted.join(" "),
    });
    if state
        .cache
        .set(&code_key(&code), &grant.to_string(), Some(CODE_TTL_SECS))
        .await
        .is_err()
    {
        return invalid_response(
            params,
            Invalid::Back("server_error", "Could not issue the authorization code"),
            true,
        );
    }
    state
        .audit
        .record(
            audit
                .event(AuditAction::UserUpdate, AuditOutcome::Success)
                .with_resource("user", Some(who.to_string()))
                .with_details(json!({
                    "action": "oauth.authorize",
                    "client": prepared.client.n,
                    "scope": granted.join(" "),
                    "role": role,
                })),
        )
        .await;

    send_back(
        &redirect_with(
            &params.redirect_uri,
            &[("code", &code), ("state", &params.state)],
        ),
        true,
    )
}

// ---------------------------------------------------------------- シングルサインオン（OpenID Connect）

/// SSO を始めたブラウザを覚えておく Cookie。IdP のログインのあとの戻り（コールバック）が、
/// 始めたのと同じブラウザからだと確かめる。無いと、攻撃者が始めた IdP の URL を被害者に踏ませて、
/// 被害者のアカウントのコードを攻撃者のクライアントへ発行させられる
const SSO_COOKIE: &str = "aegis_oauth_sso";

fn sso_cookie(value: &str, max_age: u32, secure: bool) -> HeaderValue {
    HeaderValue::from_str(&format!(
        "{SSO_COOKIE}={value}; Max-Age={max_age}; Path=/; HttpOnly; SameSite=Lax{}",
        if secure { "; Secure" } else { "" }
    ))
    .expect("cookie is ASCII")
}

fn cookie_value<'a>(headers: &'a HeaderMap, name: &str) -> Option<&'a str> {
    headers
        .get_all(header::COOKIE)
        .iter()
        .filter_map(|v| v.to_str().ok())
        .flat_map(|v| v.split(';'))
        .filter_map(|pair| pair.trim().split_once('='))
        .find(|(k, _)| *k == name)
        .map(|(_, v)| v)
}

/// 画面で SSO のボタンが押された: 選んだ内容と始めたブラウザを覚えて、IdP へ送る
async fn start_sso(
    state: &AppState,
    headers: &HeaderMap,
    params: &AuthParams,
    chosen: Chosen,
) -> Response {
    let Some(sso) = state.oidc.as_ref() else {
        return error_page("Single sign-on is not enabled on this site");
    };
    let failed = || {
        invalid_response(
            params,
            Invalid::Back("server_error", "Could not start single sign-on"),
            true,
        )
    };
    let Ok(Some(redirect)) = sso.provider.initiate_login("").await else {
        return failed();
    };
    let idp_state = url::Url::parse(&redirect.url).ok().and_then(|u| {
        u.query_pairs()
            .find(|(k, _)| k == "state")
            .map(|(_, v)| v.into_owned())
    });
    let Some(idp_state) = idp_state else {
        return failed();
    };
    let binding = format!(
        "{}{}",
        uuid::Uuid::new_v4().simple(),
        uuid::Uuid::new_v4().simple()
    );
    let pending = json!({
        "params": params,
        "draft": chosen.draft,
        "publish": chosen.publish,
        "manage": chosen.manage,
        "binding": sha256_hex(&binding),
    });
    if state
        .cache
        .set(
            &format!("oauth_sso:{idp_state}"),
            &pending.to_string(),
            Some(CODE_TTL_SECS),
        )
        .await
        .is_err()
    {
        return failed();
    }
    let secure = public_base_url(state, headers).starts_with("https://");
    let mut response = send_back(&redirect.url, true);
    response.headers_mut().append(
        header::SET_COOKIE,
        sso_cookie(&binding, CODE_TTL_SECS as u32, secure),
    );
    response
}

/// `GET /api/v1/auth/oidc/callback` から呼ぶ。この戻りが「MCP クライアントの許可」の途中のものなら、
/// ログインを済ませて認可コードを発行し、クライアントへ戻す応答を返す。そうでなければ `None`（普通のログイン）
pub async fn resume_after_sso(
    state: &AppState,
    audit: &AuditContext,
    sso: &SsoLogin,
    headers: &HeaderMap,
    parameters: &HashMap<String, String>,
) -> Option<Response> {
    let key = format!("oauth_sso:{}", parameters.get("state")?);
    let raw = state.cache.get(&key).await.ok()??;
    let _ = state.cache.delete(&key).await;
    let pending: Value = serde_json::from_str(&raw).ok()?;
    let params: AuthParams = serde_json::from_value(pending["params"].clone()).ok()?;

    let secure = public_base_url(state, headers).starts_with("https://");
    let done = |mut response: Response| {
        response
            .headers_mut()
            .append(header::SET_COOKIE, sso_cookie("", 0, secure));
        Some(response)
    };

    // 始めたのと同じブラウザか
    let same_browser = cookie_value(headers, SSO_COOKIE)
        .map(sha256_hex)
        .is_some_and(|hash| Some(hash.as_str()) == pending["binding"].as_str());
    if !same_browser {
        return done(error_page(
            "This sign-in was not started in this browser. Start again from the application.",
        ));
    }
    let prepared = match prepare(state, &params).await {
        Ok(p) => p,
        Err(invalid) => return done(invalid_response(&params, invalid, true)),
    };
    let chosen = Chosen {
        draft: prepared.scopes.contains(&SCOPE_DRAFT) && pending["draft"].as_bool() == Some(true),
        publish: prepared.scopes.contains(&SCOPE_PUBLISH)
            && pending["publish"].as_bool() == Some(true),
        manage: prepared.scopes.contains(&SCOPE_MANAGE)
            && pending["manage"].as_bool() == Some(true),
    };
    let denied = |description: &'static str| {
        invalid_response(&params, Invalid::Back("access_denied", description), true)
    };

    let credentials = aegis_core::AuthCredentials {
        kind: aegis_auth::OIDC_CALLBACK_CREDENTIALS.to_string(),
        parameters: parameters.clone(),
        ..Default::default()
    };
    let identity = match sso.provider.authenticate(&credentials).await {
        Ok(identity) => identity,
        Err(e) => {
            tracing::warn!("Single sign-on for an OAuth authorization failed: {e}");
            return done(denied("Single sign-on failed"));
        }
    };
    let session =
        match complete_login(state, audit, sso.provider.provider_id(), identity, Some(60)).await {
            Ok(session) => session,
            Err(_) => return done(denied("This account cannot sign in")),
        };
    // 許可のためのログインなので、セッションはすぐ捨てる
    let _ = state.sessions.destroy_session(&session.id).await;
    done(
        finish_authorization(
            state,
            audit,
            &prepared,
            &params,
            chosen,
            (&session.user_id, &session.role),
        )
        .await,
    )
}

// ---------------------------------------------------------------- トークン

#[derive(Debug, Default, Deserialize)]
pub struct TokenForm {
    #[serde(default)]
    grant_type: String,
    #[serde(default)]
    code: String,
    #[serde(default)]
    redirect_uri: String,
    #[serde(default)]
    client_id: String,
    #[serde(default)]
    code_verifier: String,
    #[serde(default)]
    refresh_token: String,
}

/// 同意された内容（コードの引き換えでもリフレッシュでも、ここに揃えてからトークンを作る）
struct Granted {
    username: String,
    client_name: String,
    client_id: String,
    role: String,
    scope: String,
    /// リフレッシュで置き換える、前のアクセストークンの ID
    replaces: Option<String>,
}

fn sha256_hex(value: &str) -> String {
    hex::encode(Sha256::digest(value.as_bytes()))
}

fn invalid_grant(description: &str) -> Response {
    oauth_error(StatusCode::BAD_REQUEST, "invalid_grant", description)
}

/// トークンの引き換えの失敗（応答は `token` で作る。`Response` は大きいので持ち回らない）
struct TokenError {
    status: StatusCode,
    error: &'static str,
    description: String,
}

impl TokenError {
    fn grant(description: &str) -> Self {
        Self {
            status: StatusCode::BAD_REQUEST,
            error: "invalid_grant",
            description: description.to_string(),
        }
    }

    fn server(description: &str) -> Self {
        Self {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            error: "server_error",
            description: description.to_string(),
        }
    }
}

/// `POST /oauth/token`（`authorization_code` と `refresh_token`）
pub async fn token(
    State(state): State<Arc<AppState>>,
    audit: AuditContext,
    Form(form): Form<TokenForm>,
) -> Response {
    let granted = match form.grant_type.as_str() {
        "authorization_code" => redeem_code(&state, &form).await,
        "refresh_token" => redeem_refresh(&state, &form).await,
        _ => {
            return oauth_error(
                StatusCode::BAD_REQUEST,
                "unsupported_grant_type",
                "Supported grant types: authorization_code, refresh_token",
            )
        }
    };
    match granted {
        Ok(granted) => issue_tokens(&state, &audit, granted, &form.grant_type).await,
        Err(e) => oauth_error(e.status, e.error, &e.description),
    }
}

/// 認可コードを検証する（1 回限り・PKCE・登録したリダイレクト先）
async fn redeem_code(state: &AppState, form: &TokenForm) -> Result<Granted, TokenError> {
    if form.code.is_empty() {
        return Err(TokenError::grant("code is required"));
    }
    let key = code_key(&form.code);
    let Ok(Some(raw)) = state.cache.get(&key).await else {
        return Err(TokenError::grant("The code is invalid or expired"));
    };
    // 1 回しか使えない: 数えて 1 回目だけ通す
    let first_use = state
        .cache
        .increment(&format!("{key}:used"), Some(CODE_TTL_SECS + 60))
        .await
        .map(|n| n == 1)
        .unwrap_or(false);
    let _ = state.cache.delete(&key).await;
    if !first_use {
        return Err(TokenError::grant("The code was already used"));
    }
    let Ok(grant) = serde_json::from_str::<Value>(&raw) else {
        return Err(TokenError::grant("The code is invalid"));
    };
    if grant["client_id"] != form.client_id.as_str()
        || grant["redirect_uri"] != form.redirect_uri.as_str()
    {
        return Err(TokenError::grant(
            "client_id or redirect_uri does not match the authorization request",
        ));
    }
    let verifier_ok = (43..=128).contains(&form.code_verifier.len())
        && URL_SAFE_NO_PAD.encode(Sha256::digest(form.code_verifier.as_bytes()))
            == grant["code_challenge"].as_str().unwrap_or_default();
    if !verifier_ok {
        return Err(TokenError::grant(
            "code_verifier does not match the code_challenge",
        ));
    }
    Ok(Granted {
        username: grant["username"].as_str().unwrap_or_default().to_string(),
        client_name: grant["client_name"]
            .as_str()
            .unwrap_or("OAuth client")
            .to_string(),
        client_id: form.client_id.clone(),
        role: grant["role"].as_str().unwrap_or("viewer").to_string(),
        scope: grant["scope"].as_str().unwrap_or_default().to_string(),
        replaces: None,
    })
}

/// リフレッシュトークンを検証する。使えるのは 1 回だけ（使うと新しいものに替わる）
async fn redeem_refresh(state: &AppState, form: &TokenForm) -> Result<Granted, TokenError> {
    if form.refresh_token.is_empty() {
        return Err(TokenError::grant("refresh_token is required"));
    }
    let taken = state
        .db
        .take_oauth_refresh_by_hash(&sha256_hex(&form.refresh_token))
        .await
        .map_err(|e| {
            tracing::warn!("OAuth refresh lookup failed: {e}");
            TokenError::server("Could not check the refresh token")
        })?;
    let Some(record) = taken else {
        return Err(TokenError::grant(
            "The refresh token is invalid or already used",
        ));
    };
    if record.client_hash != sha256_hex(&form.client_id) {
        return Err(TokenError::grant(
            "The refresh token was issued to another client",
        ));
    }
    let expired = aegis_core::time::parse_rfc3339(&record.expires_at)
        .map(|at| at < jiff::Timestamp::now())
        .unwrap_or(true);
    if expired {
        return Err(TokenError::grant("The refresh token has expired"));
    }
    // ロールが下げられていたら、それに合わせる
    let role = match state.db.get_user_by_username(&record.username).await {
        Ok(Some(user)) if !user.disabled => {
            if aegis_auth::role_level(&record.role) <= aegis_auth::role_level(&user.role) {
                record.role.clone()
            } else {
                user.role
            }
        }
        _ => return Err(TokenError::grant("The account is no longer active")),
    };
    Ok(Granted {
        username: record.username,
        client_name: record.client_name,
        client_id: form.client_id.clone(),
        role,
        scope: record.scope,
        replaces: Some(record.access_token_id),
    })
}

/// アクセストークン（API トークン）とリフレッシュトークンを発行して返す
async fn issue_tokens(
    state: &AppState,
    audit: &AuditContext,
    granted: Granted,
    grant_type: &str,
) -> Response {
    let name: String = format!("OAuth: {}", granted.client_name)
        .chars()
        .take(aegis_auth::api_tokens::MAX_NAME_CHARS)
        .collect();
    let existing = state
        .db
        .list_api_tokens(&granted.username)
        .await
        .unwrap_or_default();
    // 前のアクセストークンは置き換える。同じアプリを別の端末で許可したトークンには触れない
    if let Some(old) = &granted.replaces {
        let _ = state.db.delete_api_token(&granted.username, old).await;
    }
    // 期限が切れた OAuth のトークンは片付ける。上限の数に溜まらないように
    let now = jiff::Timestamp::now();
    for old in existing.iter().filter(|t| {
        t.name.starts_with("OAuth: ")
            && t.expires_at
                .as_deref()
                .and_then(|at| aegis_core::time::parse_rfc3339(at).ok())
                .is_some_and(|at| at < now)
    }) {
        let _ = state.db.delete_api_token(&granted.username, &old.id).await;
    }
    // それでも上限に近いときは、同じアプリの古いものから外す（新しい許可が通らないよりよい）
    if let Ok(mut left) = state.db.list_api_tokens(&granted.username).await {
        left.reverse();
        let mut over = (left.len() + 1).saturating_sub(aegis_auth::api_tokens::MAX_TOKENS_PER_USER);
        for old in left.iter().filter(|t| t.name == name) {
            if over == 0 {
                break;
            }
            let _ = state.db.delete_api_token(&granted.username, &old.id).await;
            over -= 1;
        }
    }

    let issued = issue_api_token(
        &*state.db,
        NewApiToken {
            username: granted.username.clone(),
            name,
            role: granted.role.clone(),
            expires_in_days: Some(ACCESS_TOKEN_DAYS),
        },
    )
    .await;
    let issued = match issued {
        Ok(issued) => issued,
        Err(e) => {
            tracing::warn!("OAuth token issue failed: {e}");
            return invalid_grant("The token could not be issued (is the account still active?)");
        }
    };

    // 取り直すためのトークン。使うたびに替わる
    let refresh = format!(
        "{}{}{}",
        REFRESH_TOKEN_PREFIX,
        uuid::Uuid::new_v4().simple(),
        uuid::Uuid::new_v4().simple()
    );
    let created = now.to_rfc3339_secs();
    let expires = (now + aegis_core::time::days(i64::from(REFRESH_TOKEN_DAYS))).to_rfc3339_secs();
    let _ = state.db.delete_expired_oauth_refresh(&created).await;
    let stored = state
        .db
        .insert_oauth_refresh(&aegis_db::OAuthRefreshRecord {
            id: uuid::Uuid::new_v4().to_string(),
            token_hash: sha256_hex(&refresh),
            username: granted.username.clone(),
            client_hash: sha256_hex(&granted.client_id),
            client_name: granted.client_name.clone(),
            scope: granted.scope.clone(),
            role: granted.role.clone(),
            access_token_id: issued.record.id.clone(),
            created_at: created,
            expires_at: expires,
        })
        .await;
    if let Err(e) = stored {
        tracing::warn!("OAuth refresh token could not be stored: {e}");
        let _ = state
            .db
            .delete_api_token(&granted.username, &issued.record.id)
            .await;
        return oauth_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "server_error",
            "Could not issue the token",
        );
    }

    state
        .audit
        .record(
            audit
                .event(AuditAction::UserUpdate, AuditOutcome::Success)
                .with_resource("user", Some(granted.username.clone()))
                .with_details(json!({
                    "action": if grant_type == "refresh_token" { "oauth.refresh" } else { "oauth.token" },
                    "token_id": issued.record.id,
                    "client": granted.client_name,
                    "role": issued.record.role,
                    "expires_at": issued.record.expires_at,
                })),
        )
        .await;
    json_no_store(
        StatusCode::OK,
        json!({
            "access_token": issued.token,
            "token_type": "Bearer",
            "expires_in": i64::from(ACCESS_TOKEN_DAYS) * 86_400,
            "refresh_token": refresh,
            "scope": granted.scope,
        }),
    )
}
