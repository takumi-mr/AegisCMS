//! 宣言的クエリの副表（`published_columns`）をアダプタが実行するための共通部分。
//! 3 つの DB で手順は同じで、接続の型・真偽値のバインド・SQL の方言だけを呼び出し側が決める。

use crate::published_columns::PublishedColumnsPlan;
use crate::ContentRecord;
use serde_json::{Map, Value};
use std::sync::{Arc, RwLock};

/// `run_query` の結果
#[derive(Debug, Clone)]
pub enum QueryRows {
    /// 集計の行（`group_by` の値と `select` の `as` をキーにした JSON）
    Rows(Vec<Map<String, Value>>),
    /// `select` を省略したクエリの、公開版のレコード
    Records(Vec<ContentRecord>),
}

/// 副表の計画を共有する入れ物（アダプタが持ち、起動時に差し替える）
pub type SharedPlan = Arc<RwLock<Arc<PublishedColumnsPlan>>>;

pub fn new_shared_plan() -> SharedPlan {
    Arc::new(RwLock::new(Arc::new(PublishedColumnsPlan::default())))
}

pub fn current_plan(shared: &SharedPlan) -> Arc<PublishedColumnsPlan> {
    shared
        .read()
        .map(|g| g.clone())
        .unwrap_or_else(|e| e.into_inner().clone())
}

pub fn replace_plan(shared: &SharedPlan, plan: PublishedColumnsPlan) {
    let mut guard = shared.write().unwrap_or_else(|e| e.into_inner());
    *guard = Arc::new(plan);
}

