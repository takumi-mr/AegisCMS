//! リダイレクト（`/api/v1/redirects`、`admin`）。規則の一覧・追加・変更・削除と、コンテンツの slug を変えたときの
//! 自動の規則。仕組みと規則は [`crate::redirects`]

use std::sync::Arc;

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Json,
};
use serde::Deserialize;
use serde_json::json;

use crate::audit::AuditContext;
use crate::auth::authorize::RequireRole;
use crate::auth::AdminSession;
use crate::cluster;
use crate::redirects::{
    public_paths, reserved_prefixes, RedirectError, RedirectRule, RedirectTable, RuleInput,
};
use crate::state::AppState;
use aegis_core::{AuditAction, AuditOutcome};

type ApiError = (StatusCode, Json<serde_json::Value>);

const DEFAULT_LIMIT: usize = 50;
const MAX_LIMIT: usize = 200;

fn redirect_error(e: RedirectError) -> ApiError {
    let status = match &e {
        RedirectError::Invalid { .. } | RedirectError::Loop | RedirectError::TooMany => {
            StatusCode::BAD_REQUEST
        }
        RedirectError::Duplicate => StatusCode::CONFLICT,
        RedirectError::NotFound => StatusCode::NOT_FOUND,
    };
    let mut body = json!({ "error": e.to_string(), "code": e.code() });
    if let RedirectError::Invalid { field, .. } = &e {
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

fn now() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Micros, true)
}

/// 規則を変えて DB に保存し、メモリに反映して、ほかのサーバーに知らせる。保存に失敗したらメモリは変えない
async fn mutate<T>(
    state: &AppState,
    change: impl FnOnce(&mut RedirectTable) -> Result<T, RedirectError>,
) -> Result<T, ApiError> {
    let mut guard = state.redirects.write().await;
    let mut next = guard.clone();
    let out = change(&mut next).map_err(redirect_error)?;
    next.save(state.db.as_ref())
        .await
        .map_err(|e| internal_error("Could not save the redirects", e))?;
    *guard = next;
    drop(guard);
    cluster::announce(state, cluster::TOPIC_REDIRECTS, "").await;
    Ok(out)
}

#[derive(Debug, Default, Deserialize)]
pub struct ListParams {
    /// `from` / `to` への部分一致（大文字・小文字を区別しない）
    pub q: Option<String>,
    pub limit: Option<usize>,
    #[serde(default)]
    pub offset: usize,
}

/// `GET /api/v1/redirects` — 規則の一覧（新しい順）と総数（`admin`）
pub async fn list_redirects(
    State(state): State<Arc<AppState>>,
    RequireRole(_session): AdminSession,
    Query(params): Query<ListParams>,
) -> Json<serde_json::Value> {
    let needle = params
        .q
        .as_deref()
        .map(|q| q.trim().to_lowercase())
        .filter(|q| !q.is_empty());
    let table = state.redirects.read().await;
    let mut rules: Vec<&RedirectRule> = table
        .rules()
        .iter()
        .filter(|r| {
            needle.as_ref().is_none_or(|n| {
                r.from.to_lowercase().contains(n) || r.to.to_lowercase().contains(n)
            })
        })
        .collect();
    rules.sort_by(|a, b| {
        b.created_at
            .cmp(&a.created_at)
            .then_with(|| a.from.cmp(&b.from))
    });
    let total = rules.len();
    let limit = params.limit.unwrap_or(DEFAULT_LIMIT).clamp(1, MAX_LIMIT);
    let page: Vec<&RedirectRule> = rules.into_iter().skip(params.offset).take(limit).collect();
    Json(json!({ "redirects": page, "total": total }))
}

