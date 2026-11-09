//! 宣言的クエリ（`queries/*.yaml`。設計は docs/queries.md）。
//!
//! クエリの結果は閲覧者にもページにも依存しないので、サーバーが「クエリ名 → 結果」の一式を持ち、
//! テーマのグローバル変数 `queries` として読ませる。コンテンツの変更（公開・取り下げ・削除など）で、
//! その型を使うクエリだけを計算し直して差し替える。

use std::collections::HashMap;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use aegis_core::{AegisError, Result};
use aegis_db::published_exec::QueryRows;
use aegis_db::DatabaseAdapter;
use aegis_schema::{FieldType, QueryDefinition, QueryLimits, RelationTarget, SchemaDefinition};
use aegis_theme::QueriesHandle;
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use tokio::sync::Semaphore;

use crate::state::AppState;

/// コンテンツが変わった（payload はコンテンツの種類）。受け取ったサーバーが、その型を使うクエリを計算し直す
pub const TOPIC_QUERIES: &str = "queries";

/// 宣言的クエリの設定（`[queries]`）
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct QueriesConfig {
    /// `limit` の最大値（行数）
    pub max_limit: usize,
    /// 1 回の実行のタイムアウト（ミリ秒）
    pub timeout_ms: u64,
    /// 同時に実行できるクエリの数
    pub max_concurrency: usize,
    /// `in` のリストの最大の長さ
    pub max_in_items: usize,
    /// 日付の `bucket` と日時の条件で使うタイムゾーン（IANA 名。例: `Asia/Tokyo`）。変えると副表を作り直す
    pub timezone: String,
}

impl Default for QueriesConfig {
    fn default() -> Self {
        Self {
            max_limit: 100,
            timeout_ms: 2000,
            max_concurrency: 8,
            max_in_items: 100,
            timezone: "UTC".to_string(),
        }
    }
}

impl QueriesConfig {
    pub fn validate(&self) -> Result<()> {
        let bad = |key: &str, range: &str| {
            Err(AegisError::Config(format!(
                "[queries].{key} must be {range}"
            )))
        };
        if !(1..=1000).contains(&self.max_limit) {
            return bad("max_limit", "between 1 and 1000");
        }
        if !(100..=60_000).contains(&self.timeout_ms) {
            return bad("timeout_ms", "between 100 and 60000");
        }
        if !(1..=64).contains(&self.max_concurrency) {
            return bad("max_concurrency", "between 1 and 64");
        }
        if !(1..=1000).contains(&self.max_in_items) {
            return bad("max_in_items", "between 1 and 1000");
        }
        if self.timezone.parse::<chrono_tz::Tz>().is_err() {
            return Err(AegisError::Config(format!(
                "[queries].timezone {:?} is not an IANA time zone name (e.g. \"UTC\", \"Asia/Tokyo\")",
                self.timezone
            )));
        }
        Ok(())
    }

    /// 設定のタイムゾーン（検証済みでなければ UTC）
    pub fn tz(&self) -> chrono_tz::Tz {
        self.timezone.parse().unwrap_or(chrono_tz::Tz::UTC)
    }

    pub fn limits(&self) -> QueryLimits {
        QueryLimits {
            max_limit: self.max_limit,
            max_in_items: self.max_in_items,
        }
    }
}

pub struct QueryService {
    definitions: Vec<QueryDefinition>,
    config: QueriesConfig,
    handle: QueriesHandle,
    permits: Semaphore,
    /// 計算を始めた順の番号。遅れて終わった古い計算が、新しい結果を上書きしないようにする
    started: AtomicU64,
    applied: Mutex<HashMap<String, u64>>,
}

impl QueryService {
    /// クエリを持たないサービス（`queries/` が無い、テストなど）
    pub fn empty(config: QueriesConfig) -> Self {
        Self::new(Vec::new(), config)
    }

