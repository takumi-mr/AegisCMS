//! 一覧の検索・絞り込み・ページ送り（`list_contents_page`）の検証。
//! SQLite（インメモリ）は常に実行し、PostgreSQL / MySQL はコンテナ稼働時（無ければスキップ）に実行する。

use std::collections::HashMap;
use std::env;

use aegis_core::Result;
use aegis_db::{
    ContentListQuery, ContentRecord, DatabaseAdapter, MySqlAdapter, PostgresAdapter, SqliteAdapter,
};
use aegis_schema::SchemaDefinition;
use serde_json::json;

fn article_schema() -> SchemaDefinition {
    serde_json::from_value(json!({
        "content_type": "article",
        "label": "記事",
        "fields": [
            { "name": "title", "type": "string", "required": true },
            { "name": "excerpt", "type": "string", "required": false, "nullable": true },
            { "name": "category_id", "type": "string", "required": false, "nullable": true }
        ]
    }))
    .unwrap()
}

/// (id, title, slug, status, category_id, excerpt)。作成日は id の番号が大きいほど新しい
type Article = (
    &'static str,
    &'static str,
    &'static str,
    &'static str,
    Option<&'static str>,
    &'static str,
);

const ARTICLES: &[Article] = &[
    (
        "a1",
        "Rust Streaming Guide",
        "rust-streaming",
        "published",
        Some("cat-1"),
        "about wasm",
    ),
    (
        "a2",
        "rust basics",
        "learn-it",
        "draft",
        Some("cat-2"),
        "first steps",
    ),
    (
        "a3",
        "Cooking 50%_off deals",
        "cooking-deals",
        "published",
        Some("cat-1"),
        "",
    ),
    (
        "a4",
        "日本語のタイトル",
        "japanese-title",
        "draft",
        None,
        "概要です",
    ),
    (
        "a5",
        "Design notes",
        "design-notes",
        "published",
        Some(""),
        "typography",
    ),
    (
        "a6",
        "Second Rust note",
        "rust-note-2",
        "draft",
        Some("cat-1"),
        "",
    ),
    ("a7", "Everything else", "misc", "scheduled", None, ""),
];

async fn ids(
    adapter: &impl DatabaseAdapter,
    query: ContentListQuery,
) -> Result<(Vec<String>, u64)> {
    let page = adapter
        .list_contents_page(&article_schema(), &query)
        .await?;
    Ok((
        page.records.into_iter().map(|record| record.id).collect(),
        page.total,
    ))
}

