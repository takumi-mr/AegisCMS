//! サイトの機能が配信するスクリプト・スタイル（`/_aegis/...`）。機能が無効なら 404

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
