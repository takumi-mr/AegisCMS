//! OAuth 2.1 認可サーバー（`oauth.rs`）の結合テスト（実際のルーターを通す）

use std::sync::Arc;

use aegis_auth::users::{create_user, NewUser};
use aegis_runtime::PluginSupervisor;
use axum::body::Body;
use axum::http::{header, Request, StatusCode};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use tower::ServiceExt;

use crate::config::AegisConfig;
use crate::state::AppState;

const REDIRECT: &str = "https://app.example/callback";

async fn state() -> Arc<AppState> {
    let state = Arc::new(
        crate::test_support::test_app_state(
            AegisConfig::default(),
            PluginSupervisor::new(),
            Vec::new(),
        )
        .await,
    );
    for (username, role) in [("alice", "editor"), ("vera", "viewer")] {
        create_user(
            state.db.as_ref(),
            NewUser {
                username: username.to_string(),
                password: "correct-horse-battery".to_string(),
                role: role.to_string(),
                email: None,
                display_name: None,
            },
        )
        .await
        .unwrap();
    }
    state
}

struct Reply {
    status: StatusCode,
    headers: axum::http::HeaderMap,
    text: String,
}

impl Reply {
    fn json(&self) -> Value {
        serde_json::from_str(&self.text).unwrap_or(Value::Null)
    }
}

async fn send(
    state: &Arc<AppState>,
    method: &str,
    path: &str,
    content_type: Option<&str>,
    body: String,
    extra: &[(&str, &str)],
) -> Reply {
    let mut request = Request::builder()
        .method(method)
        .uri(path)
        .header(header::HOST, "cms.test")
        .header("x-forwarded-proto", "https");
    if let Some(content_type) = content_type {
        request = request.header(header::CONTENT_TYPE, content_type);
    }
    for (k, v) in extra {
        request = request.header(*k, *v);
    }
    let response = crate::app::router(&state.config, state.clone())
        .oneshot(request.body(Body::from(body)).unwrap())
        .await
        .unwrap();
    let status = response.status();
    let headers = response.headers().clone();
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    Reply {
        status,
        headers,
        text: String::from_utf8_lossy(&bytes).into_owned(),
    }
}

fn form(pairs: &[(&str, &str)]) -> String {
    url::form_urlencoded::Serializer::new(String::new())
        .extend_pairs(pairs.iter().copied())
        .finish()
}

async fn register(state: &Arc<AppState>, redirect: &str) -> String {
    let reply = send(
        state,
        "POST",
        "/oauth/register",
        Some("application/json"),
        json!({ "client_name": "Test <Agent>", "redirect_uris": [redirect],
                "token_endpoint_auth_method": "none" })
        .to_string(),
        &[],
    )
    .await;
    assert_eq!(reply.status, StatusCode::CREATED, "{}", reply.text);
    reply.json()["client_id"].as_str().unwrap().to_string()
}

fn verifier() -> String {
    "v".repeat(50)
}

fn challenge() -> String {
    URL_SAFE_NO_PAD.encode(Sha256::digest(verifier().as_bytes()))
}

fn authorize_query(client_id: &str, redirect: &str, scope: &str) -> String {
    form(&[
        ("response_type", "code"),
        ("client_id", client_id),
        ("redirect_uri", redirect),
        ("state", "xyz"),
        ("code_challenge", &challenge()),
        ("code_challenge_method", "S256"),
        ("scope", scope),
    ])
}

/// 許可の画面に、ログインして Allow を押す
async fn allow(
    state: &Arc<AppState>,
    client_id: &str,
    redirect: &str,
    scope: &str,
    username: &str,
    password: &str,
) -> Reply {
    allow_with(state, client_id, redirect, scope, username, password, &[]).await
}

/// `extra` は画面のチェックの値（`grants`・`grant_draft`・`grant_publish`）
async fn allow_with(
    state: &Arc<AppState>,
    client_id: &str,
    redirect: &str,
    scope: &str,
    username: &str,
    password: &str,
    extra: &[(&str, &str)],
) -> Reply {
    let mut pairs: Vec<(String, String)> =
        url::form_urlencoded::parse(authorize_query(client_id, redirect, scope).as_bytes())
            .into_owned()
            .collect();
    pairs.push(("action".into(), "allow".into()));
    pairs.push(("username".into(), username.into()));
    pairs.push(("password".into(), password.into()));
    for (k, v) in extra {
        pairs.push((k.to_string(), v.to_string()));
    }
    let borrowed: Vec<(&str, &str)> = pairs
        .iter()
        .map(|(k, v)| (k.as_str(), v.as_str()))
        .collect();
    send(
        state,
        "POST",
        "/oauth/authorize",
        Some("application/x-www-form-urlencoded"),
        form(&borrowed),
        &[],
    )
    .await
}

