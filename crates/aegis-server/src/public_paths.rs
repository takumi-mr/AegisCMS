//! Admin Studio から変えた、型ごとの公開 URL の先頭（スキーマの `public_path`）。
//!
//! スキーマの YAML は書き換えない（Git で管理しているスキーマと食い違わないように）。変更は DB の
//! `_aegis_settings` に `public_path.<型名>` として保存し、起動時にスキーマへ当てる（サイト名の
//! 変更と同じ仕組み）。ルートは起動時に決まるので、反映にはサーバーの再起動が要る

use std::collections::{BTreeMap, HashMap};

use aegis_schema::{SchemaDefinition, SchemaLoader};

/// `_aegis_settings` の名前の接頭辞（`public_path.works` など）
pub const SETTING_PREFIX: &str = "public_path.";

/// 公開ページを別に持っていて、`public_path` を付けられない型
pub fn is_fixed_type(content_type: &str, product_type: &str) -> bool {
    matches!(content_type, "page" | "category") || content_type == product_type
}

pub fn setting_name(content_type: &str) -> String {
    format!("{SETTING_PREFIX}{content_type}")
}

/// 入力を保存する値にそろえる。空は「上書きしない」（`None`）。`auto` は `/<型名>`
pub fn normalize_input(content_type: &str, input: Option<&str>) -> Option<String> {
    let value = input.map(str::trim).filter(|v| !v.is_empty())?;
    if value == "auto" {
        return Some(format!("/{content_type}"));
    }
    Some(value.to_string())
}

/// 起動時に呼ぶ。DB の上書きをスキーマの `public_path` に当てる。戻り値は、上書きする前の
/// （スキーマファイルの）値。上書きが検査に通らない・ほかの型と重複するときは、警告して無視する
/// （DB の値のせいでサーバーが起動しなくなるのを避ける）
pub fn apply_overrides(
    schemas: &mut HashMap<String, SchemaDefinition>,
    settings: &HashMap<String, String>,
) -> HashMap<String, Option<String>> {
    let files: HashMap<String, Option<String>> = schemas
        .iter()
        .map(|(content_type, schema)| (content_type.clone(), schema.public_path.clone()))
        .collect();
    let overrides: BTreeMap<&str, &str> = settings
        .iter()
        .filter_map(|(name, value)| Some((name.strip_prefix(SETTING_PREFIX)?, value.as_str())))
        .collect();
    for (content_type, value) in overrides {
        let Some(current) = schemas.get(content_type) else {
            tracing::warn!(
                "Ignoring the public_path setting for '{}': no such content type",
                content_type
            );
            continue;
        };
        let mut candidate = current.clone();
        candidate.public_path = Some(value.to_string());
        let mut all: Vec<SchemaDefinition> = schemas.values().cloned().collect();
        all.retain(|s| s.content_type != content_type);
        all.push(candidate.clone());
        let checked = candidate
            .validate_public_path()
            .and_then(|()| SchemaLoader::check_public_paths(&all).map_err(|e| e.to_string()));
        match checked {
            Ok(()) => {
                schemas.insert(content_type.to_string(), candidate);
            }
            Err(e) => tracing::warn!(
                "Ignoring the public_path setting for '{}' ({}): {}",
                content_type,
                value,
                e
            ),
        }
    }
    files
}

