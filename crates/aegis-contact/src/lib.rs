//! 問い合わせフォーム（`[contact]`）: 公開ページのフォームから届いたメッセージの検査・保存・通知と、
//! Admin Studio の受信箱。HTTP 層（公開ページ・API）はサーバー側にあり、ここは HTTP に依存しない
//!
//! - フォームと項目は設定ファイル（`[[contact.forms]]`）で決める（[`config`]）。HTML は [`form::form_html`]
//! - ボット除け: 見えない欄（[`config::HONEYPOT_FIELD`]）に値があれば、受け付けたふりをして保存しない。
//!   IP ごとの送信数（`max_per_ip_per_hour`）を超えたら断る。スパムの語・リンクの数でスパムとして保存する
//!   （通知しない）。仕組みはコメントの `aegis_comments::moderation` と同じ
//! - 表は `_aegis_contact_messages`（`aegis_db::ContactStorage`）。状態は `new` / `read` / `done` / `spam`
//!
//! 通知（`[notifications]`）:
//!
//! - `contact.received`: 問い合わせが届いた → 運営者（`notify_staff`）
//! - `contact.auto_reply`: 受け付けた → 送信した人（フォームの `auto_reply`）

pub mod config;
pub mod form;
pub mod messages;

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use aegis_audit::{AuditOrigin, AuditService};
use aegis_core::{
    AegisError, AuditAction, AuditOutcome, CacheBackend, NotificationEvent, Notifier, Recipient,
    StorageBackend,
};
use aegis_db::{ContactListQuery, ContactMessageRecord, DatabaseAdapter};
use chrono::{DateTime, SecondsFormat, Utc};
use serde::Serialize;
use serde_json::json;

pub use config::{ContactConfig, FieldConfig, FieldKind, FormConfig};
pub use form::{Attachment, FieldError, FieldValue, FileRules, UploadedFile};

/// IP ごとの送信数を数えるキャッシュのキーの名前空間
const RATE_LIMIT_SCOPE: &str = "contact";
/// まとめて操作できる問い合わせの数
pub const MAX_BULK: usize = 200;
/// 受信箱の状態
pub const STATUSES: [&str; 4] = ["new", "read", "done", "spam"];

#[derive(Debug, thiserror::Error)]
pub enum ContactError {
    /// 入力の誤り（400。項目ごと）
    #[error("Please check the form")]
    Invalid(Vec<FieldError>),
    /// 操作の誤り（400）
    #[error("{0}")]
    BadRequest(String),
    /// フォーム・問い合わせが無い（404）
    #[error("{0}")]
    NotFound(String),
    /// 同じ IP からの送信が多すぎる（429）
    #[error("Too many messages from this address. Try again later.")]
    RateLimited,
    /// それ以外のサーバー側の失敗（500）
    #[error("{0}")]
    Internal(String),
}

impl From<AegisError> for ContactError {
    fn from(e: AegisError) -> Self {
        Self::Internal(e.to_string())
    }
}

pub type ContactResult<T> = std::result::Result<T, ContactError>;

fn now() -> String {
    Utc::now().to_rfc3339_opts(SecondsFormat::Micros, true)
}

/// 問い合わせの処理が使うサーバーの部品
#[derive(Clone)]
pub struct ContactDeps {
    pub db: Arc<dyn DatabaseAdapter>,
    /// IP ごとの送信数を数える
    pub cache: Arc<dyn CacheBackend>,
    pub audit: Arc<AuditService>,
    /// 添付ファイルの保存先（`contact/{id}/` の下。公開しない）
    pub storage: Arc<dyn StorageBackend>,
    /// `contact.*` の通知（`[notifications]`）。無効なら `None`
    pub notifier: Option<Arc<dyn Notifier>>,
    /// Admin Studio の URL（運営者への通知に入れる。空なら入れない）
    pub admin_url: String,
}

/// 送信した人のクライアント情報
#[derive(Debug, Clone, Default)]
pub struct ClientInfo {
    pub ip: Option<String>,
    pub user_agent: Option<String>,
}

/// Admin Studio の一覧の条件
#[derive(Debug, Clone, Default)]
pub struct AdminListQuery {
    pub status: Option<String>,
    pub form: Option<String>,
    /// 全文検索（空白で区切った語をすべて含むもの。大文字・小文字を区別しない）
    pub q: Option<String>,
    pub limit: u64,
    pub offset: u64,
}

