use axum::{extract::State, http::StatusCode, Json};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::sync::Arc;

use crate::audit::AuditContext;
use crate::auth::authorize::RequireRole;
use crate::auth::AdminSession;
use crate::site::{self, SiteInfo, SiteOverrides, FAVICON_SETTING, TITLE_SETTING};
use crate::state::AppState;
use aegis_core::{AuditAction, AuditActor, AuditOutcome};

type ApiError = (StatusCode, Json<serde_json::Value>);

/// `GET` / `PUT /api/v1/site` の応答
#[derive(Debug, Serialize)]
pub struct SiteSettingsResponse {
    /// Admin Studio で設定した値（`null` は設定ファイルの値を使う）
    pub title: Option<String>,
    pub favicon: Option<String>,
    /// 設定ファイル（`[theme].site_title` / `favicon`）の値
    pub defaults: SiteInfo,
}

/// `PUT /api/v1/site` の本文。`null` または空文字は設定ファイルの値に戻す
#[derive(Debug, Deserialize)]
pub struct UpdateSiteSettings {
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub favicon: Option<String>,
}

fn response(state: &AppState, overrides: SiteOverrides) -> SiteSettingsResponse {
    SiteSettingsResponse {
        title: overrides.title,
        favicon: overrides.favicon,
        defaults: SiteInfo::defaults(&state.config.theme),
    }
}

fn bad_request(message: &str) -> ApiError {
    (StatusCode::BAD_REQUEST, Json(json!({ "error": message })))
}

/// 空白だけの値は「設定しない」として扱う
fn normalize(value: Option<String>) -> Option<String> {
    value
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
}

/// `GET /api/v1/site` — 公開サイトの名前と favicon の設定（admin 権限必須）
pub async fn get_site_settings(
    State(state): State<Arc<AppState>>,
    RequireRole(_session): AdminSession,
) -> Json<SiteSettingsResponse> {
    let overrides = state
        .site
        .read()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .clone();
    Json(response(&state, overrides))
}

