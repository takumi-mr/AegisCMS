//! システム運用・動的設定の管理。
//! 設定ファイル（`AegisConfig`）の値をデフォルトとし、Admin Studio から変更した値
//! （DB の `_aegis_settings` テーブル）で動的にオーバーライドする。
//! サーバープロセスの再起動なしに即時反映される。

use std::collections::HashMap;
use std::sync::Arc;

use aegis_core::{AuditAction, AuditActor, AuditOutcome};
use aegis_db::DatabaseAdapter;
use axum::{extract::State, http::StatusCode, Json};
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::audit::AuditContext;
use crate::auth::authorize::RequireRole;
use crate::auth::AdminSession;
use crate::config::AegisConfig;
use crate::state::AppState;

/// `_aegis_settings` のキー名
pub const MAINTENANCE_MODE_SETTING: &str = "system.maintenance_mode";
pub const MAINTENANCE_MESSAGE_SETTING: &str = "system.maintenance_message";
pub const MAX_UPLOAD_MB_SETTING: &str = "system.max_upload_mb";
pub const CACHE_ENABLED_SETTING: &str = "system.cache_enabled";
pub const CACHE_TTL_SECS_SETTING: &str = "system.cache_ttl_secs";
pub const REGISTRATION_ENABLED_SETTING: &str = "system.registration_enabled";
pub const MAX_LOGIN_ATTEMPTS_SETTING: &str = "system.max_login_attempts";
pub const COMMENTS_ENABLED_SETTING: &str = "system.comments_enabled";
pub const FORUM_ENABLED_SETTING: &str = "system.forum_enabled";

/// メンテナンスメッセージの既定値
pub const DEFAULT_MAINTENANCE_MESSAGE: &str =
    "Site is currently under maintenance. Please check back soon.";
/// メンテナンスメッセージの最大文字数
pub const MAX_MAINTENANCE_MESSAGE_CHARS: usize = 1000;
/// 最大アップロードサイズの上限（MB）
pub const MAX_UPLOAD_MB_LIMIT: u64 = 1024;
/// キャッシュ TTL の上限（1年 = 31,536,000秒）
pub const MAX_CACHE_TTL_SECS_LIMIT: u64 = 31_536_000;
/// ログイン試行制限の上限回数
pub const MAX_LOGIN_ATTEMPTS_LIMIT: u64 = 1000;

/// Admin Studio からオーバーライドされた値（`None` は設定ファイルの値を使用）
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SystemOverrides {
    pub maintenance_mode: Option<bool>,
    pub maintenance_message: Option<String>,
    pub max_upload_mb: Option<u64>,
    pub cache_enabled: Option<bool>,
    pub cache_ttl_secs: Option<u64>,
    pub registration_enabled: Option<bool>,
    pub max_login_attempts: Option<u64>,
    pub comments_enabled: Option<bool>,
    pub forum_enabled: Option<bool>,
}

impl SystemOverrides {
    pub fn from_settings(settings: &HashMap<String, String>) -> Self {
        let maintenance_mode = settings
            .get(MAINTENANCE_MODE_SETTING)
            .and_then(|v| parse_bool(v));
        let maintenance_message = settings.get(MAINTENANCE_MESSAGE_SETTING).cloned();
        let max_upload_mb = settings
            .get(MAX_UPLOAD_MB_SETTING)
            .and_then(|v| v.parse::<u64>().ok());
        let cache_enabled = settings
            .get(CACHE_ENABLED_SETTING)
            .and_then(|v| parse_bool(v));
        let cache_ttl_secs = settings
            .get(CACHE_TTL_SECS_SETTING)
            .and_then(|v| v.parse::<u64>().ok());
        let registration_enabled = settings
            .get(REGISTRATION_ENABLED_SETTING)
            .and_then(|v| parse_bool(v));
        let max_login_attempts = settings
            .get(MAX_LOGIN_ATTEMPTS_SETTING)
            .and_then(|v| v.parse::<u64>().ok());
        let comments_enabled = settings
            .get(COMMENTS_ENABLED_SETTING)
            .and_then(|v| parse_bool(v));
        let forum_enabled = settings
            .get(FORUM_ENABLED_SETTING)
            .and_then(|v| parse_bool(v));

        Self {
            maintenance_mode,
            maintenance_message,
            max_upload_mb,
            cache_enabled,
            cache_ttl_secs,
            registration_enabled,
            max_login_attempts,
            comments_enabled,
            forum_enabled,
        }
    }

