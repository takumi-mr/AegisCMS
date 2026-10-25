//! 通知の管理 API（`admin`）。配信の状態と送信待ち（送れなかった通知の再送・破棄）、テンプレートの
//! 編集・プレビュー、テスト送信。編集したテンプレートは `_aegis_settings`（`notification_template:<名前>`）に
//! 保存し、イベントバスでほかのサーバーにも読み直させる

use std::collections::HashMap;
use std::sync::Arc;

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Json,
};
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::audit::AuditContext;
use crate::auth::AdminSession;
use crate::state::AppState;
use aegis_core::{AuditAction, AuditOutcome, NotificationMessage, Recipient};
use aegis_db::{NotificationListQuery, QueuedNotification, QUEUE_FAILED, QUEUE_PENDING};
use aegis_notify::templates::{self, TemplateSource};
use aegis_notify::NotificationService;

type ApiError = (StatusCode, Json<serde_json::Value>);

/// 編集したテンプレートを保存する `_aegis_settings` の名前の接頭辞
pub const TEMPLATE_SETTING_PREFIX: &str = "notification_template:";

/// 一覧の既定の件数と上限
const DEFAULT_LIMIT: u64 = 50;
const MAX_LIMIT: u64 = 200;

fn api_error(status: StatusCode, message: impl Into<String>) -> ApiError {
    (status, Json(json!({ "error": message.into() })))
}

fn internal(context: &str, e: impl std::fmt::Display) -> ApiError {
    tracing::error!("{}: {}", context, e);
    api_error(StatusCode::INTERNAL_SERVER_ERROR, "Internal server error")
}

fn service(state: &AppState) -> Result<&NotificationService, ApiError> {
    state.notifications.as_ref().ok_or_else(|| {
        api_error(
            StatusCode::NOT_FOUND,
            "Notifications are not enabled ([notifications].enabled)",
        )
    })
}

/// DB に保存した編集済みのテンプレートを読み直して差し替える（起動時・編集時・ほかのサーバーでの編集の知らせ）
pub async fn reload_templates(state: &AppState) {
    let Some(service) = &state.notifications else {
        return;
    };
    match state.db.list_settings().await {
        Ok(settings) => service.set_template_overrides(
            settings
                .into_iter()
                .filter_map(|(name, value)| {
                    let name = name.strip_prefix(TEMPLATE_SETTING_PREFIX)?;
                    templates::is_editable_name(name).then(|| (name.to_string(), value))
                })
                .collect(),
        ),
        Err(e) => tracing::error!(
            "Could not load the notification templates edited in Admin Studio (keeping the current ones): {}",
            e
        ),
    }
}

fn recipient_label(recipient: &Recipient) -> String {
    match recipient {
        Recipient::Person { email, .. } => email.clone(),
        Recipient::Staff => "staff".to_string(),
    }
}

/// 通知の種類ごとの、テンプレートのある宛先
#[derive(Debug, Serialize)]
pub struct KindInfo {
    pub kind: String,
    /// 顧客・著者など特定の人向けのテンプレートがある
    pub person: bool,
    /// 運営者向けのテンプレートがある（共通のものを含む）
    pub staff: bool,
    /// `[notifications].disabled_events` で止めている
    pub disabled: bool,
}

#[derive(Debug, Default, Serialize)]
pub struct QueueCounts {
    pub pending: u64,
    pub failed: u64,
}

/// `GET /api/v1/notifications` の応答
#[derive(Debug, Default, Serialize)]
pub struct NotificationStatus {
    pub enabled: bool,
    pub channels: Vec<String>,
    pub locale: String,
    pub html: bool,
    /// 送信待ちを DB に保存している（`[notifications.queue].persistent`）
    pub persistent: bool,
    pub max_attempts: u32,
    /// メールの運営者の宛先が設定されている
    pub staff_recipients: bool,
    pub queue: QueueCounts,
    pub kinds: Vec<KindInfo>,
}

