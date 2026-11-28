//! 前後の記事（前の記事・次の記事）の選び方。テンプレートの `prev_article` / `next_article` と、
//! プラグインの `on-render-page` に渡す値に使う。
//!
//! 値の優先順: Admin Studio で変えた値（DB の `_aegis_settings`）→ 設定ファイルの `[adjacent_articles]` →
//! 以前の `[plugins.settings.official-post-navigation]`（移行のため。起動時に警告する）→ 既定値

use std::collections::HashMap;

use aegis_db::DatabaseAdapter;
use aegis_runtime::PluginsConfig;
use serde::{Deserialize, Serialize};

/// `_aegis_settings` の名前
pub const SAME_CATEGORY_SETTING: &str = "adjacent_articles.same_category";
pub const FALLBACK_TO_ALL_SETTING: &str = "adjacent_articles.fallback_to_all";
pub const ORDER_BY_SETTING: &str = "adjacent_articles.order_by";

/// 以前、この設定を持っていたプラグイン
const LEGACY_PLUGIN: &str = "official-post-navigation";
/// 以前のプラグインの設定のうち、前後の記事の選び方に使っていたもの
const LEGACY_KEYS: [&str; 5] = [
    "mode",
    "navigation_mode",
    "same_category",
    "fallback_to_all",
    "order_by",
];

/// 並べる順
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AdjacentOrder {
    /// 公開日時
    #[default]
    PublishedAt,
    /// 作成日時
    CreatedAt,
}

impl AdjacentOrder {
    fn parse(value: &str) -> Option<Self> {
        match value.trim() {
            "published_at" => Some(Self::PublishedAt),
            "created_at" => Some(Self::CreatedAt),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::PublishedAt => "published_at",
            Self::CreatedAt => "created_at",
        }
    }
}

impl From<AdjacentOrder> for aegis_db::PublishedOrder {
    fn from(order: AdjacentOrder) -> Self {
        match order {
            AdjacentOrder::PublishedAt => Self::PublishedAt,
            AdjacentOrder::CreatedAt => Self::CreatedAt,
        }
    }
}

/// `[adjacent_articles]`。書かない項目は、以前のプラグインの設定、無ければ既定値になる
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct AdjacentArticlesConfig {
    /// 同じカテゴリの記事から選ぶ（既定: `true`。カテゴリの無い記事は全記事から）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub same_category: Option<bool>,
    /// 同じカテゴリに前後が無ければ、全記事から選ぶ（既定: `true`）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fallback_to_all: Option<bool>,
    /// 並べる順（既定: `published_at`）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub order_by: Option<AdjacentOrder>,
}

/// 実際に使う値
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct AdjacentSettings {
    pub same_category: bool,
    pub fallback_to_all: bool,
    pub order_by: AdjacentOrder,
}

impl Default for AdjacentSettings {
    fn default() -> Self {
        Self {
            same_category: true,
            fallback_to_all: true,
            order_by: AdjacentOrder::PublishedAt,
        }
    }
}

/// 値の一部（Admin Studio で変えた値・以前のプラグインの設定）。`None` は次の段階の値を使う
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AdjacentOverrides {
    pub same_category: Option<bool>,
    pub fallback_to_all: Option<bool>,
    pub order_by: Option<AdjacentOrder>,
}

fn parse_bool(value: &str) -> Option<bool> {
    match value.trim().to_ascii_lowercase().as_str() {
        "true" | "1" | "yes" | "on" => Some(true),
        "false" | "0" | "no" | "off" => Some(false),
        _ => None,
    }
}

impl AdjacentOverrides {
    pub fn from_settings(settings: &HashMap<String, String>) -> Self {
        Self {
            same_category: settings
                .get(SAME_CATEGORY_SETTING)
                .and_then(|v| parse_bool(v)),
            fallback_to_all: settings
                .get(FALLBACK_TO_ALL_SETTING)
                .and_then(|v| parse_bool(v)),
            order_by: settings
                .get(ORDER_BY_SETTING)
                .and_then(|v| AdjacentOrder::parse(v)),
        }
    }

    /// DB から読み込む。読めなければ警告して設定ファイルの値で動かす
    pub async fn load(db: &dyn DatabaseAdapter) -> Self {
        match db.list_settings().await {
            Ok(settings) => Self::from_settings(&settings),
            Err(e) => {
                tracing::warn!(
                    "Could not load the previous/next article settings, using [adjacent_articles]: {}",
                    e
                );
                Self::default()
            }
        }
    }

    /// 以前の `[plugins.settings.official-post-navigation]` の値（読み方は以前の本体と同じ）
    pub fn from_legacy_plugin_settings(plugins: &PluginsConfig) -> Self {
        let get = |key: &str| plugins.setting(LEGACY_PLUGIN, key);
        Self {
            // `same_category` があればそれ、無ければ `mode`（`category` なら同じカテゴリ）
            same_category: get("same_category")
                .and_then(|v| parse_bool(&v))
                .or_else(|| {
                    get("mode")
                        .or_else(|| get("navigation_mode"))
                        .map(|mode| mode.trim().eq_ignore_ascii_case("category"))
                }),
            // `false` などと書いたときだけ止める
            fallback_to_all: get("fallback_to_all").map(|v| parse_bool(&v).unwrap_or(true)),
            order_by: get("order_by").map(|v| {
                if v.trim() == "created_at" {
                    AdjacentOrder::CreatedAt
                } else {
                    AdjacentOrder::PublishedAt
                }
            }),
        }
    }
}

