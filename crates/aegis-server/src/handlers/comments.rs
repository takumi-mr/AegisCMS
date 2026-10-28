//! コメントの API。公開（ログイン不要）: コメントの一覧・投稿、返信通知の停止ページ。
//! 管理（`editor` 以上）: 一覧・承認などの状態の変更・削除・返信。承認制の設定の変更は `admin`

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::sync::Arc;

use crate::audit::{AuditContext, ClientIp};
use crate::auth::{AdminSession, EditorSession};
use crate::handlers::public::{render_not_found, themed_response};
use crate::render;
use crate::state::AppState;
use aegis_comments::{
    AdminCommentPage, AdminListQuery, ClientInfo, CommentError, CommentRequest, CommentService,
    ModerationSettings, PolicyOverrides, StaffAuthor,
};
use aegis_core::{CommentSection, ModerationStatus, PageRenderContext, PublicComment};

type ApiError = (StatusCode, Json<serde_json::Value>);

/// 一覧の既定の件数と上限
const DEFAULT_LIMIT: u64 = 50;
const MAX_LIMIT: u64 = 200;

fn api_error(status: StatusCode, message: impl Into<String>) -> ApiError {
    (status, Json(json!({ "error": message.into() })))
}

fn comment_error(e: CommentError) -> ApiError {
    let status = match &e {
        CommentError::Invalid(_) => StatusCode::BAD_REQUEST,
        CommentError::NotFound(_) => StatusCode::NOT_FOUND,
        CommentError::Closed => StatusCode::FORBIDDEN,
        CommentError::Duplicate => StatusCode::CONFLICT,
        CommentError::RateLimited => StatusCode::TOO_MANY_REQUESTS,
        CommentError::Rejected(_) => StatusCode::UNPROCESSABLE_ENTITY,
        CommentError::Internal(message) => {
            tracing::error!("Comment request failed: {}", message);
            return api_error(StatusCode::INTERNAL_SERVER_ERROR, "Internal server error");
        }
    };
    api_error(status, e.to_string())
}

fn comments(state: &AppState) -> Result<Arc<CommentService>, ApiError> {
    state.comments.clone().ok_or_else(|| {
        api_error(
            StatusCode::NOT_FOUND,
            "Comments are not enabled ([comments].enabled)",
        )
    })
}

/// 公開ページの投稿先（`POST`、ベースパス込み）
fn post_url(state: &AppState, content_type: &str, id: &str) -> String {
    let base_path = state
        .config
        .server
        .normalized_base_path()
        .unwrap_or_default();
    format!(
        "{}/api/v1/contents/{}/{}/comments",
        base_path, content_type, id
    )
}

/// 公開ページのコメント欄（コメントを受け付ける種類のときだけ。失敗したら出さない）
pub async fn page_section(
    state: &AppState,
    content_type: &str,
    id: &str,
) -> Option<CommentSection> {
    let service = state.comments.as_ref()?;
    if !service.config.accepts(content_type) {
        return None;
    }
    let target = match service.target(content_type, id).await {
        Ok(target) => target,
        Err(CommentError::NotFound(_)) => return None,
        Err(e) => {
            tracing::error!(
                "Could not load the comments of {}/{}: {}",
                content_type,
                id,
                e
            );
            return None;
        }
    };
    match service
        .section(&target, post_url(state, content_type, &target.id))
        .await
    {
        Ok(section) => Some(section),
        Err(e) => {
            tracing::error!(
                "Could not load the comments of {}/{}: {}",
                content_type,
                id,
                e
            );
            None
        }
    }
}

// --- 公開 ---

/// `GET /api/v1/contents/{content_type}/{slug}/comments` — 承認済みのコメント（スレッドの順）
pub async fn list_public_comments(
    State(state): State<Arc<AppState>>,
    Path((content_type, slug)): Path<(String, String)>,
) -> Result<Json<CommentSection>, ApiError> {
    let service = comments(&state)?;
    let target = service
        .target(&content_type, &slug)
        .await
        .map_err(comment_error)?;
    let url = post_url(&state, &content_type, &target.id);
    Ok(Json(
        service.section(&target, url).await.map_err(comment_error)?,
    ))
}

