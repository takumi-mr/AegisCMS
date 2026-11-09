//! 宣言的クエリ（docs/queries.md）の実行。公開側の列（副表）への写し、集計と一覧、
//! 「公開後に編集中の値は出ない」「`access` が空でないものは出ない」「取り下げ・削除で消える」
//! 「定義が変わったら作り直す」を確かめる。SQLite（インメモリ）は常に実行し、
//! PostgreSQL / MySQL はコンテナ稼働時（無ければスキップ）に実行する。

use std::collections::HashMap;
use std::env;

use aegis_core::Result;
use aegis_db::published_columns::PublishedColumnsPlan;
use aegis_db::published_exec::QueryRows;
use aegis_db::{
    ContentPublicationChange, ContentRecord, ContentVersionRecord, DatabaseAdapter, MySqlAdapter,
    PostgresAdapter, SqliteAdapter,
};
use aegis_schema::{QueryDefinition, QueryLimits, SchemaDefinition, SchemaLoader};
use chrono_tz::Tz;
use serde_json::{json, Value};

fn product_schema() -> SchemaDefinition {
    SchemaLoader::load_from_str(
        "content_type: product\nlabel: P\nfields:\n  - name: price\n    type: integer\n    required: true\n  - name: brand\n    type: string\n  - name: on_sale\n    type: boolean\n  - name: discount\n    type: integer\n    nullable: true\n",
    )
    .unwrap()
}

fn query(yaml: &str) -> QueryDefinition {
    let q = QueryDefinition::parse(yaml).unwrap();
    q.validate(&[product_schema()], &QueryLimits::default())
        .unwrap();
    q
}

fn product(
    id: &str,
    brand: &str,
    price: i64,
    on_sale: bool,
    published_at: &str,
    extra: &[(&str, Value)],
) -> ContentRecord {
    let mut custom_fields = HashMap::from([
        ("brand".to_string(), json!(brand)),
        ("price".to_string(), json!(price)),
        ("on_sale".to_string(), json!(on_sale)),
    ]);
    for (k, v) in extra {
        custom_fields.insert(k.to_string(), v.clone());
    }
    ContentRecord {
        id: id.to_string(),
        content_type: "product".to_string(),
        title: format!("Product {id}"),
        slug: format!("p-{id}"),
        status: "published".to_string(),
        blocks_json: "[]".to_string(),
        author_id: None,
        published_at: Some(published_at.to_string()),
        unpublish_at: None,
        created_at: "2026-01-01T00:00:00Z".to_string(),
        updated_at: "2026-01-01T00:00:00Z".to_string(),
        custom_fields,
    }
}

fn version_of(record: &ContentRecord) -> ContentVersionRecord {
    ContentVersionRecord {
        content_type: record.content_type.clone(),
        content_id: record.id.clone(),
        revision: 0,
        snapshot_json: serde_json::to_string(record).unwrap(),
        created_at: record.updated_at.clone(),
        created_by: None,
        change_note: None,
    }
}

async fn publish_new(adapter: &impl DatabaseAdapter, record: &ContentRecord) -> Result<()> {
    adapter
        .insert_content_with_versions(
            &product_schema(),
            record,
            &[version_of(record)],
            ContentPublicationChange::PublishVersion(0),
        )
        .await
}

async fn rows(adapter: &impl DatabaseAdapter, q: &QueryDefinition) -> Result<Value> {
    match adapter.run_query(q, &product_schema(), 100).await? {
        QueryRows::Rows(rows) => Ok(Value::Array(rows.into_iter().map(Value::Object).collect())),
        QueryRows::Records(_) => panic!("expected aggregate rows"),
    }
}

async fn list_ids(adapter: &impl DatabaseAdapter, q: &QueryDefinition) -> Result<Vec<String>> {
    match adapter.run_query(q, &product_schema(), 100).await? {
        QueryRows::Records(records) => Ok(records.into_iter().map(|r| r.id).collect()),
        QueryRows::Rows(_) => panic!("expected records"),
    }
}

