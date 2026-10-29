use axum::{
    extract::{Multipart, Path, State},
    http::StatusCode,
    Json,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::HashMap;
use std::sync::Arc;

use crate::audit::AuditContext;
use crate::auth::authorize::RequireRole;
use crate::auth::AdminSession;
use crate::state::AppState;
use aegis_auth::SessionData;
use aegis_core::{AegisError, AuditAction, AuditActor, AuditOutcome};
use aegis_runtime::{PluginInfo, PluginSupervisor, WasmPluginLoader};

type ApiError = (StatusCode, Json<serde_json::Value>);

fn api_error(status: StatusCode, message: impl Into<String>) -> ApiError {
    (status, Json(json!({ "error": message.into() })))
}

pub(crate) async fn get_effective_plugin_settings(
    state: &AppState,
    plugin_name: &str,
) -> HashMap<String, String> {
    let mut map = HashMap::new();

    // 1. 静的設定（aegis.toml）
    if let Some(static_settings) = state.config.plugins.settings.get(plugin_name) {
        for (k, v) in static_settings {
            let val_str = match v {
                toml::Value::String(s) => match s.strip_prefix("env:") {
                    Some(name) => std::env::var(name.trim()).unwrap_or_default(),
                    None => s.clone(),
                },
                other => serde_json::to_string(other).unwrap_or_default(),
            };
            map.insert(k.clone(), val_str);
        }
    }

    // 2. メモリ内動的設定の確認
    let loaded = {
        let guard = state.plugin_settings.read().unwrap();
        guard.get(plugin_name).cloned()
    };

    if let Some(dyn_map) = loaded {
        map.extend(dyn_map);
    } else {
        // キャッシュからロード
        let key = format!("plugin_settings:{}", plugin_name);
        if let Ok(Some(json_str)) = state.cache.get(&key).await {
            if let Ok(dyn_map) = serde_json::from_str::<HashMap<String, String>>(&json_str) {
                if let Ok(mut guard) = state.plugin_settings.write() {
                    guard.insert(plugin_name.to_string(), dyn_map.clone());
                }
                map.extend(dyn_map);
            }
        }
    }

    map
}

/// メモリに載せた設定値をキャッシュ（`plugin_settings:<名前>`）から読み直す（ほかのサーバーで変更されたとき）。
/// キャッシュに無ければメモリからも消す。キャッシュを読めなければ今の値のまま
pub(crate) async fn refresh_plugin_settings(state: &AppState, plugin_name: &str) {
    let key = format!("plugin_settings:{}", plugin_name);
    let saved = match state.cache.get(&key).await {
        Ok(saved) => saved,
        Err(e) => {
            tracing::warn!(
                "Could not reload the settings of plugin '{}': {}",
                plugin_name,
                e
            );
            return;
        }
    };
    let settings =
        saved.and_then(|json| serde_json::from_str::<HashMap<String, String>>(&json).ok());
    let mut guard = state
        .plugin_settings
        .write()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    match settings {
        Some(settings) => {
            guard.insert(plugin_name.to_string(), settings);
        }
        None => {
            guard.remove(plugin_name);
        }
    }
}

/// `GET /api/v1/plugins` — インストール済み・有効化済みのプラグイン一覧（admin 権限必須）。
/// 有効なもの（有効化リストの順 = フックの実行順）→ 無効なもの（名前順）の順に返す
/// `GET /api/v1/blocks` — 本文に置けるプラグインのカスタムブロック（有効なプラグインのマニフェストの
/// `blocks`）。`name` は本文に保存する名前（`<プラグイン名>/<ブロック名>`）。Admin Studio の
/// ブロックの追加と属性のフォームに使う
pub async fn list_blocks(
    State(state): State<Arc<AppState>>,
    _session: crate::auth::ViewerSession,
) -> Json<serde_json::Value> {
    let supervisor = state.supervisor.read().await;
    let blocks: Vec<serde_json::Value> = supervisor
        .block_definitions()
        .into_iter()
        .map(|(plugin, block)| {
            let display_name = supervisor
                .plugin(&plugin)
                .and_then(|p| p.manifest().display_name.clone())
                .unwrap_or_else(|| plugin.clone());
            let mut value = serde_json::to_value(&block).unwrap_or_default();
            if let Some(obj) = value.as_object_mut() {
                obj.insert("name".into(), json!(format!("{}/{}", plugin, block.name)));
                obj.insert("plugin".into(), json!(plugin));
                obj.insert("plugin_display_name".into(), json!(display_name));
            }
            value
        })
        .collect();
    Json(json!({ "blocks": blocks }))
}

pub async fn list_plugins(
    State(state): State<Arc<AppState>>,
    RequireRole(_session): AdminSession,
) -> Result<Json<Vec<PluginInfo>>, ApiError> {
    let enabled = state.plugin_registry.enabled().await;
    plugin_list(&state, &enabled).await.map(Json)
}

async fn plugin_list(state: &AppState, enabled: &[String]) -> Result<Vec<PluginInfo>, ApiError> {
    let mut plugins = WasmPluginLoader::list(
        &state.config.paths.plugins_dir,
        enabled,
        &state.config.runtime.default_resource_limits(),
    )
    .map_err(|e| {
        tracing::error!("Failed to list plugins: {}", e);
        api_error(StatusCode::INTERNAL_SERVER_ERROR, "Failed to list plugins")
    })?;

    for p in &mut plugins {
        p.settings = get_effective_plugin_settings(state, &p.name).await;
    }

    Ok(plugins)
}

#[derive(Debug, Deserialize, Serialize)]
pub struct SetPluginEnabledRequest {
    pub enabled: bool,
}

/// `PUT /api/v1/plugins/{name}/enabled` — プラグインを有効・無効にする（admin 権限必須）。
/// 有効にしたプラグインはフックの実行順の最後に加わる。一覧を DB（`_aegis_settings` の `plugins.enabled`）に
/// 保存し、再起動せずに反映する（以後 `[plugins].enabled` より優先）。応答は `GET /api/v1/plugins` と同じ
pub async fn set_plugin_enabled(
    State(state): State<Arc<AppState>>,
    RequireRole(session): AdminSession,
    audit: AuditContext,
    Path(name): Path<String>,
    Json(payload): Json<SetPluginEnabledRequest>,
) -> Result<Json<Vec<PluginInfo>>, ApiError> {
    if !aegis_core::is_valid_plugin_name(&name) {
        return Err(api_error(StatusCode::BAD_REQUEST, "Invalid plugin name"));
    }
    require_switchable(&state)?;

    // 同時の切り替えは順に処理する（一覧の読み書きと supervisor の差し替えの間ロックを持つ）
    let mut enabled = state.plugin_registry.lock().await;
    if enabled.contains(&name) == payload.enabled {
        return plugin_list(&state, &enabled).await.map(Json);
    }
    let next: Vec<String> = if payload.enabled {
        enabled.iter().cloned().chain([name.clone()]).collect()
    } else {
        enabled.iter().filter(|n| **n != name).cloned().collect()
    };
    let (action, verb) = if payload.enabled {
        (AuditAction::PluginEnable, "enabled")
    } else {
        (AuditAction::PluginDisable, "disabled")
    };
    let change = Change {
        action,
        plugin: Some(name.clone()),
        persist: Persist::Save,
    };
    switch_plugins(&state, &session, &audit, &mut enabled, next, change).await?;
    tracing::info!("Plugin '{}' {} from Admin Studio", name, verb);
    plugin_list(&state, &enabled).await.map(Json)
}

/// 有効なプラグインの実行順（`GET` / `PUT` / `DELETE /api/v1/plugins/order` の応答）
#[derive(Debug, Serialize, Deserialize)]
pub struct PluginOrder {
    /// 有効なプラグイン（フックの実行順）
    pub enabled: Vec<String>,
    /// 設定ファイルの `[plugins].enabled`
    pub configured: Vec<String>,
    /// Admin Studio で変えた一覧を DB に保存しているか（`true` なら `[plugins].enabled` より優先）
    pub customized: bool,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct SetPluginOrderRequest {
    /// 有効なプラグインを新しい順に並べたもの（今有効なものをちょうど 1 回ずつ）
    pub enabled: Vec<String>,
}

/// `GET /api/v1/plugins/order` — 有効なプラグインの実行順と、設定ファイルの値（admin 権限必須）
pub async fn get_plugin_order(
    State(state): State<Arc<AppState>>,
    RequireRole(_session): AdminSession,
) -> Result<Json<PluginOrder>, ApiError> {
    let enabled = state.plugin_registry.enabled().await;
    plugin_order(&state, enabled).await.map(Json)
}

/// `PUT /api/v1/plugins/order` — 有効なプラグインを並べ替える（admin 権限必須）。DB に保存して再起動せずに反映する
pub async fn set_plugin_order(
    State(state): State<Arc<AppState>>,
    RequireRole(session): AdminSession,
    audit: AuditContext,
    Json(payload): Json<SetPluginOrderRequest>,
) -> Result<Json<PluginOrder>, ApiError> {
    require_switchable(&state)?;
    let mut enabled = state.plugin_registry.lock().await;
    let mut requested = payload.enabled.clone();
    let mut current = enabled.clone();
    requested.sort();
    current.sort();
    if requested != current {
        return Err(api_error(
            StatusCode::BAD_REQUEST,
            "The order must list each enabled plugin exactly once",
        ));
    }
    if payload.enabled != *enabled {
        let change = Change {
            action: AuditAction::PluginReorder,
            plugin: None,
            persist: Persist::Save,
        };
        switch_plugins(
            &state,
            &session,
            &audit,
            &mut enabled,
            payload.enabled,
            change,
        )
        .await?;
        tracing::info!("Plugin order changed from Admin Studio: {:?}", *enabled);
    }
    plugin_order(&state, enabled.clone()).await.map(Json)
}

/// `DELETE /api/v1/plugins/order` — Admin Studio で保存した一覧を消し、設定ファイルの `[plugins].enabled`
/// （有効なプラグインとその順）に戻す（admin 権限必須）。再起動せずに反映する
pub async fn reset_plugin_order(
    State(state): State<Arc<AppState>>,
    RequireRole(session): AdminSession,
    audit: AuditContext,
) -> Result<Json<PluginOrder>, ApiError> {
    require_switchable(&state)?;
    let mut enabled = state.plugin_registry.lock().await;
    let configured = state.config.plugins.enabled.clone();
    if *enabled == configured && !is_customized(&state).await? {
        return plugin_order(&state, enabled.clone()).await.map(Json);
    }
    let change = Change {
        action: AuditAction::PluginReset,
        plugin: None,
        persist: Persist::Clear,
    };
    switch_plugins(&state, &session, &audit, &mut enabled, configured, change).await?;
    tracing::info!(
        "Plugins reset to [plugins].enabled from Admin Studio: {:?}",
        *enabled
    );
    plugin_order(&state, enabled.clone()).await.map(Json)
}

async fn plugin_order(state: &AppState, enabled: Vec<String>) -> Result<PluginOrder, ApiError> {
    Ok(PluginOrder {
        enabled,
        configured: state.config.plugins.enabled.clone(),
        customized: is_customized(state).await?,
    })
}

async fn is_customized(state: &AppState) -> Result<bool, ApiError> {
    let settings = state.db.list_settings().await.map_err(|e| {
        tracing::error!("Failed to read the saved plugin list: {}", e);
        api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Failed to read the saved plugin list",
        )
    })?;
    Ok(settings.contains_key(crate::plugin_registry::ENABLED_SETTING))
}

/// 切り替えられないサーバー（`[runtime].enable_wasm_plugins = false`）なら 409
fn require_switchable(state: &AppState) -> Result<(), ApiError> {
    if state.plugin_registry.loader().is_none() {
        return Err(api_error(
            StatusCode::CONFLICT,
            "Wasm plugins are turned off on this server ([runtime].enable_wasm_plugins = false)",
        ));
    }
    Ok(())
}

/// 切り替えた一覧を DB にどう残すか
enum Persist {
    /// 一覧を保存する（`[plugins].enabled` より優先）
    Save,
    /// 保存した一覧を消す（`[plugins].enabled` に戻る）
    Clear,
}

/// 切り替えの種類（監査ログ）と保存のしかた
struct Change {
    action: AuditAction,
    /// 監査ログの対象のプラグイン（並べ替え・戻すときは `None`）
    plugin: Option<String>,
    persist: Persist,
}

/// 一覧を `next` に切り替える: 組み立て → DB に保存 → 差し替え → ほかのサーバーに知らせる（監査ログも残す）。
/// 呼ぶ側は `plugin_registry.lock()` のガード（`enabled`）を持つ。失敗したら何も変えない
async fn switch_plugins(
    state: &AppState,
    session: &SessionData,
    audit: &AuditContext,
    enabled: &mut Vec<String>,
    next: Vec<String>,
    change: Change,
) -> Result<(), ApiError> {
    if let Some(channel) = missing_notification_channel(state, &next) {
        return Err(api_error(
            StatusCode::CONFLICT,
            format!(
                "Plugin '{}' is a notification channel in [notifications].channels; remove it there first",
                channel
            ),
        ));
    }
    let event = |outcome| {
        audit
            .event(change.action, outcome)
            .with_actor(Some(AuditActor {
                user_id: session.user_id.clone(),
                role: session.role.clone(),
            }))
            .with_resource("plugin", change.plugin.clone())
    };

    let supervisor = match prepare_and_save(state, enabled, &next, &change.persist).await {
        Ok(supervisor) => supervisor,
        Err((status, message)) => {
            state
                .audit
                .record(event(AuditOutcome::Failure).with_details(json!({ "error": message })))
                .await;
            return Err(api_error(status, message));
        }
    };

    crate::plugin_registry::install(state, supervisor).await;
    let previous = std::mem::replace(enabled, next);
    crate::cluster::announce(state, crate::cluster::TOPIC_PLUGINS, "").await;
    state
        .audit
        .record(
            event(AuditOutcome::Success).with_details(json!({ "from": previous, "to": *enabled })),
        )
        .await;
    Ok(())
}

/// 新しい supervisor を組み立ててから、一覧を DB に残す（組み立てに失敗したら保存しない）
async fn prepare_and_save(
    state: &AppState,
    current: &[String],
    next: &[String],
    persist: &Persist,
) -> Result<PluginSupervisor, (StatusCode, String)> {
    let supervisor = crate::plugin_registry::prepare(state, current, next)
        .await
        .map_err(|e| match e {
            AegisError::Validation(message) => (StatusCode::BAD_REQUEST, message),
            other => {
                tracing::error!("Failed to load the plugins: {}", other);
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "Failed to load the plugins".to_string(),
                )
            }
        })?;
    let saved = match persist {
        Persist::Save => {
            let json = serde_json::to_string(next).unwrap_or_default();
            state
                .db
                .set_setting(crate::plugin_registry::ENABLED_SETTING, &json)
                .await
        }
        Persist::Clear => {
            state
                .db
                .delete_setting(crate::plugin_registry::ENABLED_SETTING)
                .await
        }
    };
    saved.map_err(|e| {
        tracing::error!("Failed to save the enabled plugins: {}", e);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            "Failed to save the enabled plugins".to_string(),
        )
    })?;
    Ok(supervisor)
}

