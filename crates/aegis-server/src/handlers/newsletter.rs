//! ニュースレター（`[newsletter]`）。公開（ログイン不要）: 申し込みのページ `/newsletter` と送信（HTML のフォームと
//! JSON の API）、メールのリンクの確認 `/newsletter/confirm`・解除 `/newsletter/unsubscribe`（どちらも開くとボタンを
//! 出し、押すと実行する）。管理（`admin`）: 購読者の一覧・削除・書き出し、公開中のコンテンツの配信と履歴

use std::collections::HashMap;
use std::sync::Arc;

use axum::{
    extract::{Form, Query, State},
    http::{header, HeaderMap, HeaderValue, StatusCode},
    response::{IntoResponse, Redirect, Response},
    Json,
};
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::audit::{AuditContext, ClientIp};
use crate::auth::authorize::RequireRole;
use crate::auth::AdminSession;
use crate::handlers::content::{is_currently_published, summary_excerpt};
use crate::handlers::public::{render_not_found, themed_response};
use crate::render;
use crate::state::AppState;
use aegis_captcha::CaptchaError;
use aegis_core::PageRenderContext;
use aegis_db::{NewsletterIssueRecord, NewsletterListQuery, NewsletterSubscriberRecord};
use aegis_newsletter::{
    ClientInfo, ConfirmOutcome, IssueContent, NewsletterError, NewsletterService,
    UnsubscribeOutcome, STATUSES,
};

type ApiError = (StatusCode, Json<serde_json::Value>);

const DEFAULT_LIMIT: u64 = 50;
const MAX_LIMIT: u64 = 200;
/// 要約の最大文字数（自動で作るものも、管理者が書いたものも）
const MAX_SUMMARY_CHARS: usize = 500;

/// 申し込みのページの最低限の見た目（テーマの CSS が優先されるよう詳細度は 0）
const STYLE: &str = "<style>\
:where(.aegis-newsletter-form){display:flex;flex-direction:column;gap:.75rem;margin:1.5rem 0;max-width:30rem}\
:where(.aegis-newsletter-form) input[type=email]{font:inherit;color:inherit;background:transparent;border:1px solid rgba(127,127,127,.45);border-radius:6px;padding:.55rem .7rem;width:100%;box-sizing:border-box}\
:where(.aegis-newsletter-submit){align-self:flex-start;font:inherit;font-weight:600;padding:.6rem 1.4rem;border-radius:6px;border:1px solid rgba(127,127,127,.45);background:rgba(127,127,127,.12);color:inherit;cursor:pointer}\
:where(.aegis-newsletter-notice){border-left:3px solid #ef4444;padding:.5rem .9rem;margin:1rem 0}\
:where(.aegis-newsletter-sent){border-left:3px solid #10b981;padding:.75rem 1rem;margin:1.5rem 0}\
</style>";

fn api_error(status: StatusCode, code: &str, message: impl Into<String>) -> ApiError {
    (
        status,
        Json(json!({ "error": message.into(), "code": code })),
    )
}

fn newsletter_error(e: NewsletterError) -> ApiError {
    let message = e.to_string();
    match e {
        NewsletterError::Invalid(_) => api_error(StatusCode::BAD_REQUEST, "invalid_email", message),
        NewsletterError::RateLimited => {
            api_error(StatusCode::TOO_MANY_REQUESTS, "rate_limited", message)
        }
        NewsletterError::Captcha(CaptchaError::Failed) => {
            api_error(StatusCode::BAD_REQUEST, "captcha_failed", message)
        }
        NewsletterError::Captcha(CaptchaError::Unavailable) => api_error(
            StatusCode::SERVICE_UNAVAILABLE,
            "captcha_unavailable",
            message,
        ),
        NewsletterError::NotFound(_) => api_error(StatusCode::NOT_FOUND, "not_found", message),
        NewsletterError::AlreadySent(_) => api_error(StatusCode::CONFLICT, "already_sent", message),
        NewsletterError::Unavailable(_) => {
            api_error(StatusCode::SERVICE_UNAVAILABLE, "unavailable", message)
        }
        NewsletterError::NoSubscribers => {
            api_error(StatusCode::CONFLICT, "no_subscribers", message)
        }
        NewsletterError::Internal(message) => {
            tracing::error!("Newsletter request failed: {}", message);
            api_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "internal",
                "Internal server error",
            )
        }
    }
}

fn newsletter(state: &AppState) -> Result<Arc<NewsletterService>, ApiError> {
    state.newsletter.clone().ok_or_else(|| {
        api_error(
            StatusCode::NOT_FOUND,
            "newsletter_disabled",
            "The newsletter is not enabled ([newsletter].enabled)",
        )
    })
}

