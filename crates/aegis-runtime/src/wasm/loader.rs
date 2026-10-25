use crate::plugin::PluginSupervisor;
use crate::services::PluginServices;
use crate::wasm::engine::WasmEngine;
use crate::wasm::instance::WasmPluginInstance;
use aegis_core::{is_valid_plugin_name, AegisError, PluginManifest, PluginResourceLimits, Result};
use serde::Serialize;
use std::path::{Path, PathBuf};
use std::sync::Arc;

const MANIFEST_FILE: &str = "plugin.manifest.json";

/// `plugins_dir` 直下にインストールされているプラグイン（1 ディレクトリ = 1 プラグイン）。
/// 有効かどうか（`[plugins].enabled`）とは独立で、一覧表示や将来のストアからの管理に使う
#[derive(Debug, Clone, Serialize)]
pub struct InstalledPlugin {
    /// ディレクトリ名。正しくインストールされていればマニフェストの `name` と同じ
    pub dir_name: String,
    #[serde(skip)]
    pub path: PathBuf,
    /// 読み込めたマニフェスト（`problem` があっても読めていれば入る）
    pub manifest: Option<PluginManifest>,
    /// 有効にできない理由（マニフェストが無い・壊れている、名前とディレクトリ名の不一致など）
    pub problem: Option<String>,
}

/// インストール済み・有効化済みのプラグインの状態（`GET /api/v1/plugins`、`aegis plugin list`）
#[derive(Debug, Clone, Serialize)]
pub struct PluginInfo {
    /// プラグイン名（インストール先のディレクトリ名、`[plugins].enabled` の値）
    pub name: String,
    /// `[plugins].enabled` に含まれるか
    pub is_enabled: bool,
    /// `plugins_dir/<name>/` が存在するか
    pub is_installed: bool,
    /// 有効にできない理由（未インストール、マニフェストの不備など）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub problem: Option<String>,
    pub manifest: Option<PluginManifest>,
    /// プラグインの設定値
    #[serde(default, skip_serializing_if = "std::collections::HashMap::is_empty")]
    pub settings: std::collections::HashMap<String, String>,
}

pub struct WasmPluginLoader;

impl WasmPluginLoader {
    /// `plugins_dir` 直下のプラグインをディレクトリ名順に列挙する（Wasm はコンパイルしない）。
    /// 隠しディレクトリとファイルは無視し、`plugins_dir` が無ければ空を返す
    pub fn discover<P: AsRef<Path>>(
        plugins_dir: P,
        default_limits: &PluginResourceLimits,
    ) -> Result<Vec<InstalledPlugin>> {
        let dir = plugins_dir.as_ref();
        if !dir.exists() {
            return Ok(Vec::new());
        }
        let mut plugins = Vec::new();
        for entry in std::fs::read_dir(dir).map_err(AegisError::Io)? {
            let path = entry.map_err(AegisError::Io)?.path();
            let Some(dir_name) = path.file_name().and_then(|n| n.to_str()) else {
                continue;
            };
            if !path.is_dir() || dir_name.starts_with('.') {
                continue;
            }
            let dir_name = dir_name.to_string();
            let (manifest, problem) = match Self::read_manifest(&path, default_limits) {
                Ok(manifest) => {
                    let problem = Self::check_installation(&dir_name, &path, &manifest).err();
                    (Some(manifest), problem.map(|e| e.to_string()))
                }
                Err(e) => (None, Some(e.to_string())),
            };
            plugins.push(InstalledPlugin {
                dir_name,
                path,
                manifest,
                problem,
            });
        }
        plugins.sort_by(|a, b| a.dir_name.cmp(&b.dir_name));
        Ok(plugins)
    }

    /// 有効なプラグイン（`enabled` の順）→ 無効なインストール済みプラグイン（名前順）の順に状態を返す
    pub fn list<P: AsRef<Path>>(
        plugins_dir: P,
        enabled: &[String],
        default_limits: &PluginResourceLimits,
    ) -> Result<Vec<PluginInfo>> {
        let mut installed = Self::discover(plugins_dir, default_limits)?;
        let mut plugins = Vec::new();
        for name in enabled {
            plugins.push(match installed.iter().position(|p| &p.dir_name == name) {
                Some(i) => Self::info(installed.remove(i), true),
                None => PluginInfo {
                    name: name.clone(),
                    is_enabled: true,
                    is_installed: false,
                    problem: Some("Not installed".to_string()),
                    manifest: None,
                    settings: Default::default(),
                },
            });
        }
        plugins.extend(installed.into_iter().map(|p| Self::info(p, false)));
        Ok(plugins)
    }

