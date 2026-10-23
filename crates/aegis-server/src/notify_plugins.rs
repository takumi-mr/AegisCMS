//! Wasm プラグインを通知の配信チャネルにする（`[notifications].channels = ["plugin:<名前>"]`）。
//! 通知のたびにプラグインの `send-notification` フックを呼ぶ

use std::sync::Arc;

use aegis_core::{AegisError, Delivered, NotificationMessage, Recipient, Result, SendError};
use aegis_notify::NotificationChannel;
use aegis_runtime::PluginSupervisor;
use async_trait::async_trait;
use tokio::sync::RwLock;

const HOOK: &str = "send-notification";

pub struct PluginNotificationChannel {
    plugin: String,
    /// `plugin:<名前>`（監査ログ・ログに出す名前）
    label: String,
    supervisor: Arc<RwLock<PluginSupervisor>>,
}

impl PluginNotificationChannel {
    /// プラグインが読み込まれていて `send-notification` を宣言していることを確かめて作る
    pub async fn checked(plugin: &str, supervisor: Arc<RwLock<PluginSupervisor>>) -> Result<Self> {
        {
            let loaded = supervisor.read().await;
            if loaded.plugin(plugin).is_none() {
                return Err(AegisError::Config(format!(
                    "[notifications].channels: plugin \"{}\" is not loaded (add it to [plugins].enabled)",
                    plugin
                )));
            }
            if !loaded.plugin_declares_hook(plugin, HOOK) {
                return Err(AegisError::Config(format!(
                    "[notifications].channels: plugin \"{}\" does not declare the \"{}\" hook in its manifest",
                    plugin, HOOK
                )));
            }
        }
        Ok(Self::unchecked(plugin, supervisor))
    }

    fn unchecked(plugin: &str, supervisor: Arc<RwLock<PluginSupervisor>>) -> Self {
        Self {
            plugin: plugin.to_string(),
            label: format!("plugin:{}", plugin),
            supervisor,
        }
    }
}

#[async_trait]
impl NotificationChannel for PluginNotificationChannel {
    fn name(&self) -> &str {
        &self.label
    }

    /// 宛先の選び分けはプラグインが行う（扱わないものは `Skipped` を返す）
    fn accepts(&self, _recipient: &Recipient) -> bool {
        true
    }

    async fn send(
        &self,
        message: &NotificationMessage,
    ) -> std::result::Result<Delivered, SendError> {
        let plugin = self.supervisor.read().await.plugin(&self.plugin).cloned();
        let Some(plugin) = plugin else {
            return Err(SendError::permanent(format!(
                "plugin \"{}\" is not loaded",
                self.plugin
            )));
        };
        plugin.send_notification(message.clone()).await
    }
}

/// 設定の `plugin:<名前>` のチャネルを作る（`NotificationService::from_config_with_plugins` 用）。
/// 検証は呼ぶ前に `validate_plugin_channels` で済ませておく
pub fn plugin_channel_factory(
    supervisor: Arc<RwLock<PluginSupervisor>>,
) -> impl Fn(&str) -> Result<Arc<dyn NotificationChannel>> {
    move |plugin: &str| {
        Ok(Arc::new(PluginNotificationChannel::unchecked(
            plugin,
            supervisor.clone(),
        )) as Arc<dyn NotificationChannel>)
    }
}

