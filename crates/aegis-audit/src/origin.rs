use aegis_core::{AuditAction, AuditActor, AuditEvent, AuditOutcome};

/// 監査イベントの操作主体とクライアント情報（HTTP リクエストから解決する。リクエストに由来しない
/// 処理（定期処理など）では `default()`）
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AuditOrigin {
    pub actor: Option<AuditActor>,
    pub ip: Option<String>,
    pub user_agent: Option<String>,
}

impl AuditOrigin {
    /// 操作主体とクライアント情報を埋めた監査イベントを生成する
    pub fn event(&self, action: AuditAction, outcome: AuditOutcome) -> AuditEvent {
        AuditEvent::new(action, outcome)
            .with_actor(self.actor.clone())
            .with_client(self.ip.clone(), self.user_agent.clone())
    }
}
