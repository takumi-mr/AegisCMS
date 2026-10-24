//! 有効なプラグインの一覧と、再起動なしの有効・無効の切り替え。
//! 一覧は `[plugins].enabled` を、Admin Studio で変更した値（DB の `_aegis_settings` の `plugins.enabled`）で
//! 上書きする。切り替えは新しい `PluginSupervisor` を組み立ててから差し替える（読み込み済みのプラグインは使い回す）

use std::path::PathBuf;
use std::sync::Arc;

use aegis_core::{is_valid_plugin_name, AegisError, PluginResourceLimits, Result};
use aegis_db::DatabaseAdapter;
use aegis_runtime::{AegisPlugin, PluginServices, PluginSupervisor, WasmEngine, WasmPluginLoader};
use tokio::sync::Mutex;

/// `_aegis_settings` の名前（値は JSON の文字列の配列。順序がフックの実行順）
pub const ENABLED_SETTING: &str = "plugins.enabled";

const MANIFEST_FILE: &str = "plugin.manifest.json";

/// 有効なプラグインの一覧（Admin Studio で保存した値 → `[plugins].enabled` の順）。
/// 保存した値が壊れていればエラーにする（アクセス制限などのプラグインが黙って外れることを防ぐため）
pub async fn resolve_enabled(
    db: &dyn DatabaseAdapter,
    configured: &[String],
) -> Result<Vec<String>> {
    let settings = db.list_settings().await?;
    let Some(saved) = settings.get(ENABLED_SETTING) else {
        return Ok(configured.to_vec());
    };
    let enabled: Vec<String> = serde_json::from_str(saved).map_err(|e| {
        AegisError::Config(format!(
            "_aegis_settings \"{}\" is not a JSON array of plugin names: {}",
            ENABLED_SETTING, e
        ))
    })?;
    if let Some(invalid) = enabled.iter().find(|name| !is_valid_plugin_name(name)) {
        return Err(AegisError::Config(format!(
            "_aegis_settings \"{}\" has an invalid plugin name {:?}",
            ENABLED_SETTING, invalid
        )));
    }
    tracing::info!(
        "Using the plugin list saved from Admin Studio (overrides [plugins].enabled): {:?}",
        enabled
    );
    Ok(enabled)
}

/// プラグインを読み込むための部品（Wasm エンジン・ホスト API・インストール先）
pub struct PluginLoader {
    engine: WasmEngine,
    services: PluginServices,
    plugins_dir: PathBuf,
    default_limits: PluginResourceLimits,
}

impl PluginLoader {
    pub fn new(
        engine: WasmEngine,
        services: PluginServices,
        plugins_dir: PathBuf,
        default_limits: PluginResourceLimits,
    ) -> Self {
        Self {
            engine,
            services,
            plugins_dir,
            default_limits,
        }
    }

    /// 起動時: `enabled` をその順に読み込む（インストール済みで無効なものはログに出す）
    pub fn load(&self, enabled: &[String]) -> Result<PluginSupervisor> {
        let mut supervisor = PluginSupervisor::new();
        WasmPluginLoader::load_enabled(
            &self.plugins_dir,
            enabled,
            self.engine.clone(),
            &mut supervisor,
            self.services.clone(),
            &self.default_limits,
        )?;
        Ok(supervisor)
    }

    /// 切り替え時: `enabled` の順に組み立てる。`loaded` にあるものは使い回し、Wasm をコンパイルし直さない
    pub fn rebuild(
        &self,
        enabled: &[String],
        loaded: &[Arc<dyn AegisPlugin>],
    ) -> Result<PluginSupervisor> {
        let mut supervisor = PluginSupervisor::new();
        for name in enabled {
            if let Some(plugin) = loaded.iter().find(|p| &p.manifest().name == name) {
                supervisor.register(plugin.clone());
                continue;
            }
            let plugin_dir = self.plugins_dir.join(name);
            if !plugin_dir.join(MANIFEST_FILE).is_file() {
                return Err(AegisError::Validation(format!(
                    "Plugin '{}' is not installed",
                    name
                )));
            }
            let instance = WasmPluginLoader::load_plugin(
                &plugin_dir,
                self.engine.clone(),
                self.services.clone(),
                &self.default_limits,
            )
            .map_err(|e| {
                AegisError::Validation(format!("Failed to load plugin '{}': {}", name, e))
            })?;
            supervisor.register(instance);
        }
        Ok(supervisor)
    }
}

/// 有効なプラグインの一覧と、切り替えに使うローダー
pub struct PluginRegistry {
    /// `[runtime].enable_wasm_plugins = false` なら `None`（切り替えられない）
    loader: Option<Arc<PluginLoader>>,
    /// 有効なプラグイン（フックの実行順）。切り替えの間はロックしたままにして、同時の切り替えを順に処理する
    enabled: Mutex<Vec<String>>,
}

impl PluginRegistry {
    pub fn new(loader: Option<PluginLoader>, enabled: Vec<String>) -> Self {
        Self {
            loader: loader.map(Arc::new),
            enabled: Mutex::new(enabled),
        }
    }

    pub fn loader(&self) -> Option<&Arc<PluginLoader>> {
        self.loader.as_ref()
    }

    pub async fn enabled(&self) -> Vec<String> {
        self.enabled.lock().await.clone()
    }

    /// 切り替えの間に持つロック（中身が一覧）
    pub async fn lock(&self) -> tokio::sync::MutexGuard<'_, Vec<String>> {
        self.enabled.lock().await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_resolve_enabled_prefers_the_saved_list() {
        let state = crate::test_support::test_state(PluginSupervisor::new()).await;
        let db = state.db.as_ref();
        let configured = vec!["from-config".to_string()];

        assert_eq!(resolve_enabled(db, &configured).await.unwrap(), configured);

        db.set_setting(ENABLED_SETTING, r#"["b","a"]"#)
            .await
            .unwrap();
        assert_eq!(
            resolve_enabled(db, &configured).await.unwrap(),
            vec!["b", "a"]
        );

        // An empty saved list disables everything (it is not "unset")
        db.set_setting(ENABLED_SETTING, "[]").await.unwrap();
        assert!(resolve_enabled(db, &configured).await.unwrap().is_empty());

        for broken in [r#"{"a":1}"#, r#"["../etc"]"#] {
            db.set_setting(ENABLED_SETTING, broken).await.unwrap();
            assert!(
                resolve_enabled(db, &configured).await.is_err(),
                "{}",
                broken
            );
        }
    }
}
