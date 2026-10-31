//! ニュースレター（`[newsletter]`）: メールアドレスの申し込み（確認メールを開いて始まるダブルオプトイン）、
//! 配信の停止（メールのリンク）、公開中のコンテンツを購読者に送る配信。HTTP に依存しない。
//! 公開ページ・API はサーバー側にあり、ここは検査・保存・通知の組み立てだけを行う
//!
//! - 申し込みは**常に同じ応答**にする（登録済みか、確認待ちか、解除済みかを申し込んだ人に教えない）。
//!   申し込まれたアドレスに確認メール（`newsletter.confirm`）を送り、リンクを開いた（確定した）ときに配信の対象になる
//! - 配信（`newsletter.issue`）は購読者 1 人につき 1 通の通知として `Notifier`（`aegis-notify` の送信待ち）に渡す。
//!   解除のリンクは購読者ごとのトークンで、確認と解除のページは**ボタンを押して**実行する（メールのリンクを
//!   自動で開くセキュリティソフトに、購読や解除を実行させない）
//! - 表は `_aegis_newsletter_subscribers`・`_aegis_newsletter_issues`（`aegis_db::NewsletterStorage`）

pub mod config;
pub mod messages;

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use aegis_audit::{AuditOrigin, AuditService};
use aegis_captcha::{Captcha, CaptchaError};
use aegis_core::{
    AegisError, AuditAction, AuditOutcome, CacheBackend, NotificationEvent, Notifier, Recipient,
};
use aegis_db::{
    DatabaseAdapter, NewsletterIssueRecord, NewsletterListQuery, NewsletterPage,
    NewsletterSubscriberRecord,
};
use chrono::{DateTime, SecondsFormat, Utc};
use serde::Serialize;
use serde_json::json;

pub use config::{NewsletterConfig, HONEYPOT_FIELD};

/// 確認待ち
pub const STATUS_PENDING: &str = "pending";
/// 配信する
pub const STATUS_ACTIVE: &str = "active";
/// 解除した
pub const STATUS_UNSUBSCRIBED: &str = "unsubscribed";
/// 購読者の状態
pub const STATUSES: [&str; 3] = [STATUS_PENDING, STATUS_ACTIVE, STATUS_UNSUBSCRIBED];

/// IP ごとの申し込みを数えるキャッシュのキーの名前空間
const RATE_LIMIT_SCOPE: &str = "newsletter";
/// 確認待ちのアドレスに確認メールを送り直せる間隔（秒）。同じアドレスに何通も送らせない
const RESEND_AFTER_SECS: i64 = 60;
/// 配信で 1 回に読み込む購読者の数
const SEND_BATCH: u64 = 500;
/// 配信の二重実行を止める鍵の有効時間（秒）
const SEND_LOCK_SECS: u64 = 300;
/// 確認待ちのまま残す日数（これより古いものは消す。確認リンクの有効期限はもっと短い）
const PENDING_RETENTION_DAYS: i64 = 7;
/// まとめて操作できる購読者の数
pub const MAX_BULK: usize = 500;
/// 配信のサンプルのトークン（テスト送信の解除リンク。どの購読者にも一致しない）
const TEST_TOKEN: &str = "test";

#[derive(Debug, thiserror::Error)]
pub enum NewsletterError {
    /// 入力の誤り（400。メッセージは申し込んだ人に見せられる）
    #[error("{0}")]
    Invalid(String),
    /// 同じ IP からの申し込みが多すぎる（429）
    #[error("Too many sign-ups from this address. Try again later.")]
    RateLimited,
    /// CAPTCHA を通らなかった（400）、検証サービスに届かなかった（503）
    #[error("{0}")]
    Captcha(CaptchaError),
    /// リンクが無効・期限切れ、購読者がいない（404）
    #[error("{0}")]
    NotFound(String),
    /// このコンテンツはすでに配信した（409。`force` で送り直せる）。値は配信した日時
    #[error("This content was already sent on {0}")]
    AlreadySent(String),
    /// 送れない状態（通知が無効、ほかで配信中）（503）
    #[error("{0}")]
    Unavailable(String),
    /// 配信する購読者がいない（409）
    #[error("There are no confirmed subscribers to send to")]
    NoSubscribers,
    /// それ以外のサーバー側の失敗（500）
    #[error("{0}")]
    Internal(String),
}

