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
    /// 本文に置けるカスタムブロック（`render-blocks` フックで HTML にする）。
    /// 保存するブロックの名前は `<プラグイン名>/<ブロック名>`
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub blocks: Vec<BlockDefinition>,
}

/// 文字列、または言語ごとの文字列（`{"en": "Link card", "ja": "リンクカード"}`）
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(untagged)]
pub enum LocalizedText {
    Plain(String),
    Localized(std::collections::BTreeMap<String, String>),
}

impl LocalizedText {
    /// 空でない文字列があるか
    pub fn is_empty(&self) -> bool {
        match self {
            Self::Plain(s) => s.trim().is_empty(),
            Self::Localized(map) => map.values().all(|s| s.trim().is_empty()),
        }
    }
}

/// プラグインが提供するカスタムブロックの宣言（`plugin.manifest.json` の `blocks`）
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BlockDefinition {
    /// ブロック名（プラグイン名と同じ形式。保存する名前は `<プラグイン名>/<name>`）
    pub name: String,
    /// Admin Studio に出す名前
    pub label: LocalizedText,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<LocalizedText>,
    /// 編集画面の入力欄。値はブロックの `attributes` に `name` をキーにして入る
    #[serde(default)]
    pub fields: Vec<BlockField>,
}

/// カスタムブロックの入力欄
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BlockField {
    /// 属性名（英字で始まり、英数字と `_` のみ）
    pub name: String,
    #[serde(rename = "type")]
    pub field_type: BlockFieldType,
    pub label: LocalizedText,
    #[serde(default)]
    pub required: bool,
    /// 新しく置いたブロックの初期値
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default: Option<serde_json::Value>,
    /// `select` の選択肢
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub options: Vec<BlockFieldOption>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub placeholder: Option<LocalizedText>,
    /// 入力欄の下に出す説明
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub help: Option<LocalizedText>,
}