    /// DB から読み込む。読めなければ警告を出して空のオーバーライドを返す
    pub async fn load(db: &dyn DatabaseAdapter) -> Self {
        match db.list_settings().await {
            Ok(settings) => Self::from_settings(&settings),
            Err(e) => {
                tracing::warn!("Could not load system settings from database: {}", e);
                Self::default()
            }
        }
    }
}

/// 実際にシステム上で有効な設定値
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SystemSettings {
    pub maintenance_mode: bool,
    pub maintenance_message: String,
    pub max_upload_mb: u64,
    pub cache_enabled: bool,
    pub cache_ttl_secs: u64,
    pub registration_enabled: bool,
    pub max_login_attempts: u64,
    pub comments_enabled: bool,
    pub forum_enabled: bool,
}

impl SystemSettings {
    /// 設定ファイル（`AegisConfig`）の値から既定値を導出
    pub fn defaults(config: &AegisConfig) -> Self {
        Self {
            maintenance_mode: false,
            maintenance_message: DEFAULT_MAINTENANCE_MESSAGE.to_string(),
            max_upload_mb: config.storage.max_upload_mb,
            cache_enabled: config.cdn.enabled,
            cache_ttl_secs: config.cache.default_ttl_secs,
            registration_enabled: config.auth.registration.enabled,
            max_login_attempts: config.auth.login_rate_limit.max_failures_per_ip,
            comments_enabled: config.comments.enabled,
            forum_enabled: config.forum.enabled,
        }
    }

    /// 設定ファイルの値と DB のオーバーライドを統合して有効な設定を解決
    pub fn resolve(config: &AegisConfig, overrides: &SystemOverrides) -> Self {
        let defaults = Self::defaults(config);
        Self {
            maintenance_mode: overrides
                .maintenance_mode
                .unwrap_or(defaults.maintenance_mode),
            maintenance_message: overrides
                .maintenance_message
                .clone()
                .filter(|s| !s.trim().is_empty())
                .unwrap_or(defaults.maintenance_message),
            max_upload_mb: overrides.max_upload_mb.unwrap_or(defaults.max_upload_mb),
            cache_enabled: overrides.cache_enabled.unwrap_or(defaults.cache_enabled),
            cache_ttl_secs: overrides.cache_ttl_secs.unwrap_or(defaults.cache_ttl_secs),
            registration_enabled: overrides
                .registration_enabled
                .unwrap_or(defaults.registration_enabled),
            max_login_attempts: overrides
                .max_login_attempts
                .unwrap_or(defaults.max_login_attempts),
            comments_enabled: overrides
                .comments_enabled
                .unwrap_or(defaults.comments_enabled),
            forum_enabled: overrides.forum_enabled.unwrap_or(defaults.forum_enabled),
        }
    }
}

/// `GET` / `PUT /api/v1/system/settings` のレスポンス
#[derive(Debug, Serialize)]
pub struct SystemSettingsResponse {
    /// 現在有効な設定値
    pub settings: SystemSettings,
    /// DB に保存されたオーバーライド（`null` は設定ファイル値を使用中）
    pub overrides: SystemOverrides,
    /// 設定ファイルに基づく既定値
    pub defaults: SystemSettings,
}

