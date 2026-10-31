//! 問い合わせフォーム（`[contact]`）。公開（ログイン不要）: フォームのページ `/contact/{form}` と送信（HTML の
//! フォームからの `POST` と JSON の API）、フォームの一覧。管理（`editor` 以上）: 受信箱の一覧・状態の変更・削除

use std::collections::HashMap;
use std::sync::Arc;

use axum::{
    extract::{FromRequest, Multipart, Path, Query, Request, State},
    http::{header, HeaderMap, HeaderValue, StatusCode},
    response::{IntoResponse, Redirect, Response},
    Form, Json,
};
use serde::Deserialize;
use serde_json::json;

use crate::audit::{AuditContext, ClientIp};
use crate::auth::EditorSession;
use crate::handlers::public::{render_not_found, themed_response};
use crate::render;
use crate::state::AppState;
use aegis_contact::{
    AdminListQuery, AdminMessage, AdminMessagePage, CaptchaError, ClientInfo, ContactError,
    ContactService, FieldError, FormConfig, FormSummary, UploadedFile,
};
use aegis_core::PageRenderContext;

type ApiError = (StatusCode, Json<serde_json::Value>);

const DEFAULT_LIMIT: u64 = 50;
const MAX_LIMIT: u64 = 200;

fn api_error(status: StatusCode, message: impl Into<String>) -> ApiError {
    (status, Json(json!({ "error": message.into() })))
}

fn contact_error(e: ContactError) -> ApiError {
    let status = match &e {
        ContactError::Invalid(errors) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({ "error": e.to_string(), "errors": errors })),
            )
        }
        ContactError::BadRequest(_) => StatusCode::BAD_REQUEST,
        ContactError::NotFound(_) => StatusCode::NOT_FOUND,
        ContactError::RateLimited => StatusCode::TOO_MANY_REQUESTS,
        ContactError::Captcha(failure) => {
            let (status, code) = match failure {
                CaptchaError::Failed => (StatusCode::BAD_REQUEST, "captcha_failed"),
                CaptchaError::Unavailable => {
                    (StatusCode::SERVICE_UNAVAILABLE, "captcha_unavailable")
                }
            };
            return (
                status,
                Json(json!({ "error": e.to_string(), "code": code })),
            );
        }
        ContactError::Internal(message) => {
            tracing::error!("Contact request failed: {}", message);
            return api_error(StatusCode::INTERNAL_SERVER_ERROR, "Internal server error");
        }
    };
    api_error(status, e.to_string())
}