/// `POST` の応答。承認待ち（スパムを含む）なら `comment` は無い
#[derive(Debug, Serialize)]
pub struct SubmitResponse {
    pub status: ModerationStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub comment: Option<PublicComment>,
}

/// `POST /api/v1/contents/{content_type}/{slug}/comments` — コメントを投稿する（ログイン不要）
pub async fn submit_comment(
    State(state): State<Arc<AppState>>,
    Path((content_type, slug)): Path<(String, String)>,
    ClientIp(ip): ClientIp,
    headers: axum::http::HeaderMap,
    Json(request): Json<CommentRequest>,
) -> Result<(StatusCode, Json<SubmitResponse>), ApiError> {
    let service = comments(&state)?;
    let target = service
        .target(&content_type, &slug)
        .await
        .map_err(comment_error)?;
    let client = ClientInfo {
        ip,
        user_agent: headers
            .get(axum::http::header::USER_AGENT)
            .and_then(|v| v.to_str().ok())
            .map(|s| s.chars().take(512).collect()),
    };
    let submitted = service
        .submit(&target, request, &client)
        .await
        .map_err(comment_error)?;
    let status = if submitted.status == ModerationStatus::Approved {
        StatusCode::CREATED
    } else {
        StatusCode::ACCEPTED
    };
    Ok((
        status,
        Json(SubmitResponse {
            status: submitted.status,
            comment: submitted.comment,
        }),
    ))
}

#[derive(Debug, Deserialize)]
pub struct UnsubscribeParams {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub token: String,
}

/// `GET /comments/unsubscribe?id=...&token=...` — 返信通知を止める（通知のメールのリンク）
pub async fn unsubscribe_page(
    State(state): State<Arc<AppState>>,
    Query(params): Query<UnsubscribeParams>,
) -> Response {
    let Some(service) = state.comments.clone() else {
        return render_not_found(&state, "Comments are not enabled.").await;
    };
    match service.unsubscribe(&params.id, &params.token).await {
        Ok(()) => {}
        Err(CommentError::NotFound(_)) => {
            return render_not_found(&state, "This link is invalid or has expired.").await
        }
        Err(e) => {
            tracing::error!("Failed to stop reply notifications: {}", e);
            return (StatusCode::INTERNAL_SERVER_ERROR, "Internal server error").into_response();
        }
    }
    let mut ctx = PageRenderContext {
        template: "page.html".to_string(),
        content_type: "page".to_string(),
        title: "Notifications stopped".to_string(),
        slug: "comments-unsubscribe".to_string(),
        content_html: "<p>You will no longer receive emails about replies to this comment.</p>"
            .to_string(),
        ..render::site_page_context(&state)
    };
    {
        let supervisor = state.supervisor.read().await;
        if let Ok(hooked) = supervisor.apply_render_page(ctx.clone()).await {
            ctx = hooked;
        }
    }
    let mut response = themed_response(
        state
            .theme
            .render(&ctx.template, &ctx)
            .map_err(|e| e.to_string()),
    );
    // トークンを外部に送らない
    response.headers_mut().insert(
        axum::http::header::REFERRER_POLICY,
        axum::http::HeaderValue::from_static("no-referrer"),
    );
    response
}

// --- 管理 ---

#[derive(Debug, Default, Deserialize)]
pub struct AdminListParams {
    /// `pending` / `approved` / `spam` / `trash`。省略時はすべて
    pub status: Option<String>,
    pub content_type: Option<String>,
    pub content_id: Option<String>,
    pub limit: Option<u64>,
    #[serde(default)]
    pub offset: u64,
}

