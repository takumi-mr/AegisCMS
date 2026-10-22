use serde::{Deserialize, Serialize};
use std::path::Path;

/// テーマ作者情報
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(untagged)]
pub enum ThemeAuthor {
    Simple(String),
    Detailed {
        name: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        email: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        url: Option<String>,
    },
}

impl Default for ThemeAuthor {
    fn default() -> Self {
        ThemeAuthor::Simple("Aegis Community".to_string())
    }
}

/// テーマのマニフェスト定義 (`theme.json`)
/// 将来のテーママーケットプレイス (Market) からの配布・インポートを見据えた仕様
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ThemeManifest {
    /// テーマの一意識別子 (kebab-case, 例: "cyber-neon")
    pub id: String,
    /// テーマの表示名 (例: "Tokyo Cyber Neon")
    pub name: String,
    /// バージョン (セマンティックバージョニング, 例: "1.0.0")
    #[serde(default = "default_version")]
    pub version: String,
    /// テーマの概要・説明文
    #[serde(default)]
    pub description: String,
    /// テーマの作者情報
    #[serde(default)]
    pub author: ThemeAuthor,
    /// ライセンス (例: "MIT", "Apache-2.0")
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub license: Option<String>,
    /// プレビュー画像ファイル名 (例: "preview.svg", "preview.png")
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub preview_image: Option<String>,
    /// カテゴリやタグ (例: ["cyberpunk", "dark", "neon"])
    #[serde(default)]
    pub tags: Vec<String>,
    /// テーマの公式サイトまたはデモURL
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub homepage: Option<String>,
    /// ソースコードリポジトリURL
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repository: Option<String>,
    /// 対応する最小 AegisCMS バージョン
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min_aegis_version: Option<String>,
}

fn default_version() -> String {
    "1.0.0".to_string()
}

impl ThemeManifest {
    /// ディレクトリ名のみからデフォルトのマニフェストを生成するフォールバック
    pub fn fallback_from_id(id: &str) -> Self {
        let name = id
            .split(['-', '_'])
            .map(|word| {
                let mut chars = word.chars();
                match chars.next() {
                    None => String::new(),
                    Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
                }
            })
            .collect::<Vec<_>>()
            .join(" ");

        Self {
            id: id.to_string(),
            name,
            version: "1.0.0".to_string(),
            description: format!("AegisCMS theme '{}'", id),
            author: ThemeAuthor::Simple("Aegis Community".to_string()),
            license: Some("MIT".to_string()),
            preview_image: None,
            tags: vec![],
            homepage: None,
            repository: None,
            min_aegis_version: None,
        }
    }

    /// テーマディレクトリから `theme.json` を読み込む
    pub fn load_from_dir(dir: &Path) -> Option<Self> {
        let manifest_path = dir.join("theme.json");
        if manifest_path.exists() {
            match std::fs::read_to_string(&manifest_path) {
                Ok(content) => match serde_json::from_str::<ThemeManifest>(&content) {
                    Ok(mut manifest) => {
                        // ディレクトリ名と id を同期（id が空の場合はディレクトリ名）
                        if manifest.id.is_empty() {
                            if let Some(folder_name) = dir.file_name().and_then(|s| s.to_str()) {
                                manifest.id = folder_name.to_string();
                            }
                        }
                        Some(manifest)
                    }
                    Err(e) => {
                        tracing::warn!(
                            "Failed to parse theme manifest at {}: {}",
                            manifest_path.display(),
                            e
                        );
                        None
                    }
                },
                Err(e) => {
                    tracing::warn!(
                        "Failed to read theme manifest at {}: {}",
                        manifest_path.display(),
                        e
                    );
                    None
                }
            }
        } else {
            None
        }
    }
}

/// インストール済みテーマの情報 (API 返却用)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ThemeInfo {
    /// テーマのマニフェスト
    #[serde(flatten)]
    pub manifest: ThemeManifest,
    /// 現在アクティブなテーマかどうか
    pub is_active: bool,
    /// テンプレートディレクトリが存在するか
    pub has_templates: bool,
    /// アセットディレクトリが存在するか
    pub has_assets: bool,
    /// プレビュー画像の配信URL (存在する場合)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub preview_url: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_theme_manifest_serde() {
        let json = r#"{
            "id": "cyber-neon",
            "name": "Tokyo Cyber Neon",
            "version": "1.2.0",
            "description": "High contrast cyberpunk theme",
            "author": {
                "name": "Aegis Core Team",
                "url": "https://aegiscms.dev"
            },
            "license": "MIT",
            "preview_image": "preview.svg",
            "tags": ["cyberpunk", "dark"]
        }"#;

        let manifest: ThemeManifest = serde_json::from_str(json).unwrap();
        assert_eq!(manifest.id, "cyber-neon");
        assert_eq!(manifest.name, "Tokyo Cyber Neon");
        assert_eq!(manifest.version, "1.2.0");
        assert_eq!(manifest.tags, vec!["cyberpunk", "dark"]);
        assert_eq!(manifest.preview_image.as_deref(), Some("preview.svg"));
    }

    #[test]
    fn test_fallback_from_id() {
        let manifest = ThemeManifest::fallback_from_id("minimal-editorial");
        assert_eq!(manifest.id, "minimal-editorial");
        assert_eq!(manifest.name, "Minimal Editorial");
    }
}
