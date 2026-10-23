//! 監査ログ（`[audit]`）: 出力先（DB / ログ / Prometheus）への配送と保持期間による削除

pub mod config;
pub mod database;
pub mod log;
pub mod metrics;
pub mod origin;

pub use database::DatabaseAuditSink;
pub use log::LogAuditSink;
pub use metrics::PrometheusAuditSink;

pub use config::{AuditConfig, AuditSinkKind};
pub use origin::AuditOrigin;

use aegis_core::{
    format_audit_timestamp, AuditAction, AuditEvent, AuditOutcome, AuditSink, Result,
};
use chrono::{DateTime, Utc};
use serde_json::json;
use std::sync::Arc;
use std::time::Duration;

use aegis_db::DatabaseAdapter;

/// 設定された全 `AuditSink` へ監査イベントを配送するサービス
/// 出力先の失敗はリクエスト処理を妨げないよう、エラーログに留める
pub struct AuditService {
    enabled: bool,
    trust_forwarded_for: bool,
    sinks: Vec<Arc<dyn AuditSink>>,
    /// `database` 出力先が有効な場合の検索用アダプター
    reader: Option<Arc<dyn DatabaseAdapter>>,
    /// `prometheus` 出力先が有効な場合のメトリクス
    metrics: Option<Arc<PrometheusAuditSink>>,
    /// 監査ログの保持日数（0 で無期限）
    retention_days: u32,
    purge_interval: Duration,
}

impl AuditService {
    pub fn from_config(config: &AuditConfig, db: Arc<dyn DatabaseAdapter>) -> Self {
        let mut service = Self {
            enabled: config.enabled,
            trust_forwarded_for: config.trust_forwarded_for,
            sinks: Vec::new(),
            reader: None,
            metrics: None,
            retention_days: config.retention_days,
            purge_interval: Duration::from_secs(config.purge_interval_secs.max(60)),
        };
        if !config.enabled {
            tracing::info!("Audit logging is disabled");
            return service;
        }

        for &kind in &config.sinks {
            // 同じ出力先が重複指定されても 1 回だけ登録する
            if service.has_sink(kind) {
                continue;
            }
            match kind {
                AuditSinkKind::Database => {
                    service
                        .sinks
                        .push(Arc::new(DatabaseAuditSink::new(db.clone())));
                    service.reader = Some(db.clone());
                }
                AuditSinkKind::Log => service.sinks.push(Arc::new(LogAuditSink)),
                AuditSinkKind::Prometheus => {
                    let metrics = Arc::new(PrometheusAuditSink::new());
                    service.sinks.push(metrics.clone());
                    service.metrics = Some(metrics);
                }
            }
        }

        let names: Vec<&str> = service.sinks.iter().map(|s| s.name()).collect();
        tracing::info!("Audit logging enabled (sinks: {:?})", names);
        service
    }

    /// 出力先を直接渡して作る（独自の出力先・テスト用）
    pub fn with_sinks(sinks: Vec<Arc<dyn AuditSink>>) -> Self {
        Self {
            enabled: true,
            trust_forwarded_for: false,
            sinks,
            reader: None,
            metrics: None,
            retention_days: 0,
            purge_interval: Duration::from_secs(3600),
        }
    }

    /// 出力先を加える（監査ログが有効なときのみ。`[notifications.audit]` の通知など）
    pub fn add_sink(&mut self, sink: Arc<dyn AuditSink>) {
        if self.enabled {
            self.sinks.push(sink);
        }
    }

    fn has_sink(&self, kind: AuditSinkKind) -> bool {
        match kind {
            AuditSinkKind::Database => self.reader.is_some(),
            AuditSinkKind::Log => self.sinks.iter().any(|s| s.name() == "log"),
            AuditSinkKind::Prometheus => self.metrics.is_some(),
        }
    }

    pub fn is_enabled(&self) -> bool {
        self.enabled && !self.sinks.is_empty()
    }

    pub fn trust_forwarded_for(&self) -> bool {
        self.trust_forwarded_for
    }

    pub fn reader(&self) -> Option<&Arc<dyn DatabaseAdapter>> {
        self.reader.as_ref()
    }

    pub fn metrics(&self) -> Option<&Arc<PrometheusAuditSink>> {
        self.metrics.as_ref()
    }

    /// 保持期間による削除が有効な場合の保持日数（`database` 出力先が無効なら `None`）
    pub fn retention_days(&self) -> Option<u32> {
        (self.is_enabled() && self.reader.is_some() && self.retention_days > 0)
            .then_some(self.retention_days)
    }

    /// `now` から保持日数より前の監査ログを削除し、削除件数を返す。
    /// 1 件以上削除した場合は、削除したこと自体を `audit.purge` イベントとして記録する
    pub async fn purge_expired(&self, now: DateTime<Utc>) -> Result<u64> {
        let (Some(days), Some(db)) = (self.retention_days(), self.reader.as_ref()) else {
            return Ok(0);
        };
        let cutoff = format_audit_timestamp(&(now - chrono::Duration::days(i64::from(days))));
        let deleted = db.delete_audit_events_before(&cutoff).await?;
        if deleted > 0 {
            tracing::info!(
                "Purged {} audit events older than {} (retention: {} days)",
                deleted,
                cutoff,
                days
            );
            self.record(
                AuditEvent::new(AuditAction::AuditPurge, AuditOutcome::Success)
                    .with_resource("audit_log", None)
                    .with_details(json!({
                        "deleted": deleted,
                        "cutoff": cutoff,
                        "retention_days": days,
                    })),
            )
            .await;
        }
        Ok(deleted)
    }

