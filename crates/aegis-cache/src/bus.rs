//! サーバー間の変更の知らせ（イベントバス）。`[cache]` と同じバックエンドを使う:
//! メモリは同じプロセスの中だけ（1 台構成）、Redis は Pub/Sub で同じ Redis を使う全サーバーに届く。
//! どちらも送ったサーバー自身にも届くので、受け取る側は「知らせが来たら共有の状態（DB など）を読み直す」
//! だけでよく、1 台でも複数台でも同じ処理になる

use aegis_core::{AegisError, Result};
use async_trait::async_trait;
use futures_util::StreamExt;
use redis::aio::{ConnectionManager, PubSub};
use serde::{Deserialize, Serialize};
use std::time::Duration;
use tokio::sync::broadcast;

/// Redis の Pub/Sub のチャネル
pub const BUS_CHANNEL: &str = "aegis:events";

/// 受け取り口ごとに溜めておける知らせの数（超えたら `Resync` になる）
const LOCAL_CAPACITY: usize = 256;
/// Redis の購読が切れたときの再接続の待ち（最初と上限）
const MIN_RECONNECT_DELAY: Duration = Duration::from_millis(500);
const MAX_RECONNECT_DELAY: Duration = Duration::from_secs(30);

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BusMessage {
    /// `publish` された知らせ（送ったサーバー自身にも届く）
    Event { topic: String, payload: String },
    /// 知らせを取りこぼしたかもしれない（Redis への再接続・受け取りの遅れ）。状態をすべて読み直す
    Resync,
}

/// サーバー間の変更の知らせ
#[async_trait]
pub trait EventBus: Send + Sync {
    /// 全サーバー（自分を含む）に知らせる
    async fn publish(&self, topic: &str, payload: &str) -> Result<()>;

    /// 受け取り口。作ったあとに送られた知らせを受け取る
    fn subscribe(&self) -> BusSubscription;

    /// ほかのサーバーにも届くか（メモリは `false`）
    fn reaches_other_servers(&self) -> bool;
}

/// [`EventBus::subscribe`] の受け取り口
pub struct BusSubscription {
    rx: broadcast::Receiver<BusMessage>,
}

impl BusSubscription {
    /// 次の知らせ（バスが無くなったら `None`）。受け取りが遅れて溢れた分は `Resync` にまとめる
    pub async fn recv(&mut self) -> Option<BusMessage> {
        match self.rx.recv().await {
            Ok(message) => Some(message),
            Err(broadcast::error::RecvError::Lagged(skipped)) => {
                tracing::warn!("Event bus subscriber lagged behind by {} messages", skipped);
                Some(BusMessage::Resync)
            }
            Err(broadcast::error::RecvError::Closed) => None,
        }
    }
}

/// 同じプロセスの中だけに届くバス（`[cache].backend = "memory"`、1 台構成）
#[derive(Clone)]
pub struct MemoryEventBus {
    tx: broadcast::Sender<BusMessage>,
}

impl Default for MemoryEventBus {
    fn default() -> Self {
        Self::new()
    }
}

impl MemoryEventBus {
    pub fn new() -> Self {
        Self {
            tx: broadcast::channel(LOCAL_CAPACITY).0,
        }
    }
}

#[async_trait]
impl EventBus for MemoryEventBus {
    async fn publish(&self, topic: &str, payload: &str) -> Result<()> {
        // 受け取り口が無ければ捨てる（エラーにしない）
        let _ = self.tx.send(BusMessage::Event {
            topic: topic.to_string(),
            payload: payload.to_string(),
        });
        Ok(())
    }

    fn subscribe(&self) -> BusSubscription {
        BusSubscription {
            rx: self.tx.subscribe(),
        }
    }

    fn reaches_other_servers(&self) -> bool {
        false
    }
}

/// Redis の Pub/Sub で同じ Redis を使う全サーバーに届くバス
pub struct RedisEventBus {
    manager: ConnectionManager,
    tx: broadcast::Sender<BusMessage>,
    listener: tokio::task::JoinHandle<()>,
}