    fn info(installed: InstalledPlugin, is_enabled: bool) -> PluginInfo {
        PluginInfo {
            name: installed.dir_name,
            is_enabled,
            is_installed: true,
            problem: installed.problem,
            manifest: installed.manifest,
            settings: Default::default(),
        }
    }

    /// Loads one plugin directory: reads the manifest, checks it is installed under its own
    /// name, compiles the Wasm and checks its imports against the host API. `default_limits`
    /// (the server's `[runtime].default_*`) fills the `resource_limits` fields its manifest omits.
    pub fn load_plugin<P: AsRef<Path>>(
        plugin_dir: P,
        engine: WasmEngine,
        services: PluginServices,
        default_limits: &PluginResourceLimits,
    ) -> Result<Arc<WasmPluginInstance>> {
        let dir = plugin_dir.as_ref();
        let manifest = Self::read_manifest(dir, default_limits)?;
        let dir_name = dir.file_name().and_then(|n| n.to_str()).unwrap_or_default();
        Self::check_installation(dir_name, dir, &manifest)?;

        let wasm_path = dir.join(&manifest.entrypoint);
        let wasm_bytes = std::fs::read(&wasm_path).map_err(AegisError::Io)?;
        let instance = WasmPluginInstance::from_bytes(engine, manifest, &wasm_bytes, services)?;
        Ok(Arc::new(instance))
    }

    /// `[plugins].enabled` のプラグインを、その順に読み込んで `supervisor` に登録する。
    /// 有効にしたプラグインが見つからない・読み込めない場合はエラーを返す（起動を止める）。
    /// アクセス制限などを実装したプラグインが黙って外れたまま動き続けることを防ぐため
    pub fn load_enabled<P: AsRef<Path>>(
        plugins_dir: P,
        enabled: &[String],
        engine: WasmEngine,
        supervisor: &mut PluginSupervisor,
        services: PluginServices,
        default_limits: &PluginResourceLimits,
    ) -> Result<usize> {
        let dir = plugins_dir.as_ref();
        for name in enabled {
            let plugin_dir = dir.join(name);
            if !plugin_dir.join(MANIFEST_FILE).is_file() {
                return Err(AegisError::Config(format!(
                    "Plugin '{}' is enabled in [plugins].enabled but not installed ({} not found)",
                    name,
                    plugin_dir.join(MANIFEST_FILE).display()
                )));
            }
            let instance = Self::load_plugin(
                &plugin_dir,
                engine.clone(),
                services.clone(),
                default_limits,
            )
            .map_err(|e| AegisError::Config(format!("Failed to load plugin '{}': {}", name, e)))?;
            let limits = &instance.manifest.resource_limits;
            tracing::info!(
                "Loaded Wasm plugin: {} v{} (memory {} MiB, fuel {}, timeout {} ms)",
                instance.manifest.name,
                instance.manifest.version,
                limits.max_memory_mb,
                limits.max_fuel_units,
                limits.execution_timeout_ms
            );
            supervisor.register(instance);
        }

        for installed in Self::discover(dir, default_limits)? {
            if !enabled.contains(&installed.dir_name) {
                match &installed.problem {
                    Some(problem) => tracing::warn!(
                        "Installed plugin '{}' cannot be enabled: {}",
                        installed.dir_name,
                        problem
                    ),
                    None => tracing::info!(
                        "Installed plugin '{}' is disabled (add it to [plugins].enabled to use it)",
                        installed.dir_name
                    ),
                }
            }
        }
        Ok(enabled.len())
    }

    fn read_manifest(dir: &Path, default_limits: &PluginResourceLimits) -> Result<PluginManifest> {
        let manifest_path = dir.join(MANIFEST_FILE);
        let manifest_str = std::fs::read_to_string(&manifest_path).map_err(|e| {
            AegisError::Config(format!("Failed to read {}: {}", manifest_path.display(), e))
        })?;
        PluginManifest::from_json_with_defaults(&manifest_str, default_limits)
            .map_err(|e| AegisError::Config(format!("Failed to parse manifest: {}", e)))
    }

    /// インストールの規約: 名前が有効な形式で、ディレクトリ名と一致し、エントリポイントがある
    fn check_installation(dir_name: &str, dir: &Path, manifest: &PluginManifest) -> Result<()> {
        if !is_valid_plugin_name(&manifest.name) {
            return Err(AegisError::Config(format!(
                "Plugin name {:?} is invalid (lowercase letters, digits, '-', '_')",
                manifest.name
            )));
        }
        if manifest.name != dir_name {
            return Err(AegisError::Config(format!(
                "Plugin '{}' must be installed in a directory named '{}' (found '{}')",
                manifest.name, manifest.name, dir_name
            )));
        }
        let wasm_path = dir.join(&manifest.entrypoint);
        if !wasm_path.is_file() {
            return Err(AegisError::NotFound(format!(
                "Wasm binary not found: {}",
                wasm_path.display()
            )));
        }
        Ok(())
    }

