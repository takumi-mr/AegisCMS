use aegis_core::{AuditAction, AuditEvent, AuditOutcome, AuditSink, Result};
use async_trait::async_trait;
use std::collections::BTreeMap;
use std::fmt::Write;
use std::sync::Mutex;

/// Prometheus テキスト形式の Content-Type
pub const PROMETHEUS_CONTENT_TYPE: &str = "text/plain; version=0.0.4; charset=utf-8";

/// 監査イベント数を action / outcome ごとに集計し、Prometheus 形式で公開する出力先
pub struct PrometheusAuditSink {
    counters: Mutex<BTreeMap<(AuditAction, AuditOutcome), u64>>,
}

impl Default for PrometheusAuditSink {
    fn default() -> Self {
        Self::new()
    }
}

impl PrometheusAuditSink {
    pub fn new() -> Self {
        // 全ラベルの組み合わせを 0 で初期化し、イベント発生前から系列が存在するようにする
        let mut counters = BTreeMap::new();
        for action in AuditAction::ALL {
            for outcome in AuditOutcome::ALL {
                counters.insert((action, outcome), 0);
            }
        }
        Self {
            counters: Mutex::new(counters),
        }
    }

    pub fn render(&self) -> String {
        let mut out = String::new();
        out.push_str(
            "# HELP aegis_audit_events_total Total number of audit events recorded since startup.\n",
        );
        out.push_str("# TYPE aegis_audit_events_total counter\n");
        let counters = self.counters.lock().unwrap_or_else(|e| e.into_inner());
        for ((action, outcome), count) in counters.iter() {
            let _ = writeln!(
                out,
                "aegis_audit_events_total{{action=\"{}\",outcome=\"{}\"}} {}",
                action.as_str(),
                outcome.as_str(),
                count
            );
        }
        out
    }
}

#[async_trait]
impl AuditSink for PrometheusAuditSink {
    fn name(&self) -> &str {
        "prometheus"
    }

    async fn record(&self, event: &AuditEvent) -> Result<()> {
        let mut counters = self.counters.lock().unwrap_or_else(|e| e.into_inner());
        *counters.entry((event.action, event.outcome)).or_insert(0) += 1;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_counters_render_in_prometheus_format() {
        let sink = PrometheusAuditSink::new();
        let initial = sink.render();
        assert!(initial.contains("# TYPE aegis_audit_events_total counter"));
        assert!(initial
            .contains("aegis_audit_events_total{action=\"plugin.error\",outcome=\"failure\"} 0"));

        for _ in 0..2 {
            sink.record(&AuditEvent::new(
                AuditAction::ContentCreate,
                AuditOutcome::Success,
            ))
            .await
            .unwrap();
        }
        sink.record(&AuditEvent::new(
            AuditAction::ContentCreate,
            AuditOutcome::Denied,
        ))
        .await
        .unwrap();

        let rendered = sink.render();
        assert!(rendered
            .contains("aegis_audit_events_total{action=\"content.create\",outcome=\"success\"} 2"));
        assert!(rendered
            .contains("aegis_audit_events_total{action=\"content.create\",outcome=\"denied\"} 1"));
        assert_eq!(
            rendered.lines().filter(|l| !l.starts_with('#')).count(),
            AuditAction::ALL.len() * AuditOutcome::ALL.len()
        );
    }
}
