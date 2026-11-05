//! 公開版の一覧（`list_published_contents_page`）と前後（`list_published_neighbors`）の検証。
//! 並べ替え・絞り込み（カテゴリ・タグ）・ページ送りを SQL で行うこと。SQLite（インメモリ）は常に実行し、
//! PostgreSQL / MySQL はコンテナ稼働時（無ければスキップ）に実行する。

use std::collections::HashMap;
use std::env;

use aegis_core::Result;
use aegis_db::{
    ContentListQuery, ContentPublicationChange, ContentRecord, ContentVersionRecord,
    DatabaseAdapter, MySqlAdapter, PostgresAdapter, PublishedNeighborQuery, PublishedOrder,
    SqliteAdapter, TagRecord,
};
use aegis_schema::SchemaDefinition;
use serde_json::json;

fn article_schema() -> SchemaDefinition {
    serde_json::from_value(json!({
        "content_type": "article",
        "label": "記事",
        "fields": [
            { "name": "title", "type": "string", "required": true },
            { "name": "category_id", "type": "string", "required": false, "nullable": true }
        ]
    }))
    .unwrap()
}

fn record(
    id: &str,
    day: u32,
    status: &str,
    category: Option<&str>,
    published_day: Option<u32>,
) -> ContentRecord {
    let mut custom_fields = HashMap::new();
    if let Some(category) = category {
        custom_fields.insert("category_id".to_string(), json!(category));
    }
    ContentRecord {
        id: id.to_string(),
        content_type: "article".to_string(),
        title: format!("Title {id}"),
        slug: format!("slug-{id}"),
        status: status.to_string(),
        blocks_json: "[]".to_string(),
        author_id: None,
        published_at: published_day.map(|d| format!("2026-11-{d:02}T00:00:00Z")),
        unpublish_at: None,
        created_at: format!("2026-10-{day:02}T00:00:00Z"),
        updated_at: format!("2026-10-{day:02}T00:00:00Z"),
        custom_fields,
    }
}

/// 公開の記録つきで書く（本番の書き込み経路と同じ）
async fn publish(adapter: &impl DatabaseAdapter, record: &ContentRecord) -> Result<()> {
    let schema = article_schema();
    let version = ContentVersionRecord {
        content_type: record.content_type.clone(),
        content_id: record.id.clone(),
        revision: 0,
        snapshot_json: serde_json::to_string(record).unwrap(),
        created_at: record.created_at.clone(),
        created_by: None,
        change_note: None,
    };
    adapter
        .insert_content_with_versions(
            &schema,
            record,
            &[version],
            ContentPublicationChange::PublishVersion(0),
        )
        .await
}

async fn page_ids(
    adapter: &impl DatabaseAdapter,
    query: ContentListQuery,
) -> Result<(Vec<String>, u64)> {
    let page = adapter
        .list_published_contents_page(&article_schema(), &query)
        .await?;
    Ok((page.records.into_iter().map(|r| r.id).collect(), page.total))
}

async fn neighbor_ids(
    adapter: &impl DatabaseAdapter,
    order: PublishedOrder,
    newer: bool,
    base: &ContentRecord,
    category_id: Option<&str>,
) -> Result<Vec<String>> {
    let records = adapter
        .list_published_neighbors(
            &article_schema(),
            &PublishedNeighborQuery {
                order,
                newer,
                key: order.key_of(base).to_string(),
                id: base.id.clone(),
                category_id: category_id.map(str::to_string),
                limit: 10,
            },
        )
        .await?;
    Ok(records.into_iter().map(|r| r.id).collect())
}

fn strings(list: &[&str]) -> Vec<String> {
    list.iter().map(|s| s.to_string()).collect()
}

