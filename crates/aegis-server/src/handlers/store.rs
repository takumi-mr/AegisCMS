//! パッケージストアの API（docs/store.md）

use std::collections::HashMap;
use std::sync::Arc;

use axum::extract::State;
use axum::http::StatusCode;
use axum::Json;
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::audit::AuditContext;
use crate::auth::authorize::RequireRole;
use crate::auth::AdminSession;
use crate::state::AppState;
use crate::store::{
    self, CatalogPackage, CatalogVersion, InstallError, PackageFetchError, PackageKind, StoreSource,
};

type ApiError = (StatusCode, Json<serde_json::Value>);

fn api_error(status: StatusCode, code: &str, message: impl Into<String>) -> ApiError {
    (
        status,
        Json(json!({ "error": message.into(), "code": code })),
    )
}

fn store_disabled() -> ApiError {
    api_error(
        StatusCode::NOT_FOUND,
        "store_disabled",
        "The package store is disabled",
    )
}

/// カタログごとの取得結果
#[derive(Debug, Serialize)]
pub struct CatalogStatus {
    pub name: String,
    pub url: String,
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sequence: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub generated_at: Option<jiff::Timestamp>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<jiff::Timestamp>,
    /// 失敗したときの機械可読な理由（`catalog_bad_signature` など）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub code: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct StoreVersion {
    #[serde(flatten)]
    pub version: CatalogVersion,
    /// このサーバーで動くか（`min_aegis_version`）
    pub compatible: bool,
}

#[derive(Debug, Serialize)]
pub struct StorePackage {
    /// 載っていたカタログの名前
    pub catalog: String,
    pub id: String,
    pub kind: PackageKind,
    pub name: String,
    pub description: String,
    pub publisher: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub license: Option<String>,
    /// カタログでおすすめとされている
    pub recommended: bool,
    /// インストール済みのバージョン（未インストールなら省略）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub installed_version: Option<String>,
    /// このサーバーで動く最も新しいバージョン
    #[serde(skip_serializing_if = "Option::is_none")]
    pub latest_compatible: Option<String>,
    /// インストール済みのものが、このカタログの鍵で署名されている（同じ出どころ）。違えば更新を知らせない
    /// （同じ ID の自作の改造版などを、公式版で置き換えないように）
    pub installed_from_catalog: bool,
    /// インストール済みのものの `capabilities`（テーマは `{ csp }`）。更新の前に、許可されることの差分を出す
    #[serde(skip_serializing_if = "Option::is_none")]
    pub installed_capabilities: Option<serde_json::Value>,
    /// インストール済みで、このカタログから入れたもので、`latest_compatible` の方が新しい
    pub update_available: bool,
    /// 新しい順
    pub versions: Vec<StoreVersion>,
}

#[derive(Debug, Serialize)]
pub struct StorePackagesResponse {
    pub catalogs: Vec<CatalogStatus>,
    pub packages: Vec<StorePackage>,
}

/// `GET /api/v1/store/packages` — 設定したカタログを取得・検証し、パッケージの一覧を返す（admin 権限必須）。
/// 取得や検証に失敗したカタログは `catalogs` に理由を出し、ほかのカタログの分は返す。
/// `[store].enabled = false` なら `404 store_disabled`
pub async fn list_store_packages(
    State(state): State<Arc<AppState>>,
    RequireRole(_session): AdminSession,
) -> Result<Json<StorePackagesResponse>, ApiError> {
    let config = &state.config.store;
    if !config.enabled {
        return Err(store_disabled());
    }
    let client = client(config)?;

    let installed = installed_packages(&state);
    let mut catalogs = Vec::new();
    let mut packages = Vec::new();
    for catalog_config in &config.catalogs {
        let status = match store::load_and_cache_catalog(
            &client,
            state.db.as_ref(),
            state.cache.as_ref(),
            config,
            catalog_config,
        )
        .await
        {
            Ok(catalog) => {
                packages.extend(
                    catalog
                        .packages
                        .into_iter()
                        .map(|p| store_package(&catalog_config.name, p, &installed)),
                );
                CatalogStatus {
                    name: catalog_config.name.clone(),
                    url: catalog_config.url.clone(),
                    ok: true,
                    sequence: Some(catalog.sequence),
                    generated_at: Some(catalog.generated_at),
                    expires_at: Some(catalog.expires_at),
                    code: None,
                    error: None,
                }
            }
            Err(e) => {
                tracing::warn!("Store catalog '{}' rejected: {}", catalog_config.name, e);
                CatalogStatus {
                    name: catalog_config.name.clone(),
                    url: catalog_config.url.clone(),
                    ok: false,
                    sequence: None,
                    generated_at: None,
                    expires_at: None,
                    code: Some(e.code()),
                    error: Some(e.to_string()),
                }
            }
        };
        catalogs.push(status);
    }
    Ok(Json(StorePackagesResponse { catalogs, packages }))
}