fn parse_status(value: &str) -> Result<ModerationStatus, ApiError> {
    value
        .parse()
        .map_err(|e: String| api_error(StatusCode::BAD_REQUEST, e))
}

/// `GET /api/v1/comments` — コメントの一覧（新しい順）と状態ごとの件数（`editor`）
pub async fn list_comments(
    State(state): State<Arc<AppState>>,
    _session: EditorSession,
    Query(params): Query<AdminListParams>,
) -> Result<Json<AdminCommentPage>, ApiError> {
    let service = comments(&state)?;
    let status = match params.status.as_deref().filter(|s| !s.is_empty()) {
        Some(s) => Some(parse_status(s)?),
        None => None,
    };
    let query = AdminListQuery {
        status,
        content_type: params.content_type.filter(|s| !s.is_empty()),
        content_id: params.content_id.filter(|s| !s.is_empty()),
        limit: params.limit.unwrap_or(DEFAULT_LIMIT).clamp(1, MAX_LIMIT),
        offset: params.offset,
    };
    Ok(Json(service.list(&query).await.map_err(comment_error)?))
}

#[derive(Debug, Deserialize)]
pub struct UpdateCommentRequest {
    /// 新しい状態（`approved` など）
    #[serde(default)]
    pub status: Option<String>,
    /// 新しい本文（プレーンテキスト）
    #[serde(default)]
    pub body: Option<String>,
}

/// `PATCH /api/v1/comments/{id}` — 状態を変える（`{"status": "approved"}` など）、本文を直す
/// （`{"body": "..."}`）。両方を同時に渡せる。`editor`
pub async fn update_comment(
    State(state): State<Arc<AppState>>,
    _session: EditorSession,
    Path(id): Path<String>,
    audit: AuditContext,
    Json(payload): Json<UpdateCommentRequest>,
) -> Result<StatusCode, ApiError> {
    let service = comments(&state)?;
    if payload.status.is_none() && payload.body.is_none() {
        return Err(api_error(
            StatusCode::BAD_REQUEST,
            "Give a status, a body, or both",
        ));
    }
    let status = payload.status.as_deref().map(parse_status).transpose()?;
    // 本文は状態より先に直す（承認と同時に直したとき、通知に直した本文を入れる）
    if let Some(body) = &payload.body {
        service
            .edit(&audit, &id, body)
            .await
            .map_err(comment_error)?;
    }
    if let Some(status) = status {
        let changed = service
            .moderate(&audit, std::slice::from_ref(&id), status)
            .await
            .map_err(comment_error)?;
        if changed == 0 {
            return Err(api_error(
                StatusCode::NOT_FOUND,
                format!("Comment '{}' not found", id),
            ));
        }
    }
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Debug, Deserialize)]
pub struct ContentCommentsRequest {
    /// 新しいコメントを受け付けるか
    pub open: bool,
}

/// `GET /api/v1/contents/{content_type}/{id}/comments/state` — そのコンテンツへのコメントの受付の状態を
/// 取得する（`editor`）
pub async fn get_content_comments(
    State(state): State<Arc<AppState>>,
    _session: EditorSession,
    Path((content_type, id)): Path<(String, String)>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let service = comments(&state)?;
    let (content_id, open) = service
        .get_content_open(&content_type, &id)
        .await
        .map_err(comment_error)?;
    Ok(Json(json!({
        "content_type": content_type,
        "content_id": content_id,
        "open": open,
    })))
}

/// `PUT /api/v1/contents/{content_type}/{id}/comments/state` — そのコンテンツへのコメントの受付を
/// 止める・再開する（`{"open": false}`。`editor`）。すでに付いたコメントは公開ページに残る
pub async fn set_content_comments(
    State(state): State<Arc<AppState>>,
    _session: EditorSession,
    Path((content_type, id)): Path<(String, String)>,
    audit: AuditContext,
    Json(payload): Json<ContentCommentsRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let service = comments(&state)?;
    let target = service
        .set_content_open(&audit, &content_type, &id, payload.open)
        .await
        .map_err(comment_error)?;
    Ok(Json(json!({
        "content_type": target.content_type,
        "content_id": target.id,
        "open": !target.comments_closed,
    })))
}