fn base_path(state: &AppState) -> String {
    state
        .config
        .server
        .normalized_base_path()
        .unwrap_or_default()
}

fn path(state: &AppState, path: &str) -> String {
    format!("{}{}", base_path(state), path)
}

/// 申し込み・確認・解除のページを、テーマの `page.html` で描く。トークンを含むページなので保存しない。
/// リンクのトークン（URL のクエリ）は、サイト共通の `Referrer-Policy`（既定は `strict-origin-when-cross-origin`）
/// により別のサイトには送られない
async fn render_page(
    state: &AppState,
    title: &str,
    content_html: String,
    status: StatusCode,
) -> Response {
    let mut ctx = PageRenderContext {
        template: "page.html".to_string(),
        content_type: "page".to_string(),
        title: title.to_string(),
        slug: "newsletter".to_string(),
        content_html: format!("{}{}", STYLE, content_html),
        ..render::site_page_context(state)
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
    if response.status() == StatusCode::OK {
        *response.status_mut() = status;
    }
    let headers = response.headers_mut();
    headers.insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("private, no-store"),
    );
    response
}

fn notice(class: &str, text: &str) -> String {
    format!(
        "<div class=\"aegis-newsletter-{}\" role=\"{}\"><p style=\"margin:0\">{}</p></div>",
        class,
        if class == "sent" { "status" } else { "alert" },
        aegis_ast::renderer::escape_html(text)
    )
}

// --- 公開: 申し込み ---

#[derive(Debug, Default, Deserialize)]
pub struct PageParams {
    /// 申し込んだ後のリダイレクト（`?sent=1`）
    #[serde(default)]
    pub sent: Option<String>,
}

/// 申し込みのページの本文（説明とフォーム、または「確認のメールを送りました」）
async fn signup_page(
    state: &AppState,
    service: &NewsletterService,
    email: &str,
    error: Option<&str>,
    sent: bool,
    status: StatusCode,
) -> Response {
    let description = format!(
        "<p>{}</p>",
        aegis_ast::renderer::escape_html(service.text("description"))
    );
    let body = if sent {
        notice("sent", service.text("sent"))
    } else {
        format!(
            "{}{}",
            description,
            service.form_html(&path(state, "/newsletter/subscribe"), email, error)
        )
    };
    render_page(state, service.text("title"), body, status).await
}

/// `GET /newsletter` — 申し込みのページ（`?sent=1` は申し込んだ後の表示）
pub async fn render_newsletter_page(
    State(state): State<Arc<AppState>>,
    Query(params): Query<PageParams>,
) -> Response {
    let Some(service) = state.newsletter.clone() else {
        return render_not_found(&state, "This page does not exist.").await;
    };
    signup_page(
        &state,
        &service,
        "",
        None,
        params.sent.is_some(),
        StatusCode::OK,
    )
    .await
}

fn client_info(ip: Option<String>) -> ClientInfo {
    ClientInfo { ip }
}

/// `POST /newsletter/subscribe` — HTML のフォームからの申し込み（`application/x-www-form-urlencoded`）。
/// 受け付けたら `?sent=1` へリダイレクトし、誤りはフォームに出して返す（JavaScript なしで動く）。
/// アドレスが登録済みか・確認待ちかは、どの応答からも分からない
pub async fn submit_subscribe_form(
    State(state): State<Arc<AppState>>,
    ClientIp(ip): ClientIp,
    Form(values): Form<HashMap<String, String>>,
) -> Response {
    let Some(service) = state.newsletter.clone() else {
        return render_not_found(&state, "This page does not exist.").await;
    };
    let email = values.get("email").cloned().unwrap_or_default();
    let result = service
        .subscribe(
            &email,
            values
                .get(aegis_newsletter::HONEYPOT_FIELD)
                .map(String::as_str),
            service.captcha_token(&values),
            &client_info(ip),
        )
        .await;
    let (key, status) = match result {
        Ok(()) => {
            return Redirect::to(&path(&state, "/newsletter?sent=1")).into_response();
        }
        Err(NewsletterError::Invalid(_)) => ("invalid_email", StatusCode::BAD_REQUEST),
        Err(NewsletterError::RateLimited) => ("rate_limited", StatusCode::TOO_MANY_REQUESTS),
        Err(NewsletterError::Captcha(CaptchaError::Failed)) => {
            ("captcha_failed", StatusCode::BAD_REQUEST)
        }
        Err(NewsletterError::Captcha(CaptchaError::Unavailable)) => {
            ("captcha_unavailable", StatusCode::SERVICE_UNAVAILABLE)
        }
        Err(e) => {
            tracing::error!("Newsletter sign-up failed: {}", e);
            ("failed", StatusCode::INTERNAL_SERVER_ERROR)
        }
    };
    signup_page(
        &state,
        &service,
        &email,
        Some(service.text(key)),
        false,
        status,
    )
    .await
}

