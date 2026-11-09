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

/// `GET /_aegis/fonts/{file}` — 組み込みの default テーマのフォント（Latin のサブセットの woff2）。
/// 外部のフォントの CDN に頼らない（CSP に止められず、訪問者の IP を送らない）。ライセンスは SIL OFL 1.1
/// （`assets/fonts/OFL-*.txt`）。知らない名前は 404
pub async fn font_file(axum::extract::Path(file): axum::extract::Path<String>) -> Response {
    let bytes: &'static [u8] = match file.as_str() {
        "plus-jakarta-sans-latin.woff2" => {
            include_bytes!("../../assets/fonts/plus-jakarta-sans-latin.woff2")
        }
        "jetbrains-mono-latin.woff2" => {
            include_bytes!("../../assets/fonts/jetbrains-mono-latin.woff2")
        }
        _ => return StatusCode::NOT_FOUND.into_response(),
    };
    (
        StatusCode::OK,
        [
            (header::CONTENT_TYPE, "font/woff2"),
            (header::CACHE_CONTROL, "public, max-age=604800"),
            (header::X_CONTENT_TYPE_OPTIONS, "nosniff"),
        ],
        bytes,
    )
        .into_response()
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

    /// 組み込みの default テーマは、フォントを自分で配信する（外部のフォントの CDN に頼らない）
    #[tokio::test]
    async fn test_the_default_theme_serves_its_fonts_itself() {
        let config = AegisConfig::default();
        let ns = config.server.namespace();
        let state = std::sync::Arc::new(
            test_app_state(config.clone(), PluginSupervisor::new(), Vec::new()).await,
        );
        let app = crate::app::router(&config, state);
        let get = |path: String| {
            let app = app.clone();
            async move {
                app.oneshot(Request::get(path).body(Body::empty()).unwrap())
                    .await
                    .unwrap()
            }
        };

        // ページは、Google Fonts を読まず、自分のフォントを読む。CSP に外部のフォントのホストは要らない
        let res = get("/".to_string()).await;
        assert_eq!(res.status(), StatusCode::OK);
        let csp = res.headers()["content-security-policy"]
            .to_str()
            .unwrap()
            .to_string();
        let html = String::from_utf8_lossy(
            &axum::body::to_bytes(res.into_body(), usize::MAX)
                .await
                .unwrap(),
        )
        .to_string();
        assert!(!html.contains("fonts.googleapis.com") && !html.contains("fonts.gstatic.com"));
        assert!(
            !csp.contains("googleapis") && !csp.contains("gstatic"),
            "{csp}"
        );
        for file in [
            "plus-jakarta-sans-latin.woff2",
            "jetbrains-mono-latin.woff2",
        ] {
            assert!(html.contains(&format!("/_{ns}/fonts/{file}")), "{file}");
            let res = get(format!("/_{ns}/fonts/{file}")).await;
            assert_eq!(res.status(), StatusCode::OK, "{file}");
            assert_eq!(res.headers()["content-type"], "font/woff2");
            let body = axum::body::to_bytes(res.into_body(), usize::MAX)
                .await
                .unwrap();
            assert_eq!(&body[..4], b"wOF2", "{file} is a woff2 file");
        }
        // 知らない名前（パスの横断を含む）は 404
        for bad in [
            "nope.woff2",
            "..%2Fcomments.js",
            "plus-jakarta-sans-latin.woff",
        ] {
            assert_eq!(
                get(format!("/_{ns}/fonts/{bad}")).await.status(),
                StatusCode::NOT_FOUND,
                "{bad}"
            );
        }
    }

    /// テーマが `theme.json` の `csp` で宣言したホスト（Google Fonts など）が、公開ページの CSP の
    /// `style-src` / `font-src` に入る。スクリプトには入らない
    #[tokio::test]
    async fn test_the_active_themes_declared_hosts_reach_the_csp() {
        let config = AegisConfig::default();
        let mut state = test_app_state(config.clone(), PluginSupervisor::new(), Vec::new()).await;
        let themes = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../themes");
        state.theme = std::sync::Arc::new(aegis_theme::ThemeEngine::new(&themes, "roastery"));
        let app = crate::app::router(&config, std::sync::Arc::new(state));
        let res = app
            .oneshot(Request::get("/").body(Body::empty()).unwrap())
            .await
            .unwrap();
        let csp = res.headers()["content-security-policy"].to_str().unwrap();
        let directive = |name: &str| {
            csp.split(';')
                .map(str::trim)
                .find(|d| d.starts_with(name))
                .unwrap_or_default()
                .to_string()
        };
        assert!(
            directive("style-src").contains("https://fonts.googleapis.com"),
            "{csp}"
        );
        assert!(
            directive("font-src").contains("https://fonts.gstatic.com"),
            "{csp}"
        );
        assert!(!directive("script-src").contains("googleapis"), "{csp}");
        assert!(!directive("script-src").contains("gstatic"), "{csp}");
    }
}