async fn run_declarative_queries_test<A: DatabaseAdapter>(adapter: &A) -> Result<()> {
    adapter.init().await?;
    let schema = product_schema();
    adapter
        .run_migrations(&HashMap::from([("product".to_string(), schema.clone())]))
        .await?;

    // 副表の計画を取り込む前に公開済みのものがある（起動時の埋め直し）
    let a1 = product("a1", "A", 1000, true, "2026-01-10T10:00:00Z", &[]);
    let a2 = product("a2", "A", 3000, true, "2026-01-20T10:00:00Z", &[]);
    let b1 = product(
        "b1",
        "B",
        500,
        true,
        "2026-02-05T10:00:00Z",
        &[("discount", json!(400))],
    );
    let b2 = product("b2", "B", 9000, false, "2026-02-06T10:00:00Z", &[]);
    // 会員限定は集計に出さない
    let r1 = product(
        "r1",
        "A",
        100000,
        true,
        "2026-01-11T10:00:00Z",
        &[("access", json!("gold"))],
    );
    for p in [&a1, &a2, &b1, &b2, &r1] {
        publish_new(adapter, p).await?;
    }
    // 下書き（公開していない）も出さない
    let mut draft = product("d1", "A", 777777, true, "2026-01-12T10:00:00Z", &[]);
    draft.status = "draft".into();
    adapter
        .insert_content_with_versions(
            &schema,
            &draft,
            &[version_of(&draft)],
            ContentPublicationChange::Keep,
        )
        .await?;

    let by_brand = query("name: by_brand\nfrom: product\nwhere: { on_sale: { eq: true } }\ngroup_by: [{ field: brand }]\nselect:\n  - { fn: count, as: n }\n  - { fn: sum, field: price, as: total }\n  - { fn: avg, field: price, as: mean }\n  - { fn: max, field: price, as: top }\norder_by: [{ field: total, dir: desc }]\n");
    let by_month = query("name: by_month\nfrom: product\ngroup_by: [{ field: published_at, bucket: month }]\nselect: [{ fn: count, as: n }]\norder_by: [{ field: month, dir: asc }]\n");
    let cheap_sale = query("name: cheap_sale\nfrom: product\nwhere: { on_sale: { eq: true }, price: { lte: 1000 } }\norder_by: [{ field: price, dir: asc }]\nlimit: 5\n");
    let plan1 = PublishedColumnsPlan::build(
        &[by_brand.clone(), by_month.clone(), cheap_sale.clone()],
        std::slice::from_ref(&schema),
        Tz::UTC,
    );
    adapter.configure_published_columns(plan1).await?;

    assert_eq!(
        rows(adapter, &by_brand).await?,
        json!([
            {"brand": "A", "n": 2, "total": 4000, "mean": 2000.0, "top": 3000},
            {"brand": "B", "n": 1, "total": 500, "mean": 500.0, "top": 500}
        ])
    );
    assert_eq!(
        rows(adapter, &by_month).await?,
        json!([{"month": "2026-01", "n": 2}, {"month": "2026-02", "n": 2}])
    );
    // 一覧は公開版のスナップショット（価格の昇順）
    assert_eq!(list_ids(adapter, &cheap_sale).await?, vec!["b1", "a1"]);

    // 公開後の編集（作業版だけの更新）は、公開版を更新するまで集計に出ない
    let mut edited = a1.clone();
    edited.custom_fields.insert("price".into(), json!(999_999));
    adapter
        .update_content_with_versions(
            &schema,
            &edited,
            &[version_of(&edited)],
            ContentPublicationChange::Keep,
        )
        .await?;
    assert_eq!(rows(adapter, &by_brand).await?[0]["total"], json!(4000));
    // 公開版を更新すると反映される
    adapter
        .update_content_with_versions(
            &schema,
            &edited,
            &[version_of(&edited)],
            ContentPublicationChange::PublishVersion(0),
        )
        .await?;
    assert_eq!(
        rows(adapter, &by_brand).await?[0]["total"],
        json!(1_002_999)
    );

    // 使う列が増えたら作り直して、公開中のスナップショットから埋め直す
    let no_discount = query("name: no_discount\nfrom: product\nwhere: { discount: { is_null: true } }\nselect: [{ fn: count, as: n }]\n");
    let with_discount = query("name: with_discount\nfrom: product\nwhere: { discount: { is_null: false } }\nselect:\n  - { fn: count, as: n }\n  - { fn: min, field: discount, as: lowest }\n");
    let plan2 = PublishedColumnsPlan::build(
        &[
            by_brand.clone(),
            by_month.clone(),
            cheap_sale.clone(),
            no_discount.clone(),
            with_discount.clone(),
        ],
        std::slice::from_ref(&schema),
        Tz::UTC,
    );
    adapter.configure_published_columns(plan2).await?;
    assert_eq!(rows(adapter, &no_discount).await?, json!([{"n": 3}]));
    assert_eq!(
        rows(adapter, &with_discount).await?,
        json!([{"n": 1, "lowest": 400}])
    );

    // タイムゾーンを変えると作り直され、日付の区切りが現地時間になる。
    // UTC では 1/31 20:00 の公開が、東京では 2/1 05:00（翌月）
    let t1 = product("t1", "A", 700, true, "2026-01-31T20:00:00Z", &[]);
    publish_new(adapter, &t1).await?;
    adapter
        .configure_published_columns(PublishedColumnsPlan::build(
            &[
                by_brand.clone(),
                by_month.clone(),
                cheap_sale.clone(),
                no_discount.clone(),
                with_discount.clone(),
            ],
            std::slice::from_ref(&schema),
            Tz::UTC,
        ))
        .await?;
    // 追加は「定義が同じ」なので作り直されず、公開操作の書き込みで入っている
    assert_eq!(
        rows(adapter, &by_month).await?,
        json!([{"month": "2026-01", "n": 3}, {"month": "2026-02", "n": 2}])
    );
    adapter
        .configure_published_columns(PublishedColumnsPlan::build(
            &[
                by_brand.clone(),
                by_month.clone(),
                cheap_sale.clone(),
                no_discount.clone(),
                with_discount.clone(),
            ],
            std::slice::from_ref(&schema),
            Tz::Asia__Tokyo,
        ))
        .await?;
    assert_eq!(
        rows(adapter, &by_month).await?,
        json!([{"month": "2026-01", "n": 2}, {"month": "2026-02", "n": 3}])
    );
    // 条件の日時も現地時刻で比べる（東京の 2/1 0:00 以降 = UTC の 1/31 15:00 以降）
    let since_feb = query(
        "name: since_feb
from: product
where: { published_at: { gte: \"2026-02-01T00:00:00+09:00\" } }
select: [{ fn: count, as: n }]
",
    );
    adapter
        .configure_published_columns(PublishedColumnsPlan::build(
            &[
                by_brand.clone(),
                by_month.clone(),
                cheap_sale.clone(),
                no_discount.clone(),
                with_discount.clone(),
                since_feb.clone(),
            ],
            std::slice::from_ref(&schema),
            Tz::Asia__Tokyo,
        ))
        .await?;
    assert_eq!(rows(adapter, &since_feb).await?, json!([{"n": 3}]));

    // 取り下げ・削除で副表からも消える
    adapter
        .update_content_with_versions(
            &schema,
            &a2,
            &[version_of(&a2)],
            ContentPublicationChange::Unpublish,
        )
        .await?;
    assert!(adapter.delete_content(&schema, "b2").await?);
    assert_eq!(
        rows(adapter, &by_month).await?,
        json!([{"month": "2026-01", "n": 1}, {"month": "2026-02", "n": 2}])
    );
    assert_eq!(rows(adapter, &no_discount).await?, json!([{"n": 2}]));

    // 副表に書かれないまま公開の記録だけが変わったとき（`task restore` など）は、定義が同じだと古いまま。
    // 強制の作り直しで、公開の記録に合わせてリセットされる
    let final_plan = PublishedColumnsPlan::build(
        &[
            by_brand.clone(),
            by_month.clone(),
            cheap_sale.clone(),
            no_discount.clone(),
            with_discount.clone(),
            since_feb.clone(),
        ],
        std::slice::from_ref(&schema),
        Tz::Asia__Tokyo,
    );
    adapter
        .configure_published_columns(PublishedColumnsPlan::default())
        .await?;
    publish_new(
        adapter,
        &product("z1", "A", 10, true, "2026-03-10T00:00:00Z", &[]),
    )
    .await?;
    adapter
        .configure_published_columns(final_plan.clone())
        .await?;
    assert_eq!(
        rows(adapter, &by_month).await?,
        json!([{"month": "2026-01", "n": 1}, {"month": "2026-02", "n": 2}])
    );
    adapter.rebuild_published_columns(final_plan).await?;
    assert_eq!(
        rows(adapter, &by_month).await?,
        json!([
            {"month": "2026-01", "n": 1},
            {"month": "2026-02", "n": 2},
            {"month": "2026-03", "n": 1}
        ])
    );
    Ok(())
}

#[tokio::test]
async fn test_declarative_queries_sqlite() {
    let adapter = SqliteAdapter::connect("sqlite::memory:", 1).await.unwrap();
    run_declarative_queries_test(&adapter)
        .await
        .expect("SQLite declarative queries test failed");
}

fn is_port_reachable(host_port: &str) -> bool {
    let Ok(addr) = host_port.parse() else {
        return false;
    };
    std::net::TcpStream::connect_timeout(&addr, std::time::Duration::from_millis(150)).is_ok()
}

const CLEANUP: &str = "DROP TABLE IF EXISTS content_product, _aegis_published_product, _aegis_published_meta, _aegis_content_tags, _aegis_tags, _aegis_content_publications, _aegis_content_versions";

#[tokio::test]
async fn test_declarative_queries_postgres() {
    let custom_url = env::var("POSTGRES_URL").ok();
    if custom_url.is_none() && !is_port_reachable("127.0.0.1:5432") {
        eprintln!("Skipping PostgreSQL test: 127.0.0.1:5432 is not reachable (start with task docker:dbs:up)");
        return;
    }
    let url = custom_url
        .unwrap_or_else(|| "postgres://aegis:password@127.0.0.1:5432/aegis_test".to_string());
    let Ok(adapter) = PostgresAdapter::connect(&url, 5).await else {
        eprintln!("Skipping PostgreSQL test: failed to connect");
        return;
    };
    let _ = sqlx::query("DROP TABLE IF EXISTS _sqlx_migrations;")
        .execute(adapter.pool())
        .await;
    let _ = aegis_db::sqlx_compat::query(&format!("{CLEANUP} CASCADE;"))
        .execute(adapter.pool())
        .await;
    run_declarative_queries_test(&adapter)
        .await
        .expect("PostgreSQL declarative queries test failed");
}

#[tokio::test]
async fn test_declarative_queries_mysql() {
    let custom_url = env::var("MYSQL_URL").ok();
    if custom_url.is_none() && !is_port_reachable("127.0.0.1:3306") {
        eprintln!(
            "Skipping MySQL test: 127.0.0.1:3306 is not reachable (start with task docker:dbs:up)"
        );
        return;
    }
    let url = custom_url
        .unwrap_or_else(|| "mysql://aegis:password@127.0.0.1:3306/aegis_test".to_string());
    let Ok(adapter) = MySqlAdapter::connect(&url, 5).await else {
        eprintln!("Skipping MySQL test: failed to connect");
        return;
    };
    let _ = sqlx::query("DROP TABLE IF EXISTS _sqlx_migrations;")
        .execute(adapter.pool())
        .await;
    let _ = aegis_db::sqlx_compat::query(&format!("{CLEANUP};"))
        .execute(adapter.pool())
        .await;
    run_declarative_queries_test(&adapter)
        .await
        .expect("MySQL declarative queries test failed");
}