async fn run_list_query_test<A: DatabaseAdapter>(adapter: &A) -> Result<()> {
    adapter.init().await?;
    let schema = article_schema();
    adapter
        .run_migrations(&HashMap::from([("article".to_string(), schema.clone())]))
        .await?;
    for (index, (id, title, slug, status, category, excerpt)) in ARTICLES.iter().enumerate() {
        let mut custom_fields = HashMap::from([("excerpt".to_string(), json!(excerpt))]);
        if let Some(category) = category {
            custom_fields.insert("category_id".to_string(), json!(category));
        }
        adapter
            .insert_content(
                &schema,
                &ContentRecord {
                    id: id.to_string(),
                    content_type: "article".to_string(),
                    title: title.to_string(),
                    slug: slug.to_string(),
                    status: status.to_string(),
                    blocks_json: "[]".to_string(),
                    author_id: None,
                    published_at: None,
                    unpublish_at: None,
                    created_at: format!("2026-10-2{}T00:00:00Z", index + 1),
                    updated_at: format!("2026-10-2{}T00:00:00Z", index + 1),
                    custom_fields,
                },
            )
            .await?;
    }
    let search = |term: &str| ContentListQuery {
        search: Some(term.to_string()),
        ..Default::default()
    };
    let strings = |list: &[&str]| list.iter().map(|s| s.to_string()).collect::<Vec<_>>();

    // 条件なし: 新しい順
    assert_eq!(
        ids(adapter, ContentListQuery::default()).await?,
        (strings(&["a7", "a6", "a5", "a4", "a3", "a2", "a1"]), 7)
    );

    // タイトルの部分一致（大文字小文字を区別しない）とスラッグ
    assert_eq!(
        ids(adapter, search("RUST")).await?,
        (strings(&["a6", "a2", "a1"]), 3)
    );
    assert_eq!(
        ids(adapter, search("  streaming ")).await?,
        (strings(&["a1"]), 1)
    );
    assert_eq!(
        ids(adapter, search("learn-it")).await?,
        (strings(&["a2"]), 1)
    );
    assert_eq!(
        ids(adapter, search("タイトル")).await?,
        (strings(&["a4"]), 1)
    );
    assert_eq!(ids(adapter, search("no such text")).await?, (vec![], 0));
    // 空の検索語は条件にしない
    assert_eq!(ids(adapter, search("   ")).await?.1, 7);

    // `%` と `_` は文字として探す（ワイルドカードにしない）
    assert_eq!(ids(adapter, search("50%")).await?, (strings(&["a3"]), 1));
    assert_eq!(ids(adapter, search("%_o")).await?, (strings(&["a3"]), 1));
    assert_eq!(ids(adapter, search("%")).await?, (strings(&["a3"]), 1));
    assert_eq!(ids(adapter, search("_")).await?, (strings(&["a3"]), 1));

    // 要約（excerpt）も探す
    let with_excerpt = |term: &str| ContentListQuery {
        search: Some(term.to_string()),
        search_columns: vec!["excerpt".to_string()],
        ..Default::default()
    };
    assert_eq!(
        ids(adapter, with_excerpt("wasm")).await?,
        (strings(&["a1"]), 1)
    );
    assert_eq!(
        ids(adapter, with_excerpt("概要")).await?,
        (strings(&["a4"]), 1)
    );
    assert_eq!(
        ids(adapter, search("wasm")).await?.1,
        0,
        "excerpt is not searched by default"
    );

    // カテゴリの絞り込み。未設定は NULL と空文字の両方
    let category = |value: Option<&str>| ContentListQuery {
        filters: vec![("category_id".to_string(), value.map(str::to_string))],
        ..Default::default()
    };
    assert_eq!(
        ids(adapter, category(Some("cat-1"))).await?,
        (strings(&["a6", "a3", "a1"]), 3)
    );
    assert_eq!(
        ids(adapter, category(None)).await?,
        (strings(&["a7", "a5", "a4"]), 3)
    );

    // 状態・検索・カテゴリの組み合わせ
    let combined = ContentListQuery {
        status: Some("draft".to_string()),
        search: Some("rust".to_string()),
        filters: vec![("category_id".to_string(), Some("cat-1".to_string()))],
        ..Default::default()
    };
    assert_eq!(ids(adapter, combined).await?, (strings(&["a6"]), 1));

    // ページ送り: `total` は条件に合う全件数
    let page = |offset: u64| ContentListQuery {
        search: Some("rust".to_string()),
        limit: Some(2),
        offset,
        ..Default::default()
    };
    assert_eq!(ids(adapter, page(0)).await?, (strings(&["a6", "a2"]), 3));
    assert_eq!(ids(adapter, page(2)).await?, (strings(&["a1"]), 3));
    assert_eq!(ids(adapter, page(4)).await?, (vec![], 3));

    // 安全でない列名では何にも一致しない（SQL に埋め込まない）
    let unsafe_filter = ContentListQuery {
        filters: vec![(
            "id; DROP TABLE content_article".to_string(),
            Some("x".to_string()),
        )],
        ..Default::default()
    };
    assert_eq!(ids(adapter, unsafe_filter).await?, (vec![], 0));
    assert_eq!(ids(adapter, ContentListQuery::default()).await?.1, 7);

    // タグでの絞り込み
    adapter
        .insert_tag(&aegis_db::TagRecord {
            id: "t-rust".to_string(),
            slug: "rust".to_string(),
            name: "Rust".to_string(),
            description: None,
            created_at: "2026-10-01T00:00:00Z".to_string(),
            updated_at: "2026-10-01T00:00:00Z".to_string(),
        })
        .await?;
    adapter
        .insert_tag(&aegis_db::TagRecord {
            id: "t-web".to_string(),
            slug: "web".to_string(),
            name: "Web".to_string(),
            description: None,
            created_at: "2026-10-01T00:00:00Z".to_string(),
            updated_at: "2026-10-01T00:00:00Z".to_string(),
        })
        .await?;

    // a1 と a6 に rust、a2 に web を付ける
    adapter
        .set_content_tags(
            "article",
            "a1",
            &["t-rust".to_string()],
            "2026-10-01T00:00:00Z",
        )
        .await?;
    adapter
        .set_content_tags(
            "article",
            "a6",
            &["t-rust".to_string()],
            "2026-10-01T00:00:00Z",
        )
        .await?;
    adapter
        .set_content_tags(
            "article",
            "a2",
            &["t-web".to_string()],
            "2026-10-01T00:00:00Z",
        )
        .await?;

    let by_tag = |tag_id: &str| ContentListQuery {
        tag_id: Some(tag_id.to_string()),
        ..Default::default()
    };
    assert_eq!(
        ids(adapter, by_tag("t-rust")).await?,
        (strings(&["a6", "a1"]), 2)
    );
    assert_eq!(ids(adapter, by_tag("t-web")).await?, (strings(&["a2"]), 1));
    assert_eq!(ids(adapter, by_tag("no-such-tag")).await?, (vec![], 0));

    // タグとステータスの組み合わせ
    let tag_and_status = ContentListQuery {
        tag_id: Some("t-rust".to_string()),
        status: Some("published".to_string()),
        ..Default::default()
    };
    assert_eq!(ids(adapter, tag_and_status).await?, (strings(&["a1"]), 1));

    Ok(())
}

#[tokio::test]
async fn test_content_list_query_sqlite() {
    let adapter = SqliteAdapter::connect("sqlite::memory:", 1).await.unwrap();
    run_list_query_test(&adapter)
        .await
        .expect("SQLite content list test failed");
}

fn is_port_reachable(host_port: &str) -> bool {
    let Ok(addr) = host_port.parse() else {
        return false;
    };
    std::net::TcpStream::connect_timeout(&addr, std::time::Duration::from_millis(150)).is_ok()
}

#[tokio::test]
async fn test_content_list_query_postgres() {
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
    let _ = sqlx::query("DROP TABLE IF EXISTS content_article, _aegis_content_tags, _aegis_tags, _aegis_migrations CASCADE;")
        .execute(adapter.pool())
        .await;
    run_list_query_test(&adapter)
        .await
        .expect("PostgreSQL content list test failed");
}

#[tokio::test]
async fn test_content_list_query_mysql() {
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
    let _ = sqlx::query("DROP TABLE IF EXISTS content_article, _aegis_content_tags, _aegis_tags, _aegis_migrations;")
        .execute(adapter.pool())
        .await;
    run_list_query_test(&adapter)
        .await
        .expect("MySQL content list test failed");
}
