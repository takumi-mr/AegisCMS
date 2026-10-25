//! `[plugins]` / `[runtime]` の設定

use aegis_core::{AegisError, PluginResourceLimits, Result};
use serde::{Deserialize, Serialize};

/// `[plugins]` — インストール済みプラグイン（`[paths].plugins_dir/<name>/`）のうち有効にするもの
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct PluginsConfig {
    /// 有効にするプラグイン名（マニフェストの `name`）。フックはこの順に実行する。
    /// 列挙したプラグインが見つからない・読み込めない場合はサーバーを起動しない
    pub enabled: Vec<String>,
    /// プラグインごとの設定値（`[plugins.settings.<name>]`）。プラグインは `host.config_get` で読む。
    /// 文字列の値 `env:NAME` は環境変数 `NAME` の値になる（API キーなどを設定ファイルに書かないため）
    pub settings: std::collections::HashMap<String, std::collections::HashMap<String, toml::Value>>,
    /// プラグインのインストール時に署名を必須とするか（既定: false）
    #[serde(default)]
    pub require_signatures: bool,
    /// 信頼する公開鍵リスト（hex形式、未指定時は任意の有効な署名を受け入れる）
    #[serde(default)]
    pub trusted_public_keys: Vec<String>,
}

impl PluginsConfig {
    /// プラグイン `plugin` の設定値 `key` を文字列で返す（文字列はそのまま、それ以外は JSON）
    pub fn setting(&self, plugin: &str, key: &str) -> Option<String> {
        match self.settings.get(plugin)?.get(key)? {
            toml::Value::String(value) => match value.strip_prefix("env:") {
                Some(name) => std::env::var(name.trim()).ok(),
                None => Some(value.clone()),
            },
            other => serde_json::to_string(other).ok(),
        }
    }

    /// 名前の形式（`is_valid_plugin_name`）と重複を検証する
    pub fn validate(&self) -> Result<()> {
        let mut seen = std::collections::HashSet::new();
        for name in &self.enabled {
            if !aegis_core::is_valid_plugin_name(name) {
                return Err(AegisError::Config(format!(
                    "[plugins].enabled: {:?} is not a valid plugin name (lowercase letters, digits, '-', '_')",
                    name
                )));
            }
            if !seen.insert(name) {
                return Err(AegisError::Config(format!(
                    "[plugins].enabled: {:?} is listed more than once",
                    name
                )));
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct RuntimeConfig {
    pub enable_wasm_plugins: bool,
    pub default_max_memory_mb: u32,
    pub default_max_fuel_units: u64,
    pub default_timeout_ms: u64,
    /// プーリングアロケータの枠数（= 同時に実行できるフック呼び出しの数。超えた分は空きを待つ）。
    /// 0 で無効（呼び出しごとにメモリを確保・解放する）
    pub instance_pool_size: u32,
    /// プールの 1 枠あたりの線形メモリの上限（MiB）。`max_memory_mb` がこれを超えるプラグインは読み込めない
    pub pool_max_memory_mb: u32,
}

impl RuntimeConfig {
    /// Resource limits applied to plugins whose manifest omits them (field by field)
    pub fn default_resource_limits(&self) -> PluginResourceLimits {
        PluginResourceLimits {
            max_memory_mb: self.default_max_memory_mb,
            max_fuel_units: self.default_max_fuel_units,
            execution_timeout_ms: self.default_timeout_ms,
        }
    }
}

impl Default for RuntimeConfig {
    fn default() -> Self {
        Self {
            enable_wasm_plugins: true,
            default_max_memory_mb: 16,
            default_max_fuel_units: 100_000_000,
            default_timeout_ms: 250,
            instance_pool_size: 64,
            pool_max_memory_mb: 64,
        }
    }
}