impl From<AegisError> for NewsletterError {
    fn from(e: AegisError) -> Self {
        Self::Internal(e.to_string())
    }
}

pub type NewsletterResult<T> = std::result::Result<T, NewsletterError>;

fn now() -> String {
    Utc::now().to_rfc3339_opts(SecondsFormat::Micros, true)
}

/// ニュースレターの処理が使うサーバーの部品
#[derive(Clone)]
pub struct NewsletterDeps {
    pub db: Arc<dyn DatabaseAdapter>,
    /// IP ごとの申し込み数を数える
    pub cache: Arc<dyn CacheBackend>,
    pub audit: Arc<AuditService>,
    /// `newsletter.*` の通知（`[notifications]`）。無効なら `None`
    pub notifier: Option<Arc<dyn Notifier>>,
    /// 公開サイトの URL（確認・解除のリンクと配信の本文に使う）
    pub site_url: String,
}

/// 申し込んだ人のクライアント情報
#[derive(Debug, Clone, Default)]
pub struct ClientInfo {
    pub ip: Option<String>,
}

/// 確認リンクを開いた結果
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfirmOutcome {
    /// 今、購読が始まった
    Confirmed,
    /// すでに確定している
    AlreadyConfirmed,
}

/// 解除のリンクを開いた結果
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnsubscribeOutcome {
    Unsubscribed,
    /// すでに解除している
    AlreadyUnsubscribed,
}

/// 状態ごとの人数
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
pub struct Counts {
    pub pending: u64,
    pub active: u64,
    pub unsubscribed: u64,
}

/// 配信するコンテンツ
#[derive(Debug, Clone)]
pub struct IssueContent {
    pub content_type: String,
    pub content_id: String,
    pub title: String,
    /// メール本文に入れる要約（空なら入れない）
    pub summary: String,
    /// 公開ページの URL
    pub url: String,
}

pub struct NewsletterService {
    pub config: NewsletterConfig,
    deps: NewsletterDeps,
    captcha: Option<Captcha>,
}

/// メールアドレスの整形と検査（前後の空白を除いて小文字にする）。正しくなければ `None`
pub fn normalize_email(raw: &str) -> Option<String> {
    let email = raw.trim().to_lowercase();
    if email.is_empty() || email.len() > 254 {
        return None;
    }
    if email.chars().any(|c| {
        c.is_control()
            || c.is_whitespace()
            || matches!(
                c,
                '<' | '>' | '(' | ')' | ',' | ';' | ':' | '"' | '\\' | '\''
            )
    }) {
        return None;
    }
    let (local, domain) = email.split_once('@')?;
    let valid_domain = domain.contains('.')
        && !domain.contains('@')
        && !domain.starts_with(['.', '-'])
        && !domain.ends_with(['.', '-'])
        && !domain.contains("..");
    // 先頭が `= + - @` のアドレスは、書き出した CSV を表計算ソフトが式として読んでしまうので受け付けない
    let formula = local.starts_with(['=', '+', '-', '@']);
    (!local.is_empty() && local.len() <= 64 && valid_domain && !formula).then_some(email)
}

/// 確認・解除のリンクに入れる推測できない文字列（64 桁の 16 進数）
fn new_token() -> String {
    format!(
        "{}{}",
        uuid::Uuid::new_v4().simple(),
        uuid::Uuid::new_v4().simple()
    )
}

/// リンクのトークンとして読める形か（DB に聞く前の足切り）
fn plausible_token(token: &str) -> bool {
    !token.is_empty() && token.len() <= 128 && token.bytes().all(|b| b.is_ascii_alphanumeric())
}

fn escape_html(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#x27;"),
            _ => out.push(c),
        }
    }
    out
}

