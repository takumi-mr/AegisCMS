//! タグの保存（`TagStorage`）を SQLite と、コンテナ稼働時は PostgreSQL / MySQL でも確かめる。
//! テーブル・索引の作成（MySQL の主キーの長さ）、slug の重複、名前の検索と使われている数、
//! コンテンツへの付け外し、タグの削除での結びつきの掃除

use std::env;

use aegis_core::Result;
use aegis_db::{
    ContentTagRecord, DatabaseAdapter, MySqlAdapter, PostgresAdapter, SqliteAdapter, TagListQuery,
    TagRecord,
};

fn tag(id: &str, slug: &str, name: &str) -> TagRecord {
    TagRecord {
        id: id.to_string(),
        slug: slug.to_string(),
        name: name.to_string(),
        description: None,
        created_at: "2026-10-01T00:00:00Z".to_string(),
        updated_at: "2026-10-01T00:00:00Z".to_string(),
    }
}

fn ids(tags: &[TagRecord]) -> Vec<&str> {
    tags.iter().map(|t| t.id.as_str()).collect()
}

async fn run_tag_storage_test<A: DatabaseAdapter>(adapter: &A) -> Result<()> {
    adapter.init().await?;
    // 2 回目の init（再起動）でも失敗しない
    adapter.init().await?;

    adapter.insert_tag(&tag("t-rust", "rust", "Rust")).await?;
    adapter.insert_tag(&tag("t-go", "go", "Go")).await?;
    let mut docker = tag("t-docker", "docker", "Docker_100%");
    docker.description = Some("Containers".to_string());
    adapter.insert_tag(&docker).await?;

    // slug は重複できない
    assert!(adapter
        .insert_tag(&tag("t-dup", "rust", "Dup"))
        .await
        .is_err());

    assert_eq!(adapter.get_tag("t-go").await?.unwrap().name, "Go");
    assert_eq!(
        adapter
            .get_tag_by_slug("docker")
            .await?
            .unwrap()
            .description,
        Some("Containers".to_string())
    );
    assert!(adapter.get_tag("missing").await?.is_none());

    // 付ける: 記事 2 件と固定ページ 1 件
    adapter
        .set_content_tags(
            "article",
            "a1",
            &["t-rust".to_string(), "t-go".to_string(), "t-go".to_string()],
            "2026-10-02T00:00:00Z",
        )
        .await?;
    adapter
        .set_content_tags(
            "article",
            "a2",
            &["t-rust".to_string()],
            "2026-10-02T00:00:00Z",
        )
        .await?;
    adapter
        .set_content_tags(
            "page",
            "p1",
            &["t-rust".to_string()],
            "2026-10-02T00:00:00Z",
        )
        .await?;

    let page = adapter.list_tags(&TagListQuery::default()).await?;
    assert_eq!(page.total, 3);
    // 名前の昇順: Docker_100%, Go, Rust
    let names: Vec<&str> = page.tags.iter().map(|t| t.tag.name.as_str()).collect();
    assert_eq!(names, vec!["Docker_100%", "Go", "Rust"]);
    let usage: Vec<u64> = page.tags.iter().map(|t| t.usage).collect();
    assert_eq!(usage, vec![0, 1, 3]);

    // 検索（`%` と `_` は文字として探す）とページング
    let found = adapter
        .list_tags(&TagListQuery {
            search: Some("_100%".to_string()),
            ..Default::default()
        })
        .await?;
    assert_eq!(found.total, 1);
    assert_eq!(found.tags[0].tag.id, "t-docker");
    let by_slug = adapter
        .list_tags(&TagListQuery {
            search: Some("RUS".to_string()),
            ..Default::default()
        })
        .await?;
    assert_eq!(by_slug.total, 1);
    let paged = adapter
        .list_tags(&TagListQuery {
            limit: Some(1),
            offset: 1,
            ..Default::default()
        })
        .await?;
    assert_eq!(paged.total, 3);
    assert_eq!(paged.tags[0].tag.id, "t-go");

    // コンテンツのタグ
    assert_eq!(
        ids(&adapter.list_content_tags("article", "a1").await?),
        vec!["t-go", "t-rust"]
    );
    assert!(adapter
        .list_content_tags("article", "none")
        .await?
        .is_empty());
    let several = adapter
        .list_tags_for_contents("article", &["a1".to_string(), "a2".to_string()])
        .await?;
    let pairs: Vec<(&str, &str)> = several
        .iter()
        .map(|(content, t)| (content.as_str(), t.id.as_str()))
        .collect();
    assert_eq!(pairs.len(), 3);
    assert!(pairs.contains(&("a2", "t-rust")));
    assert!(adapter
        .list_tags_for_contents("article", &[])
        .await?
        .is_empty());

    // タグが付いているコンテンツ
    let all = adapter.list_tagged_contents("t-rust", None).await?;
    assert_eq!(all.len(), 3);
    let only_articles = adapter
        .list_tagged_contents("t-rust", Some("article"))
        .await?;
    assert_eq!(only_articles.len(), 2);

    // 付け替え: a1 を Docker だけにする
    adapter
        .set_content_tags(
            "article",
            "a1",
            &["t-docker".to_string()],
            "2026-10-03T00:00:00Z",
        )
        .await?;
    assert_eq!(
        ids(&adapter.list_content_tags("article", "a1").await?),
        vec!["t-docker"]
    );
    adapter
        .set_content_tags("article", "a1", &[], "2026-10-03T00:00:00Z")
        .await?;
    assert!(adapter.list_content_tags("article", "a1").await?.is_empty());

    // 名前・slug の更新
    let mut renamed = tag("t-go", "golang", "Golang");
    renamed.updated_at = "2026-10-04T00:00:00Z".to_string();
    assert!(adapter.update_tag(&renamed).await?);
    assert_eq!(adapter.get_tag("t-go").await?.unwrap().slug, "golang");
    assert!(!adapter.update_tag(&tag("missing", "x", "X")).await?);

    // 復元（バックアップ）: 既にある結びつきはそのまま
    let links = adapter.list_all_content_tags().await?;
    assert_eq!(links.len(), 2);
    let link = ContentTagRecord {
        content_type: "article".to_string(),
        content_id: "a2".to_string(),
        tag_id: "t-rust".to_string(),
        created_at: "2026-10-05T00:00:00Z".to_string(),
    };
    adapter.insert_content_tag(&link).await?;
    assert_eq!(adapter.list_all_content_tags().await?.len(), 2);

    // コンテンツを消したとき
    assert_eq!(adapter.delete_content_tags("article", "a2").await?, 1);
    // タグを消すと、結びつきも消える
    assert!(adapter.delete_tag("t-rust").await?);
    assert!(!adapter.delete_tag("t-rust").await?);
    assert!(adapter
        .list_tagged_contents("t-rust", None)
        .await?
        .is_empty());
    assert!(adapter.get_tag("t-rust").await?.is_none());
    Ok(())
}

