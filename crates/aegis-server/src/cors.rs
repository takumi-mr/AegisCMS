//! CORS（`[server].cors_origins`）。docs/configuration.md の 4.3
//!
//! 既定は「公開の読み取りだけ全オリジン、それ以外は同一オリジンだけ」。
//!
//! - **公開の読み取り**（`GET` / `HEAD`。`/api/` の外の公開ページ・静的ファイルと、`/api/v1/public/`・
//!   `/api/v1/search`・`/api/v1/health`）には `Access-Control-Allow-Origin: *` を返す（ヘッドレスのクライアントと、
//!   別のドメインの CDN から読むフォントのため）。
//! - **それ以外**（認証が要る API、書き込み）は、`cors_origins` に書いたオリジンにだけ CORS を許す。
//!   許可するオリジンには、そのオリジンの値を返し（`Vary: Origin`）、メソッドは `GET`〜`DELETE`、ヘッダーは
//!   プリフライトが求めたものを許す。`"*"` を書くと、前の動き（全オリジンに全メソッド）に戻る。
//! - 許可されないオリジンには CORS のヘッダーを返さない（ブラウザーが応答の読み取りを止める）。
//!
//! 認証は Cookie ではなく Bearer トークンで、`Access-Control-Allow-Credentials` は返さない。

use std::sync::Arc;

use axum::extract::{Request, State};
use axum::http::{header, HeaderMap, HeaderName, HeaderValue, Method, StatusCode};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};

use aegis_core::{AegisError, Result};

/// `[server].cors_origins` を検査して正規化する（小文字・末尾の `/` なし）。
/// `*` か、`http(s)://host[:port]`（パス・クエリなし）だけ
pub fn normalize_origins(origins: &[String]) -> Result<Vec<String>> {
    let mut out = Vec::new();
    for raw in origins {
        let o = raw.trim().trim_end_matches('/').to_ascii_lowercase();
        let valid = o == "*" || {
            let rest = o
                .strip_prefix("https://")
                .or_else(|| o.strip_prefix("http://"));
            rest.is_some_and(|r| {
                !r.is_empty()
                    && r.bytes().all(|b| {
                        b.is_ascii_alphanumeric() || matches!(b, b'-' | b'.' | b':' | b'[' | b']')
                    })
            })
        };
        if !valid {
            return Err(AegisError::Config(format!(
                "[server].cors_origins {raw:?} is invalid: use \"*\" or an origin like \"https://admin.example.com\" (scheme, host and optional port; no path)"
            )));
        }
        if !out.contains(&o) {
            out.push(o);
        }
    }
    Ok(out)
}

/// 起動時に決めた方針
#[derive(Debug, Clone, Default)]
pub struct CorsPolicy {
    /// `"*"` が書いてある（前の動き）
    allow_any: bool,
    /// 許可するオリジン（小文字）
    origins: Vec<String>,
}

impl CorsPolicy {
    pub fn new(origins: &[String]) -> Self {
        let origins = normalize_origins(origins).unwrap_or_default();
        Self {
            allow_any: origins.iter().any(|o| o == "*"),
            origins,
        }
    }
}

/// 公開の読み取りか（認証なしで誰にでも見せるもの）
fn is_public_read(path: &str, method: &Method) -> bool {
    if !matches!(*method, Method::GET | Method::HEAD | Method::OPTIONS) {
        return false;
    }
    if !path.starts_with("/api/") {
        return true;
    }
    ["/api/v1/public/", "/api/v1/search", "/api/v1/health"]
        .iter()
        .any(|p| path.starts_with(p))
}

enum Grant {
    /// 全オリジンに許す（公開の読み取り）
    Public,
    /// このオリジンに許す（設定したオリジン）
    Trusted,
    None,
}

fn grant(policy: &CorsPolicy, origin: &str, path: &str, method: &Method) -> Grant {
    if policy.allow_any
        || policy
            .origins
            .iter()
            .any(|o| o == &origin.to_ascii_lowercase())
    {
        return Grant::Trusted;
    }
    if is_public_read(path, method) {
        return Grant::Public;
    }
    Grant::None
}

fn set(headers: &mut HeaderMap, name: HeaderName, value: &str) {
    if let Ok(v) = HeaderValue::from_str(value) {
        headers.insert(name, v);
    }
}