/// `[notifications].channels` のプラグインがすべて読み込まれ、フックを宣言しているか
pub async fn validate_plugin_channels(
    config: &aegis_notify::NotificationsConfig,
    supervisor: &Arc<RwLock<PluginSupervisor>>,
) -> Result<()> {
    for kind in &config.channels {
        if let aegis_notify::ChannelKind::Plugin(name) = kind {
            PluginNotificationChannel::checked(name, supervisor.clone()).await?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use aegis_core::{PluginCapabilities, PluginManifest};
    use aegis_runtime::AegisPlugin;
    use std::sync::Mutex;

    struct SlackLike {
        manifest: PluginManifest,
        received: Mutex<Vec<NotificationMessage>>,
    }

    #[async_trait]
    impl AegisPlugin for SlackLike {
        fn manifest(&self) -> &PluginManifest {
            &self.manifest
        }

        async fn send_notification(
            &self,
            message: NotificationMessage,
        ) -> std::result::Result<Delivered, SendError> {
            if message.recipient != Recipient::Staff {
                return Ok(Delivered::Skipped);
            }
            self.received.lock().unwrap().push(message);
            Ok(Delivered::Sent)
        }
    }

    fn plugin(name: &str, hooks: &[&str]) -> Arc<SlackLike> {
        Arc::new(SlackLike {
            manifest: PluginManifest {
                name: name.to_string(),
                version: "1.0.0".to_string(),
                entrypoint: "plugin.wasm".to_string(),
                capabilities: PluginCapabilities {
                    network: None,
                    storage: None,
                    hooks: hooks.iter().map(|h| h.to_string()).collect(),
                },
                ..Default::default()
            },
            received: Mutex::new(Vec::new()),
        })
    }

    fn message(recipient: Recipient) -> NotificationMessage {
        NotificationMessage {
            kind: "audit.session.create".to_string(),
            recipient,
            subject: "Admin login".to_string(),
            body: "root logged in".to_string(),
            data: serde_json::json!({}),
        }
    }

    #[tokio::test]
    async fn delivers_through_the_plugin_hook() {
        let slack = plugin("slack", &[HOOK]);
        let mut supervisor = PluginSupervisor::new();
        supervisor.register(slack.clone());
        supervisor.register(plugin("quiet", &["on-render-page"]));
        let supervisor = Arc::new(RwLock::new(supervisor));

        let channel = PluginNotificationChannel::checked("slack", supervisor.clone())
            .await
            .unwrap();
        assert_eq!(channel.name(), "plugin:slack");
        assert_eq!(
            channel.send(&message(Recipient::Staff)).await,
            Ok(Delivered::Sent)
        );
        assert_eq!(
            channel
                .send(&message(Recipient::person("a@example.com", "")))
                .await,
            Ok(Delivered::Skipped)
        );
        assert_eq!(slack.received.lock().unwrap()[0].subject, "Admin login");

        let missing = PluginNotificationChannel::checked("nope", supervisor.clone())
            .await
            .err()
            .unwrap();
        assert!(missing.to_string().contains("is not loaded"), "{}", missing);
        let undeclared = PluginNotificationChannel::checked("quiet", supervisor)
            .await
            .err()
            .unwrap();
        assert!(
            undeclared.to_string().contains("does not declare"),
            "{}",
            undeclared
        );
    }

    /// 公式プラグイン（ビルド済みの plugin.wasm）を通知のチャネルとして使う
    #[tokio::test]
    async fn official_order_notifier_wasm_is_a_notification_channel() {
        use aegis_runtime::{HostServices, WasmEngine, WasmPluginInstance};
        use serde_json::{json, Value};

        // 受け取った本文を覚え、パスに応じて 200 / 503 / 400 を返す Webhook
        let received = Arc::new(Mutex::new(Vec::<(String, Value)>::new()));
        let sink = received.clone();
        let app = axum::Router::new().route(
            "/{status}",
            axum::routing::post(
                move |axum::extract::Path(status): axum::extract::Path<u16>,
                      axum::Json(body): axum::Json<Value>| {
                    let sink = sink.clone();
                    async move {
                        sink.lock().unwrap().push((status.to_string(), body));
                        axum::http::StatusCode::from_u16(status).unwrap()
                    }
                },
            ),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });

        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../plugins/official-order-notifier");
        let mut manifest = PluginManifest::from_json_with_defaults(
            &std::fs::read_to_string(dir.join("plugin.manifest.json")).unwrap(),
            &Default::default(),
        )
        .unwrap();
        manifest
            .capabilities
            .network
            .as_mut()
            .unwrap()
            .allowed_domains = vec!["127.0.0.1".to_string()];
        let wasm = std::fs::read(dir.join("plugin.wasm")).unwrap();
        let engine = WasmEngine::new().unwrap();

        let service_for = |status: u16| {
            let plugins: aegis_runtime::PluginsConfig = toml::from_str(&format!(
                "enabled = [\"official-order-notifier\"]\n\
                 [settings.official-order-notifier]\nwebhook_url = \"{}/{}\"\nnotify_on = \"none\"\n",
                base, status
            ))
            .unwrap();
            let services: Arc<dyn HostServices> = Arc::new(
                crate::plugin_host::ServerHostServices::new(
                    Arc::new(aegis_cache::MemoryCacheBackend::new()),
                    plugins,
                )
                .unwrap(),
            );
            let mut supervisor = PluginSupervisor::new();
            supervisor.register(Arc::new(
                WasmPluginInstance::from_bytes(engine.clone(), manifest.clone(), &wasm, services)
                    .unwrap(),
            ));
            let config: aegis_notify::NotificationsConfig = toml::from_str(
                "enabled = true\nchannels = [\"plugin:official-order-notifier\"]\nlocale = \"ja\"\n",
            )
            .unwrap();
            aegis_notify::NotificationService::from_config_with_plugins(
                &config,
                aegis_notify::SiteInfo {
                    name: "朝凪珈琲".to_string(),
                    ..Default::default()
                },
                None,
                plugin_channel_factory(Arc::new(RwLock::new(supervisor))),
            )
            .unwrap()
            .with_retry_delay(std::time::Duration::from_millis(1))
        };
        let event = aegis_core::NotificationEvent::new(
            "audit.session.create",
            vec![
                Recipient::person("hanako@example.com", ""),
                Recipient::Staff,
            ],
            json!({
                "title": "",
                "audit": {
                    "action": "session.create",
                    "outcome": "failure",
                    "occurred_at": "2026-10-23T05:00:00Z",
                    "ip": "203.0.113.7",
                    "details": { "username": "root", "locked": "username" }
                }
            }),
        );

        // 運営者向けだけを送る（顧客向けは Skipped で結果に出ない）
        let deliveries = service_for(200).deliver(&event).await;
        assert_eq!(deliveries.len(), 1, "{:?}", deliveries);
        assert_eq!(deliveries[0].channel, "plugin:official-order-notifier");
        assert_eq!(deliveries[0].recipient, Recipient::Staff);
        assert!(deliveries[0].result.is_ok(), "{:?}", deliveries[0].result);
        let text = received.lock().unwrap()[0].1["text"]
            .as_str()
            .unwrap()
            .to_string();
        assert!(
            text.starts_with("*【朝凪珈琲】ログインをロックしました（root）*\n"),
            "{}",
            text
        );
        assert!(text.contains("IP アドレス: 203.0.113.7"), "{}", text);

        // 5xx は再試行する（max_attempts = 3）
        received.lock().unwrap().clear();
        let deliveries = service_for(503).deliver(&event).await;
        assert_eq!(deliveries[0].attempts, 3);
        assert!(deliveries[0]
            .result
            .as_ref()
            .unwrap_err()
            .message
            .contains("503"));
        assert_eq!(received.lock().unwrap().len(), 3);

        // 4xx（設定の誤り）は再試行しない
        received.lock().unwrap().clear();
        let deliveries = service_for(400).deliver(&event).await;
        let error = deliveries[0].result.as_ref().unwrap_err();
        assert_eq!(deliveries[0].attempts, 1);
        assert!(error.permanent, "{:?}", error);
        assert_eq!(received.lock().unwrap().len(), 1);
    }
}