/// `PUT /api/v1/system/settings` のリクエスト本文。
/// 各フィールドで `null` を渡すと設定ファイルの既定値へ戻す。
#[derive(Debug, Deserialize)]
pub struct UpdateSystemSettings {
    #[serde(default)]
    pub maintenance_mode: Option<bool>,
    #[serde(default)]
    pub maintenance_message: Option<String>,
    #[serde(default)]
    pub max_upload_mb: Option<u64>,
    #[serde(default)]
    pub cache_enabled: Option<bool>,
    #[serde(default)]
    pub cache_ttl_secs: Option<u64>,
    #[serde(default)]
    pub registration_enabled: Option<bool>,
    #[serde(default)]
    pub max_login_attempts: Option<u64>,
    #[serde(default)]
    pub comments_enabled: Option<bool>,
    #[serde(default)]
    pub forum_enabled: Option<bool>,
}

fn parse_bool(s: &str) -> Option<bool> {
    match s.trim().to_ascii_lowercase().as_str() {
        "true" | "1" | "yes" | "on" => Some(true),
        "false" | "0" | "no" | "off" => Some(false),
        _ => None,
    }
}

pub fn validate_system_overrides(overrides: &SystemOverrides) -> Result<(), &'static str> {
    if let Some(msg) = &overrides.maintenance_message {
        if msg.chars().count() > MAX_MAINTENANCE_MESSAGE_CHARS {
            return Err("Maintenance message is too long (maximum 1000 characters)");
        }
    }
    if let Some(upload_mb) = overrides.max_upload_mb {
        if upload_mb == 0 || upload_mb > MAX_UPLOAD_MB_LIMIT {
            return Err("Upload limit must be between 1 MB and 1024 MB");
        }
    }
    if let Some(ttl) = overrides.cache_ttl_secs {
        if ttl > MAX_CACHE_TTL_SECS_LIMIT {
            return Err("Cache TTL is too long (maximum 31536000 seconds)");
        }
    }
    if let Some(attempts) = overrides.max_login_attempts {
        if attempts > MAX_LOGIN_ATTEMPTS_LIMIT {
            return Err("Max login attempts is too high (maximum 1000)");
        }
    }
    Ok(())
}

type ApiError = (StatusCode, Json<serde_json::Value>);

fn bad_request(message: &'static str) -> ApiError {
    (StatusCode::BAD_REQUEST, Json(json!({ "error": message })))
}

/// `GET /api/v1/system/settings` — システム運用設定の取得（Admin 権限必須）
pub async fn get_system_settings(
    State(state): State<Arc<AppState>>,
    RequireRole(_session): AdminSession,
) -> Json<SystemSettingsResponse> {
    let overrides = state
        .system
        .read()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .clone();
    let settings = SystemSettings::resolve(&state.config, &overrides);
    let defaults = SystemSettings::defaults(&state.config);

    Json(SystemSettingsResponse {
        settings,
        overrides,
        defaults,
    })
}

