use std::sync::Arc;

use axum::extract::State;
use axum::http::HeaderMap;
use axum::{response::IntoResponse, Json};
use serde_json::json;

use crate::auth::authorize::{role_level, ADMIN};
use crate::auth::extract_session_id;
use crate::state::AppState;

/// `GET /health`, `GET /api/v1/health`.
/// 外向けには稼働状態だけを返し、製品名とバージョンは `admin` のセッションにだけ返す
/// （バージョンから既知の脆弱性を引かれないようにするため）
pub async fn health_check(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> impl IntoResponse {
    let mut body = json!({
        "status": "ok",
        "timestamp": chrono::Utc::now().to_rfc3339()
    });
    if is_admin(&state, &headers).await {
        body["system"] = json!("AegisCMS");
        body["version"] = json!(env!("CARGO_PKG_VERSION"));
    }
    Json(body)
}

/// ヘッダーのセッションが `admin` か。無効なセッションでも拒否はせず（監査ログにも残さず）、
/// 公開用の応答にするだけ
async fn is_admin(state: &AppState, headers: &HeaderMap) -> bool {
    let Some(session_id) = extract_session_id(headers) else {
        return false;
    };
    matches!(
        state.sessions.get_session(&session_id).await,
        Ok(Some(session)) if role_level(&session.role) >= ADMIN
    )
}

#[cfg(test)]
mod tests {
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use serde_json::Value;
    use tower::ServiceExt;

    async fn get_health(token: Option<&str>) -> Value {
        let state = crate::test_support::test_state(aegis_runtime::PluginSupervisor::new()).await;
        let token = match token {
            Some(role @ ("admin" | "editor")) => Some(
                state
                    .sessions
                    .create_session("u1", role, Default::default(), None)
                    .await
                    .unwrap()
                    .id,
            ),
            Some(raw) => Some(raw.to_string()),
            None => None,
        };
        let config = state.config.clone();
        let mut req = Request::get("/api/v1/health");
        if let Some(token) = token {
            req = req.header("authorization", format!("Bearer {}", token));
        }
        let res = crate::app::router(&config, state)
            .oneshot(req.body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let body = axum::body::to_bytes(res.into_body(), 1 << 16)
            .await
            .unwrap();
        serde_json::from_slice(&body).unwrap()
    }

    #[tokio::test]
    async fn test_health_hides_product_and_version_from_the_public() {
        for token in [None, Some("editor"), Some("not-a-session")] {
            let body = get_health(token).await;
            assert_eq!(body["status"], "ok");
            assert!(body.get("system").is_none(), "{:?}: {}", token, body);
            assert!(body.get("version").is_none(), "{:?}: {}", token, body);
        }
    }

    #[tokio::test]
    async fn test_health_shows_product_and_version_to_admins() {
        let body = get_health(Some("admin")).await;
        assert_eq!(body["status"], "ok");
        assert_eq!(body["system"], "AegisCMS");
        assert_eq!(body["version"], env!("CARGO_PKG_VERSION"));
    }
}
