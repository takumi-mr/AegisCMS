//! 設定ファイルの `"env:NAME"` を、読み込み時に環境変数の値へ展開する。
//!
//! 対象は TOML のすべての文字列値（テーブルの値・配列の要素。キー名は対象外）。
//! 環境変数が未設定のときは、エラーにせず警告ログを出して、その項目を書かなかったことにする
//! （= 既定値になる。配列の要素なら、その要素を取り除く）。
//!
//! 環境変数の値は文字列で渡ってくるので、既定値が数値・真偽値の項目（`port = "env:PORT"` など）は、
//! 既定値の型に合わせて変換する。変換できないときも警告を出して既定値にする。

use toml::Value;

/// `env:` の接頭辞
const PREFIX: &str = "env:";

/// 展開の結果。`touched` が 0 なら、`env:` の値は 1 つもなかった
#[derive(Debug, Default, PartialEq, Eq)]
pub(crate) struct ExpandReport {
    /// 環境変数の値に置き換えた項目の数
    pub expanded: usize,
    /// 未設定・変換失敗で、取り除いた（既定値になる）項目の数
    pub dropped: usize,
}

impl ExpandReport {
    pub fn touched(&self) -> usize {
        self.expanded + self.dropped
    }
}

/// `root` の中の `env:` の値を展開する。`defaults` は型の手がかり（既定値の設定を TOML にしたもの）
pub(crate) fn expand_env(
    root: &mut Value,
    defaults: Option<&Value>,
    lookup: &dyn Fn(&str) -> Option<String>,
) -> ExpandReport {
    let mut report = ExpandReport::default();
    walk(root, defaults, "", lookup, &mut report);
    report
}

fn walk(
    value: &mut Value,
    hint: Option<&Value>,
    path: &str,
    lookup: &dyn Fn(&str) -> Option<String>,
    report: &mut ExpandReport,
) {
    match value {
        Value::Table(table) => {
            let keys: Vec<String> = table.keys().cloned().collect();
            for key in keys {
                let child_path = if path.is_empty() {
                    key.clone()
                } else {
                    format!("{path}.{key}")
                };
                let child_hint = hint.and_then(|h| h.get(key.as_str()));
                let Some(child) = table.get_mut(&key) else {
                    continue;
                };
                if let Some(reference) = env_reference(child) {
                    match resolve(&reference, child_hint, &child_path, lookup) {
                        Some(resolved) => {
                            *child = resolved;
                            report.expanded += 1;
                        }
                        None => {
                            table.remove(&key);
                            report.dropped += 1;
                        }
                    }
                } else {
                    walk(child, child_hint, &child_path, lookup, report);
                }
            }
        }
        Value::Array(items) => {
            let mut index = 0;
            while index < items.len() {
                let child_path = format!("{path}[{index}]");
                if let Some(reference) = env_reference(&items[index]) {
                    match resolve(&reference, None, &child_path, lookup) {
                        Some(resolved) => {
                            items[index] = resolved;
                            report.expanded += 1;
                            index += 1;
                        }
                        None => {
                            items.remove(index);
                            report.dropped += 1;
                        }
                    }
                } else {
                    walk(&mut items[index], None, &child_path, lookup, report);
                    index += 1;
                }
            }
        }
        _ => {}
    }
}

/// 値が `"env:NAME"` なら `NAME`（前後の空白を除く）を返す
fn env_reference(value: &Value) -> Option<String> {
    let Value::String(s) = value else {
        return None;
    };
    s.strip_prefix(PREFIX).map(|name| name.trim().to_string())
}

