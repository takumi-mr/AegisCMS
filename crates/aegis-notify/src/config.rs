//! `[notifications]` の設定

use std::collections::HashMap;
use std::path::PathBuf;

use aegis_core::{AegisError, AuditAction, AuditOutcome, Result};
use serde::{Deserialize, Serialize};

/// 配信チャネルの種類（`[notifications].channels`）。設定では `"email"` / `"log"` /
/// `"plugin:<プラグイン名>"`（`send-notification` フックを持つ Wasm プラグイン）と書く
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub enum ChannelKind {
    /// SMTP サーバー経由のメール（`[notifications.email]`）
    Email,
    /// サーバーのログに出す（`aegis::notify` ターゲット。開発・動作確認用）
    Log,
    /// Wasm プラグイン（`[plugins].enabled` にあり、`send-notification` フックを宣言していること）
    Plugin(String),
}

impl ChannelKind {
    /// 設定での書き方（`email` / `log` / `plugin:<名前>`）
    pub fn name(&self) -> String {
        match self {
            Self::Email => "email".to_string(),
            Self::Log => "log".to_string(),
            Self::Plugin(name) => format!("plugin:{}", name),
        }
    }
}

impl TryFrom<String> for ChannelKind {
    type Error = String;

    fn try_from(value: String) -> std::result::Result<Self, Self::Error> {
        match value.trim() {
            "email" => Ok(Self::Email),
            "log" => Ok(Self::Log),
            other => match other.strip_prefix("plugin:") {
                Some(name) if aegis_core::is_valid_plugin_name(name.trim()) => {
                    Ok(Self::Plugin(name.trim().to_string()))
                }
                _ => Err(format!(
                    "unknown notification channel {:?}: use \"email\", \"log\" or \"plugin:<plugin name>\"",
                    value
                )),
            },
        }
    }
}

impl From<ChannelKind> for String {
    fn from(kind: ChannelKind) -> Self {
        kind.name()
    }
}

/// 通知（注文の受付・入金確認・発送など）
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct NotificationsConfig {
    pub enabled: bool,
    /// 使う配信チャネル。同じ通知をそれぞれのチャネルで送る
    pub channels: Vec<ChannelKind>,
    /// 組み込みテンプレートの言語（`en` / `ja`）
    pub locale: String,
    /// テンプレートを置くディレクトリ（`<種類>.subject` / `<種類>.txt`）。無いものは組み込みを使う
    pub templates_dir: Option<PathBuf>,
    /// 送らない通知の種類（`order.shipped` など）
    pub disabled_events: Vec<String>,
    /// 配信に失敗したときの試行回数（初回を含む、1〜10）
    pub max_attempts: u32,
    /// HTML の本文も作る（メールはプレーンテキストと HTML の両方を送る）
    pub html: bool,
    /// 送信待ち（`[notifications.queue]`）
    pub queue: QueueConfig,
    pub email: EmailConfig,
    /// 監査イベント（管理者のログイン・ログインのロックなど）の通知
    pub audit: AuditNotificationsConfig,
}

impl Default for NotificationsConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            channels: vec![ChannelKind::Email],
            locale: "en".to_string(),
            templates_dir: None,
            disabled_events: Vec::new(),
            max_attempts: 3,
            html: true,
            queue: QueueConfig::default(),
            email: EmailConfig::default(),
            audit: AuditNotificationsConfig::default(),
        }
    }
}

impl NotificationsConfig {
    /// 設定値の検証（`enabled` のときのみ）
    pub fn validate(&self) -> Result<()> {
        if !self.enabled {
            return Ok(());
        }
        if self.channels.is_empty() {
            return Err(AegisError::Config(
                "[notifications].channels is empty: use [\"email\"] and/or [\"log\"]".to_string(),
            ));
        }
        if !crate::templates::LOCALES.contains(&self.locale.as_str()) {
            return Err(AegisError::Config(format!(
                "[notifications].locale must be one of {:?} (got {:?})",
                crate::templates::LOCALES,
                self.locale
            )));
        }
        if !(1..=10).contains(&self.max_attempts) {
            return Err(AegisError::Config(
                "[notifications].max_attempts must be between 1 and 10".to_string(),
            ));
        }
        if self.channels.contains(&ChannelKind::Email) {
            self.email.validate()?;
        }
        self.queue.validate()?;
        self.audit.validate()?;
        Ok(())
    }

    pub fn is_event_enabled(&self, kind: &str) -> bool {
        !self.disabled_events.iter().any(|e| e == kind)
    }
}

/// 送信待ち（`[notifications.queue]`）
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct QueueConfig {
    /// 送信待ちを DB（`_aegis_notification_queue`）に保存する。再起動しても送信待ち・再試行が失われず、
    /// 複数台のサーバーでは 1 台だけが送る。`false` ならメモリ上（再起動で失われる）
    pub persistent: bool,
    /// 送る時刻を迎えた通知を DB に確かめる間隔（秒、1〜3600）。新しい通知はこの間隔を待たずに送る
    pub poll_interval_secs: u64,
    /// 送れなかった通知を残す日数（Admin Studio から再送できる期間。0 なら消さない）
    pub retention_days: u32,
}

impl Default for QueueConfig {
    fn default() -> Self {
        Self {
            persistent: true,
            poll_interval_secs: 10,
            retention_days: 7,
        }
    }
}

impl QueueConfig {
    fn validate(&self) -> Result<()> {
        if !(1..=3600).contains(&self.poll_interval_secs) {
            return Err(AegisError::Config(
                "[notifications.queue].poll_interval_secs must be between 1 and 3600".to_string(),
            ));
        }
        Ok(())
    }
}