/// チャネルに流す形
#[derive(Serialize, Deserialize)]
struct WireMessage {
    topic: String,
    payload: String,
}

impl RedisEventBus {
    /// 送信用の接続と購読用の接続を作る（どちらかに失敗したらエラー）。購読が切れたら自動で再接続し、
    /// 再接続のたびに `Resync` を流す
    pub async fn connect(url: &str, timeout: Duration) -> Result<Self> {
        let (client, manager) = crate::redis::connect_manager(url, timeout).await?;
        let pubsub = subscribe(&client, timeout).await?;
        let tx = broadcast::channel(LOCAL_CAPACITY).0;
        let listener = tokio::spawn(listen(client, timeout, pubsub, tx.clone()));
        Ok(Self {
            manager,
            tx,
            listener,
        })
    }
}

impl Drop for RedisEventBus {
    fn drop(&mut self) {
        self.listener.abort();
    }
}

async fn subscribe(client: &redis::Client, timeout: Duration) -> Result<PubSub> {
    let attempt = async {
        let mut pubsub = client.get_async_pubsub().await?;
        pubsub.subscribe(BUS_CHANNEL).await?;
        Ok::<_, redis::RedisError>(pubsub)
    };
    match tokio::time::timeout(timeout, attempt).await {
        Ok(Ok(pubsub)) => Ok(pubsub),
        Ok(Err(e)) => Err(AegisError::Database(format!(
            "Failed to subscribe to Redis channel '{}': {}",
            BUS_CHANNEL, e
        ))),
        Err(_) => Err(AegisError::Database(format!(
            "Timed out subscribing to Redis channel '{}'",
            BUS_CHANNEL
        ))),
    }
}

/// 購読した知らせを受け取り口に配る。接続が切れたら待ちを伸ばしながら購読し直す
async fn listen(
    client: redis::Client,
    timeout: Duration,
    mut pubsub: PubSub,
    tx: broadcast::Sender<BusMessage>,
) {
    loop {
        let mut messages = pubsub.into_on_message();
        while let Some(message) = messages.next().await {
            let payload: String = match message.get_payload() {
                Ok(payload) => payload,
                Err(e) => {
                    tracing::warn!("Ignoring a non-text event bus message: {}", e);
                    continue;
                }
            };
            match serde_json::from_str::<WireMessage>(&payload) {
                Ok(wire) => {
                    let _ = tx.send(BusMessage::Event {
                        topic: wire.topic,
                        payload: wire.payload,
                    });
                }
                Err(e) => tracing::warn!("Ignoring a malformed event bus message: {}", e),
            }
        }
        drop(messages);

        tracing::warn!(
            "Lost the Redis subscription to '{}', reconnecting",
            BUS_CHANNEL
        );
        let mut delay = MIN_RECONNECT_DELAY;
        pubsub = loop {
            tokio::time::sleep(delay).await;
            match subscribe(&client, timeout).await {
                Ok(pubsub) => break pubsub,
                Err(e) => {
                    tracing::warn!("{} (retrying in {:?})", e, delay * 2);
                    delay = (delay * 2).min(MAX_RECONNECT_DELAY);
                }
            }
        };
        tracing::info!("Resubscribed to Redis channel '{}'", BUS_CHANNEL);
        // 切れていた間の知らせは届かないので、受け取る側に読み直してもらう
        let _ = tx.send(BusMessage::Resync);
    }
}

#[async_trait]
impl EventBus for RedisEventBus {
    async fn publish(&self, topic: &str, payload: &str) -> Result<()> {
        let message = serde_json::to_string(&WireMessage {
            topic: topic.to_string(),
            payload: payload.to_string(),
        })
        .map_err(|e| AegisError::Internal(format!("Failed to encode event: {}", e)))?;
        // 二重に届いても受け取る側は読み直すだけなので、接続のエラーなら送り直す
        crate::redis::retry_on_reconnect(|| {
            let mut conn = self.manager.clone();
            let message = message.clone();
            async move {
                redis::cmd("PUBLISH")
                    .arg(BUS_CHANNEL)
                    .arg(message)
                    .query_async::<i64>(&mut conn)
                    .await
            }
        })
        .await
        .map_err(|e| AegisError::Database(format!("Redis PUBLISH error: {}", e)))?;
        Ok(())
    }

