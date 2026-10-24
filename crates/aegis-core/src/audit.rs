use crate::error::Result;
use async_trait::async_trait;
use chrono::{DateTime, SecondsFormat, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// 監査対象の操作種別
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum AuditAction {
    #[serde(rename = "content.create")]
    ContentCreate,
    #[serde(rename = "content.update")]
    ContentUpdate,
    #[serde(rename = "content.delete")]
    ContentDelete,
    /// Scheduled content published by the server when its `published_at` arrived
    #[serde(rename = "content.publish")]
    ContentPublish,
    /// Scheduled content unpublished (archived) by the server when its `unpublish_at` arrived
    #[serde(rename = "content.unpublish")]
    ContentUnpublish,
    #[serde(rename = "session.create")]
    SessionCreate,
    #[serde(rename = "session.delete")]
    SessionDelete,
    #[serde(rename = "storage.upload")]
    StorageUpload,
    #[serde(rename = "storage.delete")]
    StorageDelete,
    #[serde(rename = "plugin.error")]
    PluginError,
    /// Plugin settings updated via admin/API
    #[serde(rename = "plugin.settings_update")]
    PluginSettingsUpdate,
    /// Plugin enabled via admin/API (without a restart)
    #[serde(rename = "plugin.enable")]
    PluginEnable,
    /// Plugin disabled via admin/API (without a restart)
    #[serde(rename = "plugin.disable")]
    PluginDisable,
    /// Hook order of the enabled plugins changed via admin/API
    #[serde(rename = "plugin.reorder")]
    PluginReorder,
    /// Enabled plugins saved from admin/API discarded in favour of `[plugins].enabled`
    #[serde(rename = "plugin.reset")]
    PluginReset,
    #[serde(rename = "audit.purge")]
    AuditPurge,
    /// API request rejected for a missing / invalid session (401) or an insufficient role (403)
    #[serde(rename = "access.denied")]
    AccessDenied,
    /// Active theme switched via admin/API
    #[serde(rename = "theme.switch")]
    ThemeSwitch,
    /// Site-wide settings (site title, favicon) changed via admin/API
    #[serde(rename = "site.update")]
    SiteUpdate,
    /// User account created via the user management API
    #[serde(rename = "user.create")]
    UserCreate,
    /// User account or profile changed (attributes, password, avatar)
    #[serde(rename = "user.update")]
    UserUpdate,
    /// User account deleted via the user management API
    #[serde(rename = "user.delete")]
    UserDelete,
    /// Self-registration from Admin Studio: an application (`stage: requested`) or its email
    /// confirmation that creates the account (`stage: confirmed`)
    #[serde(rename = "user.register")]
    UserRegister,
    /// EC order placed (checkout)
    #[serde(rename = "order.create")]
    OrderCreate,
    /// EC order status changed (payment, shipping, cancellation, expiry) or a payment problem
    #[serde(rename = "order.update")]
    OrderUpdate,
    /// Notification delivered (or given up after retries) through a channel such as email
    #[serde(rename = "notification.send")]
    NotificationSend,
    /// Password reset requested or completed
    #[serde(rename = "user.password_reset")]
    UserPasswordReset,
    /// User invitation created or accepted
    #[serde(rename = "user.invite")]
    UserInvite,
}

impl AuditAction {
    pub const ALL: [AuditAction; 28] = [
        Self::ContentCreate,
        Self::ContentUpdate,
        Self::ContentDelete,
        Self::ContentPublish,
        Self::ContentUnpublish,
        Self::SessionCreate,
        Self::SessionDelete,
        Self::StorageUpload,
        Self::StorageDelete,
        Self::PluginError,
        Self::PluginSettingsUpdate,
        Self::PluginEnable,
        Self::PluginDisable,
        Self::PluginReorder,
        Self::PluginReset,
        Self::AuditPurge,
        Self::AccessDenied,
        Self::ThemeSwitch,
        Self::SiteUpdate,
        Self::UserCreate,
        Self::UserUpdate,
        Self::UserDelete,
        Self::UserRegister,
        Self::OrderCreate,
        Self::OrderUpdate,
        Self::NotificationSend,
        Self::UserPasswordReset,
        Self::UserInvite,
    ];

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::ContentCreate => "content.create",
            Self::ContentUpdate => "content.update",
            Self::ContentDelete => "content.delete",
            Self::ContentPublish => "content.publish",
            Self::ContentUnpublish => "content.unpublish",
            Self::SessionCreate => "session.create",
            Self::SessionDelete => "session.delete",
            Self::StorageUpload => "storage.upload",
            Self::StorageDelete => "storage.delete",
            Self::PluginError => "plugin.error",
            Self::PluginSettingsUpdate => "plugin.settings_update",
            Self::PluginEnable => "plugin.enable",
            Self::PluginDisable => "plugin.disable",
            Self::PluginReorder => "plugin.reorder",
            Self::PluginReset => "plugin.reset",
            Self::AuditPurge => "audit.purge",
            Self::AccessDenied => "access.denied",
            Self::ThemeSwitch => "theme.switch",
            Self::SiteUpdate => "site.update",
            Self::UserCreate => "user.create",
            Self::UserUpdate => "user.update",
            Self::UserDelete => "user.delete",
            Self::UserRegister => "user.register",
            Self::OrderCreate => "order.create",
            Self::OrderUpdate => "order.update",
            Self::NotificationSend => "notification.send",
            Self::UserPasswordReset => "user.password_reset",
            Self::UserInvite => "user.invite",
        }
    }
}