/// `PUT /api/v1/system/settings` — システム運用設定の更新（Admin 権限必須）
pub async fn update_system_settings(
    State(state): State<Arc<AppState>>,
    RequireRole(session): AdminSession,
    audit_ctx: AuditContext,
    Json(payload): Json<UpdateSystemSettings>,
) -> Result<Json<SystemSettingsResponse>, ApiError> {
    let next = SystemOverrides {
        maintenance_mode: payload.maintenance_mode,
        maintenance_message: payload
            .maintenance_message
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty()),
        max_upload_mb: payload.max_upload_mb,
        cache_enabled: payload.cache_enabled,
        cache_ttl_secs: payload.cache_ttl_secs,
        registration_enabled: payload.registration_enabled,
        max_login_attempts: payload.max_login_attempts,
        comments_enabled: payload.comments_enabled,
        forum_enabled: payload.forum_enabled,
    };

    validate_system_overrides(&next).map_err(bad_request)?;

    let previous = state
        .system
        .read()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .clone();
    let actor = Some(AuditActor {
        user_id: session.user_id,
        role: session.role,
    });

    let saved = async {
        // 各キーを DB に保存または削除
        let pairs: [(&str, Option<String>); 9] = [
            (
                MAINTENANCE_MODE_SETTING,
                next.maintenance_mode.map(|b| b.to_string()),
            ),
            (
                MAINTENANCE_MESSAGE_SETTING,
                next.maintenance_message.clone(),
            ),
            (
                MAX_UPLOAD_MB_SETTING,
                next.max_upload_mb.map(|v| v.to_string()),
            ),
            (
                CACHE_ENABLED_SETTING,
                next.cache_enabled.map(|b| b.to_string()),
            ),
            (
                CACHE_TTL_SECS_SETTING,
                next.cache_ttl_secs.map(|v| v.to_string()),
            ),
            (
                REGISTRATION_ENABLED_SETTING,
                next.registration_enabled.map(|b| b.to_string()),
            ),
            (
                MAX_LOGIN_ATTEMPTS_SETTING,
                next.max_login_attempts.map(|v| v.to_string()),
            ),
            (
                COMMENTS_ENABLED_SETTING,
                next.comments_enabled.map(|b| b.to_string()),
            ),
            (
                FORUM_ENABLED_SETTING,
                next.forum_enabled.map(|b| b.to_string()),
            ),
        ];

        for (key, val) in pairs {
            match val {
                Some(v) => state.db.set_setting(key, &v).await?,
                None => state.db.delete_setting(key).await?,
            }
        }
        Ok::<(), aegis_core::AegisError>(())
    }
    .await;

    match saved {
        Ok(()) => {
            *state
                .system
                .write()
                .unwrap_or_else(std::sync::PoisonError::into_inner) = next.clone();

            state
                .audit
                .record(
                    audit_ctx
                        .event(AuditAction::SystemUpdate, AuditOutcome::Success)
                        .with_actor(actor)
                        .with_details(json!({
                            "previous": previous,
                            "updated": next,
                        })),
                )
                .await;

            let settings = SystemSettings::resolve(&state.config, &next);
            let defaults = SystemSettings::defaults(&state.config);

            Ok(Json(SystemSettingsResponse {
                settings,
                overrides: next,
                defaults,
            }))
        }
        Err(e) => {
            state
                .audit
                .record(
                    audit_ctx
                        .event(AuditAction::SystemUpdate, AuditOutcome::Failure)
                        .with_actor(actor)
                        .with_details(json!({ "error": e.to_string() })),
                )
                .await;
            Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": format!("Failed to save system settings: {}", e) })),
            ))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_system_settings_resolve_defaults() {
        let config = AegisConfig::default();
        let settings = SystemSettings::resolve(&config, &SystemOverrides::default());
        let defaults = SystemSettings::defaults(&config);

        assert_eq!(settings, defaults);
        assert!(!settings.maintenance_mode);
        assert_eq!(settings.max_upload_mb, 10);
    }

    #[test]
    fn test_system_settings_overrides() {
        let config = AegisConfig::default();
        let overrides = SystemOverrides {
            maintenance_mode: Some(true),
            maintenance_message: Some("Emergency maintenance".to_string()),
            max_upload_mb: Some(50),
            cache_enabled: Some(false),
            cache_ttl_secs: Some(120),
            registration_enabled: Some(true),
            max_login_attempts: Some(3),
            comments_enabled: Some(false),
            forum_enabled: Some(false),
        };

        let resolved = SystemSettings::resolve(&config, &overrides);
        assert!(resolved.maintenance_mode);
        assert_eq!(resolved.maintenance_message, "Emergency maintenance");
        assert_eq!(resolved.max_upload_mb, 50);
        assert!(!resolved.cache_enabled);
        assert_eq!(resolved.cache_ttl_secs, 120);
        assert!(resolved.registration_enabled);
        assert_eq!(resolved.max_login_attempts, 3);
        assert!(!resolved.comments_enabled);
        assert!(!resolved.forum_enabled);
    }

    #[test]
    fn test_validation() {
        let mut overrides = SystemOverrides::default();
        assert!(validate_system_overrides(&overrides).is_ok());

        overrides.max_upload_mb = Some(0);
        assert!(validate_system_overrides(&overrides).is_err());

        overrides.max_upload_mb = Some(2048);
        assert!(validate_system_overrides(&overrides).is_err());

        overrides.max_upload_mb = Some(100);
        assert!(validate_system_overrides(&overrides).is_ok());

        overrides.cache_ttl_secs = Some(MAX_CACHE_TTL_SECS_LIMIT + 1);
        assert!(validate_system_overrides(&overrides).is_err());
    }

    use crate::test_support::test_state_with_config;
    use aegis_auth::SessionData;
    use aegis_runtime::PluginSupervisor;

    fn admin_session() -> AdminSession {
        RequireRole(SessionData {
            id: "ses_admin".to_string(),
            user_id: "admin1".to_string(),
            role: "admin".to_string(),
            created_at: String::new(),
            expires_at: String::new(),
            metadata: Default::default(),
        })
    }

    #[tokio::test]
    async fn test_system_settings_api_get_and_put() {
        let config = AegisConfig::default();
        let state = test_state_with_config(config, PluginSupervisor::new()).await;

        let Json(initial) = get_system_settings(State(state.clone()), admin_session()).await;
        assert_eq!(initial.settings.max_upload_mb, 10);
        assert!(!initial.settings.maintenance_mode);
        assert_eq!(initial.overrides.max_upload_mb, None);

        let update_payload = UpdateSystemSettings {
            maintenance_mode: Some(true),
            maintenance_message: Some("Custom maintenance msg".to_string()),
            max_upload_mb: Some(25),
            cache_enabled: Some(false),
            cache_ttl_secs: Some(300),
            registration_enabled: Some(true),
            max_login_attempts: Some(5),
            comments_enabled: Some(false),
            forum_enabled: Some(false),
        };

        let Json(updated) = update_system_settings(
            State(state.clone()),
            admin_session(),
            AuditContext::default(),
            Json(update_payload),
        )
        .await
        .unwrap();

        assert!(updated.settings.maintenance_mode);
        assert_eq!(
            updated.settings.maintenance_message,
            "Custom maintenance msg"
        );
        assert_eq!(updated.settings.max_upload_mb, 25);
        assert!(!updated.settings.cache_enabled);
        assert_eq!(updated.settings.cache_ttl_secs, 300);
        assert!(updated.settings.registration_enabled);
        assert_eq!(updated.settings.max_login_attempts, 5);
        assert!(!updated.settings.comments_enabled);
        assert!(!updated.settings.forum_enabled);

        // AppState から取得する値にも即時反映されている
        let current = state.system();
        assert!(current.maintenance_mode);
        assert_eq!(current.max_upload_mb, 25);

        // 再度 GET して確認
        let Json(fetched) = get_system_settings(State(state.clone()), admin_session()).await;
        assert_eq!(fetched.settings.max_upload_mb, 25);
        assert_eq!(fetched.overrides.max_upload_mb, Some(25));

        // null でリセット
        let reset_payload = UpdateSystemSettings {
            maintenance_mode: None,
            maintenance_message: None,
            max_upload_mb: None,
            cache_enabled: None,
            cache_ttl_secs: None,
            registration_enabled: None,
            max_login_attempts: None,
            comments_enabled: None,
            forum_enabled: None,
        };

        let Json(reset) = update_system_settings(
            State(state.clone()),
            admin_session(),
            AuditContext::default(),
            Json(reset_payload),
        )
        .await
        .unwrap();

        assert!(!reset.settings.maintenance_mode);
        assert_eq!(reset.settings.max_upload_mb, 10);
        assert_eq!(reset.overrides.max_upload_mb, None);
    }
}