/// `PUT /api/v1/site` — 公開サイトの名前と favicon を変更する（admin 権限必須）。
/// DB に保存し、再起動せずに公開サイトへ反映する
pub async fn update_site_settings(
    State(state): State<Arc<AppState>>,
    RequireRole(session): AdminSession,
    audit_ctx: AuditContext,
    Json(payload): Json<UpdateSiteSettings>,
) -> Result<Json<SiteSettingsResponse>, ApiError> {
    let next = SiteOverrides {
        title: normalize(payload.title),
        favicon: normalize(payload.favicon),
    };
    if let Some(title) = &next.title {
        site::validate_title(title).map_err(bad_request)?;
    }
    if let Some(favicon) = &next.favicon {
        site::validate_favicon(favicon).map_err(bad_request)?;
    }

    let previous = state
        .site
        .read()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .clone();
    let actor = Some(AuditActor {
        user_id: session.user_id,
        role: session.role,
    });

    let saved = async {
        for (name, value) in [
            (TITLE_SETTING, &next.title),
            (FAVICON_SETTING, &next.favicon),
        ] {
            match value {
                Some(value) => state.db.set_setting(name, value).await?,
                None => state.db.delete_setting(name).await?,
            }
        }
        Ok::<_, aegis_core::AegisError>(())
    }
    .await;
    if let Err(e) = saved {
        tracing::error!("Failed to save the site settings: {}", e);
        state
            .audit
            .record(
                audit_ctx
                    .event(AuditAction::SiteUpdate, AuditOutcome::Failure)
                    .with_actor(actor)
                    .with_details(json!({ "error": e.to_string() })),
            )
            .await;
        return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": "Failed to save the site settings" })),
        ));
    }

    *state
        .site
        .write()
        .unwrap_or_else(std::sync::PoisonError::into_inner) = next.clone();

    state
        .audit
        .record(
            audit_ctx
                .event(AuditAction::SiteUpdate, AuditOutcome::Success)
                .with_actor(actor)
                .with_details(json!({
                    "title": { "from": previous.title, "to": next.title },
                    "favicon": { "from": previous.favicon, "to": next.favicon },
                })),
        )
        .await;

    Ok(Json(response(&state, next)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::AegisConfig;
    use crate::render::site_page_context;
    use crate::test_support::test_state_with_config;
    use aegis_auth::SessionData;
    use aegis_runtime::PluginSupervisor;

    fn admin() -> AdminSession {
        RequireRole(SessionData {
            id: "ses_admin".to_string(),
            user_id: "admin1".to_string(),
            role: "admin".to_string(),
            created_at: String::new(),
            expires_at: String::new(),
            metadata: Default::default(),
        })
    }

    async fn update(
        state: &Arc<AppState>,
        title: Option<&str>,
        favicon: Option<&str>,
    ) -> Result<Json<SiteSettingsResponse>, ApiError> {
        update_site_settings(
            State(state.clone()),
            admin(),
            AuditContext::default(),
            Json(UpdateSiteSettings {
                title: title.map(str::to_string),
                favicon: favicon.map(str::to_string),
            }),
        )
        .await
    }

    #[test]
    fn test_normalize_treats_blank_as_unset() {
        assert_eq!(normalize(None), None);
        assert_eq!(normalize(Some("   ".to_string())), None);
        assert_eq!(
            normalize(Some(" My Site ".to_string())).as_deref(),
            Some("My Site")
        );
    }

    #[tokio::test]
    async fn test_site_settings_override_the_config_and_reset() {
        let mut config = AegisConfig::default();
        config.theme.site_title = "Config Title".to_string();
        let state = test_state_with_config(config, PluginSupervisor::new()).await;

        let Json(initial) = get_site_settings(State(state.clone()), admin()).await;
        assert_eq!(initial.title, None);
        assert_eq!(initial.defaults.title, "Config Title");
        let ctx = site_page_context(&state);
        assert_eq!(ctx.site_title, "Config Title");
        assert!(!ctx.extra_head.contains("<link rel=\"icon\""));

        let Json(updated) = update(&state, Some("  New Title "), Some("/uploads/icon.png"))
            .await
            .unwrap();
        assert_eq!(updated.title.as_deref(), Some("New Title"));
        assert_eq!(updated.favicon.as_deref(), Some("/uploads/icon.png"));

        // 公開ページの値に反映され、DB に保存される
        let ctx = site_page_context(&state);
        assert_eq!(ctx.site_title, "New Title");
        assert!(ctx
            .extra_head
            .contains(r#"<link rel="icon" href="/uploads/icon.png">"#));
        let saved = state.db.list_settings().await.unwrap();
        assert_eq!(saved[TITLE_SETTING], "New Title");
        // 再起動後と同じく DB から読み直しても同じ値
        assert_eq!(
            SiteOverrides::load(state.db.as_ref()).await,
            SiteOverrides {
                title: Some("New Title".to_string()),
                favicon: Some("/uploads/icon.png".to_string()),
            }
        );

        // 空にすると設定ファイルの値に戻る
        let Json(reset) = update(&state, Some(""), None).await.unwrap();
        assert_eq!(reset.title, None);
        assert_eq!(reset.favicon, None);
        assert!(state.db.list_settings().await.unwrap().is_empty());
        assert_eq!(site_page_context(&state).site_title, "Config Title");
    }

    #[tokio::test]
    async fn test_invalid_values_are_rejected_without_saving() {
        let state = test_state_with_config(AegisConfig::default(), PluginSupervisor::new()).await;
        for (title, favicon) in [
            (None, Some("javascript:alert(1)")),
            (None, Some("//evil.example.com/x.png")),
            (Some("a\nb"), None),
        ] {
            let (status, _) = update(&state, title, favicon).await.unwrap_err();
            assert_eq!(status, StatusCode::BAD_REQUEST);
        }
        assert!(state.db.list_settings().await.unwrap().is_empty());
    }
}