#[tokio::test]
async fn test_sqlite_tag_storage() {
    let adapter = SqliteAdapter::connect("sqlite::memory:", 1).await.unwrap();
    run_tag_storage_test(&adapter)
        .await
        .expect("SQLite tag storage test failed");
}

fn is_port_reachable(host_port: &str) -> bool {
    let Ok(addr) = host_port.parse() else {
        return false;
    };
    std::net::TcpStream::connect_timeout(&addr, std::time::Duration::from_millis(150)).is_ok()
}

#[tokio::test]
async fn test_postgres_tag_storage_real_db() {
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
    for sql in [
        "DROP TABLE IF EXISTS _aegis_content_tags;",
        "DROP TABLE IF EXISTS _aegis_tags;",
    ] {
        let _ = sqlx::query(sql).execute(adapter.pool()).await;
    }
    run_tag_storage_test(&adapter)
        .await
        .expect("PostgreSQL tag storage test failed");
}

#[tokio::test]
async fn test_mysql_tag_storage_real_db() {
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
    for sql in [
        "DROP TABLE IF EXISTS _aegis_content_tags;",
        "DROP TABLE IF EXISTS _aegis_tags;",
    ] {
        let _ = sqlx::query(sql).execute(adapter.pool()).await;
    }
    run_tag_storage_test(&adapter)
        .await
        .expect("MySQL tag storage test failed");
}
