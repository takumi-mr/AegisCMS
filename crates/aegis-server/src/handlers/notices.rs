//! お知らせバナー（`/api/v1/notices`、`admin`）。一覧・追加・変更・削除。仕組みは [`crate::notices`]

use std::sync::Arc;

use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use chrono::Utc;
use serde::Serialize;
use serde_json::json;

use crate::audit::AuditContext;
use crate::auth::authorize::RequireRole;
use crate::auth::AdminSession;
use crate::cluster;
use crate::notices::{Notice, NoticeError, NoticeInput, NoticeList, NoticeStatus};
use crate::state::AppState;
use aegis_core::{AuditAction, AuditOutcome};

type ApiError = (StatusCode, Json<serde_json::Value>);

fn notice_error(e: NoticeError) -> ApiError {
    let status = match &e {
        NoticeError::Invalid { .. } | NoticeError::TooMany => StatusCode::BAD_REQUEST,
        NoticeError::NotFound => StatusCode::NOT_FOUND,
    };
    let mut body = json!({ "error": e.to_string(), "code": e.code() });
    if let NoticeError::Invalid { field, .. } = &e {
        body["field"] = json!(field);
    }
    (status, Json(body))
}

fn internal_error(context: &str, e: impl std::fmt::Display) -> ApiError {
    tracing::error!("{}: {}", context, e);
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(json!({ "error": "Internal server error" })),
    )
}

/// 一覧・応答に出すお知らせ: 保存している値と、今の状態（`active` / `scheduled` / `expired` / `disabled`）
#[derive(Debug, Serialize)]
pub struct NoticeView {
    #[serde(flatten)]
    pub notice: Notice,
    pub status: NoticeStatus,
}

fn view(notice: Notice) -> NoticeView {
    let status = notice.status(Utc::now());
    NoticeView { notice, status }
}

fn now() -> String {
    Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Micros, true)
}

/// お知らせを変えて DB に保存し、メモリに反映して、ほかのサーバーに知らせる。サイト全体のキャッシュも消す
/// （キャッシュされたページに古いお知らせが残らないように）。保存に失敗したらメモリは変えない
async fn mutate<T>(
    state: &AppState,
    change: impl FnOnce(&mut NoticeList) -> Result<T, NoticeError>,
) -> Result<T, ApiError> {
    let mut guard = state.notices.write().await;
    let mut next = guard.clone();
    let out = change(&mut next).map_err(notice_error)?;
    next.save(state.db.as_ref())
        .await
        .map_err(|e| internal_error("Could not save the notices", e))?;
    *guard = next;
    drop(guard);
    cluster::announce(state, cluster::TOPIC_NOTICES, "").await;
    crate::cdn::purge_site_global(state).await;
    Ok(out)
}

async fn record(state: &AppState, audit: &AuditContext, action: AuditAction, notice: &Notice) {
    state
        .audit
        .record(
            audit
                .event(action, AuditOutcome::Success)
                .with_resource("notice", Some(notice.id.clone()))
                .with_details(json!({ "message": notice.message, "style": notice.style })),
        )
        .await;
}

/// `GET /api/v1/notices` — お知らせの一覧（作った順。サイトでもこの順に出る。`admin`）
pub async fn list_notices(
    State(state): State<Arc<AppState>>,
    RequireRole(_session): AdminSession,
) -> Json<serde_json::Value> {
    let notices: Vec<NoticeView> = state
        .notices
        .read()
        .await
        .items()
        .iter()
        .cloned()
        .map(view)
        .collect();
    Json(json!({ "total": notices.len(), "notices": notices }))
}

/// `POST /api/v1/notices` — お知らせを足す（`admin`）
pub async fn create_notice(
    State(state): State<Arc<AppState>>,
    RequireRole(_session): AdminSession,
    audit: AuditContext,
    Json(input): Json<NoticeInput>,
) -> Result<(StatusCode, Json<NoticeView>), ApiError> {
    let id = uuid::Uuid::new_v4().to_string();
    let at = now();
    let notice = mutate(&state, |list| list.add(&input, id, &at)).await?;
    record(&state, &audit, AuditAction::NoticeCreate, &notice).await;
    Ok((StatusCode::CREATED, Json(view(notice))))
}

/// `PUT /api/v1/notices/{id}` — お知らせを変える（`admin`）。変えると版が変わり、閉じた訪問者にもう一度出る
pub async fn update_notice(
    State(state): State<Arc<AppState>>,
    RequireRole(_session): AdminSession,
    audit: AuditContext,
    Path(id): Path<String>,
    Json(input): Json<NoticeInput>,
) -> Result<Json<NoticeView>, ApiError> {
    let at = now();
    let notice = mutate(&state, |list| list.update(&id, &input, &at)).await?;
    record(&state, &audit, AuditAction::NoticeUpdate, &notice).await;
    Ok(Json(view(notice)))
}

/// `DELETE /api/v1/notices/{id}` — お知らせを消す（`admin`）
pub async fn delete_notice(
    State(state): State<Arc<AppState>>,
    RequireRole(_session): AdminSession,
    audit: AuditContext,
    Path(id): Path<String>,
) -> Result<StatusCode, ApiError> {
    let notice = mutate(&state, |list| list.remove(&id).ok_or(NoticeError::NotFound)).await?;
    record(&state, &audit, AuditAction::NoticeDelete, &notice).await;
    Ok(StatusCode::NO_CONTENT)
}

#[cfg(test)]
#[path = "notices_tests.rs"]
mod tests;