    fn new(definitions: Vec<QueryDefinition>, config: QueriesConfig) -> Self {
        Self {
            permits: Semaphore::new(config.max_concurrency),
            definitions,
            config,
            handle: aegis_theme::new_queries_handle(),
            started: AtomicU64::new(0),
            applied: Mutex::new(HashMap::new()),
        }
    }

    /// `queries/` の定義を読み、スキーマと突き合わせる。1 つでも誤りがあればエラー（起動を止める）
    pub fn load(
        config: &QueriesConfig,
        dir: &Path,
        schemas: &HashMap<String, SchemaDefinition>,
    ) -> Result<Self> {
        config.validate()?;
        let schema_list: Vec<SchemaDefinition> = schemas.values().cloned().collect();
        let definitions = aegis_schema::load_queries(dir, &schema_list, &config.limits())?;
        if !definitions.is_empty() {
            tracing::info!("Loaded {} queries from {:?}", definitions.len(), dir);
        }
        Ok(Self::new(definitions, config.clone()))
    }

    /// テーマのグローバル変数 `queries` に渡す入れ物
    pub fn handle(&self) -> QueriesHandle {
        self.handle.clone()
    }

    pub fn definitions(&self) -> &[QueryDefinition] {
        &self.definitions
    }

    /// 使う列の計画（クエリごとの和集合を、コンテンツの種類ごとにまとめたもの）
    fn plan(
        &self,
        schemas: &HashMap<String, SchemaDefinition>,
    ) -> aegis_db::published_columns::PublishedColumnsPlan {
        let schema_list: Vec<SchemaDefinition> = schemas.values().cloned().collect();
        aegis_db::published_columns::PublishedColumnsPlan::build(
            &self.definitions,
            &schema_list,
            self.config.tz(),
        )
    }

    /// 起動時: 公開側の列を用意し（定義が変わった副表は作り直す）、全クエリを計算する
    pub async fn prepare(&self, state: &AppState) -> Result<()> {
        if self.definitions.is_empty() {
            return Ok(());
        }
        let schemas = state.schemas.read().await.clone();
        state
            .db
            .configure_published_columns(self.plan(&schemas))
            .await?;
        self.refresh(state, None).await;
        Ok(())
    }

    /// `content_type` を使うクエリ（`None` なら全部）を計算し直して、結果を差し替える。
    /// 失敗したクエリは、前の結果を残して警告を出す
    pub async fn refresh(&self, state: &AppState, content_type: Option<&str>) {
        let targets: Vec<&QueryDefinition> = self
            .definitions
            .iter()
            .filter(|d| content_type.is_none_or(|t| d.from == t))
            .collect();
        if targets.is_empty() {
            return;
        }
        let schemas = state.schemas.read().await.clone();
        for def in targets {
            let ticket = self.started.fetch_add(1, Ordering::SeqCst) + 1;
            match self.run_one(state, &schemas, def).await {
                Ok(result) => self.apply(&def.name, ticket, result),
                Err(e) => tracing::warn!(
                    "Query '{}' failed (keeping the last result): {}",
                    def.name,
                    e
                ),
            }
        }
    }

    fn apply(&self, name: &str, ticket: u64, result: Value) {
        let mut applied = self.applied.lock().unwrap_or_else(|e| e.into_inner());
        if applied.get(name).is_some_and(|&done| done > ticket) {
            return;
        }
        applied.insert(name.to_string(), ticket);
        let mut guard = self.handle.write().unwrap_or_else(|e| e.into_inner());
        let mut next: Map<String, Value> = (**guard).clone();
        next.insert(name.to_string(), result);
        *guard = Arc::new(next);
    }