/// `POST /api/v1/newsletter/subscribe` — JSON での申し込み（`{"email": "...", "website": ""}`。ログイン不要）。
/// 受け付けたら `202 {"status": "pending"}`（登録済みでも同じ）。CAPTCHA を設定しているときはトークンの欄も要る
pub async fn subscribe_api(
    State(state): State<Arc<AppState>>,
    ClientIp(ip): ClientIp,
    Json(body): Json<HashMap<String, serde_json::Value>>,
) -> Result<(StatusCode, Json<serde_json::Value>), ApiError> {
    let service = newsletter(&state)?;
    let values: HashMap<String, String> = body
        .into_iter()
        .filter_map(|(k, v)| v.as_str().map(|s| (k, s.to_string())))
        .collect();
    service
        .subscribe(
            values.get("email").map(String::as_str).unwrap_or_default(),
            values
                .get(aegis_newsletter::HONEYPOT_FIELD)
                .map(String::as_str),
            service.captcha_token(&values),
            &client_info(ip),
        )
        .await
        .map_err(newsletter_error)?;
    Ok((StatusCode::ACCEPTED, Json(json!({ "status": "pending" }))))
}

// --- 公開: メールのリンク ---

#[derive(Debug, Default, Deserialize)]
pub struct TokenQuery {
    #[serde(default)]
    pub token: String,
}

#[derive(Debug, Default, Deserialize)]
pub struct TokenForm {
    #[serde(default)]
    pub token: String,
}

/// リンクを開いたとき: すぐには実行せず、ボタンのあるページを出す（メールを自動で開くソフトに実行させない）
async fn token_page(state: &AppState, kind: &'static str, token: &str) -> Response {
    let Some(service) = state.newsletter.clone() else {
        return render_not_found(state, "This page does not exist.").await;
    };
    let title = service.text(&format!("{}_title", kind));
    let action = path(state, &format!("/newsletter/{}", kind));
    render_page(
        state,
        title,
        service.token_form_html(kind, &action, token),
        StatusCode::OK,
    )
    .await
}

/// `GET /newsletter/confirm?token=...` — 購読を確定するボタンのページ（メールのリンク）
pub async fn show_confirm(
    State(state): State<Arc<AppState>>,
    Query(query): Query<TokenQuery>,
) -> Response {
    token_page(&state, "confirm", &query.token).await
}

/// `GET /newsletter/unsubscribe?token=...` — 配信を停止するボタンのページ（メールのリンク）
pub async fn show_unsubscribe(
    State(state): State<Arc<AppState>>,
    Query(query): Query<TokenQuery>,
) -> Response {
    token_page(&state, "unsubscribe", &query.token).await
}

/// `POST /newsletter/confirm` — 購読を確定する
pub async fn do_confirm(
    State(state): State<Arc<AppState>>,
    Form(form): Form<TokenForm>,
) -> Response {
    let Some(service) = state.newsletter.clone() else {
        return render_not_found(&state, "This page does not exist.").await;
    };
    match service.confirm(&form.token).await {
        Ok(ConfirmOutcome::Confirmed | ConfirmOutcome::AlreadyConfirmed) => {
            render_page(
                &state,
                service.text("confirmed_title"),
                notice("sent", service.text("confirmed")),
                StatusCode::OK,
            )
            .await
        }
        Err(NewsletterError::NotFound(message)) => {
            render_page(
                &state,
                service.text("confirm_title"),
                notice("notice", &message),
                StatusCode::NOT_FOUND,
            )
            .await
        }
        Err(e) => internal_page(&state, &service, e).await,
    }
}

/// `POST /newsletter/unsubscribe` — 配信を停止する
pub async fn do_unsubscribe(
    State(state): State<Arc<AppState>>,
    Form(form): Form<TokenForm>,
) -> Response {
    let Some(service) = state.newsletter.clone() else {
        return render_not_found(&state, "This page does not exist.").await;
    };
    match service.unsubscribe(&form.token).await {
        Ok(UnsubscribeOutcome::Unsubscribed | UnsubscribeOutcome::AlreadyUnsubscribed) => {
            render_page(
                &state,
                service.text("unsubscribed_title"),
                notice("sent", service.text("unsubscribed")),
                StatusCode::OK,
            )
            .await
        }
        Err(NewsletterError::NotFound(message)) => {
            render_page(
                &state,
                service.text("unsubscribe_title"),
                notice("notice", &message),
                StatusCode::NOT_FOUND,
            )
            .await
        }
        Err(e) => internal_page(&state, &service, e).await,
    }
}

