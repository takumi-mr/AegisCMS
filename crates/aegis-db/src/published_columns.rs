//! 宣言的クエリ（docs/queries.md）が読む「公開側の列」。
//!
//! 公開ページは公開ポインターのスナップショットを読む。作業版の列を集計すると、公開後に編集中の
//! 値が出てしまうので、クエリが使う列だけを**公開操作と同じトランザクションで**型ごとの副表
//! `_aegis_published_<type>` へ写す（docs/queries.md 4.1）。
//!
//! このモジュールは DB に依存しない部分（副表の定義・DDL・値の変換・クエリの SQL 組み立て・
//! 結果の変換）だけを持つ。実行は各アダプタが行う。
//!
//! - 副表に入るのは、公開中で `access` が空（誰でも読める）のコンテンツだけ
//! - 日時は UTC の `YYYY-MM-DDTHH:MM:SS` に揃えて持つ（文字列の比較と `SUBSTR` で並べ替え・日付の区切りができる）
//! - 副表の列や中身が今の定義と合わなければ、起動時に作り直す（`signature`）

use crate::ContentRecord;
use aegis_core::{AegisError, Result};
use aegis_schema::query::{resolve_column, AggFn, Bucket, Condition, Direction, QueryMode};
use aegis_schema::{FieldType, QueryDefinition, SchemaDefinition, SchemaMigration, SqlDialect};
use serde_json::{Map, Value};
use std::collections::BTreeMap;

/// 副表の名前（`content_<type>` と同じく `-` は `_` にする）
pub fn published_table_name(content_type: &str) -> String {
    format!("_aegis_published_{}", content_type.replace('-', "_"))
}

/// 副表の定義の控え（`signature`）を持つ表
pub const PUBLISHED_META_TABLE: &str = "_aegis_published_meta";

/// NULL の型
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NullKind {
    Text,
    Int,
    Bool,
}

impl NullKind {
    pub fn of(field_type: FieldType) -> Self {
        match field_type {
            FieldType::Integer => NullKind::Int,
            FieldType::Boolean => NullKind::Bool,
            _ => NullKind::Text,
        }
    }
}