/// 設定ファイルの値（Admin Studio で戻したときの値）。`[adjacent_articles]` に無い項目は、以前の
/// プラグインの設定、無ければ既定値
pub fn defaults(config: &AdjacentArticlesConfig, plugins: &PluginsConfig) -> AdjacentSettings {
    let legacy = AdjacentOverrides::from_legacy_plugin_settings(plugins);
    let fallback = AdjacentSettings::default();
    AdjacentSettings {
        same_category: config
            .same_category
            .or(legacy.same_category)
            .unwrap_or(fallback.same_category),
        fallback_to_all: config
            .fallback_to_all
            .or(legacy.fallback_to_all)
            .unwrap_or(fallback.fallback_to_all),
        order_by: config
            .order_by
            .or(legacy.order_by)
            .unwrap_or(fallback.order_by),
    }
}

/// 公開サイトで使う値
pub fn resolve(defaults: AdjacentSettings, overrides: &AdjacentOverrides) -> AdjacentSettings {
    AdjacentSettings {
        same_category: overrides.same_category.unwrap_or(defaults.same_category),
        fallback_to_all: overrides
            .fallback_to_all
            .unwrap_or(defaults.fallback_to_all),
        order_by: overrides.order_by.unwrap_or(defaults.order_by),
    }
}

/// 以前のプラグインの設定のうち、まだ書かれているもの（起動時の警告に使う）
pub fn legacy_keys_in_use(plugins: &PluginsConfig) -> Vec<&'static str> {
    LEGACY_KEYS
        .into_iter()
        .filter(|key| plugins.setting(LEGACY_PLUGIN, key).is_some())
        .collect()
}

/// 以前のプラグインの設定が残っていれば、移すように警告する
pub fn warn_legacy_settings(plugins: &PluginsConfig) {
    let keys = legacy_keys_in_use(plugins);
    if !keys.is_empty() {
        tracing::warn!(
            "[plugins.settings.{}] {} now belong to [adjacent_articles] (same_category, fallback_to_all, order_by). \
             They are still read where [adjacent_articles] does not set them, but this will stop in a later version",
            LEGACY_PLUGIN,
            keys.join(", ")
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plugins(toml: &str) -> PluginsConfig {
        toml::from_str(toml).unwrap()
    }

    #[test]
    fn defaults_without_any_setting() {
        let settings = defaults(&AdjacentArticlesConfig::default(), &plugins(""));
        assert_eq!(settings, AdjacentSettings::default());
        assert!(legacy_keys_in_use(&plugins("")).is_empty());
    }

    #[test]
    fn the_old_plugin_settings_are_read_where_the_new_section_is_silent() {
        let old = plugins(
            r#"
            [settings.official-post-navigation]
            mode = "date"
            fallback_to_all = "false"
            order_by = "created_at"
            layout = "cards"
            "#,
        );
        assert_eq!(
            defaults(&AdjacentArticlesConfig::default(), &old),
            AdjacentSettings {
                same_category: false,
                fallback_to_all: false,
                order_by: AdjacentOrder::CreatedAt,
            }
        );
        assert_eq!(
            legacy_keys_in_use(&old),
            ["mode", "fallback_to_all", "order_by"]
        );

        // [adjacent_articles] wins where it is written
        let config: AdjacentArticlesConfig = toml::from_str("same_category = true").unwrap();
        assert_eq!(
            defaults(&config, &old),
            AdjacentSettings {
                same_category: true,
                fallback_to_all: false,
                order_by: AdjacentOrder::CreatedAt,
            }
        );

        // `same_category` was read before `mode`, as before
        let both = plugins(
            r#"
            [settings.official-post-navigation]
            mode = "date"
            same_category = "yes"
            "#,
        );
        assert!(defaults(&AdjacentArticlesConfig::default(), &both).same_category);
    }

    #[test]
    fn admin_studio_values_win_and_bad_values_are_ignored() {
        let settings = HashMap::from([
            (SAME_CATEGORY_SETTING.to_string(), "false".to_string()),
            (ORDER_BY_SETTING.to_string(), "random".to_string()),
        ]);
        let overrides = AdjacentOverrides::from_settings(&settings);
        assert_eq!(
            overrides,
            AdjacentOverrides {
                same_category: Some(false),
                fallback_to_all: None,
                order_by: None,
            }
        );
        let config: AdjacentArticlesConfig = toml::from_str("order_by = \"created_at\"").unwrap();
        assert_eq!(
            resolve(defaults(&config, &plugins("")), &overrides),
            AdjacentSettings {
                same_category: false,
                fallback_to_all: true,
                order_by: AdjacentOrder::CreatedAt,
            }
        );
    }

    #[test]
    fn an_unknown_order_in_the_configuration_file_is_refused() {
        assert!(toml::from_str::<AdjacentArticlesConfig>("order_by = \"random\"").is_err());
    }
}
