//! `[audit]` の設定

use serde::{Deserialize, Serialize};

/// 監査イベントの出力先
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AuditSinkKind {
    /// `_aegis_audit_logs` テーブルへ永続化し、`GET /api/v1/audit-logs` で検索可能にする
    Database,
    /// `aegis::audit` ターゲットの構造化ログ (1 行 1 JSON) として出力する
    Log,
    /// `aegis_audit_events_total` カウンタを `GET /metrics` で公開する
    Prometheus,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AuditConfig {
    pub enabled: bool,
    pub sinks: Vec<AuditSinkKind>,
    /// `X-Forwarded-For` の先頭アドレスをクライアント IP として扱う（監査ログとログインのレート制限に使用。
    /// 信頼できるリバースプロキシ配下でのみ有効化）
    pub trust_forwarded_for: bool,
    /// `database` 出力先に保存した監査ログの保持日数。これより古いログは定期的に削除する（0 で無期限）
    pub retention_days: u32,
    /// 保持期間切れのログを削除する間隔（秒）
    pub purge_interval_secs: u64,
}

impl Default for AuditConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            sinks: vec![AuditSinkKind::Database],
            trust_forwarded_for: false,
            retention_days: 0,
            purge_interval_secs: 3600,
        }
    }
}