async fn run_published_list_test<A: DatabaseAdapter>(adapter: &A) -> Result<()> {
    adapter.init().await?;
    let schema = article_schema();
    let schemas = HashMap::from([("article".to_string(), schema.clone())]);
    adapter.run_migrations(&schemas).await?;

    // 作成日は番号が大きいほど新しい。公開日は p3 だけ古く、p4 は公開日なし（作成日で並ぶ）
    let articles = [
        record("p1", 1, "published", Some("cat-1"), Some(1)),
        record("p2", 2, "published", Some("cat-2"), Some(2)),
        record("p3", 3, "published", Some("cat-1"), Some(1)),
        record("p4", 4, "published", None, None),
        record("p5", 5, "published", Some("cat-1"), Some(5)),
        record("p6", 6, "scheduled", Some("cat-1"), Some(20)),
    ];
    for article in &articles {
        publish(adapter, article).await?;
    }

    // 条件なし: 新しい順、総数は全件
    assert_eq!(
        page_ids(adapter, ContentListQuery::default()).await?,
        (strings(&["p6", "p5", "p4", "p3", "p2", "p1"]), 6)
    );
    // 状態の絞り込みとページ送り（総数はページに依らない）
    let published = |limit, offset| ContentListQuery {
        status: Some("published".into()),
        limit: Some(limit),
        offset,
        ..Default::default()
    };
    assert_eq!(
        page_ids(adapter, published(2, 0)).await?,
        (strings(&["p5", "p4"]), 5)
    );
    assert_eq!(
        page_ids(adapter, published(2, 2)).await?,
        (strings(&["p3", "p2"]), 5)
    );
    assert_eq!(
        page_ids(adapter, published(2, 4)).await?,
        (strings(&["p1"]), 5)
    );
    // `limit` なしは全件（`offset` は無視）
    assert_eq!(
        page_ids(
            adapter,
            ContentListQuery {
                status: Some("published".into()),
                offset: 3,
                ..Default::default()
            }
        )
        .await?,
        (strings(&["p5", "p4", "p3", "p2", "p1"]), 5)
    );
    // キーセット: 指定の作成日時・ID より古いもの（総数はキーセットを含めない）
    assert_eq!(
        page_ids(
            adapter,
            ContentListQuery {
                status: Some("published".into()),
                limit: Some(2),
                cursor_created_at: Some("2026-10-04T00:00:00Z".into()),
                cursor_id: Some("p4".into()),
                ..Default::default()
            }
        )
        .await?,
        (strings(&["p3", "p2"]), 5)
    );
    // カテゴリの絞り込み（値が無いものは「カテゴリ無し」）
    let in_category = |value: Option<&str>, limit| ContentListQuery {
        status: Some("published".into()),
        filters: vec![("category_id".into(), value.map(str::to_string))],
        limit: Some(limit),
        ..Default::default()
    };
    assert_eq!(
        page_ids(adapter, in_category(Some("cat-1"), 2)).await?,
        (strings(&["p5", "p3"]), 3)
    );
    assert_eq!(
        page_ids(adapter, in_category(None, 5)).await?,
        (strings(&["p4"]), 1)
    );

    // 前後: 作成日順。基準自身は含まない。近い順
    let base = &articles[2]; // p3
    assert_eq!(
        neighbor_ids(adapter, PublishedOrder::CreatedAt, false, base, None).await?,
        strings(&["p2", "p1"])
    );
    assert_eq!(
        neighbor_ids(adapter, PublishedOrder::CreatedAt, true, base, None).await?,
        // p6 は予約中（公開中ではない）
        strings(&["p4", "p5"])
    );
    // カテゴリ内の前後
    assert_eq!(
        neighbor_ids(
            adapter,
            PublishedOrder::CreatedAt,
            true,
            base,
            Some("cat-1")
        )
        .await?,
        strings(&["p5"])
    );
    // 公開日順（公開日が無い行は作成日。同じ時刻は ID の順）: p1(11-01) p3(11-01) p2(11-02) p5(11-05)、
    // p4 は作成日 10-04 なので最も古い
    let order = PublishedOrder::PublishedAt;
    assert_eq!(
        neighbor_ids(adapter, order, true, &articles[0], None).await?,
        strings(&["p3", "p2", "p5"])
    );
    assert_eq!(
        neighbor_ids(adapter, order, false, &articles[2], None).await?,
        strings(&["p1", "p4"])
    );

    // タグの絞り込み: タグの付いた公開中の記事だけ（ページ送りと総数も SQL）
    for (id, name) in [("t1", "one"), ("t2", "two")] {
        adapter
            .insert_tag(&TagRecord {
                id: id.to_string(),
                slug: name.to_string(),
                name: name.to_string(),
                description: None,
                created_at: "2026-10-01T00:00:00Z".to_string(),
                updated_at: "2026-10-01T00:00:00Z".to_string(),
            })
            .await?;
    }
    let at = "2026-10-01T00:00:00Z";
    // p1: t1+t2、p2: t1、p3: t1+t2、p5: t2、p6（公開前）: t1+t2
    for (id, tags) in [
        ("p1", &["t1", "t2"][..]),
        ("p2", &["t1"][..]),
        ("p3", &["t1", "t2"][..]),
        ("p5", &["t2"][..]),
        ("p6", &["t1", "t2"][..]),
    ] {
        let tags = strings(tags);
        adapter.set_content_tags("article", id, &tags, at).await?;
    }
    let tagged = |tag: &str, limit: u64, offset: u64| ContentListQuery {
        status: Some("published".into()),
        tag_id: Some(tag.to_string()),
        limit: Some(limit),
        offset,
        ..Default::default()
    };
    assert_eq!(
        page_ids(adapter, tagged("t1", 2, 0)).await?,
        (strings(&["p3", "p2"]), 3)
    );
    assert_eq!(
        page_ids(adapter, tagged("t1", 2, 2)).await?,
        (strings(&["p1"]), 3)
    );
    let mut with_category = tagged("t2", 10, 0);
    with_category.filters = vec![("category_id".into(), Some("cat-1".into()))];
    assert_eq!(
        page_ids(adapter, with_category).await?,
        (strings(&["p5", "p3", "p1"]), 3)
    );
    // 共通のタグが多い公開記事（p6 は公開前、自分自身は除く）。同数は ID の順
    assert_eq!(
        adapter.list_related_by_tags("article", "p1", 10).await?,
        vec![
            ("p3".to_string(), 2),
            ("p2".to_string(), 1),
            ("p5".to_string(), 1)
        ]
    );
    assert_eq!(
        adapter.list_related_by_tags("article", "p1", 2).await?,
        vec![("p3".to_string(), 2), ("p2".to_string(), 1)]
    );

    // `insert_content` で公開中の行を書くと、その内容で公開の記録もできる（テストと版の無い復元の用）
    adapter
        .insert_content(
            &schema,
            &record("p7", 7, "published", Some("cat-1"), Some(7)),
        )
        .await?;
    assert_eq!(
        page_ids(adapter, published(2, 0)).await?,
        (strings(&["p7", "p5"]), 6)
    );
    assert_eq!(
        neighbor_ids(
            adapter,
            PublishedOrder::CreatedAt,
            true,
            base,
            Some("cat-1")
        )
        .await?,
        strings(&["p5", "p7"])
    );
    assert_eq!(
        page_ids(adapter, in_category(Some("cat-1"), 5)).await?,
        (strings(&["p7", "p5", "p3", "p1"]), 4)
    );

    // 公開の配信 API 用: 参照先は公開版の記録から、公開中のものだけを引く
    // （p6 は予約中、"missing" は存在しない。どちらも結果に入らない）
    let by_id = adapter
        .resolve_published_relations(&schema, "id", &strings(&["p1", "p6", "missing"]))
        .await?;
    assert_eq!(by_id.len(), 1);
    assert_eq!(by_id["p1"].title, "Title p1");
    assert_eq!(by_id["p1"].slug, "slug-p1");
    assert_eq!(by_id["p1"].content_type, "article");
    let by_slug = adapter
        .resolve_published_relations(&schema, "slug", &strings(&["slug-p2", "slug-p6"]))
        .await?;
    assert_eq!(by_slug.len(), 1);
    assert_eq!(by_slug["slug-p2"].id, "p2");
    // `id` と `slug` 以外の列では引かない。参照が空のときも問い合わせない
    assert!(adapter
        .resolve_published_relations(&schema, "title", &strings(&["Title p1"]))
        .await?
        .is_empty());
    assert!(adapter
        .resolve_published_relations(&schema, "id", &[])
        .await?
        .is_empty());
    Ok(())
}