    /// 保持期間による削除をバックグラウンドで定期実行する（起動直後に 1 回実行）
    pub fn spawn_retention_task(self: &Arc<Self>) {
        let Some(days) = self.retention_days() else {
            if self.is_enabled() && self.retention_days > 0 {
                tracing::warn!(
                    "audit.retention_days is set but the 'database' audit sink is disabled; nothing to purge"
                );
            }
            return;
        };
        tracing::info!(
            "Audit log retention enabled: {} days (purge every {}s)",
            days,
            self.purge_interval.as_secs()
        );

        let service = Arc::clone(self);
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(service.purge_interval);
            interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
            loop {
                interval.tick().await;
                if let Err(e) = service.purge_expired(Utc::now()).await {
                    tracing::error!("Failed to purge expired audit events: {}", e);
                }
            }
        });
    }

    pub async fn record(&self, event: AuditEvent) {
        if !self.is_enabled() {
            return;
        }
        for sink in &self.sinks {
            if let Err(e) = sink.record(&event).await {
                tracing::error!(
                    "Failed to record audit event {} ({}) to sink '{}': {}",
                    event.id,
                    event.action.as_str(),
                    sink.name(),
                    e
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use aegis_core::AegisError;
    use async_trait::async_trait;
    use std::sync::Mutex;

    struct FailingSink;

    #[async_trait]
    impl AuditSink for FailingSink {
        fn name(&self) -> &str {
            "failing"
        }

        async fn record(&self, _event: &AuditEvent) -> Result<()> {
            Err(AegisError::Database("unavailable".to_string()))
        }
    }

    #[derive(Default)]
    struct CollectingSink(Mutex<Vec<AuditEvent>>);

    #[async_trait]
    impl AuditSink for CollectingSink {
        fn name(&self) -> &str {
            "collecting"
        }

        async fn record(&self, event: &AuditEvent) -> Result<()> {
            self.0.lock().unwrap().push(event.clone());
            Ok(())
        }
    }

    async fn sqlite_service(retention_days: u32) -> AuditService {
        let db = aegis_db::SqliteAdapter::connect("sqlite::memory:", 1)
            .await
            .unwrap();
        db.init().await.unwrap();
        let config = AuditConfig {
            retention_days,
            ..AuditConfig::default()
        };
        AuditService::from_config(&config, Arc::new(db))
    }

    fn event_at(occurred_at: &str) -> AuditEvent {
        let mut event = AuditEvent::new(AuditAction::ContentCreate, AuditOutcome::Success);
        event.occurred_at = occurred_at.to_string();
        event
    }

    #[tokio::test]
    async fn test_purge_deletes_only_expired_events_and_records_purge() {
        let service = sqlite_service(30).await;
        assert_eq!(service.retention_days(), Some(30));
        let db = service.reader().unwrap().clone();

        for ts in [
            "2026-08-01T00:00:00.000000Z",
            "2026-09-21T11:59:59.999999Z",
            "2026-09-21T12:00:00.000000Z",
            "2026-10-20T00:00:00.000000Z",
        ] {
            db.insert_audit_event(&event_at(ts)).await.unwrap();
        }

        let now = DateTime::parse_from_rfc3339("2026-10-21T12:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        assert_eq!(service.purge_expired(now).await.unwrap(), 2);

        let remaining = db
            .query_audit_events(&aegis_core::AuditQuery {
                limit: 10,
                ..Default::default()
            })
            .await
            .unwrap();
        let purge: Vec<_> = remaining
            .iter()
            .filter(|e| e.action == AuditAction::AuditPurge)
            .collect();
        assert_eq!(purge.len(), 1);
        assert_eq!(purge[0].details["deleted"], 2);
        assert_eq!(purge[0].details["cutoff"], "2026-09-21T12:00:00.000000Z");
        let kept: Vec<_> = remaining
            .iter()
            .filter(|e| e.action == AuditAction::ContentCreate)
            .map(|e| e.occurred_at.as_str())
            .collect();
        assert_eq!(
            kept,
            vec!["2026-10-20T00:00:00.000000Z", "2026-09-21T12:00:00.000000Z"]
        );

        // 削除対象が無ければ audit.purge は記録しない
        assert_eq!(service.purge_expired(now).await.unwrap(), 0);
        let purge_count = db
            .query_audit_events(&aegis_core::AuditQuery {
                action: Some(AuditAction::AuditPurge),
                limit: 10,
                ..Default::default()
            })
            .await
            .unwrap()
            .len();
        assert_eq!(purge_count, 1);
    }

    #[tokio::test]
    async fn test_retention_disabled_keeps_everything() {
        let service = sqlite_service(0).await;
        assert_eq!(service.retention_days(), None);
        let db = service.reader().unwrap().clone();
        db.insert_audit_event(&event_at("2000-01-01T00:00:00.000000Z"))
            .await
            .unwrap();
        assert_eq!(service.purge_expired(Utc::now()).await.unwrap(), 0);
    }

    #[tokio::test]
    async fn test_failing_sink_does_not_block_other_sinks() {
        let collecting = Arc::new(CollectingSink::default());
        let service = AuditService::with_sinks(vec![Arc::new(FailingSink), collecting.clone()]);

        let event = AuditEvent::new(AuditAction::SessionCreate, AuditOutcome::Success);
        service.record(event.clone()).await;

        assert_eq!(*collecting.0.lock().unwrap(), vec![event]);
    }
}