/// `[notifications].channels` の `plugin:<名前>` のうち `next` に無いもの（外すと通知が届かなくなる）
fn missing_notification_channel(state: &AppState, next: &[String]) -> Option<String> {
    let notifications = &state.config.notifications;
    if !notifications.enabled {
        return None;
    }
    notifications.channels.iter().find_map(|c| match c {
        aegis_notify::ChannelKind::Plugin(p) if !next.contains(p) => Some(p.clone()),
        _ => None,
    })
}

/// `GET /api/v1/plugins/{name}/settings` — プラグインの設定値を取得する（admin 権限必須）
pub async fn get_plugin_settings(
    State(state): State<Arc<AppState>>,
    RequireRole(_session): AdminSession,
    Path(name): Path<String>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    if !aegis_core::is_valid_plugin_name(&name) {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "Invalid plugin name" })),
        ));
    }
    let settings = get_effective_plugin_settings(&state, &name).await;
    Ok(Json(json!({ "name": name, "settings": settings })))
}

#[derive(Debug, Deserialize, Serialize)]
pub struct UpdatePluginSettingsRequest {
    pub settings: HashMap<String, String>,
}

/// `PUT /api/v1/plugins/{name}/settings` — プラグインの設定値を更新する（admin 権限必須）
pub async fn update_plugin_settings(
    State(state): State<Arc<AppState>>,
    RequireRole(_session): AdminSession,
    audit: AuditContext,
    Path(name): Path<String>,
    Json(payload): Json<UpdatePluginSettingsRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    if !aegis_core::is_valid_plugin_name(&name) {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "Invalid plugin name" })),
        ));
    }

    let key = format!("plugin_settings:{}", name);
    let json_str = serde_json::to_string(&payload.settings).map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": format!("Serialization failed: {}", e) })),
        )
    })?;

    state.cache.set(&key, &json_str, None).await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": format!("Cache set failed: {}", e) })),
        )
    })?;

    if let Ok(mut guard) = state.plugin_settings.write() {
        guard.insert(name.clone(), payload.settings.clone());
    }
    // ほかのサーバーはメモリに載せた値を持っているので、キャッシュから読み直してもらう
    crate::cluster::announce(&state, crate::cluster::TOPIC_PLUGIN_SETTINGS, &name).await;

    state
        .audit
        .record(
            audit
                .event(AuditAction::PluginSettingsUpdate, AuditOutcome::Success)
                .with_resource("plugin", Some(name.clone()))
                .with_details(json!({ "keys": payload.settings.keys().collect::<Vec<_>>() })),
        )
        .await;

    let effective = get_effective_plugin_settings(&state, &name).await;
    Ok(Json(json!({ "name": name, "settings": effective })))
}