/// `GET /api/v1/notifications` — 通知の設定と送信待ちの件数（`admin`）。無効なら `enabled: false`
pub async fn get_notification_status(
    State(state): State<Arc<AppState>>,
    _session: AdminSession,
) -> Result<Json<NotificationStatus>, ApiError> {
    let Some(service) = &state.notifications else {
        return Ok(Json(NotificationStatus::default()));
    };
    let config = service.config();
    let mut queue = QueueCounts::default();
    if service.is_persistent() {
        let counts = state
            .db
            .count_notifications_by_status()
            .await
            .map_err(|e| internal("Failed to count queued notifications", e))?;
        for (status, n) in counts {
            match status.as_str() {
                QUEUE_PENDING => queue.pending = n,
                QUEUE_FAILED => queue.failed = n,
                _ => {}
            }
        }
    }
    let person = Recipient::person("someone@example.com", "");
    let kinds = aegis_notify::samples::SAMPLE_KINDS
        .iter()
        .map(|kind| {
            let has = |recipient: &Recipient| {
                matches!(
                    service.preview(kind, recipient, HashMap::new()),
                    Ok(Some(_))
                )
            };
            KindInfo {
                kind: kind.to_string(),
                person: has(&person),
                staff: has(&Recipient::Staff),
                disabled: !config.is_event_enabled(kind),
            }
        })
        .collect();
    Ok(Json(NotificationStatus {
        enabled: true,
        channels: service.channel_names(),
        locale: service.templates().locale().to_string(),
        html: config.html,
        persistent: service.is_persistent(),
        max_attempts: config.max_attempts,
        staff_recipients: !config.email.staff_recipients.is_empty(),
        queue,
        kinds,
    }))
}

#[derive(Debug, Deserialize)]
pub struct QueueParams {
    /// `pending` / `failed`（省略するとすべて）
    #[serde(default)]
    pub status: Option<String>,
    #[serde(default)]
    pub limit: Option<u64>,
    #[serde(default)]
    pub offset: u64,
}

/// 送信待ちの 1 件
#[derive(Debug, Serialize)]
pub struct QueueItem {
    pub id: String,
    pub kind: String,
    pub channel: String,
    /// 宛先のメールアドレス（運営者向けは `staff`）
    pub recipient: String,
    pub subject: String,
    pub status: String,
    pub attempts: u32,
    pub last_error: Option<String>,
    pub next_attempt_at: String,
    /// 送っている最中（期限まで）
    pub sending: bool,
    pub created_at: String,
    pub updated_at: String,
}