/// 入力欄の種類
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum BlockFieldType {
    /// 1 行の文字列
    String,
    /// 複数行の文字列
    Text,
    Url,
    Number,
    Boolean,
    Select,
    /// 画像の URL（アップロード・メディアライブラリから選べる）
    Image,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BlockFieldOption {
    pub value: String,
    pub label: LocalizedText,
}

/// ブロックの属性名に使える形式か（英字で始まり、英数字と `_` のみ、64 文字以内）
fn is_valid_field_name(name: &str) -> bool {
    let bytes = name.as_bytes();
    !bytes.is_empty()
        && bytes.len() <= 64
        && bytes[0].is_ascii_alphabetic()
        && bytes
            .iter()
            .all(|b| b.is_ascii_alphanumeric() || *b == b'_')
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
    /// カスタムブロックの宣言を確かめる: 名前の形式と重複、入力欄の名前・選択肢、
    /// ブロックがあれば `render-blocks` フックを宣言していること、`embed.frame_domains` がホスト名で、
    /// ブロックのあるプラグインのものであること
    pub fn validate_blocks(&self) -> std::result::Result<(), String> {
        if let Some(embed) = &self.capabilities.embed {
            if let Some(bad) = embed
                .frame_domains
                .iter()
                .find(|d| !is_valid_frame_domain(d))
            {
                return Err(format!(
                    "embed.frame_domains: {:?} is not a host name (e.g. \"player.vimeo.com\")",
                    bad
                ));
            }
            if !embed.frame_domains.is_empty() && self.blocks.is_empty() {
                return Err("embed.frame_domains is only for plugins with `blocks`".to_string());
            }
        }
        if self.blocks.is_empty() {
            return Ok(());
        }
        if !self.capabilities.hooks.iter().any(|h| h == "render-blocks") {
            return Err("plugins with `blocks` must list the `render-blocks` hook".to_string());
        }
        let mut names = std::collections::HashSet::new();
        for block in &self.blocks {
            if !is_valid_plugin_name(&block.name) {
                return Err(format!(
                    "block name {:?} is invalid (lowercase letters, digits, '-', '_')",
                    block.name
                ));
            }
            if !names.insert(block.name.as_str()) {
                return Err(format!("block {:?} is declared twice", block.name));
            }
            if block.label.is_empty() {
                return Err(format!("block {:?} has no label", block.name));
            }
            let mut fields = std::collections::HashSet::new();
            for field in &block.fields {
                if !is_valid_field_name(&field.name) {
                    return Err(format!(
                        "field {:?} of block {:?} is invalid (a letter, then letters, digits or '_')",
                        field.name, block.name
                    ));
                }
                if !fields.insert(field.name.as_str()) {
                    return Err(format!(
                        "field {:?} of block {:?} is declared twice",
                        field.name, block.name
                    ));
                }
                if field.field_type == BlockFieldType::Select && field.options.is_empty() {
                    return Err(format!(
                        "select field {:?} of block {:?} has no options",
                        field.name, block.name
                    ));
                }
            }
        }
        Ok(())
    }

    /// カスタムブロックに埋め込める `<iframe>` の配信元（`embed.frame_domains`）
    pub fn frame_domains(&self) -> &[String] {
        self.capabilities
            .embed
            .as_ref()
            .map(|e| e.frame_domains.as_slice())
            .unwrap_or(&[])
    }

    /// 本文に保存するブロック名（`<プラグイン名>/<ブロック名>`）がこのプラグインのブロックか
    pub fn declares_block(&self, full_name: &str) -> bool {
        full_name
            .strip_prefix(self.name.as_str())
            .and_then(|rest| rest.strip_prefix('/'))
            .is_some_and(|block| self.blocks.iter().any(|b| b.name == block))
    }

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
    /// カスタムブロックに埋め込める外部のページ（動画のプレーヤーなど）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub embed: Option<EmbedCapability>,
}

/// カスタムブロック（`render-blocks`）の HTML に残せる `<iframe>` の配信元。
/// 宣言したホストの `https://` の `<iframe>` だけがサニタイズで残り、CSP の `frame-src` に足される
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct EmbedCapability {
    /// ホスト名（例: `www.youtube-nocookie.com`）。完全一致のみ（サブドメインは含まない）
    #[serde(default)]
    pub frame_domains: Vec<String>,
}

/// `embed.frame_domains` に書けるホスト名か（英小文字・数字・`-` のラベルを `.` でつないだもの。
/// ラベルは 2 つ以上、ワイルドカード・ポート・スキーム・パスは不可）
pub fn is_valid_frame_domain(host: &str) -> bool {
    let labels: Vec<&str> = host.split('.').collect();
    host.len() <= 253
        && labels.len() >= 2
        && labels.iter().all(|label| {
            !label.is_empty()
                && label.len() <= 63
                && !label.starts_with('-')
                && !label.ends_with('-')
                && label
                    .bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
        })
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

    fn manifest_with_blocks(
        blocks: &str,
        hooks: &str,
    ) -> std::result::Result<PluginManifest, String> {
        let json = format!(
            r#"{{"name": "cards", "version": "1.0.0", "entrypoint": "plugin.wasm",
                "capabilities": {{"hooks": [{}]}}, "blocks": {}}}"#,
            hooks, blocks
        );
        let manifest =
            PluginManifest::from_json_with_defaults(&json, &DEFAULTS).map_err(|e| e.to_string())?;
        manifest.validate_blocks().map(|_| manifest)
    }

    #[test]
    fn test_block_declarations() {
        let manifest = manifest_with_blocks(
            r#"[{"name": "link-card", "label": {"en": "Link card", "ja": "リンクカード"},
                 "fields": [
                   {"name": "url", "type": "url", "label": "URL", "required": true},
                   {"name": "style", "type": "select", "label": "Style", "default": "plain",
                    "options": [{"value": "plain", "label": "Plain"}]}
                 ]}]"#,
            r#""render-blocks""#,
        )
        .unwrap();
        assert_eq!(
            manifest.blocks[0].fields[1].field_type,
            BlockFieldType::Select
        );
        assert!(manifest.declares_block("cards/link-card"));
        assert!(!manifest.declares_block("cards/other"));
        assert!(!manifest.declares_block("other/link-card"));
        assert!(!manifest.declares_block("cardslink-card"));

        // ブロックの無いプラグインは今までどおり
        assert!(manifest_with_blocks("[]", "").is_ok());
    }

    #[test]
    fn test_invalid_block_declarations() {
        let block = |b: &str| manifest_with_blocks(&format!("[{}]", b), r#""render-blocks""#);
        // フックの宣言が無い
        assert!(manifest_with_blocks(r#"[{"name": "a", "label": "A"}]"#, "").is_err());
        for invalid in [
            r#"{"name": "Bad/Name", "label": "A"}"#,
            r#"{"name": "a", "label": ""}"#,
            r#"{"name": "a", "label": "A", "fields": [{"name": "1x", "type": "string", "label": "X"}]}"#,
            r#"{"name": "a", "label": "A", "fields": [{"name": "x", "type": "select", "label": "X"}]}"#,
            r#"{"name": "a", "label": "A", "fields": [{"name": "x", "type": "string", "label": "X"}, {"name": "x", "type": "text", "label": "Y"}]}"#,
        ] {
            assert!(block(invalid).is_err(), "{}", invalid);
        }
        assert!(manifest_with_blocks(
            r#"[{"name": "a", "label": "A"}, {"name": "a", "label": "B"}]"#,
            r#""render-blocks""#
        )
        .is_err());
        // 知らない入力欄の種類は読めない
        assert!(block(r#"{"name": "a", "label": "A", "fields": [{"name": "x", "type": "color", "label": "X"}]}"#).is_err());
    }

    #[test]
    fn test_embed_frame_domains() {
        let with_embed = |domains: &str, blocks: &str| {
            let json = format!(
                r#"{{"name": "video", "version": "1.0.0", "entrypoint": "plugin.wasm",
                    "capabilities": {{"hooks": ["render-blocks"], "embed": {{"frame_domains": {}}}}},
                    "blocks": {}}}"#,
                domains, blocks
            );
            let manifest = PluginManifest::from_json_with_defaults(&json, &DEFAULTS)
                .map_err(|e| e.to_string())?;
            manifest.validate_blocks().map(|_| manifest)
        };
        let block = r#"[{"name": "video", "label": "Video"}]"#;
        let manifest =
            with_embed(r#"["www.youtube-nocookie.com", "player.vimeo.com"]"#, block).unwrap();
        assert_eq!(
            manifest.frame_domains(),
            ["www.youtube-nocookie.com", "player.vimeo.com"]
        );
        for bad in [
            r#"["https://www.youtube.com"]"#,
            r#"["*.youtube.com"]"#,
            r#"["youtube"]"#,
            r#"["YouTube.com"]"#,
            r#"["a.com:443"]"#,
            r#"["a.com/embed"]"#,
            r#"["-a.com"]"#,
        ] {
            assert!(with_embed(bad, block).is_err(), "{}", bad);
        }
        // ブロックの無いプラグインには宣言できない
        assert!(with_embed(r#"["a.com"]"#, "[]").is_err());
        // 宣言しなければ何も許さない
        let plain =
            manifest_with_blocks(r#"[{"name": "a", "label": "A"}]"#, r#""render-blocks""#).unwrap();
        assert!(plain.frame_domains().is_empty());
    }
}