    async fn run_one(
        &self,
        state: &AppState,
        schemas: &HashMap<String, SchemaDefinition>,
        def: &QueryDefinition,
    ) -> Result<Value> {
        let schema = schemas.get(&def.from).ok_or_else(|| {
            AegisError::Schema(format!(
                "query '{}': unknown content type '{}'",
                def.name, def.from
            ))
        })?;
        let _permit = self
            .permits
            .acquire()
            .await
            .map_err(|e| AegisError::Database(e.to_string()))?;
        let work = async {
            let rows = state
                .db
                .run_query(def, schema, self.config.max_limit)
                .await?;
            match rows {
                QueryRows::Rows(rows) => {
                    aggregate_to_json(state.db.as_ref(), schemas, schema, def, rows).await
                }
                QueryRows::Records(records) => Ok(Value::Array(
                    list_to_json(state, schemas, schema, records).await,
                )),
            }
        };
        tokio::time::timeout(Duration::from_millis(self.config.timeout_ms), work)
            .await
            .map_err(|_| {
                AegisError::Database(format!(
                    "query '{}' timed out after {} ms",
                    def.name, self.config.timeout_ms
                ))
            })?
    }
}

/// コマンド（`task migrate`・`task restore`）から、公開側の列だけを用意する（結果の計算はしない）。
/// `rebuild` なら定義が同じでもすべて作り直す。クエリの定義の誤りはエラーにする（何も変えない）。
/// 戻り値は用意した副表の数
pub async fn prepare_columns(
    db: &dyn DatabaseAdapter,
    config: &QueriesConfig,
    dir: &Path,
    schemas: &HashMap<String, SchemaDefinition>,
    rebuild: bool,
) -> Result<usize> {
    let service = QueryService::load(config, dir, schemas)?;
    prepare_columns_of(&service, db, schemas, rebuild).await
}

/// `prepare_columns` の、読み込み済みのサービスを使う版（復元の前に検証して、後で使う）
pub async fn prepare_columns_of(
    service: &QueryService,
    db: &dyn DatabaseAdapter,
    schemas: &HashMap<String, SchemaDefinition>,
    rebuild: bool,
) -> Result<usize> {
    if service.definitions.is_empty() {
        return Ok(0);
    }
    let plan = service.plan(schemas);
    let tables = plan.tables.len();
    if rebuild {
        db.rebuild_published_columns(plan).await?;
    } else {
        db.configure_published_columns(plan).await?;
    }
    Ok(tables)
}

/// リレーション列の `group_by` の値を、参照先の `{id, title, slug}` にする（`IN (...)` で 1 回にまとめて解決）
async fn aggregate_to_json(
    db: &dyn DatabaseAdapter,
    schemas: &HashMap<String, SchemaDefinition>,
    schema: &SchemaDefinition,
    def: &QueryDefinition,
    mut rows: Vec<Map<String, Value>>,
) -> Result<Value> {
    for g in &def.group_by {
        let Some(field) = schema.fields.iter().find(|f| f.name == g.field) else {
            continue;
        };
        if field.field_type != FieldType::Relation {
            continue;
        }
        let Some(target) = field.target.as_deref().and_then(RelationTarget::parse) else {
            continue;
        };
        let Some(target_schema) = schemas.get(&target.content_type) else {
            continue;
        };
        let key = g.output_name();
        let mut ids: Vec<String> = rows
            .iter()
            .filter_map(|r| r.get(key).and_then(Value::as_str).map(str::to_string))
            .collect();
        ids.sort();
        ids.dedup();
        if ids.is_empty() {
            continue;
        }
        let found = db
            .resolve_published_relations(target_schema, &target.field, &ids)
            .await?;
        for row in &mut rows {
            if let Some(id) = row.get(key).and_then(Value::as_str).map(str::to_string) {
                let resolved = match found.get(&id) {
                    Some(r) => json!({"id": r.id, "title": r.title, "slug": r.slug}),
                    // 参照先が公開中でない
                    None => json!({"id": id, "title": null, "slug": null}),
                };
                row.insert(key.to_string(), resolved);
            }
        }
    }
    Ok(Value::Array(rows.into_iter().map(Value::Object).collect()))
}

