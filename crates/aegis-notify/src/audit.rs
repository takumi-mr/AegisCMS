//! 監査イベントの通知（`[[notifications.audit.rules]]`）。
//!
//! 監査ログの出力先（`AuditSink`）として `AuditService` に加え、ルールに合うイベントを
//! `audit.<action>`（`audit.session.create` など）の通知として送る。通知の送り先（`Notifier`）は
//! 監査ログより後に作るため、`attach` で後から渡す（渡すまでのイベントは通知しない）

use std::collections::VecDeque;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use aegis_core::{
    AuditAction, AuditEvent, AuditSink, NotificationEvent, Notifier, Recipient, Result,
};
use async_trait::async_trait;
use serde_json::json;

use crate::config::AuditRule;

const HOUR: Duration = Duration::from_secs(3600);

pub struct AuditNotificationSink {
    rules: Vec<AuditRule>,
    notifier: OnceLock<Arc<dyn Notifier>>,
    /// ルールごとの直近 1 時間の通知時刻（`max_per_hour` の判定）
    sent: Mutex<Vec<VecDeque<Instant>>>,
}

impl AuditNotificationSink {
    pub fn new(rules: Vec<AuditRule>) -> Self {
        let sent = rules.iter().map(|_| VecDeque::new()).collect();
        Self {
            rules,
            notifier: OnceLock::new(),
            sent: Mutex::new(sent),
        }
    }

    /// 通知の送り先を設定する（1 回だけ）
    pub fn attach(&self, notifier: Arc<dyn Notifier>) {
        if self.notifier.set(notifier).is_err() {
            tracing::warn!("The audit notification sink already has a notifier");
        }
    }

    /// イベントに合う最初のルール
    fn matching_rule(&self, event: &AuditEvent) -> Option<usize> {
        // 通知の配信結果を通知すると、失敗が続いたときに通知が通知を呼び続ける
        if event.action == AuditAction::NotificationSend {
            return None;
        }
        self.rules.iter().position(|rule| matches(rule, event))
    }

    /// `max_per_hour` を超えていなければ数えて `true`
    fn allow(&self, index: usize, now: Instant) -> bool {
        let max = self.rules[index].max_per_hour;
        let mut sent = self.sent.lock().unwrap_or_else(|e| e.into_inner());
        let times = &mut sent[index];
        while times
            .front()
            .is_some_and(|t| now.duration_since(*t) >= HOUR)
        {
            times.pop_front();
        }
        if max > 0 && times.len() >= max as usize {
            return false;
        }
        times.push_back(now);
        true
    }
}

fn matches(rule: &AuditRule, event: &AuditEvent) -> bool {
    if rule.action != event.action {
        return false;
    }
    if !rule.outcomes.is_empty() && !rule.outcomes.contains(&event.outcome) {
        return false;
    }
    if !rule.roles.is_empty() {
        let role = event.actor.as_ref().map(|a| a.role.as_str());
        if !role.is_some_and(|role| rule.roles.iter().any(|r| r == role)) {
            return false;
        }
    }
    rule.details
        .iter()
        .all(|(key, expected)| match event.details.get(key) {
            None | Some(serde_json::Value::Null) => false,
            Some(_) if expected.as_str() == Some("*") => true,
            Some(actual) => actual == expected,
        })
}

#[async_trait]
impl AuditSink for AuditNotificationSink {
    fn name(&self) -> &str {
        "notify"
    }