async fn internal_page(
    state: &AppState,
    service: &NewsletterService,
    e: NewsletterError,
) -> Response {
    tracing::error!("Newsletter link failed: {}", e);
    render_page(
        state,
        service.text("title"),
        notice("notice", service.text("failed")),
        StatusCode::INTERNAL_SERVER_ERROR,
    )
    .await
}

// --- 管理 ---

/// 一覧に出す購読者（確認・解除のトークンは出さない）
#[derive(Debug, Serialize)]
pub struct SubscriberView {
    pub id: String,
    pub email: String,
    pub status: String,
    pub ip: Option<String>,
    pub created_at: String,
    pub confirmed_at: Option<String>,
    pub unsubscribed_at: Option<String>,
}

impl From<NewsletterSubscriberRecord> for SubscriberView {
    fn from(s: NewsletterSubscriberRecord) -> Self {
        Self {
            id: s.id,
            email: s.email,
            status: s.status,
            ip: s.ip,
            created_at: s.created_at,
            confirmed_at: s.confirmed_at,
            unsubscribed_at: s.unsubscribed_at,
        }
    }
}

#[derive(Debug, Default, Deserialize)]
pub struct ListParams {
    /// `pending` / `active` / `unsubscribed`。省略時はすべて
    pub status: Option<String>,
    /// メールアドレスへの部分一致
    pub q: Option<String>,
    pub limit: Option<u64>,
    #[serde(default)]
    pub offset: u64,
}

/// `GET /api/v1/newsletter/subscribers` — 購読者（登録の新しい順）と、状態ごとの人数（`admin`）
pub async fn list_subscribers(
    State(state): State<Arc<AppState>>,
    RequireRole(_session): AdminSession,
    Query(params): Query<ListParams>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let service = newsletter(&state)?;
    let status = params.status.filter(|s| !s.is_empty());
    if status.as_deref().is_some_and(|s| !STATUSES.contains(&s)) {
        return Err(api_error(
            StatusCode::BAD_REQUEST,
            "invalid_status",
            "status must be pending, active or unsubscribed",
        ));
    }
    let (page, counts) = service
        .list(&NewsletterListQuery {
            status,
            search: params.q.filter(|q| !q.trim().is_empty()),
            limit: Some(params.limit.unwrap_or(DEFAULT_LIMIT).clamp(1, MAX_LIMIT)),
            offset: params.offset,
        })
        .await
        .map_err(newsletter_error)?;
    Ok(Json(json!({
        "subscribers": page.subscribers.into_iter().map(SubscriberView::from).collect::<Vec<_>>(),
        "total": page.total,
        "counts": counts,
    })))
}

#[derive(Debug, Deserialize)]
pub struct DeleteRequest {
    pub ids: Vec<String>,
}

/// `POST /api/v1/newsletter/subscribers/delete` — 購読者を記録ごと消す（`{"ids": [...]}`。`admin`）
pub async fn delete_subscribers(
    State(state): State<Arc<AppState>>,
    RequireRole(_session): AdminSession,
    audit: AuditContext,
    Json(payload): Json<DeleteRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let service = newsletter(&state)?;
    let count = service
        .delete(&audit, &payload.ids)
        .await
        .map_err(newsletter_error)?;
    Ok(Json(json!({ "count": count })))
}

/// `GET /api/v1/newsletter/subscribers/export` — 配信する購読者（`active`）のメールアドレスを CSV で書き出す
/// （`admin`）。表計算ソフトで式として読まれないよう、先頭の `=` `+` `-` `@` は書き出さない（登録時に弾いている）
pub async fn export_subscribers(
    State(state): State<Arc<AppState>>,
    RequireRole(_session): AdminSession,
) -> Result<Response, ApiError> {
    let service = newsletter(&state)?;
    let emails = service.active_emails().await.map_err(newsletter_error)?;
    let mut csv = String::from("email\r\n");
    for email in emails {
        csv.push_str(&email);
        csv.push_str("\r\n");
    }
    let mut response = (StatusCode::OK, csv).into_response();
    let headers: &mut HeaderMap = response.headers_mut();
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("text/csv; charset=utf-8"),
    );
    headers.insert(
        header::CONTENT_DISPOSITION,
        HeaderValue::from_static("attachment; filename=\"newsletter-subscribers.csv\""),
    );
    headers.insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("private, no-store"),
    );
    Ok(response)
}

