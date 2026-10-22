use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginManifest {
    pub name: String,
    pub version: String,
    pub entrypoint: String,
    #[serde(default)]
    pub capabilities: PluginCapabilities,
    #[serde(default)]
    pub resource_limits: PluginResourceLimits,
}

impl PluginManifest {
    /// Parses `plugin.manifest.json`, filling every `resource_limits` field the manifest omits
    /// (or the whole section) from `defaults` (the server's `[runtime].default_*` settings)
    pub fn from_json_with_defaults(
        json: &str,
        defaults: &PluginResourceLimits,
    ) -> serde_json::Result<Self> {
        let mut value: serde_json::Value = serde_json::from_str(json)?;
        if let Some(manifest) = value.as_object_mut() {
            let limits = manifest
                .entry("resource_limits")
                .or_insert_with(|| serde_json::json!({}));
            if limits.is_null() {
                *limits = serde_json::json!({});
            }
            if let (Some(limits), serde_json::Value::Object(defaults)) =
                (limits.as_object_mut(), serde_json::to_value(defaults)?)
            {
                for (key, default) in defaults {
                    limits.entry(key).or_insert(default);
                }
            }
        }
        serde_json::from_value(value)
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PluginCapabilities {
    #[serde(default)]
    pub network: Option<NetworkCapability>,
    #[serde(default)]
    pub storage: Option<StorageCapability>,
    #[serde(default)]
    pub hooks: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkCapability {
    #[serde(default)]
    pub allowed_domains: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageCapability {
    pub kv_namespace: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginResourceLimits {
    pub max_memory_mb: u32,
    pub max_fuel_units: u64,
    pub execution_timeout_ms: u64,
}

impl Default for PluginResourceLimits {
    fn default() -> Self {
        Self {
            max_memory_mb: 16,
            max_fuel_units: 100_000_000,
            execution_timeout_ms: 250,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DEFAULTS: PluginResourceLimits = PluginResourceLimits {
        max_memory_mb: 32,
        max_fuel_units: 5_000,
        execution_timeout_ms: 500,
    };

    fn parse(resource_limits: &str) -> PluginResourceLimits {
        let json = format!(
            r#"{{"name": "p", "version": "1.0.0", "entrypoint": "plugin.wasm"{}}}"#,
            resource_limits
        );
        PluginManifest::from_json_with_defaults(&json, &DEFAULTS)
            .unwrap()
            .resource_limits
    }

    #[test]
    fn test_missing_resource_limits_use_the_runtime_defaults() {
        let limits = parse("");
        assert_eq!(limits.max_memory_mb, 32);
        assert_eq!(limits.max_fuel_units, 5_000);
        assert_eq!(limits.execution_timeout_ms, 500);

        let limits = parse(r#", "resource_limits": null"#);
        assert_eq!(limits.max_fuel_units, 5_000);
    }

    #[test]
    fn test_partial_resource_limits_keep_the_manifest_values() {
        let limits = parse(r#", "resource_limits": {"max_memory_mb": 8}"#);
        assert_eq!(limits.max_memory_mb, 8);
        assert_eq!(limits.max_fuel_units, 5_000);
        assert_eq!(limits.execution_timeout_ms, 500);

        let limits = parse(
            r#", "resource_limits": {"max_memory_mb": 4, "max_fuel_units": 7, "execution_timeout_ms": 9}"#,
        );
        assert_eq!(
            (
                limits.max_memory_mb,
                limits.max_fuel_units,
                limits.execution_timeout_ms
            ),
            (4, 7, 9)
        );
    }
}