    /// 将来のストアや ZIP パッケージからのインストールを見据えたプラグインパッケージの検証
    /// （テーマの `validate_theme_package` に相当）。
    /// - パッケージディレクトリの存在確認
    /// - `plugin.manifest.json` の存在とパース
    /// - `name` が有効形式（`is_valid_plugin_name`）
    /// - `entrypoint` で指定された Wasm ファイルが存在すること
    /// - `min_aegis_version` の確認（指定されていれば現在の Aegis バージョンと比較）
    /// - 署名の確認（署名ファイルがある場合、または `require_signature == true` の場合に検証）
    pub fn validate_plugin_package(
        plugin_dir: &Path,
        trusted_keys: Option<&[String]>,
        require_signature: bool,
    ) -> Result<PluginManifest> {
        if !plugin_dir.is_dir() {
            return Err(AegisError::Validation(format!(
                "Path is not a directory: {}",
                plugin_dir.display()
            )));
        }

        let manifest_path = plugin_dir.join(MANIFEST_FILE);
        if !manifest_path.is_file() {
            return Err(AegisError::Validation(
                "Missing required 'plugin.manifest.json' manifest in plugin package".to_string(),
            ));
        }

        let manifest_bytes = std::fs::read(&manifest_path).map_err(AegisError::Io)?;
        let manifest_str = std::str::from_utf8(&manifest_bytes).map_err(|e| {
            AegisError::Validation(format!("Invalid UTF-8 in plugin manifest: {}", e))
        })?;

        let manifest =
            PluginManifest::from_json_with_defaults(manifest_str, &PluginResourceLimits::default())
                .map_err(|e| {
                    AegisError::Validation(format!("Failed to parse 'plugin.manifest.json': {}", e))
                })?;

        if !is_valid_plugin_name(&manifest.name) {
            return Err(AegisError::Validation(format!(
                "Invalid plugin name {:?} in manifest (must be lowercase alphanumeric with '-' or '_')",
                manifest.name
            )));
        }

        let wasm_path = plugin_dir.join(&manifest.entrypoint);
        if !wasm_path.is_file() {
            return Err(AegisError::Validation(format!(
                "Plugin entrypoint Wasm binary not found: {}",
                wasm_path.display()
            )));
        }

        // min_aegis_version の検証
        if let Some(ref min_ver) = manifest.min_aegis_version {
            aegis_core::check_min_aegis_version(min_ver)?;
        }

        // 署名検証
        aegis_core::verify_package_signature(
            plugin_dir,
            &manifest_bytes,
            trusted_keys,
            require_signature,
        )?;

        Ok(manifest)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Minimal module that only imports from the `host` module
    const VALID_WAT: &str = r#"(module
        (import "host" "log" (func (param i32 i32 i32)))
        (memory (export "memory") 1)
        (func (export "allocate") (param i32) (result i32) i32.const 0))"#;

