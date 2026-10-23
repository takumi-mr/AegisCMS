use aegis_core::{AuditEvent, AuditSink, Result};
use async_trait::async_trait;

/// tracing のターゲット名。`RUST_LOG=aegis::audit=info` などで出力を制御できる
pub const AUDIT_LOG_TARGET: &str = "aegis::audit";

/// 監査イベントを 1 行 1 JSON の構造化ログとして出力する出力先
/// 標準出力を CloudWatch Logs / Loki などへ転送する構成（awslogs ドライバ、Fluent Bit 等）で利用する
pub struct LogAuditSink;

#[async_trait]
impl AuditSink for LogAuditSink {
    fn name(&self) -> &str {
        "log"
    }

    async fn record(&self, event: &AuditEvent) -> Result<()> {
        let json = serde_json::to_string(event)?;
        tracing::info!(target: AUDIT_LOG_TARGET, "{}", json);
        Ok(())
    }
}