fn client(config: &store::StoreConfig) -> Result<reqwest::Client, ApiError> {
    store::build_client(config).map_err(|e| {
        tracing::error!("Failed to build the store HTTP client: {}", e);
        api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "internal",
            "Failed to prepare the store client",
        )
    })
}

#[derive(Debug, Deserialize)]
pub struct StoreInstallRequest {
    /// カタログの名前（`[[store.catalogs]].name`）
    pub catalog: String,
    pub kind: PackageKind,
    pub id: String,
    /// 入れるバージョン。省略するとこのサーバーで動く最も新しいもの
    #[serde(default)]
    pub version: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct StoreInstallResponse {
    pub catalog: String,
    pub kind: PackageKind,
    pub id: String,
    pub version: String,
    /// インストールしたもの（プラグインは `GET /api/v1/plugins` の要素、テーマは `GET /api/v1/themes` の要素）
    pub installed: serde_json::Value,
}

/// `POST /api/v1/store/install` — カタログのパッケージを取得してインストールする（admin 権限必須）。
/// カタログを取得し直して検証し、ZIP の `size`・`sha256`、中身の ID・バージョン・署名の鍵・`capabilities` が
/// カタログと一致するときだけ入れる。同じ ID のものは置き換える（更新）。有効化はしない
pub async fn install_store_package(
    State(state): State<Arc<AppState>>,
    RequireRole(session): AdminSession,
    audit: AuditContext,
    Json(req): Json<StoreInstallRequest>,
) -> Result<Json<StoreInstallResponse>, ApiError> {
    let config = &state.config.store;
    if !config.enabled {
        return Err(store_disabled());
    }
    let catalog_config = config
        .catalogs
        .iter()
        .find(|c| c.name == req.catalog)
        .ok_or_else(|| {
            api_error(
                StatusCode::NOT_FOUND,
                "catalog_not_found",
                format!("No catalog named '{}'", req.catalog),
            )
        })?;
    if req.kind == PackageKind::Plugin && state.plugin_registry.loader().is_none() {
        return Err(api_error(
            StatusCode::CONFLICT,
            "plugins_disabled",
            "Wasm plugins are turned off on this server ([runtime].enable_wasm_plugins = false)",
        ));
    }

    let client = client(config)?;
    let catalog = store::load_catalog(&client, state.db.as_ref(), config, catalog_config)
        .await
        .map_err(|e| {
            tracing::warn!("Store catalog '{}' rejected: {}", catalog_config.name, e);
            api_error(StatusCode::BAD_GATEWAY, e.code(), e.to_string())
        })?;
    let package = catalog
        .packages
        .iter()
        .find(|p| p.kind == req.kind && p.id == req.id)
        .ok_or_else(|| {
            api_error(
                StatusCode::NOT_FOUND,
                "package_not_found",
                format!("'{}' is not in the catalog '{}'", req.id, req.catalog),
            )
        })?;
    let version = match &req.version {
        Some(wanted) => package
            .versions
            .iter()
            .find(|v| &v.version == wanted)
            .ok_or_else(|| {
                api_error(
                    StatusCode::NOT_FOUND,
                    "version_not_found",
                    format!("'{}' has no version {}", req.id, wanted),
                )
            })?,
        None => package.latest_compatible().ok_or_else(|| {
            api_error(
                StatusCode::CONFLICT,
                "incompatible_version",
                format!("No version of '{}' runs on this server", req.id),
            )
        })?,
    };
    if !version.is_compatible() {
        return Err(api_error(
            StatusCode::CONFLICT,
            "incompatible_version",
            format!(
                "'{}' {} needs AegisCMS {} or later",
                req.id,
                version.version,
                version.min_aegis_version.as_deref().unwrap_or("?")
            ),
        ));
    }

    let bytes = store::fetch_package(&client, version).await.map_err(|e| {
        tracing::warn!("Store package '{}' rejected: {}", req.id, e);
        let code = match e {
            PackageFetchError::Fetch(_) => "package_fetch_failed",
            PackageFetchError::Mismatch(_) => "package_mismatch",
        };
        api_error(StatusCode::BAD_GATEWAY, code, e.to_string())
    })?;

    let source = StoreSource {
        catalog: &catalog_config.name,
        kind: req.kind,
        id: &req.id,
        version,
    };
    let actor = aegis_core::AuditActor {
        user_id: session.user_id,
        role: session.role,
    };
    let installed = match req.kind {
        PackageKind::Plugin => crate::handlers::plugin::install_plugin_package(
            &state,
            actor,
            &audit,
            &bytes,
            Some(&source),
        )
        .await
        .map(|info| serde_json::to_value(info).unwrap_or_default()),
        PackageKind::Theme => crate::handlers::theme::install_theme_package(
            &state,
            actor,
            &audit,
            &bytes,
            Some(&source),
        )
        .await
        .map(|info| serde_json::to_value(info).unwrap_or_default()),
    }
    .map_err(|e| match e {
        InstallError::Invalid(m) => {
            tracing::warn!("Store package '{}' rejected: {}", req.id, m);
            api_error(
                StatusCode::BAD_GATEWAY,
                "package_invalid",
                format!("The package from the store is invalid: {}", m),
            )
        }
        InstallError::Internal(m) => api_error(StatusCode::INTERNAL_SERVER_ERROR, "internal", m),
    })?;

    Ok(Json(StoreInstallResponse {
        catalog: catalog_config.name.clone(),
        kind: req.kind,
        id: req.id,
        version: version.version.clone(),
        installed,
    }))
}

/// インストール済みのプラグイン・テーマのバージョン（`(種類, ID)` → バージョン）
/// インストール済みのパッケージ（一覧と更新の確認で、カタログと比べる）
#[derive(Debug, Clone, Default)]
pub(crate) struct InstalledPackage {
    pub version: String,
    /// `signature.json` の公開鍵（小文字の hex）。署名の無いものは `None`
    pub signing_key: Option<String>,
    /// 今の `capabilities`（テーマは `{ csp }`）
    pub capabilities: serde_json::Value,
}

fn installed_packages(state: &AppState) -> HashMap<(PackageKind, String), InstalledPackage> {
    let mut installed = HashMap::new();
    match aegis_runtime::WasmPluginLoader::discover_in(
        &state.config.paths.plugin_dirs(),
        &state.config.runtime.default_resource_limits(),
    ) {
        Ok(plugins) => {
            for plugin in plugins {
                if let Some(manifest) = plugin.manifest {
                    installed.insert(
                        (PackageKind::Plugin, plugin.dir_name),
                        InstalledPackage {
                            version: manifest.version,
                            signing_key: store::installed_signing_key(&plugin.path),
                            capabilities: serde_json::to_value(&manifest.capabilities)
                                .unwrap_or_default(),
                        },
                    );
                }
            }
        }
        Err(e) => tracing::warn!("Failed to list installed plugins for the store: {}", e),
    }
    for theme in state.theme.list_installed_themes() {
        let dir = state.theme.theme_dir(&theme.manifest.id);
        let capabilities = match &theme.manifest.csp {
            Some(csp) => json!({ "csp": csp }),
            None => json!({}),
        };
        installed.insert(
            (PackageKind::Theme, theme.manifest.id),
            InstalledPackage {
                version: theme.manifest.version,
                signing_key: store::installed_signing_key(&dir),
                capabilities,
            },
        );
    }
    installed
}

fn store_package(
    catalog: &str,
    package: CatalogPackage,
    installed: &HashMap<(PackageKind, String), InstalledPackage>,
) -> StorePackage {
    let mut versions: Vec<StoreVersion> = package
        .versions
        .into_iter()
        .map(|version| StoreVersion {
            compatible: version.is_compatible(),
            version,
        })
        .collect();
    // 形はカタログの検証で確かめてあるので、解析できないものは無い
    let semver_of = |v: &StoreVersion| semver::Version::parse(&v.version.version).ok();
    versions.sort_by_key(|v| std::cmp::Reverse(semver_of(v)));
    let latest = versions.iter().find(|v| v.compatible).and_then(semver_of);
    let current = installed.get(&(package.kind, package.id.clone()));
    let installed_version = current.map(|c| c.version.clone());
    // 同じ出どころ: インストール済みのものの署名の鍵が、このカタログのどれかの版の鍵と同じ
    let installed_from_catalog =
        current
            .and_then(|c| c.signing_key.as_deref())
            .is_some_and(|key| {
                versions
                    .iter()
                    .any(|v| v.version.public_key.eq_ignore_ascii_case(key))
            });
    let update_available = installed_from_catalog
        && match (&installed_version, &latest) {
            (Some(current), Some(latest)) => {
                semver::Version::parse(current.trim()).map_or(true, |current| *latest > current)
            }
            _ => false,
        };
    StorePackage {
        catalog: catalog.to_string(),
        id: package.id,
        kind: package.kind,
        name: package.name,
        description: package.description,
        publisher: package.publisher,
        license: package.license,
        recommended: package.recommended,
        installed_version,
        latest_compatible: latest.map(|v| v.to_string()),
        installed_from_catalog,
        installed_capabilities: current.map(|c| c.capabilities.clone()),
        update_available,
        versions,
    }
}

#[derive(Debug, Default, Deserialize)]
pub struct StoreUpdatesQuery {
    /// `true` で、確認の間隔によらず今すぐ取り直す（「今すぐ確認」）
    #[serde(default)]
    pub refresh: bool,
}

/// 更新のあるパッケージ
#[derive(Debug, Serialize)]
pub struct StoreUpdate {
    pub catalog: String,
    pub kind: PackageKind,
    pub id: String,
    pub name: String,
    pub installed_version: String,
    pub latest_version: String,
}

/// カタログごとの確認の結果
#[derive(Debug, Serialize)]
pub struct UpdateCheckStatus {
    pub name: String,
    /// 確かめたカタログで比べられた（今回取り直したか、取ってあったものか）
    pub ok: bool,
    /// 比べたカタログを取得した時刻（一度も取れていなければ省略）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub checked_at: Option<jiff::Timestamp>,
    /// 今回の取得に失敗した理由（`catalog_fetch_failed` など）。取ってあったものがあれば、それで比べている
    #[serde(skip_serializing_if = "Option::is_none")]
    pub code: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct StoreUpdatesResponse {
    /// 自動の確認が有効か（`[store].check_updates`）
    pub check_updates: bool,
    pub catalogs: Vec<UpdateCheckStatus>,
    pub updates: Vec<StoreUpdate>,
}

/// `GET /api/v1/store/updates` — インストール済みのパッケージに、このカタログから出た新しい版があるか（admin 権限必須）。
/// 取ってあるカタログ（キャッシュ。読むたびに署名・期限・巻き戻しを確かめ直す）と手元で比べ、インストール済みの
/// 一覧は外に送らない。`refresh=true`、または自動の確認が有効で前の取得から `update_check_hours` 経っていれば
/// 取り直す。知らせるのは、同じ鍵で署名されたもの（同じ出どころ）で、このサーバーで動く版だけ
pub async fn get_store_updates(
    State(state): State<Arc<AppState>>,
    RequireRole(_session): AdminSession,
    axum::extract::Query(query): axum::extract::Query<StoreUpdatesQuery>,
) -> Result<Json<StoreUpdatesResponse>, ApiError> {
    let config = &state.config.store;
    if !config.enabled {
        return Err(store_disabled());
    }
    // 取り直しは同時に 1 つだけ（ほかの管理者の画面が同じ時に開いても、取得は 1 回）
    let _guard = store::UPDATE_CHECK_LOCK.lock().await;
    let installed = installed_packages(&state);
    let interval = jiff::SignedDuration::from_hours(config.update_check_hours as i64);
    let mut client_cell: Option<reqwest::Client> = None;
    let mut catalogs = Vec::new();
    let mut updates = Vec::new();

    for catalog_config in &config.catalogs {
        let cached =
            store::cached_catalog(state.db.as_ref(), state.cache.as_ref(), catalog_config).await;
        let stale = cached
            .as_ref()
            .is_none_or(|(_, at)| jiff::Timestamp::now().duration_since(*at) >= interval);
        let mut failure = None;
        let (catalog, checked_at) = if query.refresh || (config.check_updates && stale) {
            let client = match &client_cell {
                Some(client) => client.clone(),
                None => {
                    let client = client(config)?;
                    client_cell = Some(client.clone());
                    client
                }
            };
            match store::load_and_cache_catalog(
                &client,
                state.db.as_ref(),
                state.cache.as_ref(),
                config,
                catalog_config,
            )
            .await
            {
                Ok(catalog) => (Some(catalog), Some(jiff::Timestamp::now())),
                Err(e) => {
                    tracing::warn!(
                        "Store update check: catalog '{}' rejected: {}",
                        catalog_config.name,
                        e
                    );
                    failure = Some((e.code(), e.to_string()));
                    match cached {
                        Some((catalog, at)) => (Some(catalog), Some(at)),
                        None => (None, None),
                    }
                }
            }
        } else {
            match cached {
                Some((catalog, at)) => (Some(catalog), Some(at)),
                None => (None, None),
            }
        };

        if let Some(catalog) = catalog {
            for package in catalog.packages {
                let p = store_package(&catalog_config.name, package, &installed);
                if let (true, Some(installed_version), Some(latest_version)) =
                    (p.update_available, p.installed_version, p.latest_compatible)
                {
                    updates.push(StoreUpdate {
                        catalog: p.catalog,
                        kind: p.kind,
                        id: p.id,
                        name: p.name,
                        installed_version,
                        latest_version,
                    });
                }
            }
        }
        catalogs.push(UpdateCheckStatus {
            name: catalog_config.name.clone(),
            ok: checked_at.is_some(),
            checked_at,
            code: failure.as_ref().map(|(code, _)| *code),
            error: failure.map(|(_, message)| message),
        });
    }
    Ok(Json(StoreUpdatesResponse {
        check_updates: config.check_updates,
        catalogs,
        updates,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn version(v: &str, min: Option<&str>) -> CatalogVersion {
        CatalogVersion {
            version: v.into(),
            min_aegis_version: min.map(Into::into),
            capabilities: serde_json::Value::Null,
            url: format!("https://example.com/{v}.zip"),
            sha256: "a".repeat(64),
            size: 1,
            public_key: "b".repeat(64),
        }
    }

    /// インストール済みの `seo-tools`（`key` で署名）
    fn installed(
        version: &str,
        key: Option<&str>,
    ) -> HashMap<(PackageKind, String), InstalledPackage> {
        HashMap::from([(
            (PackageKind::Plugin, "seo-tools".to_string()),
            InstalledPackage {
                version: version.to_string(),
                signing_key: key.map(str::to_string),
                capabilities: json!({ "hooks": ["on-render-page"] }),
            },
        )])
    }

    fn package(versions: Vec<CatalogVersion>) -> CatalogPackage {
        CatalogPackage {
            id: "seo-tools".into(),
            kind: PackageKind::Plugin,
            name: "SEO Tools".into(),
            description: String::new(),
            publisher: "official".into(),
            license: None,
            recommended: false,
            versions,
        }
    }

    #[test]
    fn picks_the_newest_compatible_version_and_flags_updates() {
        let installed = installed("1.0.0", Some(&"b".repeat(64)));
        let p = store_package(
            "official",
            package(vec![
                version("1.0.0", None),
                version("9.0.0", Some("999.0.0")),
                version("1.10.0", None),
                version("1.2.0", None),
            ]),
            &installed,
        );
        let order: Vec<_> = p
            .versions
            .iter()
            .map(|v| v.version.version.as_str())
            .collect();
        assert_eq!(order, ["9.0.0", "1.10.0", "1.2.0", "1.0.0"]);
        assert!(!p.versions[0].compatible);
        assert_eq!(p.latest_compatible.as_deref(), Some("1.10.0"));
        assert_eq!(p.installed_version.as_deref(), Some("1.0.0"));
        assert!(p.installed_from_catalog);
        assert_eq!(
            p.installed_capabilities,
            Some(json!({ "hooks": ["on-render-page"] }))
        );
        assert!(p.update_available);
    }

    /// 同じ ID でも、ほかの鍵で署名したもの（自作の改造版など）・署名の無いものには、更新を知らせない
    #[test]
    fn no_update_for_a_package_from_another_source() {
        let versions = || vec![version("1.0.0", None), version("2.0.0", None)];
        for key in [Some("c".repeat(64)), None] {
            let p = store_package(
                "official",
                package(versions()),
                &installed("1.0.0", key.as_deref()),
            );
            assert_eq!(p.installed_version.as_deref(), Some("1.0.0"));
            assert!(!p.installed_from_catalog);
            assert!(!p.update_available);
        }
        // The key is compared without regard to case
        let p = store_package(
            "official",
            package(versions()),
            &installed("1.0.0", Some(&"B".repeat(64))),
        );
        assert!(p.update_available);
    }

    #[test]
    fn no_update_when_current_or_not_installed() {
        let installed = installed("1.2.0", Some(&"b".repeat(64)));
        let current = store_package(
            "official",
            package(vec![version("1.2.0", None)]),
            &installed,
        );
        assert!(!current.update_available);
        let fresh = store_package(
            "official",
            package(vec![version("1.2.0", None)]),
            &HashMap::new(),
        );
        assert!(fresh.installed_version.is_none());
        assert!(!fresh.update_available);
    }

    #[tokio::test]
    async fn disabled_store_answers_404_and_failed_catalogs_are_reported() {
        use axum::body::Body;
        use axum::http::Request;
        use tower::ServiceExt;

        async fn get(
            config: crate::config::AegisConfig,
            role: &str,
        ) -> (StatusCode, serde_json::Value) {
            let state = Arc::new(
                crate::test_support::test_app_state(
                    config,
                    aegis_runtime::PluginSupervisor::new(),
                    vec![],
                )
                .await,
            );
            let token = state
                .sessions
                .create_session("u1", role, Default::default(), None)
                .await
                .unwrap()
                .id;
            let app = crate::app::router(&state.config.clone(), state);
            let res = app
                .oneshot(
                    Request::get("/api/v1/store/packages")
                        .header("authorization", format!("Bearer {token}"))
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();
            let status = res.status();
            let bytes = axum::body::to_bytes(res.into_body(), 1 << 20)
                .await
                .unwrap();
            (status, serde_json::from_slice(&bytes).unwrap_or_default())
        }

        let mut disabled = crate::config::AegisConfig::default();
        disabled.store.enabled = false;
        let (status, body) = get(disabled, "admin").await;
        assert_eq!(status, StatusCode::NOT_FOUND);
        assert_eq!(body["code"], "store_disabled");

        let mut config = crate::config::AegisConfig::default();
        config.store.enabled = true;
        // 内部のアドレスは接続する前に断る（実際の通信は起きない）
        config.store.catalogs = vec![store::StoreCatalogConfig {
            name: "internal".into(),
            url: "https://127.0.0.1/catalog.json".into(),
            public_key: "c".repeat(64),
        }];
        let (status, _) = get(config.clone(), "editor").await;
        assert_eq!(status, StatusCode::FORBIDDEN);
        let (status, body) = get(config, "admin").await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["catalogs"][0]["ok"], false);
        assert_eq!(body["catalogs"][0]["code"], "catalog_fetch_failed");
        assert_eq!(body["packages"], serde_json::json!([]));
    }

    #[tokio::test]
    async fn install_answers_with_codes_before_any_download() {
        use axum::body::Body;
        use axum::http::Request;
        use tower::ServiceExt;

        async fn post(
            config: crate::config::AegisConfig,
            body: serde_json::Value,
        ) -> (StatusCode, serde_json::Value) {
            let state = Arc::new(
                crate::test_support::test_app_state(
                    config,
                    aegis_runtime::PluginSupervisor::new(),
                    vec![],
                )
                .await,
            );
            let token = state
                .sessions
                .create_session("u1", "admin", Default::default(), None)
                .await
                .unwrap()
                .id;
            let app = crate::app::router(&state.config.clone(), state);
            let res = app
                .oneshot(
                    Request::post("/api/v1/store/install")
                        .header("authorization", format!("Bearer {token}"))
                        .header("content-type", "application/json")
                        .body(Body::from(body.to_string()))
                        .unwrap(),
                )
                .await
                .unwrap();
            let status = res.status();
            let bytes = axum::body::to_bytes(res.into_body(), 1 << 20)
                .await
                .unwrap();
            (status, serde_json::from_slice(&bytes).unwrap_or_default())
        }

        let body = serde_json::json!({ "catalog": "official", "kind": "theme", "id": "roastery" });
        let mut disabled = crate::config::AegisConfig::default();
        disabled.store.enabled = false;
        let (status, res) = post(disabled, body.clone()).await;
        assert_eq!(status, StatusCode::NOT_FOUND);
        assert_eq!(res["code"], "store_disabled");

        let mut config = crate::config::AegisConfig::default();
        config.store.enabled = true;
        let (status, res) = post(
            config.clone(),
            serde_json::json!({ "catalog": "nope", "kind": "theme", "id": "roastery" }),
        )
        .await;
        assert_eq!(status, StatusCode::NOT_FOUND);
        assert_eq!(res["code"], "catalog_not_found");

        // 内部のアドレスのカタログは接続する前に断る（実際の通信は起きない）
        config.store.catalogs = vec![store::StoreCatalogConfig {
            name: "official".into(),
            url: "https://127.0.0.1/catalog.json".into(),
            public_key: "c".repeat(64),
        }];
        let (status, res) = post(config, body).await;
        assert_eq!(status, StatusCode::BAD_GATEWAY);
        assert_eq!(res["code"], "catalog_fetch_failed");
    }

    #[tokio::test]
    async fn updates_compare_the_cached_catalog_with_installed_packages() {
        use axum::body::Body;
        use axum::http::Request;
        use ring::rand::SystemRandom;
        use ring::signature::{Ed25519KeyPair, KeyPair};
        use tower::ServiceExt;

        let pkcs8 = Ed25519KeyPair::generate_pkcs8(&SystemRandom::new()).unwrap();
        let catalog_key = Ed25519KeyPair::from_pkcs8(pkcs8.as_ref()).unwrap();
        let package_key = "d".repeat(64);
        let theme = |id: &str, versions: &[&str]| {
            json!({
                "id": id, "kind": "theme", "name": id, "publisher": "official",
                "versions": versions.iter().map(|v| json!({
                    "version": v, "capabilities": {},
                    "url": format!("https://example.com/{id}-{v}.zip"),
                    "sha256": "a".repeat(64), "size": 1, "public_key": package_key,
                })).collect::<Vec<_>>(),
            })
        };
        let body = serde_json::to_vec(&json!({
            "schema_version": 1, "sequence": 7,
            "generated_at": "2026-11-27T00:00:00Z", "expires_at": "2099-01-01T00:00:00Z",
            "packages": [
                theme("ours", &["1.0.0", "1.1.0"]),
                theme("forked", &["1.0.0", "1.1.0"]),
                theme("current", &["1.0.0"]),
            ],
        }))
        .unwrap();
        let signature = format!("{}\n", hex::encode(catalog_key.sign(&body).as_ref()));

        // Installed themes: two signed with the catalog's package key, one with another key
        let themes_dir = std::env::temp_dir().join(format!(
            "aegis_store_updates_{}_{}",
            std::process::id(),
            uuid::Uuid::new_v4()
        ));
        for (id, key) in [
            ("ours", &package_key),
            ("forked", &"e".repeat(64)),
            ("current", &package_key),
        ] {
            let dir = themes_dir.join(id);
            std::fs::create_dir_all(dir.join("templates")).unwrap();
            std::fs::write(
                dir.join("theme.json"),
                format!(r#"{{"id": "{id}", "name": "{id}", "version": "1.0.0"}}"#),
            )
            .unwrap();
            std::fs::write(
                dir.join("signature.json"),
                format!(r#"{{"algorithm": "ed25519", "public_key": "{key}", "signature": "00"}}"#),
            )
            .unwrap();
        }

        let mut config = crate::config::AegisConfig::default();
        config.store.check_updates = false;
        config.store.catalogs = vec![store::StoreCatalogConfig {
            name: "official".into(),
            // An internal address: a fetch is refused before connecting (no real request)
            url: "https://127.0.0.1/catalog.json".into(),
            public_key: hex::encode(catalog_key.public_key().as_ref()),
        }];
        let mut state = crate::test_support::test_app_state(
            config,
            aegis_runtime::PluginSupervisor::new(),
            vec![],
        )
        .await;
        state.theme = Arc::new(aegis_theme::ThemeEngine::new(&themes_dir, "default"));
        let state = Arc::new(state);
        let token = state
            .sessions
            .create_session("u1", "admin", Default::default(), None)
            .await
            .unwrap()
            .id;
        let get = |path: &'static str| {
            let app = crate::app::router(&state.config.clone(), state.clone());
            let token = token.clone();
            async move {
                let res = app
                    .oneshot(
                        Request::get(path)
                            .header("authorization", format!("Bearer {token}"))
                            .body(Body::empty())
                            .unwrap(),
                    )
                    .await
                    .unwrap();
                let status = res.status();
                let bytes = axum::body::to_bytes(res.into_body(), 1 << 20)
                    .await
                    .unwrap();
                (
                    status,
                    serde_json::from_slice::<serde_json::Value>(&bytes).unwrap(),
                )
            }
        };

        // Never checked, and automatic checks are off: nothing is fetched or reported
        let (status, res) = get("/api/v1/store/updates").await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(res["check_updates"], false);
        assert_eq!(res["catalogs"][0]["ok"], false);
        assert!(res["catalogs"][0].get("code").is_none(), "{res}");
        assert_eq!(res["updates"], json!([]));

        // A verified catalog in the cache (as the list or an earlier check leaves it)
        let checked_at = "2026-11-01T00:00:00Z";
        state
            .cache
            .set(
                "store:catalog:official",
                &json!({ "fetched_at": checked_at, "body": String::from_utf8(body.clone()).unwrap(), "signature": signature })
                    .to_string(),
                None,
            )
            .await
            .unwrap();
        let (_, res) = get("/api/v1/store/updates").await;
        assert_eq!(res["catalogs"][0]["ok"], true);
        assert_eq!(res["catalogs"][0]["checked_at"], checked_at);
        // Only the package signed with the catalog's key; not the fork, not the current one
        assert_eq!(
            res["updates"],
            json!([{
                "catalog": "official", "kind": "theme", "id": "ours", "name": "ours",
                "installed_version": "1.0.0", "latest_version": "1.1.0",
            }])
        );

        // "Check now": the fetch fails, so the reason is shown and the cached result is kept
        let (_, res) = get("/api/v1/store/updates?refresh=true").await;
        assert_eq!(res["catalogs"][0]["ok"], true);
        assert_eq!(res["catalogs"][0]["code"], "catalog_fetch_failed");
        assert_eq!(res["catalogs"][0]["checked_at"], checked_at);
        assert_eq!(res["updates"].as_array().unwrap().len(), 1);

        // A tampered cached catalog is not used
        state
            .cache
            .set(
                "store:catalog:official",
                &json!({ "fetched_at": checked_at, "body": String::from_utf8(body).unwrap().replace("1.1.0", "9.9.9"), "signature": signature })
                    .to_string(),
                None,
            )
            .await
            .unwrap();
        let (_, res) = get("/api/v1/store/updates").await;
        assert_eq!(res["catalogs"][0]["ok"], false);
        assert_eq!(res["updates"], json!([]));

        let _ = std::fs::remove_dir_all(&themes_dir);
    }
}