/// Admin Studio に見せる問い合わせ
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct AdminMessage {
    pub id: String,
    pub form: String,
    /// フォームの見出し（設定から消えたフォームは名前のまま）
    pub form_title: String,
    pub name: String,
    pub email: String,
    pub subject: String,
    pub fields: Vec<FieldValue>,
    pub status: String,
    pub ip: Option<String>,
    pub user_agent: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct AdminMessagePage {
    pub messages: Vec<AdminMessage>,
    pub total: u64,
    /// 状態ごとの件数（すべての問い合わせ）
    pub counts: HashMap<String, u64>,
}

/// フォームの一覧（Admin Studio のブロックの選択肢・絞り込み）
#[derive(Debug, Clone, Serialize)]
pub struct FormSummary {
    pub name: String,
    pub title: String,
    /// 公開ページのパス（`base_path` を除く）
    pub path: String,
    pub fields: Vec<FieldConfig>,
    pub auto_reply: bool,
}

pub struct ContactService {
    pub config: ContactConfig,
    deps: ContactDeps,
}

/// 公開ページのパス（`base_path` を除く）
pub fn form_path(form: &str) -> String {
    format!("/contact/{}", form)
}

impl ContactService {
    /// `config` の空の文言・項目は `locale` の既定で埋める
    pub fn new(config: ContactConfig, deps: ContactDeps) -> aegis_core::Result<Self> {
        config.validate()?;
        Ok(Self {
            config: config.resolved(),
            deps,
        })
    }

    /// 組み込みの文言（`[contact].locale`。[`messages::text`]）
    pub fn text(&self, key: &str) -> &'static str {
        messages::text(&self.config.locale, key)
    }

    pub fn form(&self, name: &str) -> ContactResult<&FormConfig> {
        self.config
            .form(name)
            .ok_or_else(|| ContactError::NotFound(format!("No contact form named '{}'", name)))
    }

    /// `/contact` で開くフォーム（`default` があればそれ、無ければ最初のフォーム）
    pub fn default_form(&self) -> &FormConfig {
        self.config.form("default").unwrap_or(&self.config.forms[0])
    }

    pub fn forms(&self) -> Vec<FormSummary> {
        self.config
            .forms
            .iter()
            .map(|f| FormSummary {
                name: f.name.clone(),
                title: f.title.clone(),
                path: form_path(&f.name),
                fields: f.fields.clone(),
                auto_reply: f.auto_reply,
            })
            .collect()
    }

    /// 内容によるスパムの判定（理由を返す）
    fn spam_reason(&self, values: &[FieldValue]) -> Option<String> {
        let haystack = values
            .iter()
            .map(|v| v.value.as_str())
            .collect::<Vec<_>>()
            .join("\n")
            .to_lowercase();
        if let Some(word) = self
            .config
            .spam_words
            .iter()
            .map(|w| w.trim().to_lowercase())
            .find(|w| !w.is_empty() && haystack.contains(w.as_str()))
        {
            return Some(format!("spam word \"{}\"", word));
        }
        let links = aegis_comments::moderation::count_links(&haystack);
        (links > self.config.max_links)
            .then(|| format!("{} links (more than {})", links, self.config.max_links))
    }

    /// 送信を受け付ける。見えない欄に値があるとき・スパムと判定したときも、送信した人には成功と同じに見せる
    /// （戻り値は保存した問い合わせ。ボットのときは `None`）
    pub async fn submit(
        &self,
        form_name: &str,
        values: &HashMap<String, String>,
        client: &ClientInfo,
    ) -> ContactResult<Option<ContactMessageRecord>> {
        self.submit_with_files(form_name, values, &[], client).await
    }

    /// [`submit`](Self::submit) に添付ファイルを加えたもの（`type = "file"` の項目）
    pub async fn submit_with_files(
        &self,
        form_name: &str,
        values: &HashMap<String, String>,
        files: &[UploadedFile],
        client: &ClientInfo,
    ) -> ContactResult<Option<ContactMessageRecord>> {
        let form = self.form(form_name)?;
        if values
            .get(config::HONEYPOT_FIELD)
            .is_some_and(|v| !v.trim().is_empty())
        {
            tracing::info!("Contact form '{}': dropped a bot submission", form.name);
            return Ok(None);
        }
        let rules = FileRules {
            max_bytes: self.config.max_attachment_bytes(),
            extensions: &self.config.attachment_extensions,
        };
        let mut fields =
            form::validate_with_files(form, values, files, &rules, &self.config.locale)
                .map_err(ContactError::Invalid)?;
        if !aegis_comments::moderation::within_rate_limit(
            self.deps.cache.as_ref(),
            RATE_LIMIT_SCOPE,
            client.ip.as_deref(),
            self.config.max_per_ip_per_hour,
        )
        .await?
        {
            return Err(ContactError::RateLimited);
        }

        let value_of = |name: &str| {
            fields
                .iter()
                .find(|v| v.name == name)
                .map(|v| v.value.clone())
                .unwrap_or_default()
        };
        let email = form
            .email_field()
            .map(|f| value_of(&f.name))
            .unwrap_or_default();
        let (name, subject) = (value_of("name"), value_of(&form.subject_field));
        let spam = self.spam_reason(&fields);
        if let Some(reason) = &spam {
            tracing::info!("Contact form '{}': marked as spam ({})", form.name, reason);
        }
        let at = now();
        let id = uuid::Uuid::new_v4().to_string();
        self.store_attachments(&id, form, files, &mut fields)
            .await?;
        let record = ContactMessageRecord {
            id,
            form: form.name.clone(),
            name,
            email,
            subject,
            fields_json: serde_json::to_string(&fields).unwrap_or_else(|_| "[]".to_string()),
            status: if spam.is_some() { "spam" } else { "new" }.to_string(),
            ip: client.ip.clone(),
            user_agent: client
                .user_agent
                .as_ref()
                .map(|ua| ua.chars().take(512).collect()),
            created_at: at.clone(),
            updated_at: at,
        };
        if let Err(e) = self.deps.db.insert_contact_message(&record).await {
            self.delete_attachments(&fields).await;
            return Err(e.into());
        }
        if spam.is_none() {
            self.notify(form, &record, &fields);
        }
        Ok(Some(record))
    }

    /// 検査を通した添付ファイルを `contact/{id}/` に保存し、`fields` の `file` を埋める。
    /// 途中で失敗したら保存したものを消す
    async fn store_attachments(
        &self,
        id: &str,
        form: &FormConfig,
        files: &[UploadedFile],
        fields: &mut [FieldValue],
    ) -> ContactResult<()> {
        for field in form.fields.iter().filter(|f| f.kind == FieldKind::File) {
            let Some(file) = files.iter().find(|f| f.field == field.name) else {
                continue;
            };
            let Some(value) = fields.iter_mut().find(|v| v.name == field.name) else {
                continue;
            };
            if value.value.is_empty() {
                continue;
            }
            let extension = form::extension_of(&value.value);
            let key = format!(
                "contact/{}/{}-{}.{}",
                id,
                field.name,
                &uuid::Uuid::new_v4().to_string()[..8],
                extension
            );
            let content_type = aegis_core::guess_content_type(&key);
            if let Err(e) = self.deps.storage.put(&key, &file.data, content_type).await {
                self.delete_attachments(fields).await;
                return Err(e.into());
            }
            value.file = Some(Attachment {
                key,
                filename: value.value.clone(),
                size: file.data.len(),
                content_type: content_type.to_string(),
            });
        }
        Ok(())
    }

    /// 添付ファイルを消す（失敗は記録するだけ）
    async fn delete_attachments(&self, fields: &[FieldValue]) {
        for file in fields.iter().filter_map(|f| f.file.as_ref()) {
            if let Err(e) = self.deps.storage.delete(&file.key).await {
                tracing::warn!("Could not delete contact attachment {}: {}", file.key, e);
            }
        }
    }

    /// 添付ファイルの中身（Admin Studio の取得用）。`field` は項目の名前
    pub async fn attachment(&self, id: &str, field: &str) -> ContactResult<(Attachment, Vec<u8>)> {
        let message = self.get(id).await?;
        let file = message
            .fields
            .into_iter()
            .find(|f| f.name == field)
            .and_then(|f| f.file)
            .ok_or_else(|| ContactError::NotFound("Attachment not found".to_string()))?;
        let data = self
            .deps
            .storage
            .get(&file.key)
            .await
            .map_err(|_| ContactError::NotFound("Attachment not found".to_string()))?;
        Ok((file, data))
    }

    fn notify(&self, form: &FormConfig, record: &ContactMessageRecord, fields: &[FieldValue]) {
        let Some(notifier) = &self.deps.notifier else {
            return;
        };
        let admin_url = if self.deps.admin_url.is_empty() {
            String::new()
        } else {
            format!("{}#contact", self.deps.admin_url)
        };
        let data = json!({
            "form": { "name": form.name, "title": form.title },
            "message": {
                "id": record.id,
                "name": record.name,
                "email": record.email,
                "subject": record.subject,
                "fields": fields.iter().filter(|f| !f.value.is_empty()).collect::<Vec<_>>(),
                "created_at": record.created_at,
                "ip": record.ip,
            },
            "admin_url": admin_url,
        });
        if self.config.notify_staff {
            notifier.notify(NotificationEvent::new(
                "contact.received",
                vec![Recipient::Staff],
                data.clone(),
            ));
        }
        if form.auto_reply && !record.email.is_empty() {
            notifier.notify(NotificationEvent::new(
                "contact.auto_reply",
                vec![Recipient::person(record.email.clone(), record.name.clone())],
                data,
            ));
        }
    }

    fn to_admin(&self, record: ContactMessageRecord) -> AdminMessage {
        let form_title = self
            .config
            .form(&record.form)
            .map(|f| f.title.clone())
            .unwrap_or_else(|| record.form.clone());
        AdminMessage {
            fields: serde_json::from_str(&record.fields_json).unwrap_or_default(),
            id: record.id,
            form: record.form,
            form_title,
            name: record.name,
            email: record.email,
            subject: record.subject,
            status: record.status,
            ip: record.ip,
            user_agent: record.user_agent,
            created_at: record.created_at,
            updated_at: record.updated_at,
        }
    }

    pub async fn list(&self, query: &AdminListQuery) -> ContactResult<AdminMessagePage> {
        if let Some(status) = &query.status {
            check_status(status)?;
        }
        let terms = search_terms(query.q.as_deref());
        let limit = query.limit.clamp(1, 200);
        let page = if terms.is_empty() {
            self.deps
                .db
                .list_contact_messages(&ContactListQuery {
                    status: query.status.clone(),
                    form: query.form.clone(),
                    limit: Some(limit),
                    offset: query.offset,
                    ..Default::default()
                })
                .await?
        } else {
            // 全文検索はデータベースの方言（大文字・小文字、JSON の中身）に頼らず、絞り込んだ行をここで検索する
            let all = self
                .deps
                .db
                .list_contact_messages(&ContactListQuery {
                    status: query.status.clone(),
                    form: query.form.clone(),
                    limit: None,
                    ..Default::default()
                })
                .await?;
            let matched: Vec<_> = all
                .messages
                .into_iter()
                .filter(|m| matches_terms(m, &terms))
                .collect();
            aegis_db::ContactPage {
                total: matched.len() as u64,
                messages: matched
                    .into_iter()
                    .skip(query.offset as usize)
                    .take(limit as usize)
                    .collect(),
            }
        };
        let counts = self
            .deps
            .db
            .count_contact_messages_by_status()
            .await?
            .into_iter()
            .collect();
        Ok(AdminMessagePage {
            messages: page
                .messages
                .into_iter()
                .map(|m| self.to_admin(m))
                .collect(),
            total: page.total,
            counts,
        })
    }

    pub async fn get(&self, id: &str) -> ContactResult<AdminMessage> {
        self.deps
            .db
            .get_contact_message(id)
            .await?
            .map(|m| self.to_admin(m))
            .ok_or_else(|| ContactError::NotFound(format!("Message '{}' not found", id)))
    }

    /// 状態を変える（`new` / `read` / `done` / `spam`）
    pub async fn set_status(
        &self,
        audit: &AuditOrigin,
        ids: &[String],
        status: &str,
    ) -> ContactResult<u64> {
        check_status(status)?;
        let ids = checked_ids(ids)?;
        let result = self
            .deps
            .db
            .set_contact_messages_status(&ids, status, &now())
            .await;
        self.record(
            audit,
            AuditAction::ContactUpdate,
            &ids,
            json!({ "status": status }),
            &result,
        )
        .await;
        Ok(result?)
    }

    pub async fn delete(&self, audit: &AuditOrigin, ids: &[String]) -> ContactResult<u64> {
        let ids = checked_ids(ids)?;
        // 行を消した後では添付ファイルの場所が分からないので、先に控える
        let mut attached: Vec<FieldValue> = Vec::new();
        for id in &ids {
            if let Ok(Some(message)) = self.deps.db.get_contact_message(id).await {
                attached.extend(attachments_of(&message));
            }
        }
        let result = self.deps.db.delete_contact_messages(&ids).await;
        if result.is_ok() {
            self.delete_attachments(&attached).await;
        }
        self.record(audit, AuditAction::ContactDelete, &ids, json!({}), &result)
            .await;
        Ok(result?)
    }

    async fn record(
        &self,
        audit: &AuditOrigin,
        action: AuditAction,
        ids: &[String],
        mut details: serde_json::Value,
        result: &aegis_core::Result<u64>,
    ) {
        details["ids"] = json!(ids);
        let outcome = match result {
            Ok(count) => {
                details["count"] = json!(count);
                AuditOutcome::Success
            }
            Err(e) => {
                details["error"] = json!(e.to_string());
                AuditOutcome::Failure
            }
        };
        let resource = (ids.len() == 1).then(|| ids[0].clone());
        self.deps
            .audit
            .record(
                audit
                    .event(action, outcome)
                    .with_resource("contact", resource)
                    .with_details(details),
            )
            .await;
    }

    /// 保存期間（`retention_days`）を過ぎた問い合わせを消す
    pub async fn apply_retention(&self, now: DateTime<Utc>) -> u64 {
        if self.config.retention_days == 0 {
            return 0;
        }
        let cutoff = (now - chrono::Duration::days(i64::from(self.config.retention_days)))
            .to_rfc3339_opts(SecondsFormat::Micros, true);
        let expired = self
            .deps
            .db
            .list_contact_messages(&ContactListQuery {
                created_before: Some(cutoff.clone()),
                ..Default::default()
            })
            .await
            .map(|page| {
                page.messages
                    .iter()
                    .flat_map(attachments_of)
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        match self.deps.db.delete_contact_messages_before(&cutoff).await {
            Ok(n) => {
                if n > 0 {
                    tracing::info!("Deleted {} contact messages older than {}", n, cutoff);
                    self.delete_attachments(&expired).await;
                }
                n
            }
            Err(e) => {
                tracing::warn!("Could not delete old contact messages: {}", e);
                0
            }
        }
    }

    /// 1 日に 1 回、保存期間を過ぎた問い合わせを消す
    pub fn spawn_retention_task(self: &Arc<Self>) {
        if self.config.retention_days == 0 {
            return;
        }
        let service = self.clone();
        tokio::spawn(async move {
            loop {
                service.apply_retention(Utc::now()).await;
                tokio::time::sleep(Duration::from_secs(24 * 60 * 60)).await;
            }
        });
    }
}

/// 添付ファイルのある項目
fn attachments_of(message: &ContactMessageRecord) -> Vec<FieldValue> {
    serde_json::from_str::<Vec<FieldValue>>(&message.fields_json)
        .unwrap_or_default()
        .into_iter()
        .filter(|f| f.file.is_some())
        .collect()
}

/// 検索語（空白で区切り、小文字にしたもの）
fn search_terms(q: Option<&str>) -> Vec<String> {
    q.unwrap_or("")
        .split_whitespace()
        .map(str::to_lowercase)
        .collect()
}

/// すべての語が、名前・メールアドレス・件名・各項目の値（添付のファイル名を含む）のどこかにある
fn matches_terms(message: &ContactMessageRecord, terms: &[String]) -> bool {
    let fields: Vec<FieldValue> = serde_json::from_str(&message.fields_json).unwrap_or_default();
    let haystack = std::iter::once(message.name.as_str())
        .chain([message.email.as_str(), message.subject.as_str()])
        .chain(fields.iter().map(|f| f.value.as_str()))
        .collect::<Vec<_>>()
        .join("\n")
        .to_lowercase();
    terms.iter().all(|t| haystack.contains(t.as_str()))
}

fn check_status(status: &str) -> ContactResult<()> {
    if STATUSES.contains(&status) {
        Ok(())
    } else {
        Err(ContactError::BadRequest(format!(
            "Unknown status '{}' (use new, read, done or spam)",
            status
        )))
    }
}

fn checked_ids(ids: &[String]) -> ContactResult<Vec<String>> {
    let mut out: Vec<String> = Vec::new();
    for id in ids {
        let id = id.trim();
        if !id.is_empty() && !out.iter().any(|o| o == id) {
            out.push(id.to_string());
        }
    }
    if out.is_empty() {
        return Err(ContactError::BadRequest("No messages selected".to_string()));
    }
    if out.len() > MAX_BULK {
        return Err(ContactError::BadRequest(format!(
            "Select {} messages at most",
            MAX_BULK
        )));
    }
    Ok(out)
}

#[cfg(test)]
mod tests;