/// `POST /api/v1/plugins/install` — プラグイン ZIP パッケージを検証・展開・インストールする（admin 権限必須）
pub async fn install_plugin(
    State(state): State<Arc<AppState>>,
    RequireRole(session): AdminSession,
    audit: AuditContext,
    mut multipart: Multipart,
) -> Result<Json<PluginInfo>, ApiError> {
    require_switchable(&state)?;

    let mut zip_bytes: Option<Vec<u8>> = None;
    while let Ok(Some(field)) = multipart.next_field().await {
        if field.name() == Some("file") || field.name() == Some("package") {
            let bytes = field.bytes().await.map_err(|e| {
                api_error(
                    StatusCode::BAD_REQUEST,
                    format!("Failed to read upload: {}", e),
                )
            })?;
            zip_bytes = Some(bytes.to_vec());
            break;
        }
    }

    let bytes = zip_bytes.ok_or_else(|| {
        api_error(
            StatusCode::BAD_REQUEST,
            "No 'file' field found in multipart request",
        )
    })?;

    let temp_root = std::env::temp_dir().join(format!(
        "aegis_plugin_inst_{}_{}",
        std::process::id(),
        uuid::Uuid::new_v4()
    ));
    let _ = std::fs::remove_dir_all(&temp_root);
    std::fs::create_dir_all(&temp_root).map_err(|e| {
        api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to create temp dir: {}", e),
        )
    })?;

    let res = (|| -> aegis_core::Result<(aegis_core::PluginManifest, std::path::PathBuf)> {
        aegis_core::extract_zip_safely(&bytes, &temp_root, None)?;
        let pkg_root = find_plugin_package_root(&temp_root)?;

        let trusted_keys = if state.config.plugins.trusted_public_keys.is_empty() {
            None
        } else {
            Some(state.config.plugins.trusted_public_keys.as_slice())
        };

        let manifest = WasmPluginLoader::validate_plugin_package(
            &pkg_root,
            trusted_keys,
            state.config.plugins.require_signatures,
        )?;

        Ok((manifest, pkg_root))
    })();

    let (manifest, pkg_root) = match res {
        Ok(val) => val,
        Err(e) => {
            let _ = std::fs::remove_dir_all(&temp_root);
            return Err(api_error(
                StatusCode::BAD_REQUEST,
                format!("Package validation failed: {}", e),
            ));
        }
    };

    let plugins_dir = &state.config.paths.plugins_dir;
    let target_dir = plugins_dir.join(&manifest.name);

    if target_dir.exists() {
        if let Err(e) = std::fs::remove_dir_all(&target_dir) {
            let _ = std::fs::remove_dir_all(&temp_root);
            return Err(api_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to remove existing plugin directory: {}", e),
            ));
        }
    } else if let Err(e) = std::fs::create_dir_all(plugins_dir) {
        let _ = std::fs::remove_dir_all(&temp_root);
        return Err(api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to create plugins dir: {}", e),
        ));
    }

    if let Err(e) = copy_dir_all(&pkg_root, &target_dir) {
        let _ = std::fs::remove_dir_all(&temp_root);
        return Err(api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to install plugin files: {}", e),
        ));
    }

    let _ = std::fs::remove_dir_all(&temp_root);

    state
        .audit
        .record(
            audit
                .event(AuditAction::PluginInstall, AuditOutcome::Success)
                .with_actor(Some(AuditActor {
                    user_id: session.user_id,
                    role: session.role,
                }))
                .with_resource("plugin", Some(manifest.name.clone()))
                .with_details(json!({
                    "version": manifest.version,
                    "display_name": manifest.display_name,
                })),
        )
        .await;

    let enabled = state.plugin_registry.enabled().await;
    let list = plugin_list(&state, &enabled).await?;
    let info = list
        .into_iter()
        .find(|p| p.name == manifest.name)
        .unwrap_or(PluginInfo {
            name: manifest.name.clone(),
            is_enabled: enabled.contains(&manifest.name),
            is_installed: true,
            problem: None,
            manifest: Some(manifest),
            settings: Default::default(),
        });

    Ok(Json(info))
}