/// SMTP 接続の暗号化
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SmtpTls {
    /// 平文で接続して STARTTLS で暗号化する（必須。既定のポート 587）
    #[default]
    Starttls,
    /// 最初から TLS で接続する（既定のポート 465）
    Tls,
    /// 暗号化しない（同じホスト・社内ネットワークのリレー用。既定のポート 25）
    None,
}

/// メール（外部の SMTP サーバーから送る）
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct EmailConfig {
    pub smtp_host: String,
    /// 0 なら `tls` に応じた既定のポート
    pub smtp_port: u16,
    pub tls: SmtpTls,
    /// SMTP 認証のユーザー名（空なら認証しない）
    pub username: String,
    /// SMTP 認証のパスワード。空なら環境変数 `AEGIS_SMTP_PASSWORD`
    pub password: String,
    /// 差出人（`"Shop <shop@example.com>"` または `shop@example.com`）
    pub from: String,
    /// 返信先（空なら差出人）
    pub reply_to: String,
    /// 運営者向けの通知（新しい注文など）の宛先
    pub staff_recipients: Vec<String>,
    /// 接続・送信のタイムアウト（秒）
    pub timeout_secs: u64,
}

impl Default for EmailConfig {
    fn default() -> Self {
        Self {
            smtp_host: String::new(),
            smtp_port: 0,
            tls: SmtpTls::Starttls,
            username: String::new(),
            password: String::new(),
            from: String::new(),
            reply_to: String::new(),
            staff_recipients: Vec::new(),
            timeout_secs: 15,
        }
    }
}

impl EmailConfig {
    pub fn port(&self) -> u16 {
        match (self.smtp_port, self.tls) {
            (0, SmtpTls::Starttls) => 587,
            (0, SmtpTls::Tls) => 465,
            (0, SmtpTls::None) => 25,
            (port, _) => port,
        }
    }

    pub fn password(&self) -> String {
        if !self.password.is_empty() {
            return self.password.clone();
        }
        std::env::var("AEGIS_SMTP_PASSWORD").unwrap_or_default()
    }

    fn validate(&self) -> Result<()> {
        if self.smtp_host.trim().is_empty() {
            return Err(AegisError::Config(
                "[notifications.email].smtp_host is required for the \"email\" channel".to_string(),
            ));
        }
        let mailbox = |field: &str, value: &str| {
            value
                .parse::<lettre::message::Mailbox>()
                .map(|_| ())
                .map_err(|e| {
                    AegisError::Config(format!(
                        "[notifications.email].{} {:?} is not a valid address: {}",
                        field, value, e
                    ))
                })
        };
        if self.from.trim().is_empty() {
            return Err(AegisError::Config(
                "[notifications.email].from is required for the \"email\" channel".to_string(),
            ));
        }
        mailbox("from", &self.from)?;
        if !self.reply_to.is_empty() {
            mailbox("reply_to", &self.reply_to)?;
        }
        for address in &self.staff_recipients {
            mailbox("staff_recipients", address)?;
        }
        if self.timeout_secs == 0 {
            return Err(AegisError::Config(
                "[notifications.email].timeout_secs must be at least 1".to_string(),
            ));
        }
        Ok(())
    }
}

/// 監査イベントの通知（`[[notifications.audit.rules]]`）。監査ログに記録されたイベントのうち、
/// どれかのルールに合うものを通知する（`[audit].enabled = true` が必要）
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct AuditNotificationsConfig {
    pub rules: Vec<AuditRule>,
}

impl AuditNotificationsConfig {
    fn validate(&self) -> Result<()> {
        for (i, rule) in self.rules.iter().enumerate() {
            if rule.action == AuditAction::NotificationSend {
                return Err(AegisError::Config(format!(
                    "[[notifications.audit.rules]] #{}: \"notification.send\" cannot be notified (it would notify about its own notifications)",
                    i + 1
                )));
            }
            for address in &rule.recipients {
                address.parse::<lettre::Address>().map_err(|e| {
                    AegisError::Config(format!(
                        "[[notifications.audit.rules]] #{}: recipient {:?} is not a valid address: {}",
                        i + 1,
                        address,
                        e
                    ))
                })?;
            }
        }
        Ok(())
    }
}

/// 通知する監査イベントの条件。指定した条件をすべて満たすイベントを通知する
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AuditRule {
    /// 件名・本文に出す見出し（空なら操作と結果から作る）
    #[serde(default)]
    pub title: String,
    /// 操作（`session.create` など。監査ログの `action`）
    pub action: AuditAction,
    /// 結果（`success` / `failure` / `denied`）。空ならすべて
    #[serde(default)]
    pub outcomes: Vec<AuditOutcome>,
    /// 操作者のロール（`admin` など）。空ならすべて。操作者が分からないイベント
    /// （失敗したログインなど）は、指定すると合わない
    #[serde(default)]
    pub roles: Vec<String>,
    /// 詳細（`details`）の値。`"*"` はキーがあれば何でもよい（例: `{ locked = "*" }`）
    #[serde(default)]
    pub details: HashMap<String, serde_json::Value>,
    /// 宛先のメールアドレス。空なら運営者（`[notifications.email].staff_recipients`）
    #[serde(default)]
    pub recipients: Vec<String>,
    /// 1 時間あたりの通知の上限（0 で無制限）。総当たり攻撃などで通知が殺到しないように
    #[serde(default = "default_max_per_hour")]
    pub max_per_hour: u32,
}

fn default_max_per_hour() -> u32 {
    20
}
