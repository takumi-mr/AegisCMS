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
    /// インストール済みのバージョン（未インストールなら省略）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub installed_version: Option<String>,
    /// このサーバーで動く最も新しいバージョン
    #[serde(skip_serializing_if = "Option::is_none")]
    pub latest_compatible: Option<String>,
    /// インストール済みで、`latest_compatible` の方が新しい
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

    let installed = installed_versions(&state);
    let mut catalogs = Vec::new();
    let mut packages = Vec::new();
    for catalog_config in &config.catalogs {
        let status =
            match store::load_catalog(&client, state.db.as_ref(), config, catalog_config).await {
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
fn installed_versions(state: &AppState) -> HashMap<(PackageKind, String), String> {
    let mut installed = HashMap::new();
    match aegis_runtime::WasmPluginLoader::discover(
        &state.config.paths.plugins_dir,
        &state.config.runtime.default_resource_limits(),
    ) {
        Ok(plugins) => {
            for plugin in plugins {
                if let Some(manifest) = plugin.manifest {
                    installed.insert((PackageKind::Plugin, plugin.dir_name), manifest.version);
                }
            }
        }
        Err(e) => tracing::warn!("Failed to list installed plugins for the store: {}", e),
    }
    for theme in state.theme.list_installed_themes() {
        installed.insert(
            (PackageKind::Theme, theme.manifest.id),
            theme.manifest.version,
        );
    }
    installed
}

fn store_package(
    catalog: &str,
    package: CatalogPackage,
    installed: &HashMap<(PackageKind, String), String>,
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
    let installed_version = installed.get(&(package.kind, package.id.clone())).cloned();
    let update_available = match (&installed_version, &latest) {
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
        installed_version,
        latest_compatible: latest.map(|v| v.to_string()),
        update_available,
        versions,
    }
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

    fn package(versions: Vec<CatalogVersion>) -> CatalogPackage {
        CatalogPackage {
            id: "seo-tools".into(),
            kind: PackageKind::Plugin,
            name: "SEO Tools".into(),
            description: String::new(),
            publisher: "official".into(),
            license: None,
            versions,
        }
    }

    #[test]
    fn picks_the_newest_compatible_version_and_flags_updates() {
        let installed = HashMap::from([(
            (PackageKind::Plugin, "seo-tools".to_string()),
            "1.0.0".to_string(),
        )]);
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
        assert!(p.update_available);
    }

    #[test]
    fn no_update_when_current_or_not_installed() {
        let installed = HashMap::from([(
            (PackageKind::Plugin, "seo-tools".to_string()),
            "1.2.0".to_string(),
        )]);
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

        let (status, body) = get(crate::config::AegisConfig::default(), "admin").await;
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
        let (status, res) = post(crate::config::AegisConfig::default(), body.clone()).await;
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
}