/// 環境変数 `name` を読み、`hint`（既定値）の型に合わせた値にする。読めない・変換できないときは警告して `None`
fn resolve(
    name: &str,
    hint: Option<&Value>,
    path: &str,
    lookup: &dyn Fn(&str) -> Option<String>,
) -> Option<Value> {
    if name.is_empty() {
        tracing::warn!("Config {path}: \"env:\" has no variable name - using the default value");
        return None;
    }
    let Some(raw) = lookup(name) else {
        tracing::warn!(
            "Config {path}: environment variable {name} is not set - using the default value"
        );
        return None;
    };
    let converted = match hint {
        Some(Value::Integer(_)) => raw.trim().parse::<i64>().ok().map(Value::Integer),
        Some(Value::Float(_)) => raw.trim().parse::<f64>().ok().map(Value::Float),
        Some(Value::Boolean(_)) => match raw.trim().to_ascii_lowercase().as_str() {
            "true" => Some(Value::Boolean(true)),
            "false" => Some(Value::Boolean(false)),
            _ => None,
        },
        _ => Some(Value::String(raw)),
    };
    if converted.is_none() {
        tracing::warn!(
            "Config {path}: environment variable {name} cannot be read as the expected type - using the default value"
        );
    }
    converted
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn run(toml_src: &str, defaults_src: &str, env: &[(&str, &str)]) -> (Value, ExpandReport) {
        let env: HashMap<String, String> = env
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        let mut value: Value = toml::from_str(toml_src).unwrap();
        let defaults: Value = toml::from_str(defaults_src).unwrap();
        let report = expand_env(&mut value, Some(&defaults), &|name| env.get(name).cloned());
        (value, report)
    }

    #[test]
    fn expands_strings_in_any_table_and_array() {
        let (value, report) = run(
            "[a.b]\nname = \"env:NAME\"\nlist = [\"x\", \"env:ITEM\"]\n",
            "",
            &[("NAME", "alice"), ("ITEM", "y")],
        );
        assert_eq!(value["a"]["b"]["name"].as_str(), Some("alice"));
        assert_eq!(value["a"]["b"]["list"][1].as_str(), Some("y"));
        assert_eq!(
            report,
            ExpandReport {
                expanded: 2,
                dropped: 0
            }
        );
    }

    #[test]
    fn unset_variable_drops_the_key_so_the_default_applies() {
        let (value, report) = run("[s]\nhost = \"env:NOPE\"\nport = 1\n", "", &[]);
        assert!(value["s"].get("host").is_none());
        assert_eq!(value["s"]["port"].as_integer(), Some(1));
        assert_eq!(
            report,
            ExpandReport {
                expanded: 0,
                dropped: 1
            }
        );
    }

    #[test]
    fn unset_variable_in_array_drops_the_element() {
        let (value, _) = run("l = [\"a\", \"env:NOPE\", \"c\"]\n", "", &[]);
        let list: Vec<_> = value["l"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap())
            .collect();
        assert_eq!(list, ["a", "c"]);
    }

    #[test]
    fn converts_to_the_type_of_the_default() {
        let (value, _) = run(
            "[s]\nport = \"env:P\"\non = \"env:B\"\nratio = \"env:R\"\ntext = \"env:T\"\n",
            "[s]\nport = 8080\non = false\nratio = 1.5\ntext = \"x\"\n",
            &[("P", " 9000 "), ("B", "TRUE"), ("R", "0.25"), ("T", "123")],
        );
        assert_eq!(value["s"]["port"].as_integer(), Some(9000));
        assert_eq!(value["s"]["on"].as_bool(), Some(true));
        assert_eq!(value["s"]["ratio"].as_float(), Some(0.25));
        assert_eq!(value["s"]["text"].as_str(), Some("123"));
    }

    #[test]
    fn unparsable_value_falls_back_to_the_default() {
        let (value, report) = run(
            "[s]\nport = \"env:P\"\n",
            "[s]\nport = 8080\n",
            &[("P", "not-a-number")],
        );
        assert!(value["s"].get("port").is_none());
        assert_eq!(report.dropped, 1);
    }

    #[test]
    fn empty_value_is_kept_but_empty_name_is_dropped() {
        let (value, _) = run("a = \"env:E\"\nb = \"env:\"\n", "", &[("E", "")]);
        assert_eq!(value["a"].as_str(), Some(""));
        assert!(value.get("b").is_none());
    }

    #[test]
    fn leaves_other_values_and_keys_alone() {
        let (value, report) = run("[\"env:key\"]\nx = \"plain\"\ny = 3\n", "", &[("key", "k")]);
        assert!(value.get("env:key").is_some());
        assert_eq!(report.touched(), 0);
    }
}
