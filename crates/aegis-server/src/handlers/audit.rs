use axum::{
    extract::{Query, State},
    http::{header, StatusCode},
    response::IntoResponse,
    Json,
};
use serde::Deserialize;
use serde_json::json;
use std::sync::Arc;

use crate::auth::AdminSession;
use crate::state::AppState;
use aegis_audit::metrics::PROMETHEUS_CONTENT_TYPE;
use aegis_core::{normalize_audit_timestamp, AuditQuery};

const DEFAULT_LIMIT: u32 = 50;
const MAX_LIMIT: u32 = 500;

type ApiError = (StatusCode, Json<serde_json::Value>);

fn api_error(status: StatusCode, message: impl Into<String>) -> ApiError {
    (status, Json(json!({ "error": message.into() })))
}

#[derive(Debug, Default, Deserialize)]
pub struct AuditLogParams {
    pub action: Option<String>,
    pub outcome: Option<String>,
    pub actor_id: Option<String>,
    pub resource_type: Option<String>,
    pub resource_id: Option<String>,
    pub since: Option<String>,
    pub until: Option<String>,
    pub limit: Option<u32>,
    pub offset: Option<u32>,
}

impl AuditLogParams {
    fn into_query(self) -> Result<AuditQuery, String> {
        let non_empty =
            |v: Option<String>| v.map(|s| s.trim().to_string()).filter(|s| !s.is_empty());

        Ok(AuditQuery {
            action: non_empty(self.action).map(|s| s.parse()).transpose()?,
            outcome: non_empty(self.outcome).map(|s| s.parse()).transpose()?,
            actor_id: non_empty(self.actor_id),
            resource_type: non_empty(self.resource_type),
            resource_id: non_empty(self.resource_id),
            since: non_empty(self.since)
                .map(|s| normalize_audit_timestamp(&s))
                .transpose()?,
            until: non_empty(self.until)
                .map(|s| normalize_audit_timestamp(&s))
                .transpose()?,
            limit: self.limit.unwrap_or(DEFAULT_LIMIT).clamp(1, MAX_LIMIT),
            offset: self.offset.unwrap_or(0),
        })
    }
}

/// `GET /api/v1/audit-logs` — 監査ログの検索（`admin` ロールのセッションが必要）
pub async fn list_audit_logs(
    State(state): State<Arc<AppState>>,
    _session: AdminSession,
    Query(params): Query<AuditLogParams>,
) -> Result<impl IntoResponse, ApiError> {
    let reader = state.audit.reader().ok_or_else(|| {
        api_error(
            StatusCode::NOT_FOUND,
            "Audit log query requires the 'database' audit sink to be enabled",
        )
    })?;

    let query = params
        .into_query()
        .map_err(|e| api_error(StatusCode::BAD_REQUEST, e))?;
    let items = reader.query_audit_events(&query).await.map_err(|e| {
        api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Audit log query error: {}", e),
        )
    })?;

    Ok(Json(json!({
        "items": items,
        "limit": query.limit,
        "offset": query.offset,
        "retention_days": state.audit.retention_days(),
    })))
}

/// `GET /metrics` — Prometheus 形式のメトリクス（`prometheus` 出力先が有効な場合のみ）
pub async fn get_metrics(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    match state.audit.metrics() {
        Some(metrics) => (
            StatusCode::OK,
            [(header::CONTENT_TYPE, PROMETHEUS_CONTENT_TYPE)],
            metrics.render(),
        )
            .into_response(),
        None => api_error(
            StatusCode::NOT_FOUND,
            "Metrics endpoint requires the 'prometheus' audit sink to be enabled",
        )
        .into_response(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use aegis_core::{AuditAction, AuditOutcome};

    #[test]
    fn test_params_are_validated_and_normalized() {
        let query = AuditLogParams {
            action: Some("content.create".to_string()),
            outcome: Some("denied".to_string()),
            actor_id: Some("  ".to_string()),
            since: Some("2026-10-22T09:00:00+09:00".to_string()),
            limit: Some(10_000),
            ..Default::default()
        }
        .into_query()
        .unwrap();

        assert_eq!(query.action, Some(AuditAction::ContentCreate));
        assert_eq!(query.outcome, Some(AuditOutcome::Denied));
        assert_eq!(query.actor_id, None);
        assert_eq!(query.since.as_deref(), Some("2026-10-22T00:00:00.000000Z"));
        assert_eq!(query.limit, MAX_LIMIT);
        assert_eq!(query.offset, 0);

        assert!(AuditLogParams {
            action: Some("drop table".to_string()),
            ..Default::default()
        }
        .into_query()
        .is_err());
        assert!(AuditLogParams {
            until: Some("tomorrow".to_string()),
            ..Default::default()
        }
        .into_query()
        .is_err());
    }
}
