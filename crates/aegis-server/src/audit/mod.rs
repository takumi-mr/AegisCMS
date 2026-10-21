pub mod context;
pub mod database;
pub mod log;
pub mod metrics;

pub use context::AuditContext;
pub use database::DatabaseAuditSink;
pub use log::LogAuditSink;
pub use metrics::PrometheusAuditSink;

use aegis_core::{AuditConfig, AuditEvent, AuditSink, AuditSinkKind};
use std::sync::Arc;

use crate::db::DatabaseAdapter;

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
}

impl AuditService {
    pub fn from_config(config: &AuditConfig, db: Arc<dyn DatabaseAdapter>) -> Self {
        let mut service = Self {
            enabled: config.enabled,
            trust_forwarded_for: config.trust_forwarded_for,
            sinks: Vec::new(),
            reader: None,
            metrics: None,
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

    #[cfg(test)]
    pub fn with_sinks(sinks: Vec<Arc<dyn AuditSink>>) -> Self {
        Self {
            enabled: true,
            trust_forwarded_for: false,
            sinks,
            reader: None,
            metrics: None,
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
    use aegis_core::{AegisError, AuditAction, AuditOutcome, Result};
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

    #[tokio::test]
    async fn test_failing_sink_does_not_block_other_sinks() {
        let collecting = Arc::new(CollectingSink::default());
        let service = AuditService::with_sinks(vec![Arc::new(FailingSink), collecting.clone()]);

        let event = AuditEvent::new(AuditAction::SessionCreate, AuditOutcome::Success);
        service.record(event.clone()).await;

        assert_eq!(*collecting.0.lock().unwrap(), vec![event]);
    }
}