/// バインドする値
#[derive(Debug, Clone, PartialEq)]
pub enum PubValue {
    /// 型つきの NULL（PostgreSQL は、型の違う NULL を列に入れられない）
    Null(NullKind),
    Text(String),
    Int(i64),
    Bool(bool),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PublishedColumn {
    pub name: String,
    pub field_type: FieldType,
    /// スキーマで `index: true` の列には、副表にも索引を作る（索引は宣言に任せる）
    pub indexed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PublishedTable {
    pub content_type: String,
    pub table: String,
    pub columns: Vec<PublishedColumn>,
}

/// 全クエリが使う列から求めた、型ごとの副表の計画
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PublishedColumnsPlan {
    pub tables: BTreeMap<String, PublishedTable>,
}

impl PublishedColumnsPlan {
    /// 検証済みのクエリから、使う列の和集合を型ごとに求める
    pub fn build(queries: &[QueryDefinition], schemas: &[SchemaDefinition]) -> Self {
        let mut tables: BTreeMap<String, PublishedTable> = BTreeMap::new();
        for q in queries {
            let Some(schema) = schemas.iter().find(|s| s.content_type == q.from) else {
                continue;
            };
            let table = tables
                .entry(q.from.clone())
                .or_insert_with(|| PublishedTable {
                    content_type: q.from.clone(),
                    table: published_table_name(&q.from),
                    columns: Vec::new(),
                });
            for name in q.used_columns() {
                if table.columns.iter().any(|c| c.name == name) {
                    continue;
                }
                let Some(col) = resolve_column(schema, &name) else {
                    continue;
                };
                let indexed = schema.fields.iter().any(|f| f.name == name && f.index);
                table.columns.push(PublishedColumn {
                    name,
                    field_type: col.field_type,
                    indexed,
                });
            }
        }
        for t in tables.values_mut() {
            t.columns.sort_by(|a, b| a.name.cmp(&b.name));
        }
        Self { tables }
    }

    pub fn get(&self, content_type: &str) -> Option<&PublishedTable> {
        self.tables.get(content_type)
    }
}

/// バインド用のプレースホルダー（1 始まり）
pub fn placeholder(dialect: SqlDialect, n: usize) -> String {
    match dialect {
        SqlDialect::Sqlite => format!("?{n}"),
        SqlDialect::Postgres => format!("${n}"),
        SqlDialect::MySql => "?".to_string(),
    }
}

impl PublishedTable {
    /// 定義の控え。列の名前と型・索引が変わったら値が変わる（変わったら作り直す）
    pub fn signature(&self) -> String {
        let mut s = String::from("v1");
        for c in &self.columns {
            s.push_str(&format!(
                "|{}:{:?}:{}",
                c.name,
                c.field_type,
                if c.indexed { "i" } else { "-" }
            ));
        }
        s
    }

    /// `CREATE TABLE` と、索引の `CREATE INDEX`
    pub fn create_sql(&self, dialect: SqlDialect) -> Vec<String> {
        let id_type = match dialect {
            SqlDialect::Sqlite => "TEXT",
            _ => "VARCHAR(64)",
        };
        let mut cols = vec![format!("content_id {id_type} PRIMARY KEY")];
        for c in &self.columns {
            cols.push(format!(
                "{} {}",
                c.name,
                SchemaMigration::column_type(c.field_type, dialect)
            ));
        }
        let mut sqls = vec![format!(
            "CREATE TABLE IF NOT EXISTS {} ({});",
            self.table,
            cols.join(", ")
        )];
        for c in self.columns.iter().filter(|c| c.indexed) {
            sqls.push(match dialect {
                SqlDialect::Sqlite | SqlDialect::Postgres => format!(
                    "CREATE INDEX IF NOT EXISTS idx_{}_{} ON {} ({});",
                    self.table, c.name, self.table, c.name
                ),
                // MySQL の `CREATE INDEX` には `IF NOT EXISTS` が無い（表を作り直す直後にだけ作る）
                SqlDialect::MySql => format!(
                    "CREATE INDEX idx_{}_{} ON {} ({});",
                    self.table, c.name, self.table, c.name
                ),
            });
        }
        sqls
    }

    pub fn drop_sql(&self) -> String {
        format!("DROP TABLE IF EXISTS {};", self.table)
    }

    pub fn delete_sql(&self, dialect: SqlDialect) -> String {
        format!(
            "DELETE FROM {} WHERE content_id = {};",
            self.table,
            placeholder(dialect, 1)
        )
    }

    /// 列の並びは `content_id` のあとに `columns` の順
    pub fn insert_sql(&self, dialect: SqlDialect) -> String {
        let mut names = vec!["content_id".to_string()];
        names.extend(self.columns.iter().map(|c| c.name.clone()));
        let phs: Vec<String> = (1..=names.len()).map(|i| placeholder(dialect, i)).collect();
        format!(
            "INSERT INTO {} ({}) VALUES ({});",
            self.table,
            names.join(", "),
            phs.join(", ")
        )
    }

    /// 公開版のレコードから、副表の 1 行の値（`content_id` を除く）を求める。
    /// `access` が空でないレコードは副表に入れない（`None`）
    pub fn values_for(&self, record: &ContentRecord) -> Option<Vec<PubValue>> {
        if record.is_restricted() {
            return None;
        }
        Some(
            self.columns
                .iter()
                .map(|c| column_value(c, record))
                .collect(),
        )
    }
}

/// 日時を UTC の `YYYY-MM-DDTHH:MM:SS` にそろえる（RFC 3339 として読めなければ `None`）
pub fn normalize_timestamp(text: &str) -> Option<String> {
    chrono::DateTime::parse_from_rfc3339(text).ok().map(|t| {
        t.with_timezone(&chrono::Utc)
            .format("%Y-%m-%dT%H:%M:%S")
            .to_string()
    })
}

fn column_value(col: &PublishedColumn, record: &ContentRecord) -> PubValue {
    let json: Option<Value> = match col.name.as_str() {
        "title" => Some(Value::String(record.title.clone())),
        "slug" => Some(Value::String(record.slug.clone())),
        "published_at" => record.published_at.clone().map(Value::String),
        "created_at" => Some(Value::String(record.created_at.clone())),
        "updated_at" => Some(Value::String(record.updated_at.clone())),
        other => record.custom_fields.get(other).cloned(),
    };
    let null = PubValue::Null(NullKind::of(col.field_type));
    let Some(value) = json else {
        return null;
    };
    match (col.field_type, value) {
        (_, Value::Null) => null,
        (FieldType::String | FieldType::Relation, Value::String(s)) => PubValue::Text(s),
        (FieldType::Integer, Value::Number(n)) => n.as_i64().map_or(null, PubValue::Int),
        (FieldType::Integer, Value::String(s)) => s.trim().parse().map_or(null, PubValue::Int),
        (FieldType::Boolean, Value::Bool(b)) => PubValue::Bool(b),
        (FieldType::Boolean, Value::Number(n)) => PubValue::Bool(n.as_i64().unwrap_or(0) != 0),
        (FieldType::Boolean, Value::String(s)) => {
            PubValue::Bool(matches!(s.as_str(), "1" | "true"))
        }
        (FieldType::Timestamp, Value::String(s)) => {
            normalize_timestamp(&s).map_or(null, PubValue::Text)
        }
        _ => null,
    }
}

// ---------------------------------------------------------------------------
// クエリの SQL
// ---------------------------------------------------------------------------

/// 集計の結果の列を JSON にするときの型
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutKind {
    Int,
    Float,
    Bool,
    Text,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QueryOutput {
    pub name: String,
    pub kind: OutKind,
}

/// 組み立てた SQL。`binds` は SQL に現れる順
#[derive(Debug, Clone, PartialEq)]
pub struct BuiltQuery {
    pub sql: String,
    pub binds: Vec<PubValue>,
    pub mode: QueryMode,
    /// 集計モードの出力の列（SQL の SELECT の順。値はすべて文字列にして読む）
    pub outputs: Vec<QueryOutput>,
}

fn text_cast(dialect: SqlDialect, expr: &str) -> String {
    match dialect {
        SqlDialect::MySql => format!("CAST({expr} AS CHAR)"),
        _ => format!("CAST({expr} AS TEXT)"),
    }
}

fn bucket_expr(col: &str, bucket: Bucket) -> String {
    let len = match bucket {
        Bucket::Day => 10,
        Bucket::Month => 7,
        Bucket::Year => 4,
    };
    format!("SUBSTR({col}, 1, {len})")
}

fn agg_expr(dialect: SqlDialect, func: AggFn, col: Option<&str>) -> (String, OutKind) {
    match (func, col) {
        (AggFn::Count, None) => ("COUNT(*)".into(), OutKind::Int),
        (AggFn::Count, Some(c)) => (format!("COUNT({c})"), OutKind::Int),
        (AggFn::Sum, Some(c)) => (
            match dialect {
                SqlDialect::Sqlite => format!("SUM({c})"),
                SqlDialect::Postgres => format!("CAST(SUM({c}) AS BIGINT)"),
                SqlDialect::MySql => format!("CAST(SUM({c}) AS SIGNED)"),
            },
            OutKind::Int,
        ),
        (AggFn::Avg, Some(c)) => (
            match dialect {
                SqlDialect::Sqlite => format!("AVG({c})"),
                SqlDialect::Postgres => format!("CAST(AVG({c}) AS DOUBLE PRECISION)"),
                SqlDialect::MySql => format!("CAST(AVG({c}) AS DOUBLE)"),
            },
            OutKind::Float,
        ),
        (AggFn::Min, Some(c)) => (format!("MIN({c})"), OutKind::Text),
        (AggFn::Max, Some(c)) => (format!("MAX({c})"), OutKind::Text),
        // 検証で弾かれる組み合わせ
        (_, None) => ("NULL".into(), OutKind::Text),
    }
}

/// 値を列の型に合わせてバインド値にする（検証済みの定義が前提）
fn bind_value(ty: FieldType, v: &serde_yaml::Value) -> PubValue {
    use serde_yaml::Value as Y;
    let null = PubValue::Null(NullKind::of(ty));
    match (ty, v) {
        (FieldType::Integer, Y::Number(n)) => n.as_i64().map_or(null, PubValue::Int),
        (FieldType::Boolean, Y::Bool(b)) => PubValue::Bool(*b),
        (FieldType::Timestamp, Y::String(s)) => normalize_timestamp(s).map_or(null, PubValue::Text),
        (_, Y::String(s)) => PubValue::Text(s.clone()),
        _ => null,
    }
}

struct SqlBuilder {
    dialect: SqlDialect,
    binds: Vec<PubValue>,
}

impl SqlBuilder {
    fn bind(&mut self, v: PubValue) -> String {
        self.binds.push(v);
        placeholder(self.dialect, self.binds.len())
    }

    fn condition(&mut self, col: &str, ty: FieldType, c: &Condition, out: &mut Vec<String>) {
        let ops: [(&str, &Option<serde_yaml::Value>); 6] = [
            ("=", &c.eq),
            ("<>", &c.ne),
            (">", &c.gt),
            (">=", &c.gte),
            ("<", &c.lt),
            ("<=", &c.lte),
        ];
        for (op, value) in ops {
            if let Some(v) = value {
                let ph = self.bind(bind_value(ty, v));
                out.push(format!("{col} {op} {ph}"));
            }
        }
        if let Some(list) = &c.in_list {
            let phs: Vec<String> = list.iter().map(|v| self.bind(bind_value(ty, v))).collect();
            out.push(format!("{col} IN ({})", phs.join(", ")));
        }
        if let Some(serde_yaml::Value::Bool(b)) = &c.is_null {
            out.push(format!("{col} IS {}NULL", if *b { "" } else { "NOT " }));
        }
    }
}

fn dir_sql(d: Direction) -> &'static str {
    match d {
        Direction::Asc => "ASC",
        Direction::Desc => "DESC",
    }
}

/// 検証済みのクエリから SQL を組み立てる。値はすべてバインドする。
/// 並べ替えは NULL を最後にし、同順位は一意なキーで決める（DB ごとの違いを出さない）
pub fn build_query(
    def: &QueryDefinition,
    schema: &SchemaDefinition,
    table: &PublishedTable,
    dialect: SqlDialect,
    max_limit: usize,
) -> Result<BuiltQuery> {
    let col_type = |name: &str| -> Result<FieldType> {
        resolve_column(schema, name)
            .map(|c| c.field_type)
            .ok_or_else(|| {
                AegisError::Schema(format!("query '{}': unknown field '{name}'", def.name))
            })
    };
    let mut b = SqlBuilder {
        dialect,
        binds: Vec::new(),
    };
    let ct_ph = b.bind(PubValue::Text(def.from.clone()));
    let mut wheres = vec![
        format!("p.content_type = {ct_ph}"),
        "p.status = 'published'".to_string(),
    ];
    for (field, cond) in &def.conditions {
        let ty = col_type(field)?;
        b.condition(&format!("s.{field}"), ty, cond, &mut wheres);
    }
    let limit = def.limit.unwrap_or(max_limit).min(max_limit);
    let join = format!(
        "FROM {} s JOIN _aegis_content_publications p ON p.content_id = s.content_id",
        table.table
    );

    match def.mode() {
        QueryMode::List => {
            let mut order = Vec::new();
            for o in &def.order_by {
                let c = format!("s.{}", o.field);
                order.push(format!("({c}) IS NULL ASC, {c} {}", dir_sql(o.direction())));
            }
            if order.is_empty() {
                order.push("p.sort_at DESC".to_string());
            }
            order.push("p.content_id DESC".to_string());
            let sql = format!(
                "SELECT v.snapshot_json, p.status, p.published_at, p.unpublish_at, p.updated_at {join} \
                 JOIN _aegis_content_versions v ON v.content_type = p.content_type AND v.content_id = p.content_id AND v.revision = p.revision \
                 WHERE {} ORDER BY {} LIMIT {limit}",
                wheres.join(" AND "),
                order.join(", ")
            );
            Ok(BuiltQuery {
                sql,
                binds: b.binds,
                mode: QueryMode::List,
                outputs: Vec::new(),
            })
        }
        QueryMode::Aggregate => {
            // (出力名, SQL の式, 型)
            let mut outs: Vec<(String, String, OutKind)> = Vec::new();
            let mut group_exprs: Vec<String> = Vec::new();
            for g in &def.group_by {
                let ty = col_type(&g.field)?;
                let col = format!("s.{}", g.field);
                let expr = match g.bucket {
                    Some(bucket) => bucket_expr(&col, bucket),
                    None => col,
                };
                let kind = match ty {
                    FieldType::Integer => OutKind::Int,
                    FieldType::Boolean => OutKind::Bool,
                    _ => OutKind::Text,
                };
                group_exprs.push(expr.clone());
                outs.push((g.output_name().to_string(), expr, kind));
            }
            for s in &def.select {
                let col = s.field.as_ref().map(|f| format!("s.{f}"));
                let (expr, mut kind) = agg_expr(dialect, s.func, col.as_deref());
                // min / max は列の型に従う
                if matches!(s.func, AggFn::Min | AggFn::Max) {
                    if let Some(f) = &s.field {
                        if col_type(f)? == FieldType::Integer {
                            kind = OutKind::Int;
                        }
                    }
                }
                outs.push((s.as_name.clone(), expr, kind));
            }
            // 内側で型どおりに集計し（`GROUP BY` と同じ式を選ぶ）、外側で文字列にして並べ替える。
            // MySQL（ONLY_FULL_GROUP_BY）は、`CAST` で包んだ式を `GROUP BY` の式と同じとは見なさないため
            let inner_select: Vec<String> = outs
                .iter()
                .enumerate()
                .map(|(i, (_, expr, _))| format!("{expr} AS c{i}"))
                .collect();
            let mut inner = format!(
                "SELECT {} {join} WHERE {}",
                inner_select.join(", "),
                wheres.join(" AND ")
            );
            if !group_exprs.is_empty() {
                inner.push_str(&format!(" GROUP BY {}", group_exprs.join(", ")));
            }
            let outer_select: Vec<String> = (0..outs.len())
                .map(|i| text_cast(dialect, &format!("t.c{i}")))
                .collect();
            let mut order = Vec::new();
            for o in &def.order_by {
                if let Some(i) = outs.iter().position(|(n, _, _)| *n == o.field) {
                    order.push(format!(
                        "(t.c{i}) IS NULL ASC, t.c{i} {}",
                        dir_sql(o.direction())
                    ));
                }
            }
            // 同順位は、区切りの値の昇順で決める
            for i in 0..group_exprs.len() {
                order.push(format!("t.c{i} ASC"));
            }
            let mut sql = format!("SELECT {} FROM ({inner}) t", outer_select.join(", "));
            if !order.is_empty() {
                sql.push_str(&format!(" ORDER BY {}", order.join(", ")));
            }
            sql.push_str(&format!(" LIMIT {limit}"));
            Ok(BuiltQuery {
                sql,
                binds: b.binds,
                mode: QueryMode::Aggregate,
                outputs: outs
                    .into_iter()
                    .map(|(name, _, kind)| QueryOutput { name, kind })
                    .collect(),
            })
        }
    }
}

trait OrderDirection {
    fn direction(&self) -> Direction;
}

impl OrderDirection for aegis_schema::query::OrderBy {
    fn direction(&self) -> Direction {
        self.dir
    }
}

/// 集計の 1 行（すべて文字列で読んだ値）を、出力の型に従って JSON にする
pub fn decode_row(outputs: &[QueryOutput], cells: &[Option<String>]) -> Map<String, Value> {
    let mut row = Map::new();
    for (out, cell) in outputs.iter().zip(cells) {
        let value = match cell {
            None => Value::Null,
            Some(text) => match out.kind {
                OutKind::Int => text
                    .parse::<i64>()
                    .ok()
                    .or_else(|| text.parse::<f64>().ok().map(|f| f as i64))
                    .map_or(Value::Null, Value::from),
                OutKind::Float => text
                    .parse::<f64>()
                    .ok()
                    .and_then(serde_json::Number::from_f64)
                    .map_or(Value::Null, Value::Number),
                OutKind::Bool => Value::Bool(matches!(text.as_str(), "1" | "t" | "true" | "TRUE")),
                OutKind::Text => Value::String(text.clone()),
            },
        };
        row.insert(out.name.clone(), value);
    }
    row
}

#[cfg(test)]
mod tests {
    use super::*;
    use aegis_schema::SchemaLoader;

    fn schema() -> SchemaDefinition {
        SchemaLoader::load_from_str(
            "content_type: product\nlabel: P\nfields:\n  - name: price\n    type: integer\n    required: true\n    index: true\n  - name: brand\n    type: string\n  - name: on_sale\n    type: boolean\n",
        )
        .unwrap()
    }

    fn query(yaml: &str) -> QueryDefinition {
        let q = QueryDefinition::parse(yaml).unwrap();
        q.validate(&[schema()], &Default::default()).unwrap();
        q
    }

    fn plan(q: &QueryDefinition) -> PublishedColumnsPlan {
        PublishedColumnsPlan::build(std::slice::from_ref(q), &[schema()])
    }

    #[test]
    fn plan_has_only_the_columns_the_queries_use() {
        let q1 = query("name: a\nfrom: product\nwhere: { on_sale: { eq: true } }\nselect: [{ fn: sum, field: price, as: t }]\n");
        let q2 = query("name: b\nfrom: product\ngroup_by: [{ field: brand }]\nselect: [{ fn: count, as: n }]\n");
        let plan = PublishedColumnsPlan::build(&[q1, q2], &[schema()]);
        let table = plan.get("product").unwrap();
        let names: Vec<&str> = table.columns.iter().map(|c| c.name.as_str()).collect();
        assert_eq!(names, vec!["brand", "on_sale", "price"]);
        assert_eq!(table.table, "_aegis_published_product");
        assert!(
            table
                .columns
                .iter()
                .find(|c| c.name == "price")
                .unwrap()
                .indexed
        );
        assert!(plan.get("article").is_none());
    }

    #[test]
    fn signature_changes_with_columns_and_types() {
        let a = plan(&query(
            "name: a\nfrom: product\nselect: [{ fn: sum, field: price, as: t }]\n",
        ));
        let b = plan(&query(
            "name: a\nfrom: product\nselect: [{ fn: max, field: price, as: t }]\n",
        ));
        let c = plan(&query("name: a\nfrom: product\nwhere: { brand: { eq: x } }\nselect: [{ fn: sum, field: price, as: t }]\n"));
        assert_eq!(
            a.get("product").unwrap().signature(),
            b.get("product").unwrap().signature()
        );
        assert_ne!(
            a.get("product").unwrap().signature(),
            c.get("product").unwrap().signature()
        );
    }

    #[test]
    fn ddl_uses_the_content_table_column_types() {
        let p = plan(&query("name: a\nfrom: product\nwhere: { on_sale: { eq: true } }\nselect: [{ fn: sum, field: price, as: t }]\n"));
        let t = p.get("product").unwrap();
        let sqlite = t.create_sql(SqlDialect::Sqlite);
        assert_eq!(sqlite[0], "CREATE TABLE IF NOT EXISTS _aegis_published_product (content_id TEXT PRIMARY KEY, on_sale INTEGER, price INTEGER);");
        assert_eq!(sqlite[1], "CREATE INDEX IF NOT EXISTS idx__aegis_published_product_price ON _aegis_published_product (price);");
        let pg = t.create_sql(SqlDialect::Postgres);
        assert!(pg[0].contains("content_id VARCHAR(64) PRIMARY KEY, on_sale BOOLEAN, price BIGINT"));
        assert_eq!(t.insert_sql(SqlDialect::Postgres), "INSERT INTO _aegis_published_product (content_id, on_sale, price) VALUES ($1, $2, $3);");
        assert_eq!(
            t.insert_sql(SqlDialect::MySql),
            "INSERT INTO _aegis_published_product (content_id, on_sale, price) VALUES (?, ?, ?);"
        );
        assert_eq!(
            t.delete_sql(SqlDialect::Sqlite),
            "DELETE FROM _aegis_published_product WHERE content_id = ?1;"
        );
    }

    fn record(fields: &[(&str, Value)], access: Option<&str>) -> ContentRecord {
        let mut custom: std::collections::HashMap<String, Value> = fields
            .iter()
            .map(|(k, v)| (k.to_string(), v.clone()))
            .collect();
        if let Some(a) = access {
            custom.insert("access".into(), Value::String(a.into()));
        }
        ContentRecord {
            id: "id1".into(),
            content_type: "product".into(),
            title: "T".into(),
            slug: "t".into(),
            status: "published".into(),
            blocks_json: "[]".into(),
            author_id: None,
            published_at: Some("2026-03-05T09:00:00+09:00".into()),
            unpublish_at: None,
            created_at: "2026-03-01T00:00:00Z".into(),
            updated_at: "2026-03-01T00:00:00Z".into(),
            custom_fields: custom,
        }
    }

    #[test]
    fn values_follow_the_column_types_and_skip_restricted_records() {
        let p = plan(&query("name: a\nfrom: product\ngroup_by: [{ field: published_at, bucket: month }]\nselect: [{ fn: sum, field: price, as: t }]\nwhere: { on_sale: { eq: true } }\n"));
        let t = p.get("product").unwrap();
        let r = record(
            &[("price", Value::from(1200)), ("on_sale", Value::Bool(true))],
            None,
        );
        // 列は on_sale, price, published_at の順。日時は UTC にそろう
        assert_eq!(
            t.values_for(&r).unwrap(),
            vec![
                PubValue::Bool(true),
                PubValue::Int(1200),
                PubValue::Text("2026-03-05T00:00:00".into())
            ]
        );
        let missing = record(&[], Some("public"));
        assert_eq!(
            t.values_for(&missing).unwrap()[0],
            PubValue::Null(NullKind::Bool)
        );
        assert!(t.values_for(&record(&[], Some("gold"))).is_none());
    }

    #[test]
    fn aggregate_sql_binds_values_and_orders_deterministically() {
        let q = query("name: m\nfrom: product\nwhere:\n  price: { gte: 1000 }\n  brand: { in: [a, b] }\ngroup_by: [{ field: published_at, bucket: month }]\nselect:\n  - { fn: count, as: n }\n  - { fn: sum, field: price, as: total }\norder_by: [{ field: month, dir: desc }]\nlimit: 12\n");
        let p = plan(&q);
        let built = build_query(
            &q,
            &schema(),
            p.get("product").unwrap(),
            SqlDialect::Postgres,
            100,
        )
        .unwrap();
        assert_eq!(
            built.sql,
            concat!(
                "SELECT CAST(t.c0 AS TEXT), CAST(t.c1 AS TEXT), CAST(t.c2 AS TEXT) FROM (",
                "SELECT SUBSTR(s.published_at, 1, 7) AS c0, COUNT(*) AS c1, CAST(SUM(s.price) AS BIGINT) AS c2 ",
                "FROM _aegis_published_product s JOIN _aegis_content_publications p ON p.content_id = s.content_id ",
                "WHERE p.content_type = $1 AND p.status = 'published' AND s.brand IN ($2, $3) AND s.price >= $4 ",
                "GROUP BY SUBSTR(s.published_at, 1, 7)) t ",
                "ORDER BY (t.c0) IS NULL ASC, t.c0 DESC, t.c0 ASC LIMIT 12"
            )
        );
        assert_eq!(
            built.binds,
            vec![
                PubValue::Text("product".into()),
                PubValue::Text("a".into()),
                PubValue::Text("b".into()),
                PubValue::Int(1000)
            ]
        );
        let names: Vec<&str> = built.outputs.iter().map(|o| o.name.as_str()).collect();
        assert_eq!(names, vec!["month", "n", "total"]);
        // MySQL は `?`、CAST も MySQL 流
        let my = build_query(
            &q,
            &schema(),
            p.get("product").unwrap(),
            SqlDialect::MySql,
            100,
        )
        .unwrap();
        assert!(my.sql.contains("CAST(SUM(s.price) AS SIGNED)") && my.sql.contains("AS CHAR)"));
        assert!(!my.sql.contains('$'));
    }

    #[test]
    fn list_sql_joins_the_published_snapshot_and_caps_the_limit() {
        let q = query("name: l\nfrom: product\nwhere: { on_sale: { is_null: false } }\norder_by: [{ field: price, dir: asc }]\nlimit: 4\n");
        let p = plan(&q);
        let built = build_query(
            &q,
            &schema(),
            p.get("product").unwrap(),
            SqlDialect::Sqlite,
            3,
        )
        .unwrap();
        assert!(built.sql.starts_with("SELECT v.snapshot_json, p.status, p.published_at, p.unpublish_at, p.updated_at FROM _aegis_published_product s"));
        assert!(built.sql.contains("JOIN _aegis_content_versions v ON v.content_type = p.content_type AND v.content_id = p.content_id AND v.revision = p.revision"));
        assert!(built.sql.contains("s.on_sale IS NOT NULL"));
        assert!(built
            .sql
            .contains("(s.price) IS NULL ASC, s.price ASC, p.content_id DESC LIMIT 3"));
        assert_eq!(built.mode, QueryMode::List);
    }

    #[test]
    fn rows_decode_by_declared_kind() {
        let outs = vec![
            QueryOutput {
                name: "month".into(),
                kind: OutKind::Text,
            },
            QueryOutput {
                name: "n".into(),
                kind: OutKind::Int,
            },
            QueryOutput {
                name: "avg".into(),
                kind: OutKind::Float,
            },
            QueryOutput {
                name: "flag".into(),
                kind: OutKind::Bool,
            },
            QueryOutput {
                name: "none".into(),
                kind: OutKind::Int,
            },
        ];
        let row = decode_row(
            &outs,
            &[
                Some("2026-03".into()),
                Some("12".into()),
                Some("1500.5000".into()),
                Some("t".into()),
                None,
            ],
        );
        assert_eq!(
            Value::Object(row),
            serde_json::json!({"month": "2026-03", "n": 12, "avg": 1500.5, "flag": true, "none": null})
        );
    }

    #[test]
    fn timestamps_are_normalized_to_utc() {
        assert_eq!(
            normalize_timestamp("2026-03-05T09:00:00+09:00").as_deref(),
            Some("2026-03-05T00:00:00")
        );
        assert_eq!(
            normalize_timestamp("2026-03-05T09:00:00.123Z").as_deref(),
            Some("2026-03-05T09:00:00")
        );
        assert_eq!(normalize_timestamp("yesterday"), None);
    }
}
