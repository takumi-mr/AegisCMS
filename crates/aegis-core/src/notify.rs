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