impl From<QueuedNotification> for QueueItem {
    fn from(item: QueuedNotification) -> Self {
        let message = serde_json::from_str::<NotificationMessage>(&item.message).ok();
        Self {
            recipient: message
                .as_ref()
                .map(|m| recipient_label(&m.recipient))
                .unwrap_or_default(),
            subject: message.map(|m| m.subject).unwrap_or_default(),
            id: item.id,
            kind: item.kind,
            channel: item.channel,
            status: item.status,
            attempts: item.attempts,
            last_error: item.last_error,
            next_attempt_at: item.next_attempt_at,
            sending: item.locked_until.is_some(),
            created_at: item.created_at,
            updated_at: item.updated_at,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct QueuePage {
    pub items: Vec<QueueItem>,
    pub total: u64,
}

/// `GET /api/v1/notifications/queue` — 送信待ちと送れなかった通知（新しい順。`admin`）
pub async fn list_notification_queue(
    State(state): State<Arc<AppState>>,
    _session: AdminSession,
    Query(params): Query<QueueParams>,
) -> Result<Json<QueuePage>, ApiError> {
    service(&state)?;
    let status = params.status.filter(|s| !s.is_empty());
    if let Some(status) = &status {
        if status != QUEUE_PENDING && status != QUEUE_FAILED {
            return Err(api_error(
                StatusCode::BAD_REQUEST,
                "status must be \"pending\" or \"failed\"",
            ));
        }
    }
    let page = state
        .db
        .list_notifications(&NotificationListQuery {
            status,
            limit: params.limit.unwrap_or(DEFAULT_LIMIT).clamp(1, MAX_LIMIT),
            offset: params.offset,
        })
        .await
        .map_err(|e| internal("Failed to list queued notifications", e))?;
    Ok(Json(QueuePage {
        items: page.items.into_iter().map(QueueItem::from).collect(),
        total: page.total,
    }))
}

async fn record_queue_update(
    state: &AppState,
    audit: &AuditContext,
    item: &QueuedNotification,
    operation: &str,
) {
    state
        .audit
        .record(
            audit
                .event(AuditAction::NotificationQueueUpdate, AuditOutcome::Success)
                .with_resource("notification", Some(item.kind.clone()))
                .with_details(json!({
                    "operation": operation,
                    "id": item.id,
                    "channel": item.channel,
                    "attempts": item.attempts,
                })),
        )
        .await;
}

/// `POST /api/v1/notifications/queue/{id}/retry` — 送れなかった通知を最初の試行から送り直す（`admin`）
pub async fn retry_queued_notification(
    State(state): State<Arc<AppState>>,
    _session: AdminSession,
    Path(id): Path<String>,
    audit: AuditContext,
) -> Result<StatusCode, ApiError> {
    let service = service(&state)?;
    let item = state
        .db
        .get_notification(&id)
        .await
        .map_err(|e| internal("Failed to read a queued notification", e))?
        .ok_or_else(|| api_error(StatusCode::NOT_FOUND, "Notification not found"))?;
    let now = aegis_core::format_audit_timestamp(&chrono::Utc::now());
    let retried = state
        .db
        .retry_notification(&id, &now)
        .await
        .map_err(|e| internal("Failed to retry a queued notification", e))?;
    if !retried {
        return Err(api_error(
            StatusCode::CONFLICT,
            "Only notifications that failed can be sent again",
        ));
    }
    record_queue_update(&state, &audit, &item, "retry").await;
    service.wake();
    Ok(StatusCode::NO_CONTENT)
}

/// `DELETE /api/v1/notifications/queue/{id}` — 送信待ち・送れなかった通知を捨てる（`admin`）
pub async fn delete_queued_notification(
    State(state): State<Arc<AppState>>,
    _session: AdminSession,
    Path(id): Path<String>,
    audit: AuditContext,
) -> Result<StatusCode, ApiError> {
    service(&state)?;
    let item = state
        .db
        .get_notification(&id)
        .await
        .map_err(|e| internal("Failed to read a queued notification", e))?
        .ok_or_else(|| api_error(StatusCode::NOT_FOUND, "Notification not found"))?;
    if item.locked_until.is_some() && item.status == QUEUE_PENDING {
        return Err(api_error(
            StatusCode::CONFLICT,
            "The notification is being sent right now",
        ));
    }
    state
        .db
        .delete_notification(&id)
        .await
        .map_err(|e| internal("Failed to delete a queued notification", e))?;
    record_queue_update(&state, &audit, &item, "discard").await;
    Ok(StatusCode::NO_CONTENT)
}

/// `GET /api/v1/notifications/templates` — テンプレートの一覧と出どころ（`admin`）
pub async fn list_notification_templates(
    State(state): State<Arc<AppState>>,
    _session: AdminSession,
) -> Result<Json<Vec<aegis_notify::TemplateInfo>>, ApiError> {
    Ok(Json(service(&state)?.templates().list()))
}

#[derive(Debug, Serialize)]
pub struct TemplateBody {
    pub source: TemplateSource,
    pub content: String,
}

/// `GET /api/v1/notifications/templates/{name}` の応答
#[derive(Debug, Serialize)]
pub struct TemplateDetail {
    pub name: String,
    /// 使われているもの（無ければ `null`。新しく作れる）
    pub current: Option<TemplateBody>,
    /// 編集を元に戻したときに使われるもの（ファイルか組み込み）
    pub default: Option<TemplateBody>,
}

fn editable_name(name: &str) -> Result<(), ApiError> {
    if templates::is_editable_name(name) {
        Ok(())
    } else {
        Err(api_error(
            StatusCode::BAD_REQUEST,
            "Template names end with .subject, .txt or .html and use letters, digits, '.', '_' and '-'",
        ))
    }
}

fn template_detail(service: &NotificationService, name: &str) -> Result<TemplateDetail, ApiError> {
    let body = |found: Option<(TemplateSource, String)>| {
        found.map(|(source, content)| TemplateBody { source, content })
    };
    let templates = service.templates();
    Ok(TemplateDetail {
        name: name.to_string(),
        current: body(
            templates
                .source(name)
                .map_err(|e| internal("Failed to read a notification template", e))?,
        ),
        default: body(
            templates
                .default_source(name)
                .map_err(|e| internal("Failed to read a notification template", e))?,
        ),
    })
}

/// `GET /api/v1/notifications/templates/{name}` — テンプレートの中身（`admin`）
pub async fn get_notification_template(
    State(state): State<Arc<AppState>>,
    _session: AdminSession,
    Path(name): Path<String>,
) -> Result<Json<TemplateDetail>, ApiError> {
    let service = service(&state)?;
    editable_name(&name)?;
    Ok(Json(template_detail(service, &name)?))
}

#[derive(Debug, Deserialize)]
pub struct SaveTemplateRequest {
    pub content: String,
}

/// 編集の結果を全サーバーに反映する（自分にも知らせが届くが、応答の前に読み直しておく）
async fn apply_template_change(state: &AppState) {
    reload_templates(state).await;
    crate::cluster::announce(state, crate::cluster::TOPIC_NOTIFICATION_TEMPLATES, "").await;
}

/// `PUT /api/v1/notifications/templates/{name}` — テンプレートを保存する（`admin`）
pub async fn save_notification_template(
    State(state): State<Arc<AppState>>,
    _session: AdminSession,
    Path(name): Path<String>,
    audit: AuditContext,
    Json(payload): Json<SaveTemplateRequest>,
) -> Result<Json<TemplateDetail>, ApiError> {
    let service = service(&state)?;
    editable_name(&name)?;
    if payload.content.len() > templates::MAX_TEMPLATE_BYTES {
        return Err(api_error(
            StatusCode::PAYLOAD_TOO_LARGE,
            format!(
                "Templates are limited to {} bytes",
                templates::MAX_TEMPLATE_BYTES
            ),
        ));
    }
    templates::check_syntax(&name, &payload.content)
        .map_err(|e| api_error(StatusCode::UNPROCESSABLE_ENTITY, e.to_string()))?;
    let previous = service.templates().source(&name).ok().flatten();
    let key = format!("{}{}", TEMPLATE_SETTING_PREFIX, name);
    if let Err(e) = state.db.set_setting(&key, &payload.content).await {
        state
            .audit
            .record(
                audit
                    .event(
                        AuditAction::NotificationTemplateUpdate,
                        AuditOutcome::Failure,
                    )
                    .with_resource("notification_template", Some(name.clone()))
                    .with_details(json!({ "operation": "save", "error": e.to_string() })),
            )
            .await;
        return Err(internal("Failed to save a notification template", e));
    }
    apply_template_change(&state).await;
    state
        .audit
        .record(
            audit
                .event(
                    AuditAction::NotificationTemplateUpdate,
                    AuditOutcome::Success,
                )
                .with_resource("notification_template", Some(name.clone()))
                .with_details(json!({
                    "operation": "save",
                    "previous_source": previous.map(|(source, _)| source),
                    "bytes": payload.content.len(),
                })),
        )
        .await;
    Ok(Json(template_detail(service, &name)?))
}

/// `DELETE /api/v1/notifications/templates/{name}` — 編集を捨てて元（ファイルか組み込み）に戻す（`admin`）
pub async fn reset_notification_template(
    State(state): State<Arc<AppState>>,
    _session: AdminSession,
    Path(name): Path<String>,
    audit: AuditContext,
) -> Result<Json<TemplateDetail>, ApiError> {
    let service = service(&state)?;
    editable_name(&name)?;
    if !service.templates().overrides().contains_key(&name) {
        return Err(api_error(
            StatusCode::NOT_FOUND,
            "The template has not been edited",
        ));
    }
    let key = format!("{}{}", TEMPLATE_SETTING_PREFIX, name);
    state
        .db
        .delete_setting(&key)
        .await
        .map_err(|e| internal("Failed to reset a notification template", e))?;
    apply_template_change(&state).await;
    state
        .audit
        .record(
            audit
                .event(
                    AuditAction::NotificationTemplateUpdate,
                    AuditOutcome::Success,
                )
                .with_resource("notification_template", Some(name.clone()))
                .with_details(json!({ "operation": "reset" })),
        )
        .await;
    Ok(Json(template_detail(service, &name)?))
}

#[derive(Debug, Deserialize)]
pub struct PreviewRequest {
    pub kind: String,
    /// 運営者向け（省略すると顧客・著者など特定の人向け）
    #[serde(default)]
    pub staff: bool,
    /// 保存前の編集中のテンプレート（名前 → 中身）
    #[serde(default)]
    pub templates: HashMap<String, String>,
}

#[derive(Debug, Serialize)]
pub struct PreviewResponse {
    pub subject: String,
    pub body: String,
    pub html: Option<String>,
}

/// `POST /api/v1/notifications/preview` — 見本の値で件名・本文を作る（`admin`）
pub async fn preview_notification(
    State(state): State<Arc<AppState>>,
    _session: AdminSession,
    Json(payload): Json<PreviewRequest>,
) -> Result<Json<PreviewResponse>, ApiError> {
    let service = service(&state)?;
    for (name, content) in &payload.templates {
        editable_name(name)?;
        if content.len() > templates::MAX_TEMPLATE_BYTES {
            return Err(api_error(
                StatusCode::PAYLOAD_TOO_LARGE,
                "Template is too large",
            ));
        }
    }
    let recipient = if payload.staff {
        Recipient::Staff
    } else {
        Recipient::person("hanako@example.com", "")
    };
    match service.preview(&payload.kind, &recipient, payload.templates) {
        Ok(Some(rendered)) => Ok(Json(PreviewResponse {
            subject: rendered.subject,
            body: rendered.body,
            html: rendered.html,
        })),
        Ok(None) => Err(api_error(
            StatusCode::NOT_FOUND,
            format!(
                "No template for '{}' ({})",
                payload.kind,
                if payload.staff { "staff" } else { "person" }
            ),
        )),
        // 編集中のテンプレートの誤り（未定義のフィルタなど）
        Err(e) => Err(api_error(StatusCode::UNPROCESSABLE_ENTITY, e.to_string())),
    }
}

#[derive(Debug, Deserialize)]
pub struct TestRequest {
    /// 通知の種類（省略すると `test`）。見本の値で送る
    #[serde(default)]
    pub kind: Option<String>,
    /// 送り先のメールアドレス
    #[serde(default)]
    pub to: Option<String>,
    /// 運営者にも送る（`staff_recipients` と、運営者向けを扱うプラグインのチャネル）
    #[serde(default)]
    pub staff: bool,
}

#[derive(Debug, Serialize)]
pub struct TestDelivery {
    pub channel: String,
    pub recipient: String,
    pub subject: String,
    pub ok: bool,
    pub error: Option<String>,
}

/// `POST /api/v1/notifications/test` — 見本の値で通知を送り、結果を返す（再試行しない。`admin`）。
/// 送信待ちには入れない（設定を確かめるため、その場で送る）
pub async fn send_test_notification(
    State(state): State<Arc<AppState>>,
    _session: AdminSession,
    Json(payload): Json<TestRequest>,
) -> Result<Json<Vec<TestDelivery>>, ApiError> {
    let service = service(&state)?;
    let kind = payload
        .kind
        .filter(|k| !k.trim().is_empty())
        .unwrap_or_else(|| "test".to_string());
    let mut recipients = Vec::new();
    if let Some(to) = payload
        .to
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        if !aegis_notify::is_valid_address(to) {
            return Err(api_error(
                StatusCode::BAD_REQUEST,
                format!("{:?} is not a valid email address", to),
            ));
        }
        recipients.push(Recipient::person(to, ""));
    }
    if payload.staff {
        recipients.push(Recipient::Staff);
    }
    if recipients.is_empty() {
        return Err(api_error(
            StatusCode::BAD_REQUEST,
            "Choose a recipient: an email address and/or the staff",
        ));
    }
    let deliveries = service
        .deliver_now(&service.sample_event(&kind, recipients))
        .await;
    if deliveries.is_empty() {
        return Err(api_error(
            StatusCode::UNPROCESSABLE_ENTITY,
            format!(
                "Nothing was sent: no channel accepts the recipient, '{}' has no template, or it is in [notifications].disabled_events",
                kind
            ),
        ));
    }
    Ok(Json(
        deliveries
            .into_iter()
            .map(|d| TestDelivery {
                channel: d.channel,
                recipient: recipient_label(&d.recipient),
                subject: d.subject,
                ok: d.result.is_ok(),
                error: d.result.err().map(|e| e.message),
            })
            .collect(),
    ))
}

#[cfg(test)]
#[path = "notifications_tests.rs"]
mod tests;