/// `at`（RFC 3339）から `seconds` 秒以上たったか。読めない日時は「たった」とみなす
fn older_than(at: &str, seconds: i64) -> bool {
    DateTime::parse_from_rfc3339(at)
        .map(|at| (Utc::now() - at.with_timezone(&Utc)).num_seconds() >= seconds)
        .unwrap_or(true)
}

impl NewsletterService {
    pub fn new(config: NewsletterConfig, deps: NewsletterDeps) -> aegis_core::Result<Self> {
        config.validate()?;
        if deps.site_url.trim().is_empty() {
            return Err(AegisError::Config(
                "[newsletter] needs [commerce].public_url (the public URL of the site) for the links in the emails"
                    .to_string(),
            ));
        }
        let captcha = config
            .captcha
            .clone()
            .map(|c| Captcha::new(c, "[newsletter.captcha]"))
            .transpose()?;
        Ok(Self {
            config,
            deps,
            captcha,
        })
    }

    /// 組み込みの文言（`[newsletter].locale`。[`messages::text`]）
    pub fn text(&self, key: &str) -> &'static str {
        messages::text(&self.config.locale, key)
    }

    fn site_url(&self) -> &str {
        self.deps.site_url.trim().trim_end_matches('/')
    }

    fn confirm_url(&self, token: &str) -> String {
        format!("{}/newsletter/confirm?token={}", self.site_url(), token)
    }

    fn unsubscribe_url(&self, token: &str) -> String {
        format!("{}/newsletter/unsubscribe?token={}", self.site_url(), token)
    }

    /// 送られてきた値の中の CAPTCHA のトークン（`[newsletter.captcha]` が無ければ `None`）
    pub fn captcha_token<'a>(&self, values: &'a HashMap<String, String>) -> Option<&'a str> {
        let field = self.captcha.as_ref()?.response_field();
        values.get(field).map(String::as_str)
    }

    /// CAPTCHA のウィジェットが読み込む・つなぐ先のホスト（CSP に足す。`[newsletter.captcha]` が無ければ空）
    pub fn captcha_csp_hosts(&self) -> Vec<String> {
        self.captcha
            .as_ref()
            .map(Captcha::csp_hosts)
            .unwrap_or_default()
    }

    /// 申し込みフォームの HTML（エスケープ済み）。`email` は再表示する入力、`error` はフォームの上に出す知らせ。
    /// ボット除けの見えない欄と、あれば CAPTCHA のウィジェットが入る
    pub fn form_html(&self, action: &str, email: &str, error: Option<&str>) -> String {
        let mut html = String::new();
        if let Some(error) = error {
            html.push_str(&format!(
                "<p class=\"aegis-newsletter-notice\" role=\"alert\">{}</p>\n",
                escape_html(error)
            ));
        }
        html.push_str(&format!(
            "<form class=\"aegis-newsletter-form\" method=\"post\" action=\"{}\" accept-charset=\"utf-8\">\n",
            escape_html(action)
        ));
        // ボット除け: 人には見えない欄。値が入っていたら登録しない
        html.push_str(&format!(
            "<div class=\"aegis-newsletter-trap\" aria-hidden=\"true\" style=\"position:absolute;left:-10000px;width:1px;height:1px;overflow:hidden\">\
             <label>Leave this empty <input type=\"text\" name=\"{}\" tabindex=\"-1\" autocomplete=\"off\"></label></div>\n",
            HONEYPOT_FIELD
        ));
        html.push_str(&format!(
            "<label for=\"aegis-newsletter-email\">{}</label>\n\
             <input id=\"aegis-newsletter-email\" type=\"email\" name=\"email\" required maxlength=\"254\" autocomplete=\"email\" value=\"{}\">\n",
            escape_html(self.text("email")),
            escape_html(email)
        ));
        if let Some(captcha) = &self.captcha {
            html.push_str(&format!(
                "<div class=\"aegis-newsletter-captcha\">\n{}\n</div>\n",
                captcha.widget_html()
            ));
        }
        html.push_str(&format!(
            "<button type=\"submit\" class=\"aegis-newsletter-submit\">{}</button>\n</form>",
            escape_html(self.text("submit"))
        ));
        html
    }

    /// 確認・解除のページのボタン（`POST` でトークンを送る）。`kind` は `confirm` / `unsubscribe`
    pub fn token_form_html(&self, kind: &str, action: &str, token: &str) -> String {
        format!(
            "<p>{}</p>\n<form class=\"aegis-newsletter-form\" method=\"post\" action=\"{}\">\n\
             <input type=\"hidden\" name=\"token\" value=\"{}\">\n\
             <button type=\"submit\" class=\"aegis-newsletter-submit\">{}</button>\n</form>",
            escape_html(self.text(&format!("{}_lead", kind))),
            escape_html(action),
            escape_html(token),
            escape_html(self.text(&format!("{}_button", kind))),
        )
    }

    // --- 公開 ---

    /// 申し込む。登録済み・確認待ち・解除済みのどれでも、申し込んだ人には同じ（成功の）応答を返す。
    /// 見えない欄に値があるときも成功と同じに見せる（何もしない）。確認メールは新しいアドレスと解除済みのアドレス、
    /// 前の確認メールから [`RESEND_AFTER_SECS`] 秒以上たった確認待ちのアドレスにだけ送る
    pub async fn subscribe(
        &self,
        email: &str,
        honeypot: Option<&str>,
        captcha_token: Option<&str>,
        client: &ClientInfo,
    ) -> NewsletterResult<()> {
        if honeypot.is_some_and(|v| !v.trim().is_empty()) {
            tracing::info!("Newsletter: dropped a bot sign-up");
            return Ok(());
        }
        let email = normalize_email(email)
            .ok_or_else(|| NewsletterError::Invalid(self.text("invalid_email").to_string()))?;
        // 検証サービスへの問い合わせ・IP の枠は、アドレスが正しいときだけ使う
        if let Some(captcha) = &self.captcha {
            captcha
                .verify(captcha_token, client.ip.as_deref())
                .await
                .map_err(NewsletterError::Captcha)?;
        }
        if let Some(ip) = client
            .ip
            .as_deref()
            .filter(|_| self.config.max_per_ip_per_hour > 0)
        {
            let count = self
                .deps
                .cache
                .increment(&format!("{}:ip:{}", RATE_LIMIT_SCOPE, ip), Some(3600))
                .await?;
            if count > self.config.max_per_ip_per_hour {
                return Err(NewsletterError::RateLimited);
            }
        }

        let at = now();
        match self
            .deps
            .db
            .get_newsletter_subscriber_by_email(&email)
            .await?
        {
            None => {
                let subscriber = NewsletterSubscriberRecord {
                    id: uuid::Uuid::new_v4().to_string(),
                    email: email.clone(),
                    status: STATUS_PENDING.to_string(),
                    token: new_token(),
                    ip: client.ip.clone(),
                    created_at: at.clone(),
                    confirmed_at: None,
                    unsubscribed_at: None,
                    updated_at: at,
                };
                if let Err(e) = self.deps.db.insert_newsletter_subscriber(&subscriber).await {
                    // 同時に同じアドレスが申し込まれた: 先に入ったほうが確認メールを送っている
                    if self
                        .deps
                        .db
                        .get_newsletter_subscriber_by_email(&email)
                        .await?
                        .is_some()
                    {
                        return Ok(());
                    }
                    return Err(e.into());
                }
                self.send_confirmation(&subscriber);
            }
            Some(mut subscriber) => match subscriber.status.as_str() {
                STATUS_ACTIVE => {}
                STATUS_PENDING => {
                    if older_than(&subscriber.updated_at, RESEND_AFTER_SECS) {
                        subscriber.token = new_token();
                        subscriber.updated_at = at;
                        subscriber.ip = client.ip.clone();
                        self.deps
                            .db
                            .update_newsletter_subscriber(&subscriber)
                            .await?;
                        self.send_confirmation(&subscriber);
                    }
                }
                _ => {
                    // 解除した人が、もう一度申し込んだ: 確認からやり直す。古いリンクは使えなくする
                    subscriber.status = STATUS_PENDING.to_string();
                    subscriber.token = new_token();
                    subscriber.confirmed_at = None;
                    subscriber.unsubscribed_at = None;
                    subscriber.ip = client.ip.clone();
                    subscriber.updated_at = at;
                    self.deps
                        .db
                        .update_newsletter_subscriber(&subscriber)
                        .await?;
                    self.send_confirmation(&subscriber);
                }
            },
        }
        Ok(())
    }

    fn send_confirmation(&self, subscriber: &NewsletterSubscriberRecord) {
        let Some(notifier) = &self.deps.notifier else {
            tracing::warn!(
                "Newsletter: no [notifications], so no confirmation email was sent to a new subscriber"
            );
            return;
        };
        notifier.notify(NotificationEvent::new(
            "newsletter.confirm",
            vec![Recipient::person(subscriber.email.clone(), "")],
            json!({
                "email": subscriber.email,
                "confirm_url": self.confirm_url(&subscriber.token),
                "expires_hours": self.config.confirm_ttl_hours,
            }),
        ));
    }

    /// 確認リンクのトークンで購読を確定する
    pub async fn confirm(&self, token: &str) -> NewsletterResult<ConfirmOutcome> {
        let invalid = || NewsletterError::NotFound(self.text("invalid_link").to_string());
        if !plausible_token(token) {
            return Err(invalid());
        }
        let mut subscriber = self
            .deps
            .db
            .get_newsletter_subscriber_by_token(token)
            .await?
            .ok_or_else(invalid)?;
        match subscriber.status.as_str() {
            STATUS_ACTIVE => Ok(ConfirmOutcome::AlreadyConfirmed),
            STATUS_PENDING => {
                let ttl = i64::try_from(self.config.confirm_ttl_hours).unwrap_or(i64::MAX / 3600);
                if older_than(&subscriber.updated_at, ttl.saturating_mul(3600)) {
                    return Err(invalid());
                }
                let at = now();
                subscriber.status = STATUS_ACTIVE.to_string();
                subscriber.confirmed_at = Some(at.clone());
                subscriber.updated_at = at;
                if !self
                    .deps
                    .db
                    .update_newsletter_subscriber(&subscriber)
                    .await?
                {
                    return Err(invalid());
                }
                Ok(ConfirmOutcome::Confirmed)
            }
            _ => Err(invalid()),
        }
    }

    /// 解除リンクのトークンで配信を止める
    pub async fn unsubscribe(&self, token: &str) -> NewsletterResult<UnsubscribeOutcome> {
        let invalid = || NewsletterError::NotFound(self.text("invalid_link").to_string());
        if !plausible_token(token) {
            return Err(invalid());
        }
        let mut subscriber = self
            .deps
            .db
            .get_newsletter_subscriber_by_token(token)
            .await?
            .ok_or_else(invalid)?;
        if subscriber.status == STATUS_UNSUBSCRIBED {
            return Ok(UnsubscribeOutcome::AlreadyUnsubscribed);
        }
        let at = now();
        subscriber.status = STATUS_UNSUBSCRIBED.to_string();
        subscriber.unsubscribed_at = Some(at.clone());
        subscriber.updated_at = at;
        if !self
            .deps
            .db
            .update_newsletter_subscriber(&subscriber)
            .await?
        {
            return Err(invalid());
        }
        Ok(UnsubscribeOutcome::Unsubscribed)
    }

    // --- 管理 ---

    /// 購読者の一覧（登録の新しい順）と、状態ごとの人数
    pub async fn list(
        &self,
        query: &NewsletterListQuery,
    ) -> NewsletterResult<(NewsletterPage, Counts)> {
        let page = self.deps.db.list_newsletter_subscribers(query).await?;
        let mut counts = Counts::default();
        for (status, n) in self
            .deps
            .db
            .count_newsletter_subscribers_by_status()
            .await?
        {
            match status.as_str() {
                STATUS_PENDING => counts.pending = n,
                STATUS_ACTIVE => counts.active = n,
                STATUS_UNSUBSCRIBED => counts.unsubscribed = n,
                _ => {}
            }
        }
        Ok((page, counts))
    }

    /// 配信する購読者（`active`）のメールアドレスをすべて返す（書き出し用。登録の順ではなく id の順）
    pub async fn active_emails(&self) -> NewsletterResult<Vec<String>> {
        let mut emails = Vec::new();
        let mut after: Option<String> = None;
        loop {
            let batch = self
                .deps
                .db
                .list_active_newsletter_subscribers(after.as_deref(), SEND_BATCH)
                .await?;
            let Some(last) = batch.last() else {
                break;
            };
            after = Some(last.id.clone());
            emails.extend(batch.into_iter().map(|s| s.email));
        }
        Ok(emails)
    }

    /// 購読者を消す（解除ではなく、記録ごと消す）。消した人数を返す
    pub async fn delete(&self, audit: &AuditOrigin, ids: &[String]) -> NewsletterResult<u64> {
        if ids.is_empty() || ids.len() > MAX_BULK {
            return Err(NewsletterError::Invalid(format!(
                "Choose 1 to {} subscribers",
                MAX_BULK
            )));
        }
        let result = self.deps.db.delete_newsletter_subscribers(ids).await;
        let (outcome, details) = match &result {
            Ok(count) => (AuditOutcome::Success, json!({ "ids": ids, "count": count })),
            Err(e) => (
                AuditOutcome::Failure,
                json!({ "ids": ids, "error": e.to_string() }),
            ),
        };
        self.deps
            .audit
            .record(
                audit
                    .event(AuditAction::NewsletterDelete, outcome)
                    .with_resource("newsletter", (ids.len() == 1).then(|| ids[0].clone()))
                    .with_details(details),
            )
            .await;
        Ok(result?)
    }

    /// 配信の履歴（新しい順）と総数
    pub async fn issues(
        &self,
        limit: u64,
        offset: u64,
    ) -> NewsletterResult<(Vec<NewsletterIssueRecord>, u64)> {
        Ok(self
            .deps
            .db
            .list_newsletter_issues(Some(limit), offset)
            .await?)
    }

    /// このコンテンツの最新の配信
    pub async fn latest_issue(
        &self,
        content_type: &str,
        content_id: &str,
    ) -> NewsletterResult<Option<NewsletterIssueRecord>> {
        Ok(self
            .deps
            .db
            .get_latest_newsletter_issue(content_type, content_id)
            .await?)
    }

    fn issue_event(&self, content: &IssueContent, email: &str, token: &str) -> NotificationEvent {
        NotificationEvent::new(
            "newsletter.issue",
            vec![Recipient::person(email.to_string(), "")],
            json!({
                "title": content.title,
                "summary": content.summary,
                "url": content.url,
                "unsubscribe_url": self.unsubscribe_url(token),
            }),
        )
    }

    /// 1 つのアドレスにだけ送って見え方を確かめる（履歴には残さない。解除のリンクは見本）
    pub async fn send_test(&self, content: &IssueContent, to: &str) -> NewsletterResult<()> {
        let to = normalize_email(to)
            .ok_or_else(|| NewsletterError::Invalid(self.text("invalid_email").to_string()))?;
        let notifier = self.notifier()?;
        notifier.notify(self.issue_event(content, &to, TEST_TOKEN));
        Ok(())
    }

    fn notifier(&self) -> NewsletterResult<&Arc<dyn Notifier>> {
        self.deps.notifier.as_ref().ok_or_else(|| {
            NewsletterError::Unavailable(
                "Notifications are not enabled ([notifications].enabled)".to_string(),
            )
        })
    }

    /// 公開中のコンテンツを、確定している購読者全員に送る。すでに送ったコンテンツは `force` でないと断る
    /// （二重に送らない）。同じコンテンツの配信は同時に 1 つだけ。送った記録（履歴）と監査ログを残す
    pub async fn send_issue(
        &self,
        audit: &AuditOrigin,
        content: &IssueContent,
        created_by: Option<String>,
        force: bool,
    ) -> NewsletterResult<NewsletterIssueRecord> {
        let notifier = self.notifier()?.clone();
        if !force {
            if let Some(previous) = self
                .latest_issue(&content.content_type, &content.content_id)
                .await?
            {
                return Err(NewsletterError::AlreadySent(previous.created_at));
            }
        }
        // 同じコンテンツを同時に送らない（ボタンの二度押し・複数の管理者）
        let lock = format!(
            "newsletter:sending:{}:{}",
            content.content_type, content.content_id
        );
        if self
            .deps
            .cache
            .increment(&lock, Some(SEND_LOCK_SECS))
            .await?
            > 1
        {
            return Err(NewsletterError::Unavailable(
                "This content is already being sent".to_string(),
            ));
        }
        let result = self
            .send_to_subscribers(notifier.as_ref(), content, created_by)
            .await;
        let _ = self.deps.cache.delete(&lock).await;

        let (outcome, details) = match &result {
            Ok(issue) => (
                AuditOutcome::Success,
                json!({
                    "content_type": content.content_type,
                    "content_id": content.content_id,
                    "title": content.title,
                    "recipients": issue.recipients,
                    "force": force,
                }),
            ),
            Err(e) => (
                AuditOutcome::Failure,
                json!({
                    "content_type": content.content_type,
                    "content_id": content.content_id,
                    "error": e.to_string(),
                }),
            ),
        };
        let resource = result.as_ref().ok().map(|issue| issue.id.clone());
        self.deps
            .audit
            .record(
                audit
                    .event(AuditAction::NewsletterSend, outcome)
                    .with_resource("newsletter", resource)
                    .with_details(details),
            )
            .await;
        result
    }

    async fn send_to_subscribers(
        &self,
        notifier: &dyn Notifier,
        content: &IssueContent,
        created_by: Option<String>,
    ) -> NewsletterResult<NewsletterIssueRecord> {
        let mut recipients: i64 = 0;
        let mut after: Option<String> = None;
        loop {
            let batch = self
                .deps
                .db
                .list_active_newsletter_subscribers(after.as_deref(), SEND_BATCH)
                .await?;
            let Some(last) = batch.last() else {
                break;
            };
            after = Some(last.id.clone());
            for subscriber in &batch {
                notifier.notify(self.issue_event(content, &subscriber.email, &subscriber.token));
                recipients += 1;
            }
        }
        if recipients == 0 {
            return Err(NewsletterError::NoSubscribers);
        }
        let issue = NewsletterIssueRecord {
            id: uuid::Uuid::new_v4().to_string(),
            content_type: content.content_type.clone(),
            content_id: content.content_id.clone(),
            title: content.title.clone(),
            url: content.url.clone(),
            recipients,
            created_at: now(),
            created_by,
        };
        self.deps.db.insert_newsletter_issue(&issue).await?;
        Ok(issue)
    }

    /// 確認されないまま古くなった確認待ちを消す
    pub async fn clean_up(&self, now: DateTime<Utc>) -> u64 {
        let cutoff = (now - chrono::Duration::days(PENDING_RETENTION_DAYS))
            .to_rfc3339_opts(SecondsFormat::Micros, true);
        match self
            .deps
            .db
            .delete_stale_pending_newsletter_subscribers(&cutoff)
            .await
        {
            Ok(n) => {
                if n > 0 {
                    tracing::info!("Deleted {} unconfirmed newsletter sign-ups", n);
                }
                n
            }
            Err(e) => {
                tracing::warn!(
                    "Could not delete old unconfirmed newsletter sign-ups: {}",
                    e
                );
                0
            }
        }
    }

    /// 1 日に 1 回、確認されないまま古くなった確認待ちを消す
    pub fn spawn_cleanup_task(self: &Arc<Self>) {
        let service = self.clone();
        tokio::spawn(async move {
            loop {
                service.clean_up(Utc::now()).await;
                tokio::time::sleep(Duration::from_secs(24 * 60 * 60)).await;
            }
        });
    }
}

#[cfg(test)]
mod tests;