/// `DELETE /api/v1/comments/{id}` — 完全に削除する（`editor`）
pub async fn delete_comment(
    State(state): State<Arc<AppState>>,
    _session: EditorSession,
    Path(id): Path<String>,
    audit: AuditContext,
) -> Result<StatusCode, ApiError> {
    let service = comments(&state)?;
    let deleted = service
        .delete(&audit, std::slice::from_ref(&id))
        .await
        .map_err(comment_error)?;
    if deleted == 0 {
        return Err(api_error(
            StatusCode::NOT_FOUND,
            format!("Comment '{}' not found", id),
        ));
    }
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Debug, Deserialize)]
pub struct BulkRequest {
    pub ids: Vec<String>,
    /// `approved` / `pending` / `spam` / `trash`（状態の変更）または `delete`（完全に削除）
    pub action: String,
}

/// `POST /api/v1/comments/bulk` — まとめて状態を変える・削除する（`editor`）。変えた件数を返す
pub async fn bulk_comments(
    State(state): State<Arc<AppState>>,
    _session: EditorSession,
    audit: AuditContext,
    Json(payload): Json<BulkRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let service = comments(&state)?;
    let count = if payload.action == "delete" {
        service.delete(&audit, &payload.ids).await
    } else {
        let status = parse_status(&payload.action)?;
        service.moderate(&audit, &payload.ids, status).await
    }
    .map_err(comment_error)?;
    Ok(Json(json!({ "count": count })))
}

#[derive(Debug, Deserialize)]
pub struct ReplyRequest {
    pub body: String,
}

/// `POST /api/v1/comments/{id}/replies` — Admin Studio から返信する（`editor`）。
/// 返信は承認済みで公開し、返信先が承認待ちなら一緒に承認する
pub async fn reply_comment(
    State(state): State<Arc<AppState>>,
    session: EditorSession,
    Path(id): Path<String>,
    audit: AuditContext,
    Json(payload): Json<ReplyRequest>,
) -> Result<(StatusCode, Json<serde_json::Value>), ApiError> {
    let service = comments(&state)?;
    let username = session.0.user_id.clone();
    let user = state
        .db
        .get_user_by_username(&username)
        .await
        .map_err(|e| comment_error(e.into()))?;
    let author = StaffAuthor {
        username,
        display_name: user.as_ref().and_then(|u| u.display_name.clone()),
        email: user.and_then(|u| u.email),
    };
    let record = service
        .reply(&audit, &id, &author, &payload.body)
        .await
        .map_err(comment_error)?;
    Ok((
        StatusCode::CREATED,
        Json(json!({ "id": record.id, "status": record.status, "created_at": record.created_at })),
    ))
}

/// `GET /api/v1/comments/settings` — 承認制の設定（`editor`）
pub async fn get_comment_settings(
    State(state): State<Arc<AppState>>,
    _session: EditorSession,
) -> Result<Json<ModerationSettings>, ApiError> {
    let service = comments(&state)?;
    Ok(Json(service.settings().await.map_err(comment_error)?))
}

/// `PUT /api/v1/comments/settings` — 承認制の設定を変える（`admin`）。`null` は設定ファイルの値に戻す。
/// DB に保存し、再起動せずにすべてのサーバーで次の投稿から効く
pub async fn update_comment_settings(
    State(state): State<Arc<AppState>>,
    _session: AdminSession,
    audit: AuditContext,
    Json(payload): Json<PolicyOverrides>,
) -> Result<Json<ModerationSettings>, ApiError> {
    let service = comments(&state)?;
    Ok(Json(
        service
            .update_settings(&audit, payload)
            .await
            .map_err(comment_error)?,
    ))
}

#[cfg(test)]
#[path = "comments_tests.rs"]
mod tests;