    fn install(plugins_dir: &Path, dir_name: &str, name: &str, wat: &str) {
        let dir = plugins_dir.join(dir_name);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join(MANIFEST_FILE),
            format!(
                r#"{{"name": "{}", "version": "1.0.0", "entrypoint": "plugin.wasm"}}"#,
                name
            ),
        )
        .unwrap();
        std::fs::write(dir.join("plugin.wasm"), wat::parse_str(wat).unwrap()).unwrap();
    }

    fn temp_plugins_dir(test: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("aegis_loader_{}_{}", test, std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn load(dir: &Path, enabled: &[&str]) -> Result<Vec<String>> {
        let mut supervisor = PluginSupervisor::new();
        let enabled: Vec<String> = enabled.iter().map(|s| s.to_string()).collect();
        WasmPluginLoader::load_enabled(
            dir,
            &enabled,
            WasmEngine::new().unwrap(),
            &mut supervisor,
            std::sync::Arc::new(std::sync::Mutex::new(std::collections::HashMap::new())).into(),
            &PluginResourceLimits::default(),
        )?;
        Ok(supervisor
            .plugins()
            .iter()
            .map(|p| p.manifest().name.clone())
            .collect())
    }

    #[test]
    fn test_only_enabled_plugins_load_in_the_configured_order() {
        let dir = temp_plugins_dir("order");
        install(&dir, "alpha", "alpha", VALID_WAT);
        install(&dir, "beta", "beta", VALID_WAT);
        install(&dir, "gamma", "gamma", VALID_WAT);

        assert_eq!(load(&dir, &[]).unwrap(), Vec::<String>::new());
        assert_eq!(
            load(&dir, &["gamma", "alpha"]).unwrap(),
            vec!["gamma", "alpha"]
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_enabled_plugin_that_cannot_load_is_an_error() {
        let dir = temp_plugins_dir("errors");
        install(&dir, "renamed", "original", VALID_WAT);
        install(
            &dir,
            "needs-env",
            "needs-env",
            r#"(module (import "env" "abort" (func (param i32 i32 i32 i32))))"#,
        );

        let missing = load(&dir, &["not-installed"]).unwrap_err().to_string();
        assert!(missing.contains("not installed"), "{}", missing);
        let renamed = load(&dir, &["renamed"]).unwrap_err().to_string();
        assert!(
            renamed.contains("directory named 'original'"),
            "{}",
            renamed
        );
        let imports = load(&dir, &["needs-env"]).unwrap_err().to_string();
        assert!(imports.contains("env::abort"), "{}", imports);

        // 有効にしていなければ壊れたプラグインがあっても起動できる
        assert!(load(&dir, &[]).is_ok());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_discover_lists_installed_plugins_with_problems() {
        let dir = temp_plugins_dir("discover");
        install(&dir, "ok", "ok", VALID_WAT);
        install(&dir, "renamed", "original", VALID_WAT);
        std::fs::create_dir_all(dir.join("no-manifest")).unwrap();
        std::fs::create_dir_all(dir.join(".hidden")).unwrap();

        let installed = WasmPluginLoader::discover(&dir, &PluginResourceLimits::default()).unwrap();
        let summary: Vec<(&str, bool)> = installed
            .iter()
            .map(|p| (p.dir_name.as_str(), p.problem.is_none()))
            .collect();
        assert_eq!(
            summary,
            vec![("no-manifest", false), ("ok", true), ("renamed", false)]
        );
        assert!(installed[2].manifest.is_some());

        let list = WasmPluginLoader::list(
            &dir,
            &["ok".to_string(), "missing".to_string()],
            &PluginResourceLimits::default(),
        )
        .unwrap();
        let summary: Vec<(&str, bool, bool)> = list
            .iter()
            .map(|p| (p.name.as_str(), p.is_enabled, p.is_installed))
            .collect();
        assert_eq!(
            summary,
            vec![
                ("ok", true, true),
                ("missing", true, false),
                ("no-manifest", false, true),
                ("renamed", false, true),
            ]
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_validate_plugin_package() {
        let dir = temp_plugins_dir("pkg_val");
        let pkg_dir = dir.join("my-cool-plugin");
        std::fs::create_dir_all(&pkg_dir).unwrap();
        std::fs::write(
            pkg_dir.join(MANIFEST_FILE),
            r#"{"name": "my-cool-plugin", "version": "1.0.0", "entrypoint": "plugin.wasm", "min_aegis_version": "0.1.0"}"#,
        )
        .unwrap();
        std::fs::write(pkg_dir.join("plugin.wasm"), b"\0asm\x01\0\0\0").unwrap();

        // 1. Valid package without signatures
        let manifest = WasmPluginLoader::validate_plugin_package(&pkg_dir, None, false).unwrap();
        assert_eq!(manifest.name, "my-cool-plugin");

        // 2. Missing wasm binary
        std::fs::remove_file(pkg_dir.join("plugin.wasm")).unwrap();
        let err = WasmPluginLoader::validate_plugin_package(&pkg_dir, None, false);
        assert!(err.is_err());
        assert!(err
            .unwrap_err()
            .to_string()
            .contains("Wasm binary not found"));
        std::fs::write(pkg_dir.join("plugin.wasm"), b"\0asm\x01\0\0\0").unwrap();

        // 3. Higher min_aegis_version
        std::fs::write(
            pkg_dir.join(MANIFEST_FILE),
            r#"{"name": "my-cool-plugin", "version": "1.0.0", "entrypoint": "plugin.wasm", "min_aegis_version": "99.0.0"}"#,
        )
        .unwrap();
        let err = WasmPluginLoader::validate_plugin_package(&pkg_dir, None, false);
        assert!(err.is_err());
        assert!(err
            .unwrap_err()
            .to_string()
            .contains("Package requires Aegis version >="));

        // 4. Signature required but missing
        std::fs::write(
            pkg_dir.join(MANIFEST_FILE),
            r#"{"name": "my-cool-plugin", "version": "1.0.0", "entrypoint": "plugin.wasm"}"#,
        )
        .unwrap();
        let err = WasmPluginLoader::validate_plugin_package(&pkg_dir, None, true);
        assert!(err.is_err());
        assert!(err
            .unwrap_err()
            .to_string()
            .contains("Package signature required by policy"));

        let _ = std::fs::remove_dir_all(&dir);
    }
}