/// 戻り先の画面（`meta refresh`）から、`code` を取り出す
fn code_from(reply: &Reply) -> String {
    assert_eq!(reply.status, StatusCode::OK, "{}", reply.text);
    let marker = "code=";
    let start = reply.text.find(marker).expect("a code in the redirect") + marker.len();
    reply.text[start..]
        .chars()
        .take_while(|c| c.is_ascii_alphanumeric())
        .collect()
}

async fn exchange(
    state: &Arc<AppState>,
    client_id: &str,
    redirect: &str,
    code: &str,
    verifier: &str,
) -> Reply {
    send(
        state,
        "POST",
        "/oauth/token",
        Some("application/x-www-form-urlencoded"),
        form(&[
            ("grant_type", "authorization_code"),
            ("code", code),
            ("redirect_uri", redirect),
            ("client_id", client_id),
            ("code_verifier", verifier),
        ]),
        &[],
    )
    .await
}

#[tokio::test]
async fn test_metadata_and_the_mcp_challenge() {
    let state = state().await;
    let resource = send(
        &state,
        "GET",
        "/.well-known/oauth-protected-resource",
        None,
        String::new(),
        &[],
    )
    .await;
    assert_eq!(resource.status, StatusCode::OK);
    assert_eq!(resource.json()["resource"], "https://cms.test/mcp");
    assert_eq!(
        resource.json()["authorization_servers"][0],
        "https://cms.test"
    );
    assert_eq!(resource.headers[header::ACCESS_CONTROL_ALLOW_ORIGIN], "*");

    let server = send(
        &state,
        "GET",
        "/.well-known/oauth-authorization-server",
        None,
        String::new(),
        &[],
    )
    .await;
    let meta = server.json();
    assert_eq!(meta["issuer"], "https://cms.test");
    assert_eq!(meta["token_endpoint"], "https://cms.test/oauth/token");
    assert_eq!(
        meta["registration_endpoint"],
        "https://cms.test/oauth/register"
    );
    assert_eq!(meta["code_challenge_methods_supported"], json!(["S256"]));

    // /mcp は、認可サーバーの場所を 401 で案内する
    let unauthorized = send(
        &state,
        "POST",
        "/mcp",
        Some("application/json"),
        json!({ "jsonrpc": "2.0", "id": 1, "method": "ping" }).to_string(),
        &[],
    )
    .await;
    assert_eq!(unauthorized.status, StatusCode::UNAUTHORIZED);
    assert_eq!(
        unauthorized.headers[header::WWW_AUTHENTICATE],
        "Bearer resource_metadata=\"https://cms.test/.well-known/oauth-protected-resource\""
    );
}

#[tokio::test]
async fn test_registration_refuses_unsafe_redirects() {
    let state = state().await;
    for bad in [
        "javascript:alert(1)",
        "http://evil.example/cb",
        "https://app.example/cb#frag",
        "not a url",
    ] {
        let reply = send(
            &state,
            "POST",
            "/oauth/register",
            Some("application/json"),
            json!({ "redirect_uris": [bad] }).to_string(),
            &[],
        )
        .await;
        assert_eq!(
            reply.status,
            StatusCode::BAD_REQUEST,
            "{bad}: {}",
            reply.text
        );
        assert_eq!(reply.json()["error"], "invalid_redirect_uri");
    }
    // 秘密を持つクライアントは扱わない
    let reply = send(
        &state,
        "POST",
        "/oauth/register",
        Some("application/json"),
        json!({ "redirect_uris": [REDIRECT], "token_endpoint_auth_method": "client_secret_basic" })
            .to_string(),
        &[],
    )
    .await;
    assert_eq!(reply.status, StatusCode::BAD_REQUEST);
    // ループバックの http とアプリ用のスキームは登録できる
    for ok in [
        "http://localhost:8123/cb",
        "http://127.0.0.1/cb",
        "cursor://app/oauth/callback",
    ] {
        register(&state, ok).await;
    }
}

