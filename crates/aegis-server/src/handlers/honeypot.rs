//! WordPress スキャナー等の悪意あるボット走査を検知し、即時 IP ロックをかけるハニーポットハンドラー

use axum::{
    extract::{Request, State},
    http::StatusCode,
    response::{IntoResponse, Response},
};
use std::sync::Arc;

use crate::audit::ClientIp;
use crate::state::AppState;

/// 悪意ある WordPress 走査リクエスト（/wp-login.php, /xmlrpc.php 等）を捕捉し、
/// アクセス元 IP を即時ロックして 404 を返す。
pub async fn honeypot_trap(
    State(state): State<Arc<AppState>>,
    ClientIp(client_ip): ClientIp,
    req: Request,
) -> Response {
    let path = req.uri().path().to_string();
    if let Some(ref ip) = client_ip {
        // 1 時間（3600 秒）ロック
        let lockout_secs = 3600;
        if let Err(e) = state.login_limiter.lock_ip(ip, Some(lockout_secs)).await {
            tracing::warn!("Failed to lock IP in honeypot: {}", e);
        } else {
            tracing::warn!(
                "[SECURITY TRAP] Trapped scanner bot from IP {} accessing '{}' - locked for {}s",
                ip,
                path,
                lockout_secs
            );
        }
    } else {
        tracing::warn!(
            "[SECURITY TRAP] Trapped scanner bot accessing '{}' (unknown IP)",
            path
        );
    }

    // 攻撃者には WordPress が存在しないように 404 Not Found を返す
    StatusCode::NOT_FOUND.into_response()
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::Request;
    use tower::ServiceExt;

    #[tokio::test]
    async fn test_honeypot_traps_and_locks_ip() {
        let supervisor = aegis_runtime::PluginSupervisor::new();
        let state = crate::test_support::test_state(supervisor).await;
        let app = crate::app::router(&state.config, state.clone());

        let test_ip = "192.0.2.123";

        // 最初はログイン可能（ロックなし）
        assert_eq!(
            state
                .login_limiter
                .check("admin", Some(test_ip))
                .await
                .unwrap(),
            None
        );

        // /wp-login.php にアクセス
        let mut req = Request::builder()
            .uri("/wp-login.php")
            .body(axum::body::Body::empty())
            .unwrap();
        req.extensions_mut().insert(axum::extract::ConnectInfo(
            format!("{}:12345", test_ip)
                .parse::<std::net::SocketAddr>()
                .unwrap(),
        ));

        let resp = app.clone().oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);

        // 即座に IP がロックされていること
        let lockout = state
            .login_limiter
            .check("admin", Some(test_ip))
            .await
            .unwrap();
        assert!(lockout.is_some());
        let lockout = lockout.unwrap();
        assert_eq!(lockout.scope, aegis_auth::LockScope::Ip);
        assert!(lockout.retry_after_secs > 0);
    }

    #[tokio::test]
    async fn test_svg_request_gets_restricted_csp() {
        let supervisor = aegis_runtime::PluginSupervisor::new();
        let state = crate::test_support::test_state(supervisor).await;
        let app = crate::app::router(&state.config, state.clone());

        // 通常のページ
        let req = Request::builder()
            .uri("/")
            .body(axum::body::Body::empty())
            .unwrap();
        let resp = app.clone().oneshot(req).await.unwrap();
        let csp = resp
            .headers()
            .get("content-security-policy")
            .unwrap()
            .to_str()
            .unwrap();
        assert!(!csp.contains("default-src 'none'"));

        // SVG ファイルへのリクエスト（スクリプト実行を遮断する厳格な CSP）
        let req = Request::builder()
            .uri("/uploads/test.svg")
            .body(axum::body::Body::empty())
            .unwrap();
        let resp = app.clone().oneshot(req).await.unwrap();
        let csp = resp
            .headers()
            .get("content-security-policy")
            .unwrap()
            .to_str()
            .unwrap();
        assert_eq!(csp, "default-src 'none'; style-src 'unsafe-inline'");
        assert_eq!(
            resp.headers().get("x-content-type-options").unwrap(),
            "nosniff"
        );
    }
}