/// 副表の書き込み・削除・作り直し・クエリの実行を、アダプタの固有メソッドとして生成する。
/// 前提: アダプタは `pool`（sqlx のプール）と `published: SharedPlan` を持つ
macro_rules! impl_published_columns {
    ($adapter:ty, $dialect:expr, $conn:ty, $bool_bind:expr, $bool_ty:ty) => {
        impl $adapter {
            /// 公開版の 1 件を副表へ書く（公開の記録と同じトランザクションで呼ぶ）。
            /// 副表の無い型は何もせず、`access` が空でないレコードは古い行を消すだけ
            pub(crate) async fn write_published_row(
                conn: &mut $conn,
                plan: &$crate::published_columns::PublishedColumnsPlan,
                record: &$crate::ContentRecord,
            ) -> aegis_core::Result<()> {
                use $crate::published_columns::{NullKind, PubValue};
                let Some(table) = plan.get(&record.content_type) else {
                    return Ok(());
                };
                Self::delete_published_row(conn, plan, &record.content_type, &record.id).await?;
                let Some(values) = table.values_for(record) else {
                    return Ok(());
                };
                let sql = table.insert_sql($dialect);
                let mut q = $crate::sqlx_compat::query(&sql).bind(record.id.clone());
                for v in values {
                    q = match v {
                        PubValue::Null(NullKind::Text) => q.bind(None::<String>),
                        PubValue::Null(NullKind::Int) => q.bind(None::<i64>),
                        PubValue::Null(NullKind::Bool) => q.bind(None::<$bool_ty>),
                        PubValue::Text(s) => q.bind(s),
                        PubValue::Int(i) => q.bind(i),
                        PubValue::Bool(b) => q.bind(($bool_bind)(b)),
                    };
                }
                q.execute(&mut *conn)
                    .await
                    .map_err(|e| aegis_core::AegisError::Database(e.to_string()))?;
                Ok(())
            }

            pub(crate) async fn delete_published_row(
                conn: &mut $conn,
                plan: &$crate::published_columns::PublishedColumnsPlan,
                content_type: &str,
                content_id: &str,
            ) -> aegis_core::Result<()> {
                let Some(table) = plan.get(content_type) else {
                    return Ok(());
                };
                $crate::sqlx_compat::query(&table.delete_sql($dialect))
                    .bind(content_id.to_string())
                    .execute(&mut *conn)
                    .await
                    .map_err(|e| aegis_core::AegisError::Database(e.to_string()))?;
                Ok(())
            }

            /// 計画を取り込み、定義が変わった（または無い）副表を作り直して、公開中のスナップショットから埋める。
            /// `force` なら、定義が同じでもすべて作り直す（`task restore` のあと）
            pub(crate) async fn configure_published(
                &self,
                plan: $crate::published_columns::PublishedColumnsPlan,
                force: bool,
            ) -> aegis_core::Result<()> {
                use $crate::sqlx_compat::Row;
                let db_err = |e: $crate::sqlx_compat::Error| {
                    aegis_core::AegisError::Database(e.to_string())
                };
                $crate::sqlx_compat::query(&format!(
                    "CREATE TABLE IF NOT EXISTS {} (table_name VARCHAR(128) PRIMARY KEY, signature TEXT NOT NULL);",
                    $crate::published_columns::PUBLISHED_META_TABLE
                ))
                .execute(&self.pool)
                .await
                .map_err(db_err)?;
                for table in plan.tables.values() {
                    let signature = table.signature();
                    let stored: Option<String> = $crate::sqlx_compat::query(&format!(
                        "SELECT signature FROM {} WHERE table_name = {};",
                        $crate::published_columns::PUBLISHED_META_TABLE,
                        $crate::published_columns::placeholder($dialect, 1)
                    ))
                    .bind(table.table.clone())
                    .fetch_optional(&self.pool)
                    .await
                    .map_err(db_err)?
                    .map(|r| r.get::<String, _>(0));
                    if !force && stored.as_deref() == Some(signature.as_str()) {
                        continue;
                    }
                    tracing::info!("Rebuilding published columns table {}", table.table);
                    let mut tx = self.pool.begin().await.map_err(db_err)?;
                    $crate::sqlx_compat::query(&table.drop_sql())
                        .execute(&mut *tx)
                        .await
                        .map_err(db_err)?;
                    for sql in table.create_sql($dialect) {
                        $crate::sqlx_compat::query(&sql)
                            .execute(&mut *tx)
                            .await
                            .map_err(db_err)?;
                    }
                    let rows = $crate::sqlx_compat::query(&format!(
                        "SELECT v.snapshot_json, p.status, p.published_at, p.unpublish_at, p.updated_at \
                         FROM _aegis_content_publications p \
                         JOIN _aegis_content_versions v ON v.content_type = p.content_type AND v.content_id = p.content_id AND v.revision = p.revision \
                         WHERE p.content_type = {} AND p.status = 'published'",
                        $crate::published_columns::placeholder($dialect, 1)
                    ))
                    .bind(table.content_type.clone())
                    .fetch_all(&mut *tx)
                    .await
                    .map_err(db_err)?;
                    for row in &rows {
                        let record = $crate::published_record_from_snapshot(
                            &row.get::<String, _>("snapshot_json"),
                            row.get("status"),
                            row.get("published_at"),
                            row.get("unpublish_at"),
                            row.get("updated_at"),
                        )?;
                        Self::write_published_row(&mut tx, &plan, &record).await?;
                    }
                    $crate::sqlx_compat::query(&format!(
                        "DELETE FROM {} WHERE table_name = {};",
                        $crate::published_columns::PUBLISHED_META_TABLE,
                        $crate::published_columns::placeholder($dialect, 1)
                    ))
                    .bind(table.table.clone())
                    .execute(&mut *tx)
                    .await
                    .map_err(db_err)?;
                    $crate::sqlx_compat::query(&format!(
                        "INSERT INTO {} (table_name, signature) VALUES ({}, {});",
                        $crate::published_columns::PUBLISHED_META_TABLE,
                        $crate::published_columns::placeholder($dialect, 1),
                        $crate::published_columns::placeholder($dialect, 2)
                    ))
                    .bind(table.table.clone())
                    .bind(signature)
                    .execute(&mut *tx)
                    .await
                    .map_err(db_err)?;
                    tx.commit().await.map_err(db_err)?;
                }
                $crate::published_exec::replace_plan(&self.published, plan);
                Ok(())
            }

            pub(crate) async fn run_published_query(
                &self,
                def: &aegis_schema::QueryDefinition,
                schema: &aegis_schema::SchemaDefinition,
                max_limit: usize,
            ) -> aegis_core::Result<$crate::published_exec::QueryRows> {
                use $crate::published_columns::{NullKind, PubValue};
                use $crate::published_exec::QueryRows;
                use $crate::sqlx_compat::Row;
                let plan = $crate::published_exec::current_plan(&self.published);
                let table = plan.get(&def.from).ok_or_else(|| {
                    aegis_core::AegisError::Database(format!(
                        "query '{}': published columns of '{}' are not prepared",
                        def.name, def.from
                    ))
                })?;
                let built = $crate::published_columns::build_query(
                    def, schema, table, $dialect, max_limit,
                )?;
                let mut q = $crate::sqlx_compat::query(&built.sql);
                for v in built.binds.iter().cloned() {
                    q = match v {
                        PubValue::Null(NullKind::Text) => q.bind(None::<String>),
                        PubValue::Null(NullKind::Int) => q.bind(None::<i64>),
                        PubValue::Null(NullKind::Bool) => q.bind(None::<$bool_ty>),
                        PubValue::Text(s) => q.bind(s),
                        PubValue::Int(i) => q.bind(i),
                        PubValue::Bool(b) => q.bind(($bool_bind)(b)),
                    };
                }
                let rows = q
                    .fetch_all(&self.pool)
                    .await
                    .map_err(|e| aegis_core::AegisError::Database(e.to_string()))?;
                match built.mode {
                    aegis_schema::QueryMode::List => rows
                        .iter()
                        .map(|row| {
                            $crate::published_record_from_snapshot(
                                &row.get::<String, _>("snapshot_json"),
                                row.get("status"),
                                row.get("published_at"),
                                row.get("unpublish_at"),
                                row.get("updated_at"),
                            )
                        })
                        .collect::<aegis_core::Result<Vec<_>>>()
                        .map(QueryRows::Records),
                    aegis_schema::QueryMode::Aggregate => {
                        let width = built.outputs.len();
                        Ok(QueryRows::Rows(
                            rows.iter()
                                .map(|row| {
                                    let cells: Vec<Option<String>> = (0..width)
                                        .map(|i| row.try_get::<Option<String>, _>(i).ok().flatten())
                                        .collect();
                                    $crate::published_columns::decode_row(&built.outputs, &cells)
                                })
                                .collect(),
                        ))
                    }
                }
            }
        }
    };
}
pub(crate) use impl_published_columns;