fn find_plugin_package_root(dir: &std::path::Path) -> aegis_core::Result<std::path::PathBuf> {
    if dir.join("plugin.manifest.json").is_file() {
        return Ok(dir.to_path_buf());
    }
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() && path.join("plugin.manifest.json").is_file() {
                return Ok(path);
            }
        }
    }
    Err(AegisError::Validation(
        "Missing 'plugin.manifest.json' in archive".to_string(),
    ))
}

fn copy_dir_all(src: &std::path::Path, dst: &std::path::Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dst)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let ty = entry.file_type()?;
        let dest_path = dst.join(entry.file_name());
        if ty.is_dir() {
            copy_dir_all(&entry.path(), &dest_path)?;
        } else {
            std::fs::copy(entry.path(), dest_path)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::AegisConfig;
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

    #[tokio::test]
    async fn test_list_plugins_reports_enabled_and_installed_state() {
        let plugins_dir =
            std::env::temp_dir().join(format!("aegis_plugins_api_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&plugins_dir);
        for name in ["seo-tools", "slack-notifier"] {
            let dir = plugins_dir.join(name);
            std::fs::create_dir_all(&dir).unwrap();
            std::fs::write(
                dir.join("plugin.manifest.json"),
                format!(
                    r#"{{"name": "{}", "version": "1.2.0", "entrypoint": "plugin.wasm",
                        "display_name": "{} plugin", "capabilities": {{"hooks": ["on-before-save"]}}}}"#,
                    name, name
                ),
            )
            .unwrap();
            std::fs::write(dir.join("plugin.wasm"), b"\0asm\x01\0\0\0").unwrap();
        }
        let mut config = AegisConfig::default();
        config.paths.plugins_dir = plugins_dir.clone();
        config.plugins.enabled = vec!["slack-notifier".to_string(), "removed".to_string()];
        let state = test_state_with_config(config, PluginSupervisor::new()).await;

        let Json(plugins) = list_plugins(State(state), admin()).await.unwrap();
        let summary: Vec<(&str, bool, bool)> = plugins
            .iter()
            .map(|p| (p.name.as_str(), p.is_enabled, p.is_installed))
            .collect();
        assert_eq!(
            summary,
            vec![
                ("slack-notifier", true, true),
                ("removed", true, false),
                ("seo-tools", false, true),
            ]
        );
        let manifest = plugins[0].manifest.as_ref().unwrap();
        assert_eq!(manifest.version, "1.2.0");
        assert_eq!(
            manifest.display_name.as_deref(),
            Some("slack-notifier plugin")
        );
        let _ = std::fs::remove_dir_all(&plugins_dir);
    }

    /// The repository's `plugins/` (official plugins with their built Wasm), switchable at runtime
    async fn switchable_state(
        enabled: &[&str],
        configure: impl FnOnce(&mut AegisConfig),
    ) -> Arc<AppState> {
        switchable_server(enabled, configure, None).await
    }

    /// [`switchable_state`] on the database, cache and event bus of `shared` (another server of the site)
    async fn switchable_server(
        enabled: &[&str],
        configure: impl FnOnce(&mut AegisConfig),
        shared: Option<&Shared>,
    ) -> Arc<AppState> {
        let plugins_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../plugins");
        let mut config = AegisConfig::default();
        config.paths.plugins_dir = plugins_dir.clone();
        configure(&mut config);
        let enabled: Vec<String> = enabled.iter().map(|n| n.to_string()).collect();
        config.plugins.enabled = enabled.clone();
        let loader = crate::plugin_registry::PluginLoader::new(
            aegis_runtime::WasmEngine::new().unwrap(),
            Arc::new(std::sync::Mutex::new(HashMap::new())).into(),
            plugins_dir,
            config.runtime.default_resource_limits(),
        );
        let supervisor = loader.load(&enabled).unwrap();
        let mut state = match shared {
            Some(shared) => {
                let mut state = crate::test_support::test_app_state_with_db(
                    shared.db.clone(),
                    config,
                    supervisor,
                    Vec::new(),
                )
                .await;
                state.cache = shared.cache.clone();
                state.bus = shared.bus.clone();
                state
            }
            None => crate::test_support::test_app_state(config, supervisor, Vec::new()).await,
        };
        state.plugin_registry = Arc::new(crate::plugin_registry::PluginRegistry::new(
            Some(loader),
            enabled,
        ));
        Arc::new(state)
    }

    /// What the servers of one site share: the database and the cache (Redis), whose Pub/Sub
    /// carries the event bus
    struct Shared {
        db: Arc<dyn aegis_db::DatabaseAdapter>,
        cache: Arc<dyn aegis_core::CacheBackend>,
        bus: Arc<dyn aegis_cache::EventBus>,
    }

    /// Two servers on one database and one cache, both started with `[plugins].enabled = enabled`
    async fn two_servers(enabled: &[&str]) -> (Arc<AppState>, Arc<AppState>) {
        let first = switchable_state(enabled, |_| {}).await;
        let shared = Shared {
            db: first.db.clone(),
            cache: first.cache.clone(),
            bus: first.bus.clone(),
        };
        let second = switchable_server(enabled, |_| {}, Some(&shared)).await;
        (first, second)
    }

    async fn set_enabled(
        state: &Arc<AppState>,
        name: &str,
        enabled: bool,
    ) -> Result<Vec<PluginInfo>, StatusCode> {
        set_plugin_enabled(
            State(state.clone()),
            admin(),
            AuditContext::default(),
            Path(name.to_string()),
            Json(SetPluginEnabledRequest { enabled }),
        )
        .await
        .map(|Json(list)| list)
        .map_err(|(status, _)| status)
    }

    async fn loaded_names(state: &AppState) -> Vec<String> {
        state
            .supervisor
            .read()
            .await
            .plugins()
            .iter()
            .map(|p| p.manifest().name.clone())
            .collect()
    }

    async fn saved_list(state: &AppState) -> Option<String> {
        state
            .db
            .list_settings()
            .await
            .unwrap()
            .remove(crate::plugin_registry::ENABLED_SETTING)
    }

    #[tokio::test]
    async fn test_enable_and_disable_plugins_without_restart() {
        let state = switchable_state(&["official-code-enhancer"], |_| {}).await;
        let enhancer = state.supervisor.read().await.plugins()[0].clone();

        // Enabling appends to the hook order and keeps the loaded instance
        let list = set_enabled(&state, "official-shop-rules", true)
            .await
            .unwrap();
        let enabled: Vec<&str> = list
            .iter()
            .filter(|p| p.is_enabled)
            .map(|p| p.name.as_str())
            .collect();
        assert_eq!(
            enabled,
            vec!["official-code-enhancer", "official-shop-rules"]
        );
        assert_eq!(
            loaded_names(&state).await,
            vec!["official-code-enhancer", "official-shop-rules"]
        );
        assert!(Arc::ptr_eq(
            &enhancer,
            &state.supervisor.read().await.plugins()[0]
        ));
        assert_eq!(
            saved_list(&state).await.as_deref(),
            Some(r#"["official-code-enhancer","official-shop-rules"]"#)
        );

        // Enabling again changes nothing
        set_enabled(&state, "official-shop-rules", true)
            .await
            .unwrap();
        assert_eq!(loaded_names(&state).await.len(), 2);

        let list = set_enabled(&state, "official-code-enhancer", false)
            .await
            .unwrap();
        assert!(
            !list
                .iter()
                .find(|p| p.name == "official-code-enhancer")
                .unwrap()
                .is_enabled
        );
        assert_eq!(loaded_names(&state).await, vec!["official-shop-rules"]);
        assert_eq!(
            state.plugin_registry.enabled().await,
            vec!["official-shop-rules"]
        );
        assert_eq!(
            saved_list(&state).await.as_deref(),
            Some(r#"["official-shop-rules"]"#)
        );

        // A plugin that is not installed is rejected and nothing changes
        assert_eq!(
            set_enabled(&state, "not-installed", true)
                .await
                .unwrap_err(),
            StatusCode::BAD_REQUEST
        );
        assert_eq!(
            set_enabled(&state, "../plugins", true).await.unwrap_err(),
            StatusCode::BAD_REQUEST
        );
        assert_eq!(loaded_names(&state).await, vec!["official-shop-rules"]);
        assert_eq!(
            saved_list(&state).await.as_deref(),
            Some(r#"["official-shop-rules"]"#)
        );
    }

    #[tokio::test]
    async fn test_plugin_switch_is_refused_when_it_cannot_apply() {
        // A plugin used as a notification channel cannot be disabled
        let state = switchable_state(&["official-order-notifier"], |config| {
            config.notifications.enabled = true;
            config.notifications.channels = vec![aegis_notify::ChannelKind::Plugin(
                "official-order-notifier".into(),
            )];
        })
        .await;
        assert_eq!(
            set_enabled(&state, "official-order-notifier", false)
                .await
                .unwrap_err(),
            StatusCode::CONFLICT
        );
        assert_eq!(loaded_names(&state).await, vec!["official-order-notifier"]);

        // Without the Wasm runtime ([runtime].enable_wasm_plugins = false) there is nothing to switch
        let state = test_state_with_config(AegisConfig::default(), PluginSupervisor::new()).await;
        assert_eq!(
            set_enabled(&state, "official-shop-rules", true)
                .await
                .unwrap_err(),
            StatusCode::CONFLICT
        );
        assert_eq!(saved_list(&state).await, None);
    }

    async fn set_order(state: &Arc<AppState>, order: &[&str]) -> Result<PluginOrder, StatusCode> {
        set_plugin_order(
            State(state.clone()),
            admin(),
            AuditContext::default(),
            Json(SetPluginOrderRequest {
                enabled: order.iter().map(|n| n.to_string()).collect(),
            }),
        )
        .await
        .map(|Json(order)| order)
        .map_err(|(status, _)| status)
    }

    async fn reset_order(state: &Arc<AppState>) -> Result<PluginOrder, StatusCode> {
        reset_plugin_order(State(state.clone()), admin(), AuditContext::default())
            .await
            .map(|Json(order)| order)
            .map_err(|(status, _)| status)
    }

    #[tokio::test]
    async fn test_reorder_plugins_and_reset_to_the_configuration() {
        let state =
            switchable_state(&["official-code-enhancer", "official-shop-rules"], |_| {}).await;
        let Json(order) = get_plugin_order(State(state.clone()), admin())
            .await
            .unwrap();
        assert_eq!(order.enabled, order.configured);
        assert!(!order.customized);

        // The new order must name exactly the enabled plugins
        for wrong in [
            &["official-shop-rules"][..],
            &["official-shop-rules", "official-shop-rules"],
            &[
                "official-shop-rules",
                "official-code-enhancer",
                "official-order-notifier",
            ],
        ] {
            assert_eq!(
                set_order(&state, wrong).await.unwrap_err(),
                StatusCode::BAD_REQUEST
            );
        }
        // The same order changes nothing
        let order = set_order(&state, &["official-code-enhancer", "official-shop-rules"])
            .await
            .unwrap();
        assert!(!order.customized);
        assert_eq!(saved_list(&state).await, None);

        let order = set_order(&state, &["official-shop-rules", "official-code-enhancer"])
            .await
            .unwrap();
        assert_eq!(
            order.enabled,
            vec!["official-shop-rules", "official-code-enhancer"]
        );
        assert!(order.customized);
        assert_eq!(
            loaded_names(&state).await,
            vec!["official-shop-rules", "official-code-enhancer"]
        );
        assert_eq!(
            saved_list(&state).await.as_deref(),
            Some(r#"["official-shop-rules","official-code-enhancer"]"#)
        );

        // Resetting drops the saved list: enabled plugins and order come from [plugins].enabled
        set_enabled(&state, "official-order-notifier", true)
            .await
            .unwrap();
        let order = reset_order(&state).await.unwrap();
        assert_eq!(
            order.enabled,
            vec!["official-code-enhancer", "official-shop-rules"]
        );
        assert!(!order.customized);
        assert_eq!(saved_list(&state).await, None);
        assert_eq!(
            loaded_names(&state).await,
            vec!["official-code-enhancer", "official-shop-rules"]
        );
    }

    #[tokio::test]
    async fn test_switches_reach_the_other_servers() {
        let (first, second) = two_servers(&["official-code-enhancer"]).await;
        // Each server listens from startup (see cluster::spawn_listener)
        let mut on_first = first.bus.subscribe();
        let mut on_second = second.bus.subscribe();
        async fn deliver(state: &AppState, subscription: &mut aegis_cache::BusSubscription) {
            let message = subscription.recv().await.unwrap();
            crate::cluster::handle(state, message).await;
        }

        set_enabled(&first, "official-shop-rules", true)
            .await
            .unwrap();
        deliver(&second, &mut on_second).await;
        assert_eq!(
            loaded_names(&second).await,
            vec!["official-code-enhancer", "official-shop-rules"]
        );
        // The server that switched gets its own message too: it already has the list
        let before = first.supervisor.read().await.plugins()[1].clone();
        deliver(&first, &mut on_first).await;
        assert!(Arc::ptr_eq(
            &before,
            &first.supervisor.read().await.plugins()[1]
        ));

        set_order(&second, &["official-shop-rules", "official-code-enhancer"])
            .await
            .unwrap();
        deliver(&first, &mut on_first).await;
        deliver(&second, &mut on_second).await;
        assert_eq!(
            loaded_names(&first).await,
            vec!["official-shop-rules", "official-code-enhancer"]
        );
        assert_eq!(
            first.plugin_registry.enabled().await,
            second.plugin_registry.enabled().await
        );

        // Settings saved on one server replace what the other holds in memory
        get_effective_plugin_settings(&second, "official-shop-rules").await;
        let _ = update_plugin_settings(
            State(first.clone()),
            admin(),
            AuditContext::default(),
            Path("official-shop-rules".to_string()),
            Json(UpdatePluginSettingsRequest {
                settings: HashMap::from([("shipping_fee".to_string(), "900".to_string())]),
            }),
        )
        .await
        .unwrap();
        deliver(&first, &mut on_first).await;
        deliver(&second, &mut on_second).await;
        assert_eq!(
            get_effective_plugin_settings(&second, "official-shop-rules")
                .await
                .get("shipping_fee")
                .map(String::as_str),
            Some("900")
        );

        // A missed message (e.g. Redis was unreachable) is caught up by the resync after reconnecting
        reset_order(&first).await.unwrap();
        crate::cluster::handle(&second, aegis_cache::BusMessage::Resync).await;
        assert_eq!(loaded_names(&second).await, vec!["official-code-enhancer"]);
    }

    #[tokio::test]
    async fn test_plugin_settings_get_and_update() {
        let state = test_state_with_config(AegisConfig::default(), PluginSupervisor::new()).await;

        // 1. Initial settings (empty)
        let Json(res) = get_plugin_settings(
            State(state.clone()),
            admin(),
            Path("official-shop-rules".to_string()),
        )
        .await
        .unwrap();
        assert_eq!(res["name"], "official-shop-rules");
        assert_eq!(res["settings"], json!({}));

        // 2. Update settings
        let mut new_settings = HashMap::new();
        new_settings.insert("shipping_fee".to_string(), "700".to_string());
        new_settings.insert("coupons".to_string(), "SALE20:20%".to_string());

        let Json(updated) = update_plugin_settings(
            State(state.clone()),
            admin(),
            AuditContext::default(),
            Path("official-shop-rules".to_string()),
            Json(UpdatePluginSettingsRequest {
                settings: new_settings.clone(),
            }),
        )
        .await
        .unwrap();

        assert_eq!(updated["name"], "official-shop-rules");
        assert_eq!(updated["settings"]["shipping_fee"], "700");
        assert_eq!(updated["settings"]["coupons"], "SALE20:20%");

        // 3. Fetch again to verify persistence in cache & state
        let Json(fetched) = get_plugin_settings(
            State(state.clone()),
            admin(),
            Path("official-shop-rules".to_string()),
        )
        .await
        .unwrap();
        assert_eq!(fetched["settings"]["shipping_fee"], "700");

        // 4. Invalid name rejected
        assert_eq!(
            get_plugin_settings(
                State(state.clone()),
                admin(),
                Path("INVALID NAME!".to_string())
            )
            .await
            .unwrap_err()
            .0,
            StatusCode::BAD_REQUEST
        );
    }

    #[tokio::test]
    async fn test_list_blocks_returns_the_blocks_of_plugins_with_the_hook() {
        struct BlocksOnly(aegis_core::PluginManifest);
        #[async_trait::async_trait]
        impl aegis_runtime::AegisPlugin for BlocksOnly {
            fn manifest(&self) -> &aegis_core::PluginManifest {
                &self.0
            }
        }
        let manifest = |name: &str, hooks: serde_json::Value| -> aegis_core::PluginManifest {
            serde_json::from_value(json!({
                "name": name,
                "version": "1.0.0",
                "entrypoint": "plugin.wasm",
                "display_name": "Cards",
                "capabilities": { "hooks": hooks },
                "blocks": [{ "name": "card", "label": { "en": "Card", "ja": "カード" },
                             "fields": [{ "name": "title", "type": "string", "label": "Title" }] }]
            }))
            .unwrap()
        };
        let mut supervisor = PluginSupervisor::new();
        supervisor.register(Arc::new(BlocksOnly(manifest(
            "cards",
            json!(["render-blocks"]),
        ))));
        supervisor.register(Arc::new(BlocksOnly(manifest("nohook", json!([])))));
        let state = crate::test_support::test_state(supervisor).await;

        let Json(body) = list_blocks(
            State(state),
            RequireRole(SessionData {
                id: "s".to_string(),
                user_id: "u".to_string(),
                role: "viewer".to_string(),
                created_at: String::new(),
                expires_at: String::new(),
                metadata: Default::default(),
            }),
        )
        .await;
        assert_eq!(
            body,
            json!({ "blocks": [{
                "name": "cards/card",
                "plugin": "cards",
                "plugin_display_name": "Cards",
                "label": { "en": "Card", "ja": "カード" },
                "fields": [{ "name": "title", "type": "string", "label": "Title", "required": false }]
            }] })
        );
    }
}
