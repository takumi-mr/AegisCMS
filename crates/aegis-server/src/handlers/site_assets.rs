//! サイトの機能が配信するスクリプト・スタイル（`/_<namespace>/...`）。機能が無効なら 404

use std::sync::Arc;

use axum::{
    extract::State,
    http::{header, StatusCode},
    response::{IntoResponse, Response},
};

use crate::state::AppState;

/// 本文と種類から、1 時間キャッシュさせる応答を作る（URL に `?v=<バージョン>` を付けるので更新は反映される）
fn asset(content_type: &'static str, body: &'static str) -> Response {
    (
        StatusCode::OK,
        [
            (header::CONTENT_TYPE, content_type),
            (header::CACHE_CONTROL, "public, max-age=3600"),
            (header::X_CONTENT_TYPE_OPTIONS, "nosniff"),
        ],
        body,
    )
        .into_response()
}

/// `GET /_aegis/consent.js` — Cookie の同意バナーのスクリプト（`[consent].enabled` のとき）
pub async fn consent_script(State(state): State<Arc<AppState>>) -> Response {
    if !state.config.consent.enabled {
        return StatusCode::NOT_FOUND.into_response();
    }
    asset(
        "application/javascript; charset=utf-8",
        crate::consent::script(),
    )
}

/// `GET /_aegis/consent.css` — Cookie の同意バナーのスタイル（`[consent].enabled` のとき）
pub async fn consent_style(State(state): State<Arc<AppState>>) -> Response {
    if !state.config.consent.enabled {
        return StatusCode::NOT_FOUND.into_response();
    }
    asset("text/css; charset=utf-8", crate::consent::style())
}

/// `GET /_aegis/comments.js` — 組み込みのコメント欄の投稿フォームのスクリプト
pub async fn comments_script() -> Response {
    asset(
        "application/javascript; charset=utf-8",
        include_str!("../../assets/comments.js"),
    )
}

/// `GET /_aegis/setup.js` — 初期セットアップ画面（`/setup`）のスクリプト
pub async fn setup_script() -> Response {
    asset(
        "application/javascript; charset=utf-8",
        include_str!("../../assets/setup.js"),
    )
}

/// `GET /_aegis/notices.js` — お知らせバナーの「閉じる」のスクリプト
pub async fn notices_script() -> Response {
    asset(
        "application/javascript; charset=utf-8",
        crate::notices::script(),
    )
}

/// `GET /_aegis/notices.css` — 組み込みのお知らせバナーのスタイル
pub async fn notices_style() -> Response {
    asset("text/css; charset=utf-8", crate::notices::style())
}

#[cfg(test)]
mod tests {
    use crate::config::AegisConfig;
    use crate::test_support::test_app_state;
    use aegis_runtime::PluginSupervisor;
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use tower::ServiceExt;

    /// コメント欄とセットアップ画面のスクリプトは、インラインではなく外部ファイル（CSP の
    /// `script-src` に `'unsafe-inline'` が要らない。docs/csp.md の 5）
    #[tokio::test]
    async fn test_comment_and_setup_scripts_are_served_as_files() {
        let mut config = AegisConfig::default();
        config.comments.enabled = true;
        let state = std::sync::Arc::new(
            test_app_state(config.clone(), PluginSupervisor::new(), Vec::new()).await,
        );
        let app = crate::app::router(&config, state);
        // 配信パスの `aegis` は、`[server].namespace` に置き換わる（応答の中の識別子も同じ）
        let ns = config.server.namespace();
        for (path, needle) in [
            (format!("/_{ns}/comments.js"), format!("{ns}-comment-form")),
            (format!("/_{ns}/setup.js"), "/api/v1/setup".to_string()),
        ] {
            let res = app
                .clone()
                .oneshot(Request::get(&path).body(Body::empty()).unwrap())
                .await
                .unwrap();
            assert_eq!(res.status(), StatusCode::OK, "{path}");
            assert_eq!(
                res.headers()["content-type"],
                "application/javascript; charset=utf-8"
            );
            assert_eq!(res.headers()["x-content-type-options"], "nosniff");
            let body = axum::body::to_bytes(res.into_body(), usize::MAX)
                .await
                .unwrap();
            let text = String::from_utf8_lossy(&body);
            assert!(text.contains(&needle), "{path}");
        }
    }
}