/// 再起動後に使われる各型の `public_path`（`configured`。上書きか、無ければスキーマファイルの値）が、
/// `content_type` について検査に通り、ほかの型と重複しないか
pub fn validate_configured(
    schemas: &HashMap<String, SchemaDefinition>,
    configured: &HashMap<String, Option<String>>,
    content_type: &str,
) -> Result<(), String> {
    let all: Vec<SchemaDefinition> = schemas
        .values()
        .map(|schema| {
            let mut next = schema.clone();
            next.public_path = configured.get(&schema.content_type).cloned().flatten();
            next
        })
        .collect();
    let Some(target) = all.iter().find(|s| s.content_type == content_type) else {
        return Err(format!("No such content type: {content_type}"));
    };
    target.validate_public_path()?;
    SchemaLoader::check_public_paths(&all).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn schema(content_type: &str, public_path: Option<&str>) -> SchemaDefinition {
        serde_json::from_value(serde_json::json!({
            "content_type": content_type,
            "label": content_type,
            "public_path": public_path,
            "fields": []
        }))
        .unwrap()
    }

    fn schemas(list: &[(&str, Option<&str>)]) -> HashMap<String, SchemaDefinition> {
        list.iter()
            .map(|(ct, path)| (ct.to_string(), schema(ct, *path)))
            .collect()
    }

    fn settings(list: &[(&str, &str)]) -> HashMap<String, String> {
        list.iter()
            .map(|(name, value)| (name.to_string(), value.to_string()))
            .collect()
    }

    #[test]
    fn test_normalize_input() {
        assert_eq!(
            normalize_input("works", Some(" /works ")).as_deref(),
            Some("/works")
        );
        assert_eq!(
            normalize_input("works", Some("auto")).as_deref(),
            Some("/works")
        );
        assert_eq!(normalize_input("works", Some("  ")), None);
        assert_eq!(normalize_input("works", None), None);
    }

    #[test]
    fn test_apply_overrides_changes_the_path_and_keeps_the_file_value() {
        let mut all = schemas(&[("article", None), ("works", Some("/works"))]);
        let files = apply_overrides(
            &mut all,
            &settings(&[("public_path.article", "/news"), ("site.title", "Ignored")]),
        );
        assert_eq!(all["article"].public_path.as_deref(), Some("/news"));
        assert_eq!(all["works"].public_path.as_deref(), Some("/works"));
        // 上書きの前の値（article はファイルに書いていない）
        assert_eq!(files["article"], None);
        assert_eq!(files["works"].as_deref(), Some("/works"));
    }

    #[test]
    fn test_apply_overrides_ignores_bad_values_instead_of_failing() {
        let mut all = schemas(&[("article", None), ("works", Some("/works")), ("page", None)]);
        let files = apply_overrides(
            &mut all,
            &settings(&[
                // 別の型と同じ
                ("public_path.article", "/works"),
                // 予約済み
                ("public_path.works", "/api"),
                // 公開ページを別に持つ型
                ("public_path.page", "/pages"),
                // 無い型
                ("public_path.missing", "/missing"),
            ]),
        );
        assert_eq!(all["article"].public_path, None);
        assert_eq!(all["works"].public_path.as_deref(), Some("/works"));
        assert_eq!(all["page"].public_path, None);
        assert_eq!(files.len(), 3);
    }

    #[test]
    fn test_validate_configured() {
        let all = schemas(&[("article", None), ("works", Some("/works")), ("memo", None)]);
        let check = |ct: &str, value: Option<&str>| {
            let mut configured: HashMap<String, Option<String>> = all
                .iter()
                .map(|(ct, s)| (ct.clone(), s.public_path.clone()))
                .collect();
            if let Some(slot) = configured.get_mut(ct) {
                *slot = value.map(str::to_string);
            }
            validate_configured(&all, &configured, ct)
        };

        assert!(check("memo", Some("/memo")).is_ok());
        assert!(check("article", Some("/news")).is_ok());
        // 値が無い（上書きを消した）なら、そのままの状態で通る
        assert!(check("works", None).is_ok());
        assert!(check("works", Some("/articles"))
            .unwrap_err()
            .contains("same public_path"));
        assert!(check("memo", Some("/works"))
            .unwrap_err()
            .contains("same public_path"));
        assert!(check("memo", Some("/a/b"))
            .unwrap_err()
            .contains("single segment"));
        assert!(check("memo", Some("/api"))
            .unwrap_err()
            .contains("reserved"));
        assert!(check("nothing", Some("/x"))
            .unwrap_err()
            .contains("No such content type"));
    }

    #[test]
    fn test_fixed_types() {
        assert!(is_fixed_type("page", "product"));
        assert!(is_fixed_type("category", "product"));
        assert!(is_fixed_type("product", "product"));
        assert!(!is_fixed_type("article", "product"));
        assert!(!is_fixed_type("works", "product"));
    }
}
