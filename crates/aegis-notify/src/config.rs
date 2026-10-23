//! `[notifications]` の設定

use std::path::PathBuf;

use aegis_core::{AegisError, Result};
use serde::{Deserialize, Serialize};

/// 配信チャネルの種類（`[notifications].channels`）
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ChannelKind {
    /// SMTP サーバー経由のメール（`[notifications.email]`）
    Email,
    /// サーバーのログに出す（`aegis::notify` ターゲット。開発・動作確認用）
    Log,
}

impl ChannelKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Email => "email",
            Self::Log => "log",
        }
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
    pub email: EmailConfig,
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
            email: EmailConfig::default(),
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
        Ok(())
    }

    pub fn is_event_enabled(&self, kind: &str) -> bool {
        !self.disabled_events.iter().any(|e| e == kind)
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