#[tokio::test]
async fn test_full_flow_issues_a_scoped_token_that_works_on_mcp() {
    let state = state().await;
    let client_id = register(&state, REDIRECT).await;

    // 画面にはアプリの名前（エスケープ済み）と、戻り先のホストが出る
    let page = send(
        &state,
        "GET",
        &format!(
            "/oauth/authorize?{}",
            authorize_query(&client_id, REDIRECT, "content:read content:draft")
        ),
        None,
        String::new(),
        &[],
    )
    .await;
    assert_eq!(page.status, StatusCode::OK, "{}", page.text);
    assert!(page.text.contains("Test &lt;Agent&gt;") && !page.text.contains("<Agent>"));
    assert!(page.text.contains("app.example"));
    assert!(!page.text.contains("<script"));

    // 間違ったパスワードは断られ、パスワードは画面に戻らない
    let wrong = allow(&state, &client_id, REDIRECT, "", "alice", "nope-nope-nope").await;
    assert_eq!(wrong.status, StatusCode::UNAUTHORIZED);
    assert!(wrong.text.contains("Invalid username or password"));
    assert!(!wrong.text.contains("nope-nope-nope"));

    let approved = allow(
        &state,
        &client_id,
        REDIRECT,
        "",
        "alice",
        "correct-horse-battery",
    )
    .await;
    assert!(approved.text.contains("state=xyz"));
    assert!(approved.text.contains("Continue"));
    let code = code_from(&approved);

    // 検証子が違えば引き換えられず、コードも使い切りになる
    let bad = exchange(&state, &client_id, REDIRECT, &code, &"w".repeat(50)).await;
    assert_eq!(bad.status, StatusCode::BAD_REQUEST);
    assert_eq!(bad.json()["error"], "invalid_grant");
    let again = exchange(&state, &client_id, REDIRECT, &code, &verifier()).await;
    assert_eq!(again.json()["error"], "invalid_grant");

    // 新しいコードで引き換える
    let approved = allow(
        &state,
        &client_id,
        REDIRECT,
        "",
        "alice",
        "correct-horse-battery",
    )
    .await;
    let code = code_from(&approved);
    let token = exchange(&state, &client_id, REDIRECT, &code, &verifier()).await;
    assert_eq!(token.status, StatusCode::OK, "{}", token.text);
    assert_eq!(token.headers[header::CACHE_CONTROL], "no-store");
    let access_token = token.json()["access_token"].as_str().unwrap().to_string();
    assert!(access_token.starts_with("aegis_pat_"));
    assert_eq!(token.json()["token_type"], "Bearer");

    // editor でも、トークンは contributor まで（公開はできない）
    let me = send(
        &state,
        "GET",
        "/api/v1/auth/session",
        None,
        String::new(),
        &[("authorization", &format!("Bearer {access_token}"))],
    )
    .await;
    assert_eq!(me.json()["user_id"], "alice");
    assert_eq!(me.json()["role"], "contributor");

    // MCP で下書きを作れる
    let created = send(
        &state,
        "POST",
        "/mcp",
        Some("application/json"),
        json!({ "jsonrpc": "2.0", "id": 1, "method": "tools/call",
                "params": { "name": "create_draft",
                            "arguments": { "content_type": "page", "title": "By OAuth", "slug": "by-oauth" } } })
        .to_string(),
        &[("authorization", &format!("Bearer {access_token}"))],
    )
    .await;
    assert_eq!(
        created.json()["result"]["isError"],
        false,
        "{}",
        created.text
    );

    // 同じアプリで入れ直すと、前のトークンは置き換わる
    let approved = allow(
        &state,
        &client_id,
        REDIRECT,
        "",
        "alice",
        "correct-horse-battery",
    )
    .await;
    let second = exchange(
        &state,
        &client_id,
        REDIRECT,
        &code_from(&approved),
        &verifier(),
    )
    .await;
    assert_eq!(second.status, StatusCode::OK);
    let listed = state.db.list_api_tokens("alice").await.unwrap();
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].name, "OAuth: Test <Agent>");
    let old = send(
        &state,
        "GET",
        "/api/v1/auth/session",
        None,
        String::new(),
        &[("authorization", &format!("Bearer {access_token}"))],
    )
    .await;
    assert_eq!(old.status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn test_the_pages_are_not_renamed_by_the_namespace() {
    // 既定の `[server].namespace`（cms）は HTML の `aegis-` / `aegis_` を書き換える。
    // 画面が返す client_id（`aegis_oc_…`）などが変わると、ブラウザから送った許可が通らなくなる
    let state = state().await;
    assert_ne!(state.config.server.namespace(), "aegis");
    let client_id = register(&state, REDIRECT).await;
    assert!(client_id.starts_with("aegis_oc_"));

    let page = send(
        &state,
        "GET",
        &format!(
            "/oauth/authorize?{}",
            authorize_query(&client_id, REDIRECT, "")
        ),
        None,
        String::new(),
        &[],
    )
    .await;
    assert!(
        page.text.contains(&format!("value=\"{client_id}\"")),
        "{}",
        page.text
    );

    // 画面のとおりに送ると通る（ブラウザと同じ）
    let marker = "name=\"client_id\" value=\"";
    let from = page.text.find(marker).unwrap() + marker.len();
    let echoed: String = page.text[from..].split('"').next().unwrap().to_string();
    let approved = allow(
        &state,
        &echoed,
        REDIRECT,
        "",
        "alice",
        "correct-horse-battery",
    )
    .await;
    assert!(code_from(&approved).len() > 20);
}

#[tokio::test]
async fn test_scope_and_role_limit_the_token() {
    let state = state().await;
    let client_id = register(&state, REDIRECT).await;

    // 読み取りだけを求めたら viewer
    let approved = allow(
        &state,
        &client_id,
        REDIRECT,
        "content:read",
        "alice",
        "correct-horse-battery",
    )
    .await;
    let token = exchange(
        &state,
        &client_id,
        REDIRECT,
        &code_from(&approved),
        &verifier(),
    )
    .await;
    assert_eq!(token.json()["scope"], "content:read");
    let tok = token.json()["access_token"].as_str().unwrap().to_string();
    let me = send(
        &state,
        "GET",
        "/api/v1/auth/session",
        None,
        String::new(),
        &[("authorization", &format!("Bearer {tok}"))],
    )
    .await;
    assert_eq!(me.json()["role"], "viewer");

    // 下書きを求めても、持ち主のロールを超えない（viewer のまま）
    let approved = allow(
        &state,
        &client_id,
        REDIRECT,
        "content:draft",
        "vera",
        "correct-horse-battery",
    )
    .await;
    let token = exchange(
        &state,
        &client_id,
        REDIRECT,
        &code_from(&approved),
        &verifier(),
    )
    .await;
    let tok = token.json()["access_token"].as_str().unwrap().to_string();
    let me = send(
        &state,
        "GET",
        "/api/v1/auth/session",
        None,
        String::new(),
        &[("authorization", &format!("Bearer {tok}"))],
    )
    .await;
    assert_eq!(me.json()["role"], "viewer");
}

#[tokio::test]
async fn test_refuses_unknown_clients_and_other_redirects() {
    let state = state().await;
    let client_id = register(&state, REDIRECT).await;

    // 登録のない client_id・書き換えた client_id は、どこにも戻さずエラー画面にする
    let tampered = client_id.replacen("aegis_oc_", "aegis_oc_A", 1);
    for id in ["nope", tampered.as_str()] {
        let page = send(
            &state,
            "GET",
            &format!("/oauth/authorize?{}", authorize_query(id, REDIRECT, "")),
            None,
            String::new(),
            &[],
        )
        .await;
        assert_eq!(page.status, StatusCode::BAD_REQUEST, "{id}");
        assert!(page.headers.get(header::LOCATION).is_none());
    }
    // 登録と違うリダイレクト先
    let page = send(
        &state,
        "GET",
        &format!(
            "/oauth/authorize?{}",
            authorize_query(&client_id, "https://evil.example/cb", "")
        ),
        None,
        String::new(),
        &[],
    )
    .await;
    assert_eq!(page.status, StatusCode::BAD_REQUEST);
    assert!(page.headers.get(header::LOCATION).is_none());

    // PKCE が無いときは、登録済みのリダイレクト先へエラーで戻す
    let page = send(
        &state,
        "GET",
        &format!(
            "/oauth/authorize?{}",
            form(&[
                ("response_type", "code"),
                ("client_id", &client_id),
                ("redirect_uri", REDIRECT),
                ("state", "s")
            ])
        ),
        None,
        String::new(),
        &[],
    )
    .await;
    assert_eq!(page.status, StatusCode::FOUND);
    let location = page.headers[header::LOCATION].to_str().unwrap();
    assert!(location.starts_with(REDIRECT) && location.contains("error=invalid_request"));
}

#[tokio::test]
async fn test_deny_and_loopback_ports() {
    let state = state().await;
    let client_id = register(&state, "http://localhost:3000/cb").await;

    // ループバックはポートが違ってもよい
    let other_port = "http://localhost:51234/cb";
    let approved = allow(
        &state,
        &client_id,
        other_port,
        "",
        "alice",
        "correct-horse-battery",
    )
    .await;
    assert!(code_from(&approved).len() > 20);

    // 拒否は access_denied で戻る（資格情報は要らない）
    let mut pairs: Vec<(String, String)> =
        url::form_urlencoded::parse(authorize_query(&client_id, other_port, "").as_bytes())
            .into_owned()
            .collect();
    pairs.push(("action".into(), "deny".into()));
    let borrowed: Vec<(&str, &str)> = pairs
        .iter()
        .map(|(k, v)| (k.as_str(), v.as_str()))
        .collect();
    let denied = send(
        &state,
        "POST",
        "/oauth/authorize",
        Some("application/x-www-form-urlencoded"),
        form(&borrowed),
        &[],
    )
    .await;
    assert!(denied.text.contains("error=access_denied") && denied.text.contains("state=xyz"));
}

#[tokio::test]
async fn test_publish_scope_is_off_until_the_user_ticks_it() {
    let state = state().await;
    let client_id = register(&state, REDIRECT).await;
    let scope = "content:read content:draft content:publish";

    // 画面では、下書きはオン、公開はオフ（警告つき）で出る
    let page = send(
        &state,
        "GET",
        &format!(
            "/oauth/authorize?{}",
            authorize_query(&client_id, REDIRECT, scope)
        ),
        None,
        String::new(),
        &[],
    )
    .await;
    assert!(page
        .text
        .contains("name=\"grant_draft\" value=\"on\" checked"));
    assert!(page.text.contains("name=\"grant_publish\" value=\"on\">"));
    assert!(page.text.contains("class=\"warn\""));

    let role_after = |reply: Reply| {
        let state = state.clone();
        let client_id = client_id.clone();
        async move {
            let token = exchange(
                &state,
                &client_id,
                REDIRECT,
                &code_from(&reply),
                &verifier(),
            )
            .await;
            let scope = token.json()["scope"].as_str().unwrap().to_string();
            let tok = token.json()["access_token"].as_str().unwrap().to_string();
            let me = send(
                &state,
                "GET",
                "/api/v1/auth/session",
                None,
                String::new(),
                &[("authorization", &format!("Bearer {tok}"))],
            )
            .await;
            (scope, me.json()["role"].as_str().unwrap().to_string())
        }
    };

    // 公開にチェックを入れなければ、求められても渡さない
    let reply = allow_with(
        &state,
        &client_id,
        REDIRECT,
        scope,
        "alice",
        "correct-horse-battery",
        &[("grants", "1"), ("grant_draft", "on")],
    )
    .await;
    assert_eq!(
        role_after(reply).await,
        (
            "content:read content:draft".to_string(),
            "contributor".to_string()
        )
    );

    // チェックを入れると editor まで（持ち主のロールの範囲で）
    let reply = allow_with(
        &state,
        &client_id,
        REDIRECT,
        scope,
        "alice",
        "correct-horse-battery",
        &[
            ("grants", "1"),
            ("grant_draft", "on"),
            ("grant_publish", "on"),
        ],
    )
    .await;
    assert_eq!(
        role_after(reply).await,
        (
            "content:read content:draft content:publish".to_string(),
            "editor".to_string()
        )
    );

    // viewer の持ち主は、公開を許可しても viewer のまま
    let reply = allow_with(
        &state,
        &client_id,
        REDIRECT,
        scope,
        "vera",
        "correct-horse-battery",
        &[
            ("grants", "1"),
            ("grant_draft", "on"),
            ("grant_publish", "on"),
        ],
    )
    .await;
    assert_eq!(role_after(reply).await.1, "viewer");

    // 求めていない範囲は、チェックを偽装しても渡さない
    let reply = allow_with(
        &state,
        &client_id,
        REDIRECT,
        "content:read",
        "alice",
        "correct-horse-battery",
        &[
            ("grants", "1"),
            ("grant_draft", "on"),
            ("grant_publish", "on"),
        ],
    )
    .await;
    assert_eq!(
        role_after(reply).await,
        ("content:read".to_string(), "viewer".to_string())
    );
}