fn allow_origin(headers: &mut HeaderMap, grant: &Grant, policy: &CorsPolicy, origin: &str) {
    match grant {
        Grant::Public => set(headers, header::ACCESS_CONTROL_ALLOW_ORIGIN, "*"),
        Grant::Trusted => {
            // `"*"` を書いたときは前の動き（`*`）。オリジンを書いたときは、そのオリジンだけ返す
            if policy.allow_any {
                set(headers, header::ACCESS_CONTROL_ALLOW_ORIGIN, "*");
            } else {
                set(headers, header::ACCESS_CONTROL_ALLOW_ORIGIN, origin);
                headers.append(header::VARY, HeaderValue::from_static("Origin"));
            }
        }
        Grant::None => {}
    }
}

pub async fn cors_middleware(
    State(policy): State<Arc<CorsPolicy>>,
    request: Request,
    next: Next,
) -> Response {
    let Some(origin) = request
        .headers()
        .get(header::ORIGIN)
        .and_then(|v| v.to_str().ok())
        .map(str::to_string)
    else {
        return next.run(request).await;
    };
    let path = request.uri().path().to_string();

    // プリフライト: 実際に送りたいメソッドで判断する
    if request.method() == Method::OPTIONS
        && request
            .headers()
            .contains_key(header::ACCESS_CONTROL_REQUEST_METHOD)
    {
        let wanted = request
            .headers()
            .get(header::ACCESS_CONTROL_REQUEST_METHOD)
            .and_then(|v| v.to_str().ok())
            .and_then(|m| Method::from_bytes(m.as_bytes()).ok())
            .unwrap_or(Method::GET);
        let requested_headers = request
            .headers()
            .get(header::ACCESS_CONTROL_REQUEST_HEADERS)
            .and_then(|v| v.to_str().ok())
            .map(str::to_string);
        let granted = grant(&policy, &origin, &path, &wanted);
        let mut response = StatusCode::NO_CONTENT.into_response();
        let headers = response.headers_mut();
        match granted {
            Grant::None => {}
            Grant::Public => {
                allow_origin(headers, &granted, &policy, &origin);
                set(
                    headers,
                    header::ACCESS_CONTROL_ALLOW_METHODS,
                    "GET, HEAD, OPTIONS",
                );
                set(
                    headers,
                    header::ACCESS_CONTROL_ALLOW_HEADERS,
                    "accept, content-type",
                );
                set(headers, header::ACCESS_CONTROL_MAX_AGE, "600");
            }
            Grant::Trusted => {
                allow_origin(headers, &granted, &policy, &origin);
                set(
                    headers,
                    header::ACCESS_CONTROL_ALLOW_METHODS,
                    "GET, HEAD, POST, PUT, PATCH, DELETE, OPTIONS",
                );
                if let Some(h) = requested_headers {
                    set(headers, header::ACCESS_CONTROL_ALLOW_HEADERS, &h);
                }
                set(headers, header::ACCESS_CONTROL_MAX_AGE, "600");
            }
        }
        return response;
    }

    let granted = grant(&policy, &origin, &path, request.method());
    let mut response = next.run(request).await;
    allow_origin(response.headers_mut(), &granted, &policy, &origin);
    response
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::Request;

    #[test]
    fn origins_are_validated_and_normalized() {
        let ok = normalize_origins(&[
            "HTTPS://Admin.Example.com/".into(),
            "http://localhost:5173".into(),
            "https://admin.example.com".into(),
            "*".into(),
        ])
        .unwrap();
        assert_eq!(
            ok,
            vec!["https://admin.example.com", "http://localhost:5173", "*"]
        );
        for bad in [
            "admin.example.com",
            "https://a.example/path",
            "ftp://a.example",
            "https://",
            "https://a b",
        ] {
            assert!(normalize_origins(&[bad.to_string()]).is_err(), "{bad}");
        }
    }

    #[test]
    fn public_reads_are_only_get_head_options_of_public_paths() {
        assert!(is_public_read("/", &Method::GET));
        assert!(is_public_read("/articles/x", &Method::GET));
        assert!(is_public_read("/uploads/a.png", &Method::GET));
        assert!(is_public_read(
            "/api/v1/public/contents/article",
            &Method::GET
        ));
        assert!(is_public_read("/api/v1/search", &Method::GET));
        assert!(!is_public_read("/api/v1/contents/article", &Method::GET));
        assert!(!is_public_read("/api/v1/users", &Method::GET));
        assert!(!is_public_read(
            "/api/v1/public/contents/article",
            &Method::POST
        ));
        assert!(!is_public_read("/contact/default", &Method::POST));
    }

    use crate::config::AegisConfig;
    use crate::test_support::test_state_with_config;
    use axum::body::Body;
    use tower::ServiceExt;

    async fn app(origins: &[&str]) -> axum::Router {
        let mut config = AegisConfig::default();
        config.server.cors_origins = origins.iter().map(|o| o.to_string()).collect();
        let state = test_state_with_config(config.clone(), Default::default()).await;
        crate::app::router(&config, state)
    }

    /// （ステータス, `Access-Control-Allow-Origin`, `Access-Control-Allow-Methods`）
    async fn call(
        app: &axum::Router,
        method: &str,
        path: &str,
        origin: &str,
        preflight: Option<&str>,
    ) -> (StatusCode, String, String) {
        let mut req = Request::builder()
            .method(method)
            .uri(path)
            .header("origin", origin);
        if let Some(wanted) = preflight {
            req = req.header("access-control-request-method", wanted).header(
                "access-control-request-headers",
                "authorization, content-type",
            );
        }
        let res = app
            .clone()
            .oneshot(req.body(Body::empty()).unwrap())
            .await
            .unwrap();
        let get = |n: &str| {
            res.headers()
                .get(n)
                .and_then(|v| v.to_str().ok())
                .unwrap_or_default()
                .to_string()
        };
        (
            res.status(),
            get("access-control-allow-origin"),
            get("access-control-allow-methods"),
        )
    }

    #[tokio::test]
    async fn by_default_only_public_reads_are_open_to_every_origin() {
        let app = app(&[]).await;
        let other = "https://other.example";
        // 公開の読み取り: `*`
        assert_eq!(
            call(&app, "GET", "/api/v1/health", other, None).await.1,
            "*"
        );
        assert_eq!(call(&app, "GET", "/", other, None).await.1, "*");
        // 認証が要る API・書き込み: CORS なし（応答は読めない）
        assert_eq!(
            call(&app, "GET", "/api/v1/schemas", other, None).await.1,
            ""
        );
        assert_eq!(
            call(&app, "POST", "/api/v1/auth/session", other, None)
                .await
                .1,
            ""
        );
        // プリフライト: 公開の GET だけ通り、書き込みは何も返さない
        let (s, o, m) = call(&app, "OPTIONS", "/api/v1/search", other, Some("GET")).await;
        assert_eq!(
            (s, o.as_str(), m.as_str()),
            (StatusCode::NO_CONTENT, "*", "GET, HEAD, OPTIONS")
        );
        let (_, o, m) = call(
            &app,
            "OPTIONS",
            "/api/v1/contents/article",
            other,
            Some("POST"),
        )
        .await;
        assert!(o.is_empty() && m.is_empty());
        let (_, o, _) = call(&app, "OPTIONS", "/api/v1/search", other, Some("DELETE")).await;
        assert!(o.is_empty());
        // Origin の無い（同一オリジンの）リクエストには何も付けない
        let res = app
            .clone()
            .oneshot(Request::get("/api/v1/health").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert!(!res.headers().contains_key("access-control-allow-origin"));
    }

    #[tokio::test]
    async fn a_listed_origin_gets_the_api_and_only_that_origin() {
        let app = app(&["https://admin.example.com"]).await;
        let (_, o, _) = call(
            &app,
            "GET",
            "/api/v1/schemas",
            "https://admin.example.com",
            None,
        )
        .await;
        assert_eq!(o, "https://admin.example.com");
        let (s, o, m) = call(
            &app,
            "OPTIONS",
            "/api/v1/contents/article",
            "https://admin.example.com",
            Some("PUT"),
        )
        .await;
        assert_eq!(s, StatusCode::NO_CONTENT);
        assert_eq!(o, "https://admin.example.com");
        assert!(m.contains("PUT") && m.contains("DELETE"));
        // 別のオリジンは、認証つきの API には届かない
        let (_, o, _) = call(&app, "GET", "/api/v1/schemas", "https://evil.example", None).await;
        assert_eq!(o, "");
    }

    #[tokio::test]
    async fn a_star_restores_the_old_open_policy() {
        let app = app(&["*"]).await;
        let (_, o, _) = call(
            &app,
            "GET",
            "/api/v1/schemas",
            "https://anything.example",
            None,
        )
        .await;
        assert_eq!(o, "*");
    }
}