    async fn record(&self, event: &AuditEvent) -> Result<()> {
        let Some(index) = self.matching_rule(event) else {
            return Ok(());
        };
        let Some(notifier) = self.notifier.get() else {
            return Ok(());
        };
        if !self.allow(index, Instant::now()) {
            tracing::warn!(
                "Audit notification for {} skipped: more than {} in the last hour",
                event.action.as_str(),
                self.rules[index].max_per_hour
            );
            return Ok(());
        }
        let rule = &self.rules[index];
        let recipients = if rule.recipients.is_empty() {
            vec![Recipient::Staff]
        } else {
            rule.recipients
                .iter()
                .map(|address| Recipient::person(address.clone(), ""))
                .collect()
        };
        notifier.notify(NotificationEvent::new(
            format!("audit.{}", event.action.as_str()),
            recipients,
            json!({ "audit": event, "title": rule.title }),
        ));
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use aegis_core::{AuditActor, AuditOutcome};
    use std::collections::HashMap;

    #[derive(Default)]
    struct Recording(Mutex<Vec<NotificationEvent>>);

    impl Notifier for Recording {
        fn notify(&self, event: NotificationEvent) {
            self.0.lock().unwrap().push(event);
        }
    }

    fn rule(action: AuditAction) -> AuditRule {
        AuditRule {
            title: String::new(),
            action,
            outcomes: Vec::new(),
            roles: Vec::new(),
            details: HashMap::new(),
            recipients: Vec::new(),
            max_per_hour: 20,
        }
    }

    fn login(outcome: AuditOutcome, role: Option<&str>, details: serde_json::Value) -> AuditEvent {
        let mut event = aegis_audit::AuditOrigin::default()
            .event(AuditAction::SessionCreate, outcome)
            .with_details(details);
        event.actor = role.map(|role| AuditActor {
            user_id: "root".to_string(),
            role: role.to_string(),
        });
        event
    }

    #[tokio::test]
    async fn notifies_events_that_match_a_rule() {
        let admin_login = AuditRule {
            title: "Admin login".to_string(),
            outcomes: vec![AuditOutcome::Success],
            roles: vec!["admin".to_string()],
            ..rule(AuditAction::SessionCreate)
        };
        let lockout = AuditRule {
            details: HashMap::from([("locked".to_string(), json!("*"))]),
            recipients: vec!["security@example.com".to_string()],
            ..rule(AuditAction::SessionCreate)
        };
        let sink = AuditNotificationSink::new(vec![admin_login, lockout]);
        let notifier = Arc::new(Recording::default());

        // 送り先が無いうちは何もしない
        sink.record(&login(AuditOutcome::Success, Some("admin"), json!({})))
            .await
            .unwrap();
        sink.attach(notifier.clone());

        for event in [
            login(AuditOutcome::Success, Some("admin"), json!({})),
            login(AuditOutcome::Success, Some("editor"), json!({})),
            login(AuditOutcome::Failure, None, json!({ "username": "root" })),
            login(
                AuditOutcome::Failure,
                None,
                json!({ "username": "root", "locked": "user" }),
            ),
            login(AuditOutcome::Denied, None, json!({ "locked": null })),
        ] {
            sink.record(&event).await.unwrap();
        }

        let events = notifier.0.lock().unwrap();
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].kind, "audit.session.create");
        assert_eq!(events[0].recipients, vec![Recipient::Staff]);
        assert_eq!(events[0].data["title"], "Admin login");
        assert_eq!(events[0].data["audit"]["actor"]["role"], "admin");
        assert_eq!(
            events[1].recipients,
            vec![Recipient::person("security@example.com", "")]
        );
        assert_eq!(events[1].data["audit"]["details"]["locked"], "user");
    }

    #[tokio::test]
    async fn limits_notifications_per_hour_and_never_notifies_its_own_deliveries() {
        let sink = AuditNotificationSink::new(vec![AuditRule {
            max_per_hour: 2,
            ..rule(AuditAction::AccessDenied)
        }]);
        let notifier = Arc::new(Recording::default());
        sink.attach(notifier.clone());
        let denied = aegis_audit::AuditOrigin::default()
            .event(AuditAction::AccessDenied, AuditOutcome::Denied);
        for _ in 0..5 {
            sink.record(&denied).await.unwrap();
        }
        assert_eq!(notifier.0.lock().unwrap().len(), 2);

        // 1 時間経てばまた送る
        let later = Instant::now() + HOUR + Duration::from_secs(1);
        assert!(sink.allow(0, later));

        let sink = AuditNotificationSink::new(vec![rule(AuditAction::NotificationSend)]);
        sink.attach(notifier.clone());
        let delivery = aegis_audit::AuditOrigin::default()
            .event(AuditAction::NotificationSend, AuditOutcome::Failure);
        sink.record(&delivery).await.unwrap();
        assert_eq!(notifier.0.lock().unwrap().len(), 2);
    }
}
