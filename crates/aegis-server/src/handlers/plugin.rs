use axum::{
    extract::{Multipart, Path, State},
    http::{header, StatusCode},
    response::IntoResponse,
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
use crate::store::InstallError;
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
    let mut plugins = WasmPluginLoader::list_in(
        &state.config.paths.plugin_dirs(),
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
    // プラグインの出力（ブロック・スクリプト・CSP のホスト）と同梱のアセット（`/plugins/<name>/assets/`）を
    // CDN に残さない。無効にした（問題が見つかった）プラグインのものが、キャッシュから配られ続けないように
    crate::cdn::purge_site_global(state).await;
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

/// `plugins_dir/<name>/` のマニフェスト（読めなければ `None`）
fn installed_manifest(state: &AppState, name: &str) -> Option<aegis_core::PluginManifest> {
    WasmPluginLoader::discover_in(
        &state.config.paths.plugin_dirs(),
        &state.config.runtime.default_resource_limits(),
    )
    .ok()?
    .into_iter()
    .find(|p| p.dir_name == name)?
    .manifest
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

    // マニフェストで宣言した項目は型・範囲を検査し、空欄（未設定）は保存しない。宣言のないキーはそのまま保存する
    let mut settings = payload.settings;
    if let Some(manifest) = installed_manifest(&state, &name) {
        let errors = manifest.invalid_setting_values(&settings);
        if !errors.is_empty() {
            return Err((
                StatusCode::BAD_REQUEST,
                Json(json!({
                    "error": format!("Invalid setting value: {}", errors[0].0),
                    "code": "invalid_setting",
                    "errors": errors
                        .iter()
                        .map(|(key, message)| json!({ "key": key, "message": message }))
                        .collect::<Vec<_>>(),
                })),
            ));
        }
        settings.retain(|key, value| {
            !value.is_empty() || !manifest.settings.iter().any(|s| s.key == *key)
        });
    }

    let key = format!("plugin_settings:{}", name);
    let json_str = serde_json::to_string(&settings).map_err(|e| {
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
        guard.insert(name.clone(), settings.clone());
    }
    // ほかのサーバーはメモリに載せた値を持っているので、キャッシュから読み直してもらう
    crate::cluster::announce(&state, crate::cluster::TOPIC_PLUGIN_SETTINGS, &name).await;
    // 設定はプラグインの出力（スクリプトの URL・ブロックの見た目など）を変えるので、CDN に古いページを残さない
    crate::cdn::purge_site_global(&state).await;

    state
        .audit
        .record(
            audit
                .event(AuditAction::PluginSettingsUpdate, AuditOutcome::Success)
                .with_resource("plugin", Some(name.clone()))
                .with_details(json!({ "keys": settings.keys().collect::<Vec<_>>() })),
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

    let actor = AuditActor {
        user_id: session.user_id,
        role: session.role,
    };
    install_plugin_package(&state, actor, &audit, &bytes, None)
        .await
        .map(Json)
        .map_err(|e| match e {
            InstallError::Invalid(m) => api_error(
                StatusCode::BAD_REQUEST,
                format!("Package validation failed: {}", m),
            ),
            InstallError::Internal(m) => api_error(StatusCode::INTERNAL_SERVER_ERROR, m),
        })
}

/// プラグインの ZIP を検証して `plugins_dir/<name>/` に置く（アップロードとストアで共通）。
/// `from_store` があれば、カタログのエントリと中身（ID・バージョン・署名の鍵・`capabilities`）が
/// 一致することも確かめ、監査ログに取得元を残す。有効化はしない
pub(crate) async fn install_plugin_package(
    state: &AppState,
    actor: AuditActor,
    audit: &AuditContext,
    bytes: &[u8],
    from_store: Option<&crate::store::StoreSource<'_>>,
) -> Result<PluginInfo, InstallError> {
    let temp_root = std::env::temp_dir().join(format!(
        "aegis_plugin_inst_{}_{}",
        std::process::id(),
        uuid::Uuid::new_v4()
    ));
    let _ = std::fs::remove_dir_all(&temp_root);
    std::fs::create_dir_all(&temp_root)
        .map_err(|e| InstallError::Internal(format!("Failed to create temp dir: {}", e)))?;

    let res = (|| -> aegis_core::Result<(aegis_core::PluginManifest, std::path::PathBuf)> {
        aegis_core::extract_zip_safely(bytes, &temp_root, None)?;
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
        if let Some(source) = from_store {
            source.check_package(&pkg_root)?;
        }

        Ok((manifest, pkg_root))
    })();

    let (manifest, pkg_root) = match res {
        Ok(val) => val,
        Err(e) => {
            let _ = std::fs::remove_dir_all(&temp_root);
            return Err(InstallError::Invalid(e.to_string()));
        }
    };

    let plugins_dir = &state.config.paths.plugins_dir;
    let target_dir = plugins_dir.join(&manifest.name);

    let placed = (|| -> std::result::Result<(), String> {
        if target_dir.exists() {
            std::fs::remove_dir_all(&target_dir)
                .map_err(|e| format!("Failed to remove existing plugin directory: {}", e))?;
        } else {
            std::fs::create_dir_all(plugins_dir)
                .map_err(|e| format!("Failed to create plugins dir: {}", e))?;
        }
        copy_dir_all(&pkg_root, &target_dir)
            .map_err(|e| format!("Failed to install plugin files: {}", e))
    })();
    let _ = std::fs::remove_dir_all(&temp_root);
    placed.map_err(InstallError::Internal)?;

    let mut details = json!({
        "version": manifest.version,
        "display_name": manifest.display_name,
    });
    if let Some(source) = from_store {
        details["source"] = source.audit_details();
    }
    state
        .audit
        .record(
            audit
                .event(AuditAction::PluginInstall, AuditOutcome::Success)
                .with_actor(Some(actor))
                .with_resource("plugin", Some(manifest.name.clone()))
                .with_details(details),
        )
        .await;

    let enabled = state.plugin_registry.enabled().await;
    let listed = plugin_list(state, &enabled)
        .await
        .ok()
        .and_then(|list| list.into_iter().find(|p| p.name == manifest.name));
    Ok(listed.unwrap_or(PluginInfo {
        name: manifest.name.clone(),
        is_enabled: enabled.contains(&manifest.name),
        is_installed: true,
        problem: None,
        manifest: Some(manifest),
        settings: Default::default(),
    }))
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

/// プラグインのアセットとして配信する拡張子（小文字）。ブラウザーがスクリプトを動かす文書（HTML・XHTML・
/// XML・SVG など）は、サイトと同じオリジンで開かれるとプラグインの外でスクリプトが動くため配信しない
const PLUGIN_ASSET_EXTENSIONS: &[&str] = &[
    "js", "mjs", "css", "map", "json", "wasm", "woff", "woff2", "ttf", "otf", "png", "jpg", "jpeg",
    "gif", "webp", "avif", "ico",
];

/// プラグインのアセットとして配信してよいファイル名か（拡張子が [`PLUGIN_ASSET_EXTENSIONS`] のどれか）
fn is_servable_plugin_asset(rel_path: &str) -> bool {
    std::path::Path::new(rel_path)
        .extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| PLUGIN_ASSET_EXTENSIONS.contains(&e.to_ascii_lowercase().as_str()))
}

/// `GET /plugins/{name}/assets/{*path}` — 有効化されているプラグインの `assets/` 配下の静的ファイルを配信する。
///
/// React / Vue などでビルドしたアイランド用 JS・CSS をプラグイン自身に同梱して、同一オリジン（Strict CSP
/// の `script-src 'self'` / `style-src 'self'` 適合）で配信できるようにするためのエンドポイント。
/// 無効・未ロードのプラグインや `assets/` 外へのパストラバーサル、ドットファイル、
/// [`PLUGIN_ASSET_EXTENSIONS`] に無い種類（HTML・SVG など）は拒否する。
pub async fn get_plugin_asset(
    State(state): State<Arc<AppState>>,
    Path((plugin_name, rel_path)): Path<(String, String)>,
) -> axum::response::Response {
    if plugin_name.is_empty()
        || plugin_name.contains('/')
        || plugin_name.contains('\\')
        || plugin_name.contains("..")
    {
        return (StatusCode::BAD_REQUEST, "Invalid plugin name").into_response();
    }

    // 有効化・ロード済みのプラグインのみ配信する
    let is_enabled = {
        let supervisor = state.supervisor.read().await;
        supervisor
            .plugins()
            .iter()
            .any(|p| p.manifest().name == plugin_name)
    };
    if !is_enabled {
        return (StatusCode::NOT_FOUND, "Plugin asset not found").into_response();
    }

    if !crate::handlers::widget::is_safe_asset_path(&rel_path) {
        return (StatusCode::BAD_REQUEST, "Invalid asset path").into_response();
    }
    if !is_servable_plugin_asset(&rel_path) {
        return (StatusCode::NOT_FOUND, "Plugin asset not found").into_response();
    }
    let clean_path = std::path::Path::new(&rel_path);

    // インストール先 → 同梱の順に、そのプラグインのディレクトリを探す
    let Some(plugin_dir) = state
        .config
        .paths
        .plugin_dirs()
        .into_iter()
        .map(|dir| dir.join(&plugin_name))
        .find(|dir| dir.is_dir())
    else {
        return (StatusCode::NOT_FOUND, "Plugin assets directory not found").into_response();
    };
    let assets_dir = plugin_dir.join("assets");
    let Ok(canonical_assets_dir) = std::fs::canonicalize(&assets_dir) else {
        return (StatusCode::NOT_FOUND, "Plugin assets directory not found").into_response();
    };
    let target = assets_dir.join(clean_path);
    let Ok(canonical_target) = std::fs::canonicalize(&target) else {
        return (StatusCode::NOT_FOUND, "Plugin asset not found").into_response();
    };
    // シンボリックリンクによる assets/ 外への脱出を防止
    if !canonical_target.starts_with(&canonical_assets_dir) || !canonical_target.is_file() {
        return (StatusCode::NOT_FOUND, "Plugin asset not found").into_response();
    }

    match std::fs::read(&canonical_target) {
        Ok(bytes) => {
            let mime = aegis_core::guess_content_type(
                canonical_target
                    .file_name()
                    .and_then(|s| s.to_str())
                    .unwrap_or(""),
            );
            let resp = (
                [
                    (header::CONTENT_TYPE, mime),
                    (header::CACHE_CONTROL, "public, max-age=3600"),
                    (header::X_CONTENT_TYPE_OPTIONS, "nosniff"),
                ],
                bytes,
            )
                .into_response();
            let mut tags = aegis_cache::CacheTags::new();
            tags.add_url(&format!("/plugins/{}/assets/{}", plugin_name, rel_path));
            tags.add_tag("site-global");
            tags.add_dependency("plugin", &plugin_name);
            crate::cdn::apply_cache_tags(&state, resp, &tags)
        }
        Err(_) => (StatusCode::INTERNAL_SERVER_ERROR, "Failed to read asset").into_response(),
    }
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

    struct StubPlugin {
        manifest: aegis_core::PluginManifest,
    }

    #[async_trait::async_trait]
    impl aegis_runtime::AegisPlugin for StubPlugin {
        fn manifest(&self) -> &aegis_core::PluginManifest {
            &self.manifest
        }
    }

    #[tokio::test]
    async fn test_get_plugin_asset_serves_enabled_and_blocks_traversal() {
        let dir =
            std::env::temp_dir().join(format!("aegis_plugin_asset_test_{}", uuid::Uuid::new_v4()));
        let enabled_assets = dir.join("my-island").join("assets");
        let nested = enabled_assets.join("chunks");
        let disabled_assets = dir.join("disabled-island").join("assets");
        std::fs::create_dir_all(&nested).unwrap();
        std::fs::create_dir_all(&disabled_assets).unwrap();
        std::fs::write(enabled_assets.join("island.js"), "console.log('hi');").unwrap();
        std::fs::write(nested.join("style.css"), ".card{color:red}").unwrap();
        std::fs::write(dir.join("my-island").join("secret.txt"), "top-secret").unwrap();
        std::fs::write(disabled_assets.join("island.js"), "disabled").unwrap();

        let mut config = AegisConfig::default();
        config.paths.plugins_dir = dir.clone();

        let mut supervisor = PluginSupervisor::new();
        supervisor.register(Arc::new(StubPlugin {
            manifest: aegis_core::PluginManifest {
                name: "my-island".to_string(),
                ..Default::default()
            },
        }));
        let state = test_state_with_config(config, supervisor).await;

        // 1. 有効なプラグインの JS が正しい MIME とヘッダーで返る
        let resp = get_plugin_asset(
            State(state.clone()),
            Path(("my-island".to_string(), "island.js".to_string())),
        )
        .await;
        assert_eq!(resp.status(), StatusCode::OK);
        assert_eq!(
            resp.headers().get(header::CONTENT_TYPE).unwrap(),
            "application/javascript"
        );
        assert_eq!(
            resp.headers().get(header::X_CONTENT_TYPE_OPTIONS).unwrap(),
            "nosniff"
        );
        let body = axum::body::to_bytes(resp.into_body(), 1 << 16)
            .await
            .unwrap();
        assert_eq!(&body[..], b"console.log('hi');");

        // 2. サブディレクトリの CSS も配信できる
        let resp = get_plugin_asset(
            State(state.clone()),
            Path(("my-island".to_string(), "chunks/style.css".to_string())),
        )
        .await;
        assert_eq!(resp.status(), StatusCode::OK);
        assert_eq!(
            resp.headers().get(header::CONTENT_TYPE).unwrap(),
            "text/css"
        );

        // 3. 無効（未ロード）なプラグインは 404
        let resp = get_plugin_asset(
            State(state.clone()),
            Path(("disabled-island".to_string(), "island.js".to_string())),
        )
        .await;
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);

        // 4. パストラバーサルは 400 でブロックされる
        let resp = get_plugin_asset(
            State(state.clone()),
            Path(("my-island".to_string(), "../secret.txt".to_string())),
        )
        .await;
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);

        let resp = get_plugin_asset(
            State(state.clone()),
            Path(("../my-island".to_string(), "island.js".to_string())),
        )
        .await;
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);

        // 5. スクリプトが動く文書（HTML・SVG・XML）は、置いてあっても配信しない
        for (file, body) in [
            ("evil.html", "<script>alert(1)</script>"),
            ("evil.HTM", "<script>alert(1)</script>"),
            ("evil.svg", "<svg onload=alert(1)></svg>"),
            ("evil.xhtml", "<html/>"),
            ("evil.xml", "<x/>"),
            ("no-extension", "x"),
        ] {
            std::fs::write(enabled_assets.join(file), body).unwrap();
            let resp = get_plugin_asset(
                State(state.clone()),
                Path(("my-island".to_string(), file.to_string())),
            )
            .await;
            assert_eq!(resp.status(), StatusCode::NOT_FOUND, "{file}");
        }
        // 6. ドットファイルは配信しない
        std::fs::write(enabled_assets.join(".env.js"), "x").unwrap();
        let resp = get_plugin_asset(
            State(state.clone()),
            Path(("my-island".to_string(), ".env.js".to_string())),
        )
        .await;
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);

        let _ = std::fs::remove_dir_all(&dir);
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

    /// The repository's `test-plugins/` (test plugins with their built Wasm), switchable at runtime
    async fn switchable_state(
        enabled: &[&str],
        configure: impl FnOnce(&mut AegisConfig),
    ) -> Arc<AppState> {
        switchable_server(enabled, configure, None, None).await
    }

    /// [`switchable_state`] on the database, cache and event bus of `shared` (another server of the site)
    async fn switchable_server(
        enabled: &[&str],
        configure: impl FnOnce(&mut AegisConfig),
        shared: Option<&Shared>,
        cdn: Option<Arc<dyn aegis_cache::CdnPurger>>,
    ) -> Arc<AppState> {
        let plugins_dir =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../test-plugins");
        let mut config = AegisConfig::default();
        config.paths.plugins_dir = plugins_dir.clone();
        configure(&mut config);
        let enabled: Vec<String> = enabled.iter().map(|n| n.to_string()).collect();
        config.plugins.enabled = enabled.clone();
        let loader = crate::plugin_registry::PluginLoader::new(
            aegis_runtime::WasmEngine::new().unwrap(),
            Arc::new(std::sync::Mutex::new(HashMap::new())).into(),
            vec![plugins_dir],
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
        if let Some(cdn) = cdn {
            state = state.with_cdn(cdn);
        }
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
        let second = switchable_server(enabled, |_| {}, Some(&shared), None).await;
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
    async fn test_switching_plugins_purges_the_cdn() {
        let purger = Arc::new(aegis_cache::RecordingCdnPurger::new());
        let state = switchable_server(
            &["test-page"],
            |config| config.cdn.enabled = true,
            None,
            Some(purger.clone()),
        )
        .await;
        set_enabled(&state, "test-page", false).await.unwrap();
        let tags: Vec<String> = purger.take_batches().into_iter().flatten().collect();
        assert_eq!(tags, ["site-global"]);
        // 変わらないときはパージしない
        set_enabled(&state, "test-page", false).await.unwrap();
        assert!(purger.take_batches().is_empty());
    }

    #[tokio::test]
    async fn test_enable_and_disable_plugins_without_restart() {
        let state = switchable_state(&["test-page"], |_| {}).await;
        let enhancer = state.supervisor.read().await.plugins()[0].clone();

        // Enabling appends to the hook order and keeps the loaded instance
        let list = set_enabled(&state, "test-shop", true).await.unwrap();
        let enabled: Vec<&str> = list
            .iter()
            .filter(|p| p.is_enabled)
            .map(|p| p.name.as_str())
            .collect();
        assert_eq!(enabled, vec!["test-page", "test-shop"]);
        assert_eq!(loaded_names(&state).await, vec!["test-page", "test-shop"]);
        assert!(Arc::ptr_eq(
            &enhancer,
            &state.supervisor.read().await.plugins()[0]
        ));
        assert_eq!(
            saved_list(&state).await.as_deref(),
            Some(r#"["test-page","test-shop"]"#)
        );

        // Enabling again changes nothing
        set_enabled(&state, "test-shop", true).await.unwrap();
        assert_eq!(loaded_names(&state).await.len(), 2);

        let list = set_enabled(&state, "test-page", false).await.unwrap();
        assert!(
            !list
                .iter()
                .find(|p| p.name == "test-page")
                .unwrap()
                .is_enabled
        );
        assert_eq!(loaded_names(&state).await, vec!["test-shop"]);
        assert_eq!(state.plugin_registry.enabled().await, vec!["test-shop"]);
        assert_eq!(
            saved_list(&state).await.as_deref(),
            Some(r#"["test-shop"]"#)
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
        assert_eq!(loaded_names(&state).await, vec!["test-shop"]);
        assert_eq!(
            saved_list(&state).await.as_deref(),
            Some(r#"["test-shop"]"#)
        );
    }

    #[tokio::test]
    async fn test_plugin_switch_is_refused_when_it_cannot_apply() {
        // A plugin used as a notification channel cannot be disabled
        let state = switchable_state(&["test-notify"], |config| {
            config.notifications.enabled = true;
            config.notifications.channels =
                vec![aegis_notify::ChannelKind::Plugin("test-notify".into())];
        })
        .await;
        assert_eq!(
            set_enabled(&state, "test-notify", false).await.unwrap_err(),
            StatusCode::CONFLICT
        );
        assert_eq!(loaded_names(&state).await, vec!["test-notify"]);

        // Without the Wasm runtime ([runtime].enable_wasm_plugins = false) there is nothing to switch
        let state = test_state_with_config(AegisConfig::default(), PluginSupervisor::new()).await;
        assert_eq!(
            set_enabled(&state, "test-shop", true).await.unwrap_err(),
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
        let state = switchable_state(&["test-page", "test-shop"], |_| {}).await;
        let Json(order) = get_plugin_order(State(state.clone()), admin())
            .await
            .unwrap();
        assert_eq!(order.enabled, order.configured);
        assert!(!order.customized);

        // The new order must name exactly the enabled plugins
        for wrong in [
            &["test-shop"][..],
            &["test-shop", "test-shop"],
            &["test-shop", "test-page", "test-notify"],
        ] {
            assert_eq!(
                set_order(&state, wrong).await.unwrap_err(),
                StatusCode::BAD_REQUEST
            );
        }
        // The same order changes nothing
        let order = set_order(&state, &["test-page", "test-shop"])
            .await
            .unwrap();
        assert!(!order.customized);
        assert_eq!(saved_list(&state).await, None);

        let order = set_order(&state, &["test-shop", "test-page"])
            .await
            .unwrap();
        assert_eq!(order.enabled, vec!["test-shop", "test-page"]);
        assert!(order.customized);
        assert_eq!(loaded_names(&state).await, vec!["test-shop", "test-page"]);
        assert_eq!(
            saved_list(&state).await.as_deref(),
            Some(r#"["test-shop","test-page"]"#)
        );

        // Resetting drops the saved list: enabled plugins and order come from [plugins].enabled
        set_enabled(&state, "test-notify", true).await.unwrap();
        let order = reset_order(&state).await.unwrap();
        assert_eq!(order.enabled, vec!["test-page", "test-shop"]);
        assert!(!order.customized);
        assert_eq!(saved_list(&state).await, None);
        assert_eq!(loaded_names(&state).await, vec!["test-page", "test-shop"]);
    }

    #[tokio::test]
    async fn test_switches_reach_the_other_servers() {
        let (first, second) = two_servers(&["test-page"]).await;
        // Each server listens from startup (see cluster::spawn_listener)
        let mut on_first = first.bus.subscribe();
        let mut on_second = second.bus.subscribe();
        async fn deliver(state: &AppState, subscription: &mut aegis_cache::BusSubscription) {
            let message = subscription.recv().await.unwrap();
            crate::cluster::handle(state, message).await;
        }

        set_enabled(&first, "test-shop", true).await.unwrap();
        deliver(&second, &mut on_second).await;
        assert_eq!(loaded_names(&second).await, vec!["test-page", "test-shop"]);
        // The server that switched gets its own message too: it already has the list
        let before = first.supervisor.read().await.plugins()[1].clone();
        deliver(&first, &mut on_first).await;
        assert!(Arc::ptr_eq(
            &before,
            &first.supervisor.read().await.plugins()[1]
        ));

        set_order(&second, &["test-shop", "test-page"])
            .await
            .unwrap();
        deliver(&first, &mut on_first).await;
        deliver(&second, &mut on_second).await;
        assert_eq!(loaded_names(&first).await, vec!["test-shop", "test-page"]);
        assert_eq!(
            first.plugin_registry.enabled().await,
            second.plugin_registry.enabled().await
        );

        // Settings saved on one server replace what the other holds in memory
        get_effective_plugin_settings(&second, "test-shop").await;
        let _ = update_plugin_settings(
            State(first.clone()),
            admin(),
            AuditContext::default(),
            Path("test-shop".to_string()),
            Json(UpdatePluginSettingsRequest {
                settings: HashMap::from([("shipping_fee".to_string(), "900".to_string())]),
            }),
        )
        .await
        .unwrap();
        deliver(&first, &mut on_first).await;
        deliver(&second, &mut on_second).await;
        assert_eq!(
            get_effective_plugin_settings(&second, "test-shop")
                .await
                .get("shipping_fee")
                .map(String::as_str),
            Some("900")
        );

        // A missed message (e.g. Redis was unreachable) is caught up by the resync after reconnecting
        reset_order(&first).await.unwrap();
        crate::cluster::handle(&second, aegis_cache::BusMessage::Resync).await;
        assert_eq!(loaded_names(&second).await, vec!["test-page"]);
    }

    /// Settings a plugin declares in its manifest (`settings`) are checked on save; keys it does not
    /// declare are stored as they are
    #[tokio::test]
    async fn test_declared_plugin_settings_are_validated() {
        let mut config = AegisConfig::default();
        config.paths.plugins_dir =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../test-plugins");
        let state = test_state_with_config(config, PluginSupervisor::new()).await;
        let put = |pairs: &[(&str, &str)]| {
            let state = state.clone();
            let settings: HashMap<String, String> = pairs
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect();
            async move {
                update_plugin_settings(
                    State(state),
                    admin(),
                    AuditContext::default(),
                    Path("test-page".to_string()),
                    Json(UpdatePluginSettingsRequest { settings }),
                )
                .await
            }
        };

        // Wrong types and ranges are refused, with the reason per key
        let (status, Json(body)) = put(&[
            ("top_offset_px", "9999"),
            ("position", "top"),
            ("accent_color", "red;} body{display:none"),
            ("show_marker", "true"),
        ])
        .await
        .unwrap_err();
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert_eq!(body["code"], "invalid_setting");
        let keys: Vec<&str> = body["errors"]
            .as_array()
            .unwrap()
            .iter()
            .map(|e| e["key"].as_str().unwrap())
            .collect();
        assert_eq!(keys, ["accent_color", "position", "top_offset_px"]);
        assert!(body["errors"][2]["message"]
            .as_str()
            .unwrap()
            .contains("400 or less"));
        // Nothing was saved
        let Json(saved) =
            get_plugin_settings(State(state.clone()), admin(), Path("test-page".to_string()))
                .await
                .unwrap();
        assert_eq!(saved["settings"], json!({}));

        // Valid values are saved; an empty declared value means "unset" and is dropped;
        // undeclared keys are kept
        let Json(ok) = put(&[
            ("top_offset_px", "64"),
            ("position", "left"),
            ("accent_color", "#e11d48"),
            ("title", ""),
            ("my_custom_key", "anything"),
        ])
        .await
        .unwrap();
        assert_eq!(
            ok["settings"],
            json!({
                "top_offset_px": "64",
                "position": "left",
                "accent_color": "#e11d48",
                "my_custom_key": "anything"
            })
        );
    }

    #[tokio::test]
    async fn test_plugin_settings_get_and_update() {
        let state = test_state_with_config(AegisConfig::default(), PluginSupervisor::new()).await;

        // 1. Initial settings (empty)
        let Json(res) =
            get_plugin_settings(State(state.clone()), admin(), Path("test-shop".to_string()))
                .await
                .unwrap();
        assert_eq!(res["name"], "test-shop");
        assert_eq!(res["settings"], json!({}));

        // 2. Update settings
        let mut new_settings = HashMap::new();
        new_settings.insert("shipping_fee".to_string(), "700".to_string());
        new_settings.insert("coupons".to_string(), "SALE20:20%".to_string());

        let Json(updated) = update_plugin_settings(
            State(state.clone()),
            admin(),
            AuditContext::default(),
            Path("test-shop".to_string()),
            Json(UpdatePluginSettingsRequest {
                settings: new_settings.clone(),
            }),
        )
        .await
        .unwrap();

        assert_eq!(updated["name"], "test-shop");
        assert_eq!(updated["settings"]["shipping_fee"], "700");
        assert_eq!(updated["settings"]["coupons"], "SALE20:20%");

        // 3. Fetch again to verify persistence in cache & state
        let Json(fetched) =
            get_plugin_settings(State(state.clone()), admin(), Path("test-shop".to_string()))
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
