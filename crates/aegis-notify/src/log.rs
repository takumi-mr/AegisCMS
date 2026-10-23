//! ログに出すチャネル（`channels = ["log"]`）。SMTP サーバーの無い開発環境での動作確認用

use aegis_core::Recipient;
use async_trait::async_trait;

use crate::channel::{NotificationChannel, NotificationMessage, SendError};

pub struct LogChannel;

#[async_trait]
impl NotificationChannel for LogChannel {
    fn name(&self) -> &str {
        "log"
    }

    fn accepts(&self, _recipient: &Recipient) -> bool {
        true
    }

    async fn send(&self, message: &NotificationMessage) -> Result<(), SendError> {
        let to = match &message.recipient {
            Recipient::Person { email, .. } => email.as_str(),
            Recipient::Staff => "staff",
        };
        tracing::info!(
            target: "aegis::notify",
            "Notification '{}'\nTo: {}\nSubject: {}\n\n{}",
            message.kind,
            to,
            message.subject,
            message.body
        );
        Ok(())
    }
}
