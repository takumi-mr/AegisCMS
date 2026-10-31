//! アクセス解析（`[analytics]`、Cargo の feature `analytics`）。集計は `aegis-analytics`、ここはビーコンの受け口と
//! 集計の API（Admin Studio のダッシュボード用）

use std::sync::Arc;

use aegis_analytics::{PageView, SummaryError};
use axum::extract::{Query, State};
use axum::http::{header, HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::Deserialize;
use serde_json::json;

use crate::audit::ClientIp;
use crate::auth::ViewerSession;
use crate::state::AppState;

/// ビーコンの本文の大きさの上限（バイト）
const MAX_EVENT_BYTES: usize = 4096;
/// 集計の API で返す上位の件数の上限
const MAX_SUMMARY_LIMIT: u64 = 100;

/// 公開ページの `<head>` に入れるビーコン（無効なら空）
pub fn head_html(state: &AppState) -> String {
    if state.analytics.is_none() {
        return String::new();
    }
    let base_path = state
        .config
        .server
        .normalized_base_path()
        .unwrap_or_default();
    aegis_analytics::beacon_script(&format!("{}/api/v1/analytics/event", base_path))
}

#[derive(Debug, Deserialize)]
struct BeaconEvent {
    /// ページのパス
    p: String,
    /// クエリ文字列
    #[serde(default)]
    q: String,
    /// 参照元
    #[serde(default)]
    r: String,
}

/// ブラウザが追跡を断っている（Do Not Track・Global Privacy Control）
pub(crate) fn opted_out(headers: &HeaderMap) -> bool {
    let is_one = |name: &str| headers.get(name).and_then(|v| v.to_str().ok()) == Some("1");
    is_one("dnt") || is_one("sec-gpc")
}

/// `POST /api/v1/analytics/event` — ビーコンの受け口（ログイン不要）。本文は `{"p": パス, "r": 参照元}` の JSON
/// （`sendBeacon` が `text/plain` で送るので Content-Type は問わない）。結果にかかわらず 204 を返す
pub async fn track_event(
    State(state): State<Arc<AppState>>,
    ClientIp(ip): ClientIp,
    headers: HeaderMap,
    body: axum::body::Bytes,
) -> Response {
    let Some(analytics) = state.analytics.clone() else {
        return StatusCode::NOT_FOUND.into_response();
    };
    if body.len() > MAX_EVENT_BYTES {
        return StatusCode::PAYLOAD_TOO_LARGE.into_response();
    }
    let Ok(event) = serde_json::from_slice::<BeaconEvent>(&body) else {
        return StatusCode::BAD_REQUEST.into_response();
    };
    if !opted_out(&headers) {
        let header = |name: header::HeaderName| {
            headers
                .get(name)
                .and_then(|v| v.to_str().ok())
                .unwrap_or_default()
                .to_string()
        };
        let host = header(header::HOST);
        let user_agent: String = header(header::USER_AGENT).chars().take(512).collect();
        let ip = ip.unwrap_or_default();
        analytics
            .track(
                PageView {
                    path: &event.p,
                    query: &event.q,
                    referrer: &event.r,
                    host: &host,
                    user_agent: &user_agent,
                    ip: &ip,
                },
                chrono::Utc::now(),
            )
            .await;
    }
    (
        StatusCode::NO_CONTENT,
        [(header::CACHE_CONTROL, "no-store")],
    )
        .into_response()
}

#[derive(Debug, Default, Deserialize)]
pub struct SummaryParams {
    /// `YYYY-MM-DD`。省略時は `to` の 29 日前
    pub from: Option<String>,
    /// `YYYY-MM-DD`。省略時は今日（UTC）
    pub to: Option<String>,
    /// 上位のページ・参照元の件数（既定 10、最大 100）
    pub limit: Option<u64>,
}

/// `GET /api/v1/analytics/summary?from=2026-10-01&to=2026-10-31` — 期間の集計（`viewer` 以上）
pub async fn get_summary(
    State(state): State<Arc<AppState>>,
    _session: ViewerSession,
    Query(params): Query<SummaryParams>,
) -> Response {
    let Some(analytics) = state.analytics.clone() else {
        return (
            StatusCode::NOT_FOUND,
            Json(json!({ "error": "Analytics is not enabled" })),
        )
            .into_response();
    };
    let to = params
        .to
        .unwrap_or_else(|| chrono::Utc::now().format("%Y-%m-%d").to_string());
    let from = match params.from {
        Some(from) => from,
        None => match chrono::NaiveDate::parse_from_str(&to, "%Y-%m-%d") {
            Ok(day) => (day - chrono::Duration::days(29))
                .format("%Y-%m-%d")
                .to_string(),
            Err(_) => to.clone(),
        },
    };
    let limit = params.limit.unwrap_or(10).clamp(1, MAX_SUMMARY_LIMIT);
    match analytics.summary(&from, &to, limit).await {
        Ok(summary) => Json(summary).into_response(),
        Err(SummaryError::Database(e)) => {
            tracing::error!("Could not read analytics: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": "Could not read analytics" })),
            )
                .into_response()
        }
        Err(e) => (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": e.to_string() })),
        )
            .into_response(),
    }
}

/// `GET /api/v1/analytics/export?from=2026-10-01&to=2026-10-31` — 集計結果の CSV エクスポート（`viewer` 以上）
pub async fn export_csv(
    State(state): State<Arc<AppState>>,
    _session: ViewerSession,
    Query(params): Query<SummaryParams>,
) -> Response {
    let Some(analytics) = state.analytics.clone() else {
        return (
            StatusCode::NOT_FOUND,
            Json(json!({ "error": "Analytics is not enabled" })),
        )
            .into_response();
    };
    let to = params
        .to
        .unwrap_or_else(|| chrono::Utc::now().format("%Y-%m-%d").to_string());
    let from = match params.from {
        Some(from) => from,
        None => match chrono::NaiveDate::parse_from_str(&to, "%Y-%m-%d") {
            Ok(day) => (day - chrono::Duration::days(29))
                .format("%Y-%m-%d")
                .to_string(),
            Err(_) => to.clone(),
        },
    };
    let limit = params.limit.unwrap_or(100).clamp(1, MAX_SUMMARY_LIMIT);
    match analytics.summary(&from, &to, limit).await {
        Ok(summary) => {
            let csv = aegis_analytics::export_summary_csv(&summary);
            let filename = format!("analytics-{}_{}.csv", summary.from, summary.to);
            let disposition = format!(r#"attachment; filename="{}""#, filename);

            let mut res = Response::new(axum::body::Body::from(csv));
            res.headers_mut().insert(
                header::CONTENT_TYPE,
                header::HeaderValue::from_static("text/csv; charset=utf-8"),
            );
            if let Ok(disp_val) = header::HeaderValue::from_str(&disposition) {
                res.headers_mut()
                    .insert(header::CONTENT_DISPOSITION, disp_val);
            }
            res
        }
        Err(SummaryError::Database(e)) => {
            tracing::error!("Could not read analytics for export: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": "Could not read analytics" })),
            )
                .into_response()
        }
        Err(e) => (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": e.to_string() })),
        )
            .into_response(),
    }
}
#[cfg(test)]
mod tests;