/// `select` を省略したクエリの結果。テーマの一覧（`items`）の要素と同じ形（本文は含めない）
async fn list_to_json(
    state: &AppState,
    schemas: &HashMap<String, SchemaDefinition>,
    schema: &SchemaDefinition,
    records: Vec<aegis_db::ContentRecord>,
) -> Vec<Value> {
    let base = state
        .config
        .server
        .normalized_base_path()
        .unwrap_or_default();
    // 公開中の参照先を、リレーション列ごとに 1 回で解決する
    let mut relations: Vec<Map<String, Value>> = vec![Map::new(); records.len()];
    for (field, target) in schema.relation_fields() {
        let Some(target_schema) = schemas.get(&target.content_type) else {
            continue;
        };
        let refs: Vec<Option<String>> = records
            .iter()
            .map(|r| {
                r.custom_fields
                    .get(&field.name)
                    .and_then(aegis_db::relation_ref)
                    .map(str::to_string)
            })
            .collect();
        let mut ids: Vec<String> = refs.iter().flatten().cloned().collect();
        ids.sort();
        ids.dedup();
        if ids.is_empty() {
            continue;
        }
        let Ok(found) = state
            .db
            .resolve_published_relations(target_schema, &target.field, &ids)
            .await
        else {
            continue;
        };
        for (i, r) in refs.iter().enumerate() {
            if let Some(hit) = r.as_ref().and_then(|id| found.get(id)) {
                relations[i].insert(
                    field.name.clone(),
                    json!({"id": hit.id, "content_type": hit.content_type, "title": hit.title, "slug": hit.slug}),
                );
            }
        }
    }
    records
        .into_iter()
        .zip(relations)
        .map(|(record, relations)| {
            let blocks: Vec<aegis_ast::BlockNode> =
                serde_json::from_str(&record.blocks_json).unwrap_or_default();
            let url = format!(
                "{}{}",
                base,
                crate::handlers::public::public_content_path(
                    state,
                    &record.content_type,
                    &record.slug
                )
            );
            let mut fields = record.custom_fields;
            fields.insert("url".to_string(), json!(url));
            json!({
                "id": record.id,
                "content_type": record.content_type,
                "title": record.title,
                "slug": record.slug,
                "excerpt": crate::handlers::access::summary_excerpt(&fields, &blocks, 140),
                "published_at": record.published_at,
                "created_at": record.created_at,
                "fields": fields,
                "relations": relations,
            })
        })
        .collect()
}

/// コンテンツが変わったことを、このサーバーと、ほかのサーバーに知らせる（受け取った側が計算し直す）。
/// 知らせは自分にも届くので、1 台でも複数台でも同じ処理になる
pub async fn content_changed(state: &AppState, content_type: &str) {
    if state
        .queries
        .definitions()
        .iter()
        .any(|d| d.from == content_type)
    {
        crate::cluster::announce(state, TOPIC_QUERIES, content_type).await;
    }
}

/// 知らせを受け取ったとき（`cluster::handle`）
pub async fn handle_event(state: &AppState, content_type: &str) {
    state.queries.refresh(state, Some(content_type)).await;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_defaults_are_valid_and_bad_values_are_refused() {
        assert!(QueriesConfig::default().validate().is_ok());
        let cases = [
            QueriesConfig {
                max_limit: 0,
                ..Default::default()
            },
            QueriesConfig {
                max_limit: 1001,
                ..Default::default()
            },
            QueriesConfig {
                timeout_ms: 50,
                ..Default::default()
            },
            QueriesConfig {
                max_concurrency: 0,
                ..Default::default()
            },
            QueriesConfig {
                max_in_items: 0,
                ..Default::default()
            },
            QueriesConfig {
                timezone: "Mars/Base".into(),
                ..Default::default()
            },
        ];
        for c in cases {
            assert!(c.validate().is_err(), "{c:?}");
        }
        let tokyo = QueriesConfig {
            timezone: "Asia/Tokyo".into(),
            ..Default::default()
        };
        assert!(tokyo.validate().is_ok());
        assert_eq!(tokyo.tz(), chrono_tz::Tz::Asia__Tokyo);
        let limits = QueriesConfig {
            max_limit: 50,
            max_in_items: 7,
            ..Default::default()
        }
        .limits();
        assert_eq!((limits.max_limit, limits.max_in_items), (50, 7));
    }
}