fn contact(state: &AppState) -> Result<Arc<ContactService>, ApiError> {
    state.contact.clone().ok_or_else(|| {
        api_error(
            StatusCode::NOT_FOUND,
            "Contact forms are not enabled ([contact].enabled)",
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

/// フォームの送信先（ベースパス込み）
pub fn form_action(state: &AppState, form: &str) -> String {
    format!("{}{}", base_path(state), aegis_contact::form_path(form))
}

fn client_info(ip: Option<String>, headers: &HeaderMap) -> ClientInfo {
    ClientInfo {
        ip,
        user_agent: headers
            .get(header::USER_AGENT)
            .and_then(|v| v.to_str().ok())
            .map(|s| s.chars().take(512).collect()),
    }
}

/// 本文のブロック `core/contact-form`（`{"form": "default"}`）の HTML。無効・知らないフォームなら空
pub fn block_html(state: &AppState, attributes: &serde_json::Value) -> String {
    let Some(service) = &state.contact else {
        return String::new();
    };
    let name = attributes
        .get("form")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .unwrap_or(&service.default_form().name);
    match service.config.form(name) {
        Some(form) => format!(
            "<div class=\"aegis-contact\">\n{}\n</div>",
            service.form_html(form, &form_action(state, &form.name), &HashMap::new(), &[])
        ),
        None => String::new(),
    }
}

// --- 公開 ---

#[derive(Debug, Default, Deserialize)]
pub struct PageParams {
    /// 送信した後のリダイレクト（`?sent=1`）
    #[serde(default)]
    pub sent: Option<String>,
}

/// フォームのページを描く。`values` / `errors` は送信に失敗したときの再表示、`notice` は誤り以外の知らせ
async fn render_form_page(
    state: &AppState,
    form: &FormConfig,
    sent: bool,
    values: &HashMap<String, String>,
    errors: &[FieldError],
    notice: Option<&str>,
    status: StatusCode,
) -> Response {
    let action = form_action(state, &form.name);
    let service = state.contact.as_ref();
    let mut fields = HashMap::new();
    fields.insert(
        "form".to_string(),
        json!({
            "name": form.name,
            "title": form.title,
            "description": form.description,
            "submit_label": form.submit_label,
            "success_message": form.success_message,
            "fields": form.fields,
            "action": action,
            "honeypot": aegis_contact::config::HONEYPOT_FIELD,
            "captcha_html": service.map(|s| s.captcha_widget_html()).unwrap_or_default(),
        }),
    );
    fields.insert("sent".to_string(), json!(sent));
    fields.insert(
        "form_html".to_string(),
        json!(if sent {
            String::new()
        } else {
            service
                .map(|s| s.form_html(form, &action, values, errors))
                .unwrap_or_default()
        }),
    );
    fields.insert("values".to_string(), json!(values));
    fields.insert("errors".to_string(), json!(errors));
    fields.insert("notice".to_string(), json!(notice.unwrap_or_default()));

    let mut ctx = PageRenderContext {
        template: "contact.html".to_string(),
        content_type: "contact".to_string(),
        title: form.title.clone(),
        slug: form.name.clone(),
        fields,
        extra_head: render::build_extra_head(
            state,
            Some(&form.title),
            None,
            None,
            Some(&aegis_contact::form_path(&form.name)),
            false,
        ),
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
    if sent || status != StatusCode::OK {
        // 入力を含む・送信した直後のページは CDN・ブラウザに残さない
        response.headers_mut().insert(
            header::CACHE_CONTROL,
            HeaderValue::from_static("private, no-store"),
        );
    }
    response
}

/// `GET /contact` — 既定のフォーム（`default`、無ければ最初のフォーム）
pub async fn render_default_contact(
    State(state): State<Arc<AppState>>,
    Query(params): Query<PageParams>,
) -> Response {
    let Some(service) = state.contact.clone() else {
        return render_not_found(&state, "This page does not exist.").await;
    };
    let form = service.default_form();
    render_form_page(
        &state,
        form,
        params.sent.is_some(),
        &HashMap::new(),
        &[],
        None,
        StatusCode::OK,
    )
    .await
}

/// `GET /contact/{form}` — フォームのページ（`?sent=1` は送信した後の表示）
pub async fn render_contact(
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
    Query(params): Query<PageParams>,
) -> Response {
    let Some(service) = state.contact.clone() else {
        return render_not_found(&state, "This page does not exist.").await;
    };
    let Ok(form) = service.form(&name) else {
        return render_not_found(&state, "This form does not exist.").await;
    };
    render_form_page(
        &state,
        form,
        params.sent.is_some(),
        &HashMap::new(),
        &[],
        None,
        StatusCode::OK,
    )
    .await
}

/// 送られてきたフォーム。`multipart/form-data`（添付ファイルあり）と `application/x-www-form-urlencoded` の両方を受ける。
/// 読めない・大きすぎるときは `Err`（送信した人に見せる知らせ）
async fn read_submission(
    state: &Arc<AppState>,
    request: Request,
) -> Result<(HashMap<String, String>, Vec<UploadedFile>), &'static str> {
    let is_multipart = request
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|v| {
            v.trim_start()
                .to_ascii_lowercase()
                .starts_with("multipart/form-data")
        });
    if !is_multipart {
        return Form::<HashMap<String, String>>::from_request(request, state)
            .await
            .map(|Form(values)| (values, Vec::new()))
            .map_err(|_| "invalid");
    }
    let mut multipart = Multipart::from_request(request, state)
        .await
        .map_err(|_| "invalid")?;
    let mut values = HashMap::new();
    let mut files = Vec::new();
    while let Some(field) = multipart.next_field().await.map_err(|e| {
        if e.status() == StatusCode::PAYLOAD_TOO_LARGE {
            "too_large"
        } else {
            "invalid"
        }
    })? {
        let name = field.name().unwrap_or_default().to_string();
        if let Some(filename) = field.file_name().map(str::to_string) {
            let data = field.bytes().await.map_err(|e| {
                if e.status() == StatusCode::PAYLOAD_TOO_LARGE {
                    "too_large"
                } else {
                    "invalid"
                }
            })?;
            files.push(UploadedFile {
                field: name,
                filename,
                data: data.to_vec(),
            });
        } else {
            let text = field.text().await.map_err(|_| "invalid")?;
            values.insert(name, text);
        }
    }
    Ok((values, files))
}

/// `POST /contact/{form}` — HTML のフォームからの送信（`application/x-www-form-urlencoded`、添付ファイルがあれば
/// `multipart/form-data`）。受け付けたら `?sent=1` へリダイレクトし、入力の誤りはフォームに出して返す
/// （JavaScript なしで動く）
pub async fn submit_contact_form(
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
    ClientIp(ip): ClientIp,
    headers: HeaderMap,
    request: Request,
) -> Response {
    let Some(service) = state.contact.clone() else {
        return render_not_found(&state, "This page does not exist.").await;
    };
    let Ok(form) = service.form(&name) else {
        return render_not_found(&state, "This form does not exist.").await;
    };
    let (values, files) = match read_submission(&state, request).await {
        Ok(submission) => submission,
        Err(reason) => {
            let (key, status) = if reason == "too_large" {
                ("upload_too_large", StatusCode::PAYLOAD_TOO_LARGE)
            } else {
                ("failed", StatusCode::BAD_REQUEST)
            };
            let notice = service.text(key);
            return render_form_page(
                &state,
                form,
                false,
                &HashMap::new(),
                &[],
                Some(notice),
                status,
            )
            .await;
        }
    };
    let form = form.clone();
    match service
        .submit_with_files(&name, &values, &files, &client_info(ip, &headers))
        .await
    {
        Ok(_) => Redirect::to(&format!("{}?sent=1", form_action(&state, &name))).into_response(),
        Err(ContactError::Invalid(errors)) => {
            render_form_page(
                &state,
                &form,
                false,
                &values,
                &errors,
                Some(service.text("check_fields")),
                StatusCode::BAD_REQUEST,
            )
            .await
        }
        Err(ContactError::RateLimited) => {
            render_form_page(
                &state,
                &form,
                false,
                &values,
                &[],
                Some(service.text("rate_limited")),
                StatusCode::TOO_MANY_REQUESTS,
            )
            .await
        }
        Err(ContactError::Captcha(failure)) => {
            let (key, status) = match failure {
                CaptchaError::Failed => ("captcha_failed", StatusCode::BAD_REQUEST),
                CaptchaError::Unavailable => {
                    ("captcha_unavailable", StatusCode::SERVICE_UNAVAILABLE)
                }
            };
            render_form_page(
                &state,
                &form,
                false,
                &values,
                &[],
                Some(service.text(key)),
                status,
            )
            .await
        }
        Err(e) => {
            tracing::error!("Contact form '{}' failed: {}", name, e);
            render_form_page(
                &state,
                &form,
                false,
                &values,
                &[],
                Some(service.text("failed")),
                StatusCode::INTERNAL_SERVER_ERROR,
            )
            .await
        }
    }
}

/// `POST /api/v1/contact/forms/{form}` — JSON での送信（`{"name": "...", "email": "...", ...}`。ログイン不要）。
/// 受け付けたら `201 {"status": "received"}`、入力の誤りは `400 {"error", "errors": [{field, code, message}]}`
pub async fn submit_contact_api(
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
    ClientIp(ip): ClientIp,
    headers: HeaderMap,
    Json(body): Json<HashMap<String, serde_json::Value>>,
) -> Result<(StatusCode, Json<serde_json::Value>), ApiError> {
    let service = contact(&state)?;
    let values: HashMap<String, String> = body
        .into_iter()
        .map(|(k, v)| {
            let value = match v {
                serde_json::Value::String(s) => s,
                serde_json::Value::Bool(true) => "yes".to_string(),
                serde_json::Value::Bool(false) | serde_json::Value::Null => String::new(),
                other => other.to_string(),
            };
            (k, value)
        })
        .collect();
    service
        .submit(&name, &values, &client_info(ip, &headers))
        .await
        .map_err(contact_error)?;
    Ok((StatusCode::CREATED, Json(json!({ "status": "received" }))))
}

/// `GET /api/v1/contact/forms` — フォームと項目の一覧（ログイン不要。ブロックの選択肢・ヘッドレスのフォームに使う）
pub async fn list_forms(
    State(state): State<Arc<AppState>>,
) -> Result<Json<Vec<FormSummary>>, ApiError> {
    let service = contact(&state)?;
    let base = base_path(&state);
    Ok(Json(
        service
            .forms()
            .into_iter()
            .map(|mut f| {
                f.path = format!("{}{}", base, f.path);
                f
            })
            .collect(),
    ))
}

// --- 管理 ---

#[derive(Debug, Default, Deserialize)]
pub struct AdminListParams {
    /// `new` / `read` / `done` / `spam`。省略時はすべて
    pub status: Option<String>,
    pub form: Option<String>,
    /// 全文検索（名前・メールアドレス・件名・各項目の値・添付のファイル名）
    pub q: Option<String>,
    pub limit: Option<u64>,
    #[serde(default)]
    pub offset: u64,
}

/// `GET /api/v1/contact/messages` — 受信箱（新しい順）と状態ごとの件数（`editor`）
pub async fn list_messages(
    State(state): State<Arc<AppState>>,
    _session: EditorSession,
    Query(params): Query<AdminListParams>,
) -> Result<Json<AdminMessagePage>, ApiError> {
    let service = contact(&state)?;
    let query = AdminListQuery {
        status: params.status.filter(|s| !s.is_empty()),
        form: params.form.filter(|s| !s.is_empty()),
        q: params.q.filter(|s| !s.trim().is_empty()),
        limit: params.limit.unwrap_or(DEFAULT_LIMIT).clamp(1, MAX_LIMIT),
        offset: params.offset,
    };
    Ok(Json(service.list(&query).await.map_err(contact_error)?))
}

/// `GET /api/v1/contact/messages/{id}`（`editor`）
pub async fn get_message(
    State(state): State<Arc<AppState>>,
    _session: EditorSession,
    Path(id): Path<String>,
) -> Result<Json<AdminMessage>, ApiError> {
    let service = contact(&state)?;
    Ok(Json(service.get(&id).await.map_err(contact_error)?))
}

/// `GET /api/v1/contact/messages/{id}/attachments/{field}` — 添付ファイルをダウンロードする（`editor`）。
/// 公開 URL は無く、常に `attachment` として返す
pub async fn download_attachment(
    State(state): State<Arc<AppState>>,
    _session: EditorSession,
    Path((id, field)): Path<(String, String)>,
) -> Result<Response, ApiError> {
    let service = contact(&state)?;
    let (file, data) = service
        .attachment(&id, &field)
        .await
        .map_err(contact_error)?;
    // ファイル名は ASCII 以外を含みうるので RFC 5987 の `filename*` で渡す
    let encoded: String = file
        .filename
        .bytes()
        .map(|b| {
            if b.is_ascii_alphanumeric() || b"-_.".contains(&b) {
                (b as char).to_string()
            } else {
                format!("%{:02X}", b)
            }
        })
        .collect();
    let mut response = (StatusCode::OK, data).into_response();
    let headers = response.headers_mut();
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_str(&file.content_type)
            .unwrap_or(HeaderValue::from_static("application/octet-stream")),
    );
    if let Ok(value) = HeaderValue::from_str(&format!(
        "attachment; filename=\"download\"; filename*=UTF-8''{}",
        encoded
    )) {
        headers.insert(header::CONTENT_DISPOSITION, value);
    }
    headers.insert(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    );
    headers.insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("private, no-store"),
    );
    Ok(response)
}

#[derive(Debug, Deserialize)]
pub struct UpdateMessageRequest {
    pub status: String,
}

/// `PATCH /api/v1/contact/messages/{id}` — 状態を変える（`{"status": "done"}`。`editor`）
pub async fn update_message(
    State(state): State<Arc<AppState>>,
    _session: EditorSession,
    Path(id): Path<String>,
    audit: AuditContext,
    Json(payload): Json<UpdateMessageRequest>,
) -> Result<StatusCode, ApiError> {
    let service = contact(&state)?;
    let changed = service
        .set_status(&audit, std::slice::from_ref(&id), &payload.status)
        .await
        .map_err(contact_error)?;
    if changed == 0 {
        return Err(api_error(
            StatusCode::NOT_FOUND,
            format!("Message '{}' not found", id),
        ));
    }
    Ok(StatusCode::NO_CONTENT)
}

/// `DELETE /api/v1/contact/messages/{id}`（`editor`）
pub async fn delete_message(
    State(state): State<Arc<AppState>>,
    _session: EditorSession,
    Path(id): Path<String>,
    audit: AuditContext,
) -> Result<StatusCode, ApiError> {
    let service = contact(&state)?;
    let deleted = service
        .delete(&audit, std::slice::from_ref(&id))
        .await
        .map_err(contact_error)?;
    if deleted == 0 {
        return Err(api_error(
            StatusCode::NOT_FOUND,
            format!("Message '{}' not found", id),
        ));
    }
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Debug, Deserialize)]
pub struct BulkRequest {
    pub ids: Vec<String>,
    /// `new` / `read` / `done` / `spam`（状態の変更）か `delete`
    pub action: String,
}

/// `POST /api/v1/contact/messages/bulk` — まとめて状態を変える・削除する（`editor`）
pub async fn bulk_messages(
    State(state): State<Arc<AppState>>,
    _session: EditorSession,
    audit: AuditContext,
    Json(payload): Json<BulkRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let service = contact(&state)?;
    let count = if payload.action == "delete" {
        service.delete(&audit, &payload.ids).await
    } else {
        service
            .set_status(&audit, &payload.ids, &payload.action)
            .await
    }
    .map_err(contact_error)?;
    Ok(Json(json!({ "count": count })))
}

#[cfg(test)]
#[path = "contact_tests.rs"]
mod tests;
