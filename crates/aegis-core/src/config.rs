use crate::error::{AegisError, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AegisConfig {
    #[serde(default)]
    pub server: ServerConfig,
    #[serde(default)]
    pub database: DatabaseConfig,
    #[serde(default)]
    pub paths: PathsConfig,
    #[serde(default)]
    pub runtime: RuntimeConfig,
    #[serde(default)]
    pub security: SecurityConfig,
}

impl AegisConfig {
    pub fn load_from_file<P: AsRef<Path>>(path: P) -> Result<Self> {
        let content = std::fs::read_to_string(path.as_ref())
            .map_err(|e| AegisError::Config(format!("Failed to read config file: {}", e)))?;
        let config: AegisConfig = toml::from_str(&content)
            .map_err(|e| AegisError::Config(format!("Failed to parse config file: {}", e)))?;
        Ok(config)
    }

    pub fn load_or_default<P: AsRef<Path>>(path: P) -> Self {
        Self::load_from_file(path).unwrap_or_default()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerConfig {
    pub host: String,
    pub port: u16,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            host: "127.0.0.1".to_string(),
            port: 8080,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatabaseConfig {
    pub url: String,
    pub max_connections: u32,
}

impl Default for DatabaseConfig {
    fn default() -> Self {
        Self {
            url: "sqlite://aegis.db?mode=rwc".to_string(),
            max_connections: 10,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PathsConfig {
    pub schemas_dir: PathBuf,
    pub plugins_dir: PathBuf,
    pub admin_dist_dir: PathBuf,
}

impl Default for PathsConfig {
    fn default() -> Self {
        Self {
            schemas_dir: PathBuf::from("schemas"),
            plugins_dir: PathBuf::from("plugins"),
            admin_dist_dir: PathBuf::from("admin/dist"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeConfig {
    pub enable_wasm_plugins: bool,
    pub default_max_memory_mb: u32,
    pub default_max_fuel_units: u64,
    pub default_timeout_ms: u64,
}

impl Default for RuntimeConfig {
    fn default() -> Self {
        Self {
            enable_wasm_plugins: true,
            default_max_memory_mb: 16,
            default_max_fuel_units: 100_000_000,
            default_timeout_ms: 250,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecurityConfig {
    pub enforce_external_link_rel: bool,
    pub open_external_links_in_new_tab: bool,
    pub internal_domains: Vec<String>,
    pub enforce_image_lazy_loading: bool,
    pub enforce_image_async_decoding: bool,
}

impl Default for SecurityConfig {
    fn default() -> Self {
        Self {
            enforce_external_link_rel: true,
            open_external_links_in_new_tab: false,
            internal_domains: Vec::new(),
            enforce_image_lazy_loading: true,
            enforce_image_async_decoding: true,
        }
    }
}
