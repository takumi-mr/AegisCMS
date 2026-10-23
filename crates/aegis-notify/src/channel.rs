//! 配信チャネルの抽象。新しい配信方法（Slack・SMS・Webhook など）は `NotificationChannel` を
//! 実装し、`NotificationService::with_channel` で加える（設定から使えるようにするなら `ChannelKind` にも）

use aegis_core::Recipient;
use async_trait::async_trait;

/// テンプレートから作った、1 つの宛先への通知
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NotificationMessage {
    /// 通知の種類（`order.placed` など）
    pub kind: String,
    pub recipient: Recipient,
    pub subject: String,
    /// 本文（プレーンテキスト）
    pub body: String,
}

/// 配信の失敗
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SendError {
    pub message: String,
    /// 繰り返しても成功しない失敗（宛先の不備・認証の拒否など）。再試行しない
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

#[async_trait]
pub trait NotificationChannel: Send + Sync {
    /// 監査ログ・ログに出す名前（`email` など）
    fn name(&self) -> &str;

    /// この宛先に送れるか。送れない宛先（メールアドレスの無い運営者向けなど）は飛ばす
    fn accepts(&self, recipient: &Recipient) -> bool;

    async fn send(&self, message: &NotificationMessage) -> Result<(), SendError>;
}