#[tokio::test]
async fn test_published_list_sqlite() {
    let adapter = SqliteAdapter::connect("sqlite::memory:", 1).await.unwrap();
    run_published_list_test(&adapter)
        .await
        .expect("SQLite published list test failed");
}

/// 公開の記録に列を足す前のテーブルから、起動時に列と値が入ること
#[tokio::test]
async fn test_published_columns_are_filled_for_existing_rows_sqlite() {
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    let adapter = SqliteAdapter::new(pool.clone());
    adapter.init().await.unwrap();
    let schemas = HashMap::from([("article".to_string(), article_schema())]);
    adapter.run_migrations(&schemas).await.unwrap();
    publish(
        &adapter,
        &record("p1", 1, "published", Some("cat-1"), Some(3)),
    )
    .await
    .unwrap();
    publish(&adapter, &record("p2", 2, "published", None, None))
        .await
        .unwrap();

    // 以前のバージョンのように、列の値が入っていない状態にする
    sqlx::query("UPDATE _aegis_content_publications SET category_id = NULL, sort_at = NULL")
        .execute(&pool)
        .await
        .unwrap();
    adapter.init().await.unwrap();

    let rows: Vec<(String, Option<String>, Option<String>)> = sqlx::query_as(
        "SELECT content_id, category_id, sort_at FROM _aegis_content_publications ORDER BY content_id",
    )
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(
        rows,
        vec![
            (
                "p1".to_string(),
                Some("cat-1".to_string()),
                Some("2026-11-03T00:00:00Z".to_string())
            ),
            (
                "p2".to_string(),
                Some(String::new()),
                Some("2026-10-02T00:00:00Z".to_string())
            ),
        ]
    );
}

fn is_port_reachable(host_port: &str) -> bool {
    let Ok(addr) = host_port.parse() else {
        return false;
    };
    std::net::TcpStream::connect_timeout(&addr, std::time::Duration::from_millis(150)).is_ok()
}

#[tokio::test]
async fn test_published_list_postgres() {
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
    let _ = sqlx::query("DROP TABLE IF EXISTS content_article, _aegis_content_tags, _aegis_tags, _aegis_content_publications, _aegis_content_versions CASCADE;")
        .execute(adapter.pool())
        .await;
    run_published_list_test(&adapter)
        .await
        .expect("PostgreSQL published list test failed");
}

#[tokio::test]
async fn test_published_list_mysql() {
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
    let _ = sqlx::query("DROP TABLE IF EXISTS content_article, _aegis_content_tags, _aegis_tags, _aegis_content_publications, _aegis_content_versions;")
        .execute(adapter.pool())
        .await;
    run_published_list_test(&adapter)
        .await
        .expect("MySQL published list test failed");
}
