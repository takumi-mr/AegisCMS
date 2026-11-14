//! 宣言的クエリ（`queries/*.yaml`）の定義・解析・検証。設計は docs/queries.md。
//!
//! ここは定義の読み込みと、スキーマとの突き合わせだけを持つ。SQL の組み立てと実行は
//! ホスト側（aegis-core / aegis-server）が、検証済みの定義から行う。

use crate::definition::{FieldType, SchemaDefinition};
use aegis_core::{AegisError, Result};
use serde::{Deserialize, Deserializer};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

/// 検証に使う上限（`[queries]` の設定から渡す）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct QueryLimits {
    /// `limit` の最大値（行数）
    pub max_limit: usize,
    /// `in` のリストの最大の長さ
    pub max_in_items: usize,
}

impl Default for QueryLimits {
    fn default() -> Self {
        Self {
            max_limit: 100,
            max_in_items: 100,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AggFn {
    Count,
    Sum,
    Avg,
    Min,
    Max,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Bucket {
    Day,
    Month,
    Year,
}

impl Bucket {
    pub fn as_str(self) -> &'static str {
        match self {
            Bucket::Day => "day",
            Bucket::Month => "month",
            Bucket::Year => "year",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Direction {
    #[default]
    Asc,
    Desc,
}

/// `null` を黙って捨てず、`Some(Value::Null)` として受け取る
/// （`{ gte: }` のような書き忘れを、検証でエラーにするため）
fn keep_null<'de, D: Deserializer<'de>>(d: D) -> std::result::Result<Option<Value>, D::Error> {
    Value::deserialize(d).map(Some)
}

fn keep_null_list<'de, D: Deserializer<'de>>(
    d: D,
) -> std::result::Result<Option<Vec<Value>>, D::Error> {
    Vec::<Value>::deserialize(d).map(Some)
}

/// 1 つの列に対する条件。書かれた演算子は AND で結ばれる
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Condition {
    #[serde(default, deserialize_with = "keep_null")]
    pub eq: Option<Value>,
    #[serde(default, deserialize_with = "keep_null")]
    pub ne: Option<Value>,
    #[serde(default, deserialize_with = "keep_null")]
    pub gt: Option<Value>,
    #[serde(default, deserialize_with = "keep_null")]
    pub gte: Option<Value>,
    #[serde(default, deserialize_with = "keep_null")]
    pub lt: Option<Value>,
    #[serde(default, deserialize_with = "keep_null")]
    pub lte: Option<Value>,
    #[serde(default, rename = "in", deserialize_with = "keep_null_list")]
    pub in_list: Option<Vec<Value>>,
    #[serde(default, deserialize_with = "keep_null")]
    pub is_null: Option<Value>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GroupBy {
    pub field: String,
    #[serde(default)]
    pub bucket: Option<Bucket>,
    #[serde(default, rename = "as")]
    pub as_name: Option<String>,
}

impl GroupBy {
    /// 結果の行でのキー名（`as` → バケット名 → 列名の順）
    pub fn output_name(&self) -> &str {
        match (&self.as_name, self.bucket) {
            (Some(n), _) => n,
            (None, Some(b)) => b.as_str(),
            (None, None) => &self.field,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SelectItem {
    #[serde(rename = "fn")]
    pub func: AggFn,
    #[serde(default)]
    pub field: Option<String>,
    #[serde(rename = "as")]
    pub as_name: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OrderBy {
    pub field: String,
    #[serde(default)]
    pub dir: Direction,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct QueryDefinition {
    pub name: String,
    pub from: String,
    #[serde(default, rename = "where")]
    pub conditions: BTreeMap<String, Condition>,
    #[serde(default)]
    pub group_by: Vec<GroupBy>,
    #[serde(default)]
    pub select: Vec<SelectItem>,
    #[serde(default)]
    pub order_by: Vec<OrderBy>,
    #[serde(default)]
    pub limit: Option<usize>,
}

/// クエリが使える列の型と、NULL を取りうるか
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Column {
    pub field_type: FieldType,
    pub nullable: bool,
}

/// 標準カラムのうち、クエリで使えるもの（公開済みのレコードだけを読むので NULL にならない）
const STANDARD_QUERY_COLUMNS: &[(&str, FieldType)] = &[
    ("title", FieldType::String),
    ("slug", FieldType::String),
    ("published_at", FieldType::Timestamp),
    ("created_at", FieldType::Timestamp),
    ("updated_at", FieldType::Timestamp),
];

/// 列名を、スキーマのフィールドか使える標準カラムから探す
pub fn resolve_column(schema: &SchemaDefinition, name: &str) -> Option<Column> {
    if let Some(f) = schema.fields.iter().find(|f| f.name == name) {
        return Some(Column {
            field_type: f.field_type,
            nullable: !(f.required && !f.nullable),
        });
    }
    STANDARD_QUERY_COLUMNS
        .iter()
        .find(|(n, _)| *n == name)
        .map(|(_, t)| Column {
            field_type: *t,
            nullable: false,
        })
}

fn is_valid_identifier(s: &str) -> bool {
    let mut chars = s.chars();
    matches!(chars.next(), Some('a'..='z'))
        && s.len() <= 64
        && chars.all(|c| matches!(c, 'a'..='z' | '0'..='9' | '_'))
}

fn err<T>(query: &str, msg: impl AsRef<str>) -> Result<T> {
    Err(AegisError::Schema(format!(
        "query '{}': {}",
        query,
        msg.as_ref()
    )))
}

/// 集計か、レコードの一覧か
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QueryMode {
    Aggregate,
    List,
}

impl QueryDefinition {
    pub fn parse(yaml: &str) -> Result<Self> {
        serde_saphyr::from_str::<QueryDefinition>(yaml)
            .map_err(|e| AegisError::Schema(format!("Failed to parse query YAML: {e}")))
    }

    /// `select` を省略したクエリはレコードの一覧を返す
    pub fn mode(&self) -> QueryMode {
        if self.select.is_empty() {
            QueryMode::List
        } else {
            QueryMode::Aggregate
        }
    }

    /// 使う列の和集合（公開側の列へ写す対象。docs/queries.md 4.1）
    pub fn used_columns(&self) -> BTreeSet<String> {
        let mut cols: BTreeSet<String> = self.conditions.keys().cloned().collect();
        cols.extend(self.group_by.iter().map(|g| g.field.clone()));
        cols.extend(self.select.iter().filter_map(|s| s.field.clone()));
        if self.mode() == QueryMode::List {
            cols.extend(self.order_by.iter().map(|o| o.field.clone()));
        }
        cols
    }

    /// スキーマと突き合わせて検証する。誤りがあれば最初の 1 件を返す
    pub fn validate(&self, schemas: &[SchemaDefinition], limits: &QueryLimits) -> Result<()> {
        let q = self.name.as_str();
        if !is_valid_identifier(q) {
            return err(
                q,
                "name must be lowercase letters, digits and '_' (start with a letter)",
            );
        }
        let Some(schema) = schemas.iter().find(|s| s.content_type == self.from) else {
            return err(q, format!("unknown content type '{}' in 'from'", self.from));
        };

        for (field, cond) in &self.conditions {
            let col = usable_column(q, schema, field)?;
            validate_condition(q, field, col, cond, limits)?;
        }

        let mut outputs: BTreeSet<String> = BTreeSet::new();
        let mut add_output = |name: &str| -> Result<()> {
            if !is_valid_identifier(name) {
                return err(
                    q,
                    format!("output name '{name}' must be lowercase letters, digits and '_'"),
                );
            }
            if !outputs.insert(name.to_string()) {
                return err(q, format!("duplicate output name '{name}'"));
            }
            Ok(())
        };

        match self.mode() {
            QueryMode::List => {
                if !self.group_by.is_empty() {
                    return err(
                        q,
                        "'group_by' needs 'select' (without 'select' the query returns records)",
                    );
                }
                for o in &self.order_by {
                    usable_column(q, schema, &o.field)?;
                }
            }
            QueryMode::Aggregate => {
                for g in &self.group_by {
                    let col = usable_column(q, schema, &g.field)?;
                    match (col.field_type, g.bucket) {
                        (FieldType::Timestamp, None) => {
                            return err(q, format!("group_by '{}' is a timestamp: set 'bucket' (day, month or year)", g.field));
                        }
                        (t, Some(_)) if t != FieldType::Timestamp => {
                            return err(
                                q,
                                format!(
                                    "'bucket' is only for timestamp fields ('{}' is {t:?})",
                                    g.field
                                ),
                            );
                        }
                        _ => {}
                    }
                    add_output(g.output_name())?;
                }
                for s in &self.select {
                    validate_select(q, schema, s)?;
                    add_output(&s.as_name)?;
                }
                for o in &self.order_by {
                    if !outputs.contains(&o.field) {
                        return err(
                            q,
                            format!(
                                "order_by '{}' must be a group_by output or a select 'as' name",
                                o.field
                            ),
                        );
                    }
                }
            }
        }

        if let Some(limit) = self.limit {
            if limit == 0 || limit > limits.max_limit {
                return err(
                    q,
                    format!("limit must be between 1 and {}", limits.max_limit),
                );
            }
        }
        Ok(())
    }
}

/// クエリで使える型の列を返す（`text`・`jsonb` は索引にできないため使えない）
fn usable_column(q: &str, schema: &SchemaDefinition, name: &str) -> Result<Column> {
    let Some(col) = resolve_column(schema, name) else {
        return err(
            q,
            format!("unknown field '{name}' in '{}'", schema.content_type),
        );
    };
    if matches!(col.field_type, FieldType::Text | FieldType::Jsonb) {
        return err(
            q,
            format!(
                "field '{name}' ({:?}) cannot be used in a query",
                col.field_type
            ),
        );
    }
    Ok(col)
}

fn validate_select(q: &str, schema: &SchemaDefinition, s: &SelectItem) -> Result<()> {
    match (s.func, &s.field) {
        (AggFn::Count, None) => Ok(()),
        (_, None) => err(q, format!("'{}' needs 'field'", fn_name(s.func))),
        (func, Some(field)) => {
            let col = usable_column(q, schema, field)?;
            let ok = match func {
                AggFn::Count => true,
                AggFn::Sum | AggFn::Avg => col.field_type == FieldType::Integer,
                AggFn::Min | AggFn::Max => {
                    matches!(col.field_type, FieldType::Integer | FieldType::Timestamp)
                }
            };
            if ok {
                Ok(())
            } else {
                err(
                    q,
                    format!(
                        "'{}' cannot be applied to '{field}' ({:?})",
                        fn_name(func),
                        col.field_type
                    ),
                )
            }
        }
    }
}

fn fn_name(f: AggFn) -> &'static str {
    match f {
        AggFn::Count => "count",
        AggFn::Sum => "sum",
        AggFn::Avg => "avg",
        AggFn::Min => "min",
        AggFn::Max => "max",
    }
}

fn validate_condition(
    q: &str,
    field: &str,
    col: Column,
    c: &Condition,
    limits: &QueryLimits,
) -> Result<()> {
    let ordered = matches!(col.field_type, FieldType::Integer | FieldType::Timestamp);
    let mut any = false;

    let single: [(&str, &Option<Value>, bool); 6] = [
        ("eq", &c.eq, true),
        ("ne", &c.ne, true),
        ("gt", &c.gt, ordered),
        ("gte", &c.gte, ordered),
        ("lt", &c.lt, ordered),
        ("lte", &c.lte, ordered),
    ];
    for (op, value, allowed) in single {
        let Some(v) = value else { continue };
        any = true;
        if !allowed {
            return err(
                q,
                format!("'{op}' cannot be used on '{field}' ({:?})", col.field_type),
            );
        }
        check_value(q, field, op, col.field_type, v)?;
    }
    if let Some(list) = &c.in_list {
        any = true;
        if matches!(col.field_type, FieldType::Boolean) {
            return err(q, format!("'in' cannot be used on '{field}' (Boolean)"));
        }
        if list.is_empty() || list.len() > limits.max_in_items {
            return err(
                q,
                format!(
                    "'in' on '{field}' needs 1 to {} values",
                    limits.max_in_items
                ),
            );
        }
        for v in list {
            check_value(q, field, "in", col.field_type, v)?;
        }
    }
    if let Some(v) = &c.is_null {
        any = true;
        if !matches!(v, Value::Bool(_)) {
            return err(q, format!("'is_null' on '{field}' must be true or false"));
        }
        if !col.nullable {
            return err(
                q,
                format!("'is_null' on '{field}': the field is never null"),
            );
        }
    }
    if !any {
        return err(q, format!("condition on '{field}' has no operator"));
    }
    Ok(())
}

/// 比較する値の型を列に合わせて確かめる。`null` は `is_null` で書く
fn check_value(q: &str, field: &str, op: &str, ty: FieldType, v: &Value) -> Result<()> {
    let ok = match (ty, v) {
        (_, Value::Null) => {
            return err(
                q,
                format!("'{op}' on '{field}' cannot take null: use 'is_null'"),
            );
        }
        (FieldType::String | FieldType::Relation, Value::String(_)) => true,
        (FieldType::Integer, Value::Number(n)) => n.is_i64() || n.is_u64(),
        (FieldType::Boolean, Value::Bool(_)) => true,
        (FieldType::Timestamp, Value::String(s)) => chrono::DateTime::parse_from_rfc3339(s).is_ok(),
        _ => false,
    };
    if ok {
        Ok(())
    } else {
        err(q, format!("'{op}' on '{field}': value does not match type {ty:?} (timestamps are RFC 3339 strings)"))
    }
}

impl QueryDefinition {
    /// テーマ開発用のサンプルの結果。クエリは実行せず、結果の形（出力の型）に合う決まった値を作る。
    /// 集計は最大 3 行（`group_by` が無ければ 1 行）、一覧は `limit` と 3 の小さいほうの件数。
    /// 値に意味は無い。テーマの見た目と、テンプレートが読むキーの確認に使う。`schema` は `from` の型
    pub fn sample_result(&self, schema: &SchemaDefinition) -> serde_json::Value {
        use serde_json::{json, Map, Value};
        let column = |name: &str| resolve_column(schema, name).map(|c| c.field_type);
        match self.mode() {
            QueryMode::List => {
                let count = self.limit.unwrap_or(3).min(3);
                let items = (1..=count).map(|i| {
                    let mut fields = Map::new();
                    for f in &schema.fields {
                        let value = match f.field_type {
                            FieldType::String => json!(format!("{} {i}", f.name)),
                            FieldType::Text => json!(format!("{} {i}: sample text", f.name)),
                            FieldType::Integer => json!(i * 100),
                            FieldType::Boolean => json!(i % 2 == 1),
                            FieldType::Timestamp => json!(format!("2026-01-{i:02}T00:00:00Z")),
                            // 参照先は分からないので載せない（テーマは `relations` で読む）
                            FieldType::Relation | FieldType::Jsonb => continue,
                        };
                        if f.name != crate::ACCESS_FIELD && f.name != crate::PURCHASE_PRICE_FIELD {
                            fields.insert(f.name.clone(), value);
                        }
                    }
                    fields.insert("url".into(), json!(format!("/{}/sample-{i}", self.from)));
                    json!({
                        "id": format!("sample-{i}"),
                        "content_type": self.from,
                        "title": format!("Sample {} {i}", schema.label),
                        "slug": format!("sample-{i}"),
                        "excerpt": "Sample excerpt text for the preview.",
                        "published_at": format!("2026-01-{i:02}T00:00:00Z"),
                        "created_at": format!("2026-01-{i:02}T00:00:00Z"),
                        "fields": fields,
                        "relations": {},
                    })
                });
                Value::Array(items.collect())
            }
            QueryMode::Aggregate => {
                let rows = if self.group_by.is_empty() { 1 } else { 3 };
                let result = (1..=rows).map(|i| {
                    let mut row = Map::new();
                    for g in &self.group_by {
                        let value = match (column(&g.field), g.bucket) {
                            (_, Some(Bucket::Month)) => json!(format!("2026-{i:02}")),
                            (_, Some(Bucket::Day)) => json!(format!("2026-01-{i:02}")),
                            (_, Some(Bucket::Year)) => json!((2025 + i).to_string()),
                            (Some(FieldType::Integer), None) => json!(i * 10),
                            (Some(FieldType::Boolean), None) => json!(i % 2 == 1),
                            // 参照先は `{id, title, slug}` に展開される
                            (Some(FieldType::Relation), None) => json!({
                                "id": format!("sample-{i}"),
                                "title": format!("Sample {} {i}", g.field),
                                "slug": format!("sample-{i}"),
                            }),
                            _ => json!(format!("{} {i}", g.field)),
                        };
                        row.insert(g.output_name().to_string(), value);
                    }
                    // 先頭の行ほど大きい（`order_by: desc` の結果に見える）
                    let rank = (4 - i) as i64;
                    for s in &self.select {
                        let value = match (s.func, s.field.as_deref().and_then(column)) {
                            (AggFn::Count, _) => json!(rank * 10),
                            (AggFn::Sum, _) => json!(rank * 1000),
                            (AggFn::Avg, _) => json!(rank as f64 * 500.5),
                            (_, Some(FieldType::Integer)) => json!(rank * 100),
                            (_, Some(FieldType::Timestamp)) => {
                                json!(format!("2026-0{rank}-01T00:00:00"))
                            }
                            _ => json!(format!("{} {rank}", s.as_name)),
                        };
                        row.insert(s.as_name.clone(), value);
                    }
                    Value::Object(row)
                });
                Value::Array(result.collect())
            }
        }
    }
}

/// `queries/` の YAML をすべて読み、スキーマと突き合わせて検証する。
/// ファイル名（拡張子を除く）と `name` は一致させる。1 つでも誤りがあればエラー（起動を止める）
pub fn load_queries<P: AsRef<Path>>(
    dir: P,
    schemas: &[SchemaDefinition],
    limits: &QueryLimits,
) -> Result<Vec<QueryDefinition>> {
    let path = dir.as_ref();
    let mut queries: Vec<QueryDefinition> = Vec::new();
    if !path.exists() {
        return Ok(queries);
    }
    for entry in walkdir::WalkDir::new(path)
        .sort_by_file_name()
        .min_depth(1)
        .max_depth(1)
    {
        let entry = entry.map_err(|e| AegisError::Schema(e.to_string()))?;
        let file = entry.path();
        let is_yaml = file.extension().is_some_and(|e| e == "yaml" || e == "yml");
        if !file.is_file() || !is_yaml {
            continue;
        }
        let text = std::fs::read_to_string(file)
            .map_err(|e| AegisError::Schema(format!("Failed to read query file {file:?}: {e}")))?;
        let query = QueryDefinition::parse(&text)
            .map_err(|e| AegisError::Schema(format!("{file:?}: {e}")))?;
        let stem = file.file_stem().and_then(|s| s.to_str()).unwrap_or("");
        if stem != query.name {
            return err(
                &query.name,
                format!("file name '{stem}' does not match 'name'"),
            );
        }
        query.validate(schemas, limits)?;
        if queries.iter().any(|q| q.name == query.name) {
            return err(&query.name, "duplicate query name");
        }
        queries.push(query);
    }
    queries.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(queries)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::loader::SchemaLoader;

    fn schemas() -> Vec<SchemaDefinition> {
        let product = "content_type: product\nlabel: P\nfields:\n  - name: price\n    type: integer\n    required: true\n  - name: brand\n    type: string\n  - name: on_sale\n    type: boolean\n  - name: note\n    type: text\n  - name: meta\n    type: jsonb\n  - name: discount_price\n    type: integer\n    nullable: true\n";
        vec![SchemaLoader::load_from_str(product).unwrap()]
    }

    fn check(yaml: &str) -> Result<()> {
        QueryDefinition::parse(yaml)?.validate(&schemas(), &QueryLimits::default())
    }

    fn rejected(yaml: &str, needle: &str) {
        let e = check(yaml).expect_err("should be rejected").to_string();
        assert!(e.contains(needle), "'{needle}' not in: {e}");
    }

    #[test]
    fn accepts_the_documented_examples() {
        check(
            "name: monthly_sales\nfrom: product\nwhere:\n  price: { gte: 1000 }\ngroup_by:\n  - { field: published_at, bucket: month }\nselect:\n  - { fn: count, as: n }\n  - { fn: sum, field: price, as: total }\norder_by:\n  - { field: month, dir: desc }\nlimit: 12\n",
        )
        .unwrap();
        check("name: by_category\nfrom: product\ngroup_by: [{ field: category_id }]\nselect: [{ fn: count, as: n }]\norder_by: [{ field: n, dir: desc }]\n").unwrap();
        check("name: sale\nfrom: product\nwhere: { on_sale: { eq: true } }\norder_by: [{ field: published_at, dir: desc }]\nlimit: 4\n").unwrap();
        check("name: price_range\nfrom: product\nselect:\n  - { fn: min, field: price, as: lowest }\n  - { fn: max, field: price, as: highest }\n").unwrap();
    }

    #[test]
    fn mode_follows_select() {
        let list = QueryDefinition::parse("name: a\nfrom: product\n").unwrap();
        assert_eq!(list.mode(), QueryMode::List);
        let agg =
            QueryDefinition::parse("name: a\nfrom: product\nselect: [{ fn: count, as: n }]\n")
                .unwrap();
        assert_eq!(agg.mode(), QueryMode::Aggregate);
    }

    #[test]
    fn null_is_only_written_with_is_null() {
        check("name: a\nfrom: product\nwhere: { discount_price: { is_null: false } }\n").unwrap();
        rejected(
            "name: a\nfrom: product\nwhere: { price: { eq: null } }\n",
            "use 'is_null'",
        );
        // 値の書き忘れ（YAML では null）も、黙って通さない
        rejected(
            "name: a\nfrom: product\nwhere: { price: { gte: } }\n",
            "use 'is_null'",
        );
        rejected(
            "name: a\nfrom: product\nwhere: { price: { in: [1, null] } }\n",
            "use 'is_null'",
        );
        rejected(
            "name: a\nfrom: product\nwhere: { price: { is_null: true } }\n",
            "never null",
        );
        rejected(
            "name: a\nfrom: product\nwhere: { price: { is_null: 1 } }\n",
            "true or false",
        );
        rejected(
            "name: a\nfrom: product\nwhere: { price: {} }\n",
            "no operator",
        );
    }

    #[test]
    fn rejects_bad_conditions() {
        rejected(
            "name: a\nfrom: product\nwhere: { price: { eq: \"x\" } }\n",
            "does not match",
        );
        rejected(
            "name: a\nfrom: product\nwhere: { brand: { gt: \"a\" } }\n",
            "cannot be used",
        );
        rejected(
            "name: a\nfrom: product\nwhere: { on_sale: { in: [true] } }\n",
            "cannot be used",
        );
        rejected(
            "name: a\nfrom: product\nwhere: { published_at: { gte: yesterday } }\n",
            "RFC 3339",
        );
        check(
            "name: a\nfrom: product\nwhere: { published_at: { gte: \"2026-01-01T00:00:00Z\" } }\n",
        )
        .unwrap();
        rejected(
            "name: a\nfrom: product\nwhere: { price: { in: [] } }\n",
            "needs 1 to",
        );
        rejected(
            "name: a\nfrom: product\nwhere: { price: { bogus: 1 } }\n",
            "unknown field",
        );
    }

    #[test]
    fn rejects_unknown_or_unusable_fields_and_types() {
        rejected("name: a\nfrom: nothing\n", "unknown content type");
        rejected(
            "name: a\nfrom: product\nwhere: { missing: { eq: 1 } }\n",
            "unknown field 'missing'",
        );
        rejected(
            "name: a\nfrom: product\nwhere: { note: { eq: \"x\" } }\n",
            "cannot be used in a query",
        );
        rejected(
            "name: a\nfrom: product\nwhere: { meta: { is_null: true } }\n",
            "cannot be used in a query",
        );
        // ブロック本文などの標準カラムは使えない
        rejected(
            "name: a\nfrom: product\nwhere: { blocks_json: { eq: \"x\" } }\n",
            "unknown field",
        );
    }

    #[test]
    fn validates_aggregates_and_grouping() {
        rejected(
            "name: a\nfrom: product\nselect: [{ fn: sum, field: brand, as: s }]\n",
            "cannot be applied",
        );
        rejected(
            "name: a\nfrom: product\nselect: [{ fn: sum, as: s }]\n",
            "needs 'field'",
        );
        rejected("name: a\nfrom: product\ngroup_by: [{ field: published_at }]\nselect: [{ fn: count, as: n }]\n", "set 'bucket'");
        rejected("name: a\nfrom: product\ngroup_by: [{ field: brand, bucket: month }]\nselect: [{ fn: count, as: n }]\n", "only for timestamp");
        rejected(
            "name: a\nfrom: product\ngroup_by: [{ field: brand }]\n",
            "needs 'select'",
        );
        rejected("name: a\nfrom: product\ngroup_by: [{ field: brand }]\nselect: [{ fn: count, as: brand }]\n", "duplicate output name");
        rejected("name: a\nfrom: product\nselect: [{ fn: count, as: n }]\norder_by: [{ field: price }]\n", "must be a group_by output");
        check("name: a\nfrom: product\ngroup_by: [{ field: published_at, bucket: year, as: y }]\nselect: [{ fn: count, as: n }]\norder_by: [{ field: y }]\n").unwrap();
        check("name: a\nfrom: product\nselect: [{ fn: count, field: discount_price, as: n }]\n")
            .unwrap();
    }

    #[test]
    fn rejects_bad_names_limits_and_unknown_keys() {
        rejected("name: Bad-Name\nfrom: product\n", "lowercase");
        rejected("name: a\nfrom: product\nlimit: 0\n", "between 1 and");
        rejected("name: a\nfrom: product\nlimit: 101\n", "between 1 and");
        check("name: a\nfrom: product\nlimit: 100\n").unwrap();
        assert!(QueryDefinition::parse("name: a\nfrom: product\nwhat: 1\n").is_err());
        assert!(QueryDefinition::parse(
            "name: a\nfrom: product\nselect: [{ fn: median, as: n }]\n"
        )
        .is_err());
    }

    #[test]
    fn used_columns_are_the_union_of_every_clause() {
        let q = QueryDefinition::parse(
            "name: a\nfrom: product\nwhere: { on_sale: { eq: true } }\ngroup_by: [{ field: brand }]\nselect: [{ fn: sum, field: price, as: total }]\norder_by: [{ field: total }]\n",
        )
        .unwrap();
        let cols: Vec<String> = q.used_columns().into_iter().collect();
        assert_eq!(cols, vec!["brand", "on_sale", "price"]);
    }

    #[test]
    fn load_queries_checks_file_name_and_duplicates() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("count.yaml"),
            "name: count\nfrom: product\nselect: [{ fn: count, as: n }]\n",
        )
        .unwrap();
        let loaded = load_queries(dir.path(), &schemas(), &QueryLimits::default()).unwrap();
        assert_eq!(loaded.len(), 1);

        std::fs::write(
            dir.path().join("other.yaml"),
            "name: mismatch\nfrom: product\n",
        )
        .unwrap();
        let e = load_queries(dir.path(), &schemas(), &QueryLimits::default())
            .unwrap_err()
            .to_string();
        assert!(e.contains("does not match 'name'"), "{e}");

        let missing = dir.path().join("none");
        assert!(load_queries(missing, &schemas(), &QueryLimits::default())
            .unwrap()
            .is_empty());
    }
    #[test]
    fn sample_results_follow_the_shape_of_real_results() {
        let schema = &schemas()[0];
        let sample = |yaml: &str| QueryDefinition::parse(yaml).unwrap().sample_result(schema);

        // 集計: group_by の値と select の `as` がキー。先頭の行ほど大きい
        assert_eq!(
            sample(
                "name: m
from: product
group_by: [{ field: published_at, bucket: month }]
select:
  - { fn: count, as: n }
  - { fn: sum, field: price, as: total }
"
            ),
            serde_json::json!([
                {"month": "2026-01", "n": 30, "total": 3000},
                {"month": "2026-02", "n": 20, "total": 2000},
                {"month": "2026-03", "n": 10, "total": 1000}
            ])
        );
        // group_by が無ければ 1 行。avg は小数、min / max は列の型に従う
        assert_eq!(
            sample(
                "name: r
from: product
select:
  - { fn: min, field: price, as: lowest }
  - { fn: avg, field: price, as: mean }
"
            ),
            serde_json::json!([{"lowest": 300, "mean": 1501.5}])
        );
        // リレーション列は {id, title, slug}
        let by_category = sample(
            "name: c
from: product
group_by: [{ field: category_id }]
select: [{ fn: count, as: n }]
",
        );
        assert_eq!(by_category[0]["category_id"]["slug"], "sample-1");
        assert_eq!(by_category.as_array().unwrap().len(), 3);

        // 一覧: テーマの `items` と同じキー。本文は含めない。limit と 3 の小さいほう
        let list = sample(
            "name: l
from: product
where: { on_sale: { eq: true } }
limit: 2
",
        );
        let items = list.as_array().unwrap();
        assert_eq!(items.len(), 2);
        for key in [
            "id",
            "content_type",
            "title",
            "slug",
            "excerpt",
            "published_at",
            "created_at",
            "fields",
            "relations",
        ] {
            assert!(items[0].get(key).is_some(), "{key}");
        }
        assert_eq!(items[0]["fields"]["price"], 100);
        assert_eq!(items[0]["fields"]["on_sale"], true);
        assert_eq!(items[0]["fields"]["url"], "/product/sample-1");
        assert!(items[0]["fields"].get("access").is_none());
        assert!(items[0].get("content_html").is_none());
    }
}
