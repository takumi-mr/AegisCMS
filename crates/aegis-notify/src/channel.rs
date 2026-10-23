//! 配信チャネルの抽象。新しい配信方法（Slack・SMS・Webhook など）は `NotificationChannel` を
//! 実装し、`NotificationService::with_channels` で加える（設定から使えるようにするなら `ChannelKind` にも）。
//! Wasm プラグインも `send-notification` フックでチャネルになれる（`channels = ["plugin:<名前>"]`）

use async_trait::async_trait;

pub use aegis_core::{Delivered, NotificationMessage, Recipient, SendError};

#[async_trait]
pub trait NotificationChannel: Send + Sync {
    /// 監査ログ・ログに出す名前（`email` など）
    fn name(&self) -> &str;

    /// この宛先に送れるか。送れない宛先（メールアドレスの無い運営者向けなど）は飛ばす
    fn accepts(&self, recipient: &Recipient) -> bool;

    /// 送る。扱わない通知は `Ok(Delivered::Skipped)`
    async fn send(&self, message: &NotificationMessage) -> Result<Delivered, SendError>;
}