#[derive(Debug, Default, Deserialize)]
pub struct IssueListParams {
    pub limit: Option<u64>,
    #[serde(default)]
    pub offset: u64,
}

/// `GET /api/v1/newsletter/issues` — 配信の履歴（新しい順。`admin`）
pub async fn list_issues(
    State(state): State<Arc<AppState>>,
    RequireRole(_session): AdminSession,
    Query(params): Query<IssueListParams>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let service = newsletter(&state)?;
    let (issues, total) = service
        .issues(
            params.limit.unwrap_or(DEFAULT_LIMIT).clamp(1, MAX_LIMIT),
            params.offset,
        )
        .await
        .map_err(newsletter_error)?;
    Ok(Json(json!({ "issues": issues, "total": total })))
}

#[derive(Debug, Deserialize)]
pub struct SendIssueRequest {
    pub content_type: String,
    pub slug_or_id: String,
    /// メール本文の要約。省略すると `excerpt` フィールドか本文の最初の段落
    #[serde(default)]
    pub summary: Option<String>,
    /// 指定すると、このアドレスにだけ送る（購読者には送らず、履歴にも残さない）
    #[serde(default)]
    pub test_to: Option<String>,
    /// すでに配信したコンテンツを送り直す
    #[serde(default)]
    pub force: bool,
}

/// 公開中のコンテンツ（訪問者が今見ている版）から配信の内容を作る
async fn issue_content(
    state: &AppState,
    request: &SendIssueRequest,
) -> Result<IssueContent, ApiError> {
    let schema = state
        .schemas
        .read()
        .await
        .get(&request.content_type)
        .cloned()
        .ok_or_else(|| {
            api_error(
                StatusCode::NOT_FOUND,
                "schema_not_found",
                format!("Schema '{}' not found", request.content_type),
            )
        })?;
    let record = state
        .db
        .get_published_content(&schema, &request.slug_or_id)
        .await
        .map_err(|e| {
            tracing::error!("Could not read the content to send: {}", e);
            api_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "internal",
                "Internal server error",
            )
        })?
        .filter(|r| {
            is_currently_published(&r.status, r.unpublish_at.as_deref(), chrono::Utc::now())
        })
        .ok_or_else(|| {
            api_error(
                StatusCode::BAD_REQUEST,
                "content_not_published",
                "Only published content can be sent",
            )
        })?;
    let url = crate::site::content_url(&state.config, &record.content_type, &record.slug);
    if url.is_empty() {
        return Err(api_error(
            StatusCode::BAD_REQUEST,
            "content_not_public",
            "This content has no public page to link to",
        ));
    }
    let summary = match request.summary.as_deref().map(str::trim) {
        Some(summary) if !summary.is_empty() => summary.chars().take(MAX_SUMMARY_CHARS).collect(),
        _ => summary_excerpt(&record)
            .chars()
            .take(MAX_SUMMARY_CHARS)
            .collect(),
    };
    Ok(IssueContent {
        content_type: record.content_type,
        content_id: record.id,
        title: record.title,
        summary,
        url,
    })
}

/// `POST /api/v1/newsletter/issues` — 公開中のコンテンツを購読者全員に送る（`admin`）。`test_to` を指定すると
/// そのアドレスにだけ送る。すでに送ったコンテンツは `409 already_sent`（`force: true` で送り直す）
pub async fn send_issue(
    State(state): State<Arc<AppState>>,
    RequireRole(session): AdminSession,
    audit: AuditContext,
    Json(request): Json<SendIssueRequest>,
) -> Result<(StatusCode, Json<serde_json::Value>), ApiError> {
    let service = newsletter(&state)?;
    let content = issue_content(&state, &request).await?;
    if let Some(to) = request.test_to.as_deref().filter(|t| !t.trim().is_empty()) {
        service
            .send_test(&content, to)
            .await
            .map_err(newsletter_error)?;
        return Ok((StatusCode::ACCEPTED, Json(json!({ "status": "test_sent" }))));
    }
    let issue: NewsletterIssueRecord = service
        .send_issue(&audit, &content, Some(session.user_id), request.force)
        .await
        .map_err(newsletter_error)?;
    Ok((StatusCode::CREATED, Json(json!(issue))))
}

#[cfg(test)]
#[path = "newsletter_tests.rs"]
mod tests;
