use serde::{Deserialize, Serialize};

/// プラグインのマニフェスト (`plugin.manifest.json`)。
/// テーマ (`theme.json`) と同じく、将来のストアからの配布・インストールを見据えたメタデータを持つ
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PluginManifest {
    /// プラグインの一意識別子（`is_valid_plugin_name`、例: "slack-notifier"）。
    /// インストール先のディレクトリ名 (`plugins/<name>/`) と `[plugins].enabled` に使う
    pub name: String,
    pub version: String,
    pub entrypoint: String,
    /// 表示名（例: "Slack Notifier"）。省略時は `name`
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
    /// 概要・説明文
    #[serde(default)]
    pub description: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub author: Option<PackageAuthor>,
    /// ライセンス（例: "MIT", "Apache-2.0"）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub license: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub homepage: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repository: Option<String>,
    /// 対応する最小 AegisCMS バージョン
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min_aegis_version: Option<String>,
    #[serde(default)]
    pub capabilities: PluginCapabilities,
    #[serde(default)]
    pub resource_limits: PluginResourceLimits,
}

/// 作者情報（名前だけ、または名前・連絡先）。テーマの `ThemeAuthor` と同じ形式
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(untagged)]
pub enum PackageAuthor {
    Simple(String),
    Detailed {
        name: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        email: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        url: Option<String>,
    },
}

/// プラグイン名に使える形式か（英小文字・数字で始まり、英小文字・数字・`-`・`_` のみ、64 文字以内）。
/// ディレクトリ名・設定値・ストアの識別子として、そのままパスや URL に使えることを保証する
pub fn is_valid_plugin_name(name: &str) -> bool {
    let bytes = name.as_bytes();
    !bytes.is_empty()
        && bytes.len() <= 64
        && (bytes[0].is_ascii_lowercase() || bytes[0].is_ascii_digit())
        && bytes
            .iter()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || matches!(b, b'-' | b'_'))
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
    fn test_store_metadata_is_optional() {
        let manifest = PluginManifest::from_json_with_defaults(
            r#"{"name": "p", "version": "1.0.0", "entrypoint": "plugin.wasm"}"#,
            &DEFAULTS,
        )
        .unwrap();
        assert_eq!(manifest.display_name, None);
        assert_eq!(manifest.description, "");
        assert!(manifest.tags.is_empty());

        let manifest = PluginManifest::from_json_with_defaults(
            r#"{"name": "slack-notifier", "version": "1.0.0", "entrypoint": "plugin.wasm",
                "display_name": "Slack Notifier", "author": {"name": "Aegis", "url": "https://aegiscms.dev"},
                "license": "MIT", "tags": ["notification"], "min_aegis_version": "0.1.0"}"#,
            &DEFAULTS,
        )
        .unwrap();
        assert_eq!(manifest.display_name.as_deref(), Some("Slack Notifier"));
        assert_eq!(
            manifest.author,
            Some(PackageAuthor::Detailed {
                name: "Aegis".to_string(),
                email: None,
                url: Some("https://aegiscms.dev".to_string()),
            })
        );
    }

    #[test]
    fn test_plugin_name_validation() {
        for valid in ["slack-notifier", "seo_tools", "2fa", "a"] {
            assert!(is_valid_plugin_name(valid), "{}", valid);
        }
        for invalid in [
            "",
            "Slack",
            "-x",
            "_x",
            "a/b",
            "../x",
            "a.b",
            "a b",
            &"a".repeat(65),
        ] {
            assert!(!is_valid_plugin_name(invalid), "{}", invalid);
        }
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