/// `POST /api/v1/redirects` — 規則を足す（`admin`）。`{"from": "/old", "to": "/new", "status": 301}`
pub async fn create_redirect(
    State(state): State<Arc<AppState>>,
    RequireRole(_session): AdminSession,
    audit: AuditContext,
    Json(input): Json<RuleInput>,
) -> Result<(StatusCode, Json<RedirectRule>), ApiError> {
    let reserved = reserved_prefixes(&state.config);
    let id = uuid::Uuid::new_v4().to_string();
    let at = now();
    let rule = mutate(&state, |table| table.add(&input, &reserved, id, &at)).await?;
    record(&state, &audit, AuditAction::RedirectCreate, &rule, false).await;
    Ok((StatusCode::CREATED, Json(rule)))
}

/// `PUT /api/v1/redirects/{id}` — 規則を変える（`admin`）
pub async fn update_redirect(
    State(state): State<Arc<AppState>>,
    RequireRole(_session): AdminSession,
    audit: AuditContext,
    Path(id): Path<String>,
    Json(input): Json<RuleInput>,
) -> Result<Json<RedirectRule>, ApiError> {
    let reserved = reserved_prefixes(&state.config);
    let at = now();
    let rule = mutate(&state, |table| table.update(&id, &input, &reserved, &at)).await?;
    record(&state, &audit, AuditAction::RedirectUpdate, &rule, false).await;
    Ok(Json(rule))
}

/// `DELETE /api/v1/redirects/{id}` — 規則を消す（`admin`）
pub async fn delete_redirect(
    State(state): State<Arc<AppState>>,
    RequireRole(_session): AdminSession,
    audit: AuditContext,
    Path(id): Path<String>,
) -> Result<StatusCode, ApiError> {
    let rule = mutate(&state, |table| {
        table.remove(&id).ok_or(RedirectError::NotFound)
    })
    .await?;
    record(&state, &audit, AuditAction::RedirectDelete, &rule, false).await;
    Ok(StatusCode::NO_CONTENT)
}

async fn record(
    state: &AppState,
    audit: &aegis_audit::AuditOrigin,
    action: AuditAction,
    rule: &RedirectRule,
    auto: bool,
) {
    state
        .audit
        .record(
            audit
                .event(action, AuditOutcome::Success)
                .with_resource("redirect", Some(rule.id.clone()))
                .with_details(json!({
                    "from": rule.from,
                    "to": rule.to,
                    "status": rule.status,
                    "auto": auto,
                })),
        )
        .await;
}

/// 今公開されている版の slug（訪問者が使う URL）。公開版が無ければ `None`。保存しただけでは変わらず、
/// 新しい版を公開したとき（または予約公開の時刻になったとき）に変わる
pub async fn live_slug(
    state: &AppState,
    schema: &aegis_schema::SchemaDefinition,
    id: &str,
) -> Option<String> {
    let record = state.db.get_published_content(schema, id).await.ok()??;
    Some(record.slug)
}

/// 公開中のコンテンツの URL（slug）が変わったとき、旧 URL から新しい URL への規則を自動で残す。失敗は記録する
/// だけで、コンテンツの保存・公開は止めない
pub async fn record_slug_change(
    state: &AppState,
    audit: &aegis_audit::AuditOrigin,
    content_type: &str,
    old_slug: &str,
    new_slug: &str,
) {
    let old = public_paths(&state.config, content_type, old_slug);
    let new = public_paths(&state.config, content_type, new_slug);
    for (from, to) in old.into_iter().zip(new) {
        let id = uuid::Uuid::new_v4().to_string();
        let at = now();
        let added = mutate(state, |table| Ok(table.add_auto(&from, &to, id, &at))).await;
        match added {
            Ok(Some(rule)) => record(state, audit, AuditAction::RedirectCreate, &rule, true).await,
            Ok(None) => {}
            Err((_, body)) => tracing::warn!(
                "Could not keep a redirect from '{}' to '{}': {}",
                from,
                to,
                body.0
            ),
        }
    }
}

#[cfg(test)]
#[path = "redirects_tests.rs"]
mod tests;