    fn subscribe(&self) -> BusSubscription {
        BusSubscription {
            rx: self.tx.subscribe(),
        }
    }

    fn reaches_other_servers(&self) -> bool {
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn next_event(subscription: &mut BusSubscription) -> BusMessage {
        tokio::time::timeout(Duration::from_secs(5), subscription.recv())
            .await
            .expect("an event within 5s")
            .expect("the bus is alive")
    }

    #[tokio::test]
    async fn test_memory_bus_delivers_to_every_subscriber_including_the_sender() {
        let bus = MemoryEventBus::new();
        // Publishing with nobody listening is not an error
        bus.publish("nobody", "").await.unwrap();

        let mut a = bus.subscribe();
        let mut b = bus.subscribe();
        bus.publish("plugins.enabled", "x").await.unwrap();
        let expected = BusMessage::Event {
            topic: "plugins.enabled".into(),
            payload: "x".into(),
        };
        assert_eq!(next_event(&mut a).await, expected);
        assert_eq!(next_event(&mut b).await, expected);
        assert!(!bus.reaches_other_servers());
    }

    #[tokio::test]
    async fn test_lagging_subscriber_gets_a_resync() {
        let bus = MemoryEventBus::new();
        let mut slow = bus.subscribe();
        for i in 0..LOCAL_CAPACITY + 1 {
            bus.publish("t", &i.to_string()).await.unwrap();
        }
        assert_eq!(next_event(&mut slow).await, BusMessage::Resync);
    }

    /// Two buses on one Redis stand for two servers (`REDIS_URL`, default redis://127.0.0.1:6379;
    /// skipped when it is not reachable — start it with `task docker:dbs:up`)
    #[tokio::test]
    async fn test_redis_bus_reaches_other_servers() {
        let url = match std::env::var("REDIS_URL") {
            Ok(url) => url,
            Err(_) => {
                // Skip at once when nothing listens (connecting would retry for seconds)
                let probe = tokio::net::TcpStream::connect("127.0.0.1:6379");
                if !matches!(
                    tokio::time::timeout(Duration::from_millis(150), probe).await,
                    Ok(Ok(_))
                ) {
                    eprintln!("Skipping Redis event bus test: 127.0.0.1:6379 is not reachable (start with task docker:dbs:up)");
                    return;
                }
                "redis://127.0.0.1:6379".to_string()
            }
        };
        let timeout = Duration::from_secs(2);
        let server_a = RedisEventBus::connect(&url, timeout).await.unwrap();
        let server_b = RedisEventBus::connect(&url, timeout).await.unwrap();
        let mut on_a = server_a.subscribe();
        let mut on_b = server_b.subscribe();

        server_a.publish("plugins.enabled", "{}").await.unwrap();
        let expected = BusMessage::Event {
            topic: "plugins.enabled".into(),
            payload: "{}".into(),
        };
        assert_eq!(next_event(&mut on_b).await, expected);
        assert_eq!(next_event(&mut on_a).await, expected);
        assert!(server_a.reaches_other_servers());

        // A dropped subscription is restored and reported as a Resync (messages sent meanwhile
        // are lost); later messages arrive again
        let mut conn = server_a.manager.clone();
        redis::cmd("CLIENT")
            .arg("KILL")
            .arg("TYPE")
            .arg("pubsub")
            .query_async::<i64>(&mut conn)
            .await
            .unwrap();
        assert_eq!(next_event(&mut on_b).await, BusMessage::Resync);
        assert_eq!(next_event(&mut on_a).await, BusMessage::Resync);
        server_b.publish("again", "").await.unwrap();
        assert_eq!(
            next_event(&mut on_a).await,
            BusMessage::Event {
                topic: "again".into(),
                payload: String::new(),
            }
        );
    }
}
