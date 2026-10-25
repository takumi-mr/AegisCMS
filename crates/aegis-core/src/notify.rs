//! 通知の契約。通知を出す側（EC など）は `Notifier` に `NotificationEvent` を渡すだけで、
//! 本文の作成・宛先の解決・配信（メールなど）は実装側（`aegis-notify`）が行う

use serde::{Deserialize, Serialize};

/// 通知の宛先
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Recipient {
    /// 特定の人（注文した顧客など）
    Person {
        email: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        name: Option<String>,
    },
    /// サイトの運営者。実際の宛先は配信チャネルごとの設定で決まる（メールなら `staff_recipients`）
    Staff,
}

impl Recipient {
    pub fn person(email: impl Into<String>, name: impl Into<String>) -> Self {
        let name = name.into();
        Self::Person {
            email: email.into(),
            name: (!name.trim().is_empty()).then_some(name),
        }
    }
}

/// 通知のきっかけになった出来事
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NotificationEvent {
    /// 種類（`order.placed` など）。本文のテンプレートの名前になる
    pub kind: String,
    pub recipients: Vec<Recipient>,
    /// テンプレートに渡す値
    #[serde(default)]
    pub data: serde_json::Value,
}

impl NotificationEvent {
    pub fn new(
        kind: impl Into<String>,
        recipients: Vec<Recipient>,
        data: serde_json::Value,
    ) -> Self {
        Self {
            kind: kind.into(),
            recipients,
            data,
        }
    }
}

/// 通知の送り先。`notify` は配信を待たずに戻り、配信の失敗は実装側で記録する
pub trait Notifier: Send + Sync {
    fn notify(&self, event: NotificationEvent);
}

/// テンプレートから作った、1 つの宛先への通知。配信チャネル（メール・プラグインなど）に渡す
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NotificationMessage {
    /// 通知の種類（`order.placed` / `audit.session.create` など）
    pub kind: String,
    pub recipient: Recipient,
    pub subject: String,
    /// 本文（プレーンテキスト）
    pub body: String,
    /// HTML の本文（`[notifications].html` のとき。メールは本文と両方を送る）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub html: Option<String>,
    /// テンプレートに渡した値（注文・監査イベントの内容、`site_name` など）
    #[serde(default)]
    pub data: serde_json::Value,
}

/// 配信の結果
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Delivered {
    Sent,
    /// チャネルが送らなかった（扱わない種類・宛先）。失敗ではなく、監査ログにも残さない
    Skipped,
}

/// 配信の失敗
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SendError {
    pub message: String,
    /// 繰り返しても成功しない失敗（宛先の不備・認証の拒否など）。再試行しない
    #[serde(default)]
    pub permanent: bool,
}

impl SendError {
    pub fn temporary(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            permanent: false,
        }
    }

    pub fn permanent(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            permanent: true,
        }
    }
}

impl std::fmt::Display for SendError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for SendError {}
