use aegis_core::{AuditEvent, AuditSink, Result};
use async_trait::async_trait;
use std::sync::Arc;

use crate::db::DatabaseAdapter;

/// `_aegis_audit_logs` テーブルへ監査イベントを永続化する出力先
pub struct DatabaseAuditSink {
    db: Arc<dyn DatabaseAdapter>,
}

impl DatabaseAuditSink {
    pub fn new(db: Arc<dyn DatabaseAdapter>) -> Self {
        Self { db }
    }
}

#[async_trait]
impl AuditSink for DatabaseAuditSink {
    fn name(&self) -> &str {
        "database"
    }

    async fn record(&self, event: &AuditEvent) -> Result<()> {
        self.db.insert_audit_event(event).await
    }
}