impl std::str::FromStr for AuditAction {
    type Err = String;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        Self::ALL
            .into_iter()
            .find(|a| a.as_str() == s)
            .ok_or_else(|| format!("Unknown audit action: {}", s))
    }
}

/// 操作の結果
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AuditOutcome {
    /// 操作が成功した
    Success,
    /// サーバー側のエラーで操作が失敗した
    Failure,
    /// ポリシー（プラグインフック、認証・ロールの検証など）により操作が拒否された
    Denied,
}

impl AuditOutcome {
    pub const ALL: [AuditOutcome; 3] = [Self::Success, Self::Failure, Self::Denied];

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Success => "success",
            Self::Failure => "failure",
            Self::Denied => "denied",
        }
    }
}

impl std::str::FromStr for AuditOutcome {
    type Err = String;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        Self::ALL
            .into_iter()
            .find(|o| o.as_str() == s)
            .ok_or_else(|| format!("Unknown audit outcome: {}", s))
    }
}

/// 操作を行った主体（セッションから解決したユーザー）
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuditActor {
    pub user_id: String,
    pub role: String,
}

/// 監査イベント 1 件
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AuditEvent {
    pub id: String,
    /// UTC の RFC 3339 (マイクロ秒精度・`Z` 終端) 文字列。辞書順と時系列順が一致する
    pub occurred_at: String,
    pub action: AuditAction,
    pub outcome: AuditOutcome,
    #[serde(default)]
    pub actor: Option<AuditActor>,
    #[serde(default)]
    pub resource_type: Option<String>,
    #[serde(default)]
    pub resource_id: Option<String>,
    #[serde(default)]
    pub ip: Option<String>,
    #[serde(default)]
    pub user_agent: Option<String>,
    #[serde(default)]
    pub details: serde_json::Value,
}

impl AuditEvent {
    pub fn new(action: AuditAction, outcome: AuditOutcome) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            occurred_at: format_audit_timestamp(&Utc::now()),
            action,
            outcome,
            actor: None,
            resource_type: None,
            resource_id: None,
            ip: None,
            user_agent: None,
            details: serde_json::Value::Object(Default::default()),
        }
    }

    pub fn with_actor(mut self, actor: Option<AuditActor>) -> Self {
        self.actor = actor;
        self
    }

    pub fn with_resource(
        mut self,
        resource_type: impl Into<String>,
        resource_id: Option<String>,
    ) -> Self {
        self.resource_type = Some(resource_type.into());
        self.resource_id = resource_id;
        self
    }

    pub fn with_client(mut self, ip: Option<String>, user_agent: Option<String>) -> Self {
        self.ip = ip;
        self.user_agent = user_agent;
        self
    }

    pub fn with_details(mut self, details: serde_json::Value) -> Self {
        self.details = details;
        self
    }
}

/// 監査ログ用のタイムスタンプ表現に正規化する
pub fn format_audit_timestamp(ts: &DateTime<Utc>) -> String {
    ts.to_rfc3339_opts(SecondsFormat::Micros, true)
}

/// RFC 3339 文字列をパースし、監査ログ用のタイムスタンプ表現に正規化する
pub fn normalize_audit_timestamp(value: &str) -> std::result::Result<String, String> {
    DateTime::parse_from_rfc3339(value)
        .map(|dt| format_audit_timestamp(&dt.with_timezone(&Utc)))
        .map_err(|e| format!("Invalid RFC 3339 timestamp '{}': {}", value, e))
}

/// 監査ログの検索条件（すべて AND 条件）
#[derive(Debug, Clone, Default, PartialEq)]
pub struct AuditQuery {
    pub action: Option<AuditAction>,
    pub outcome: Option<AuditOutcome>,
    pub actor_id: Option<String>,
    pub resource_type: Option<String>,
    pub resource_id: Option<String>,
    /// この時刻以降（含む）。`normalize_audit_timestamp` 済みであること
    pub since: Option<String>,
    /// この時刻より前（含まない）。`normalize_audit_timestamp` 済みであること
    pub until: Option<String>,
    pub limit: u32,
    pub offset: u32,
}

/// 監査イベントの出力先を抽象化するトレイト
/// DB 永続化、構造化ログ出力、メトリクス集計などを同一インターフェースで扱います
#[async_trait]
pub trait AuditSink: Send + Sync {
    /// 出力先の識別子 (例: "database", "log", "prometheus")
    fn name(&self) -> &str;

    /// 監査イベントを記録する
    async fn record(&self, event: &AuditEvent) -> Result<()>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_action_and_outcome_serialize_as_dotted_names() {
        let event = AuditEvent::new(AuditAction::StorageUpload, AuditOutcome::Denied);
        let json = serde_json::to_value(&event).unwrap();
        assert_eq!(json["action"], "storage.upload");
        assert_eq!(json["outcome"], "denied");

        for action in AuditAction::ALL {
            assert_eq!(action.as_str().parse::<AuditAction>().unwrap(), action);
            assert_eq!(
                serde_json::to_value(action).unwrap(),
                serde_json::json!(action.as_str())
            );
        }
        assert!("content.archive".parse::<AuditAction>().is_err());
    }

    #[test]
    fn test_timestamps_are_normalized_and_sortable() {
        let a = normalize_audit_timestamp("2026-10-21T18:07:11Z").unwrap();
        let b = normalize_audit_timestamp("2026-10-22T03:07:11.5+09:00").unwrap();
        assert_eq!(a, "2026-10-21T18:07:11.000000Z");
        assert_eq!(b, "2026-10-21T18:07:11.500000Z");
        assert!(a < b);
        assert!(normalize_audit_timestamp("yesterday").is_err());
    }
}
