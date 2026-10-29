//! 問い合わせの保存（`ContactStorage`）を SQLite と、コンテナ稼働時は PostgreSQL / MySQL でも確かめる。
//! テーブル・索引の作成、一覧の絞り込み・新しい順・ページング、状態ごとの件数、まとめての状態変更・削除、
//! 保存期間による削除

use std::env;

use aegis_core::Result;
use aegis_db::{
    ContactListQuery, ContactMessageRecord, DatabaseAdapter, MySqlAdapter, PostgresAdapter,
    SqliteAdapter,
};

fn message(id: &str, form: &str, status: &str, created_at: &str) -> ContactMessageRecord {
    ContactMessageRecord {
        id: id.to_string(),
        form: form.to_string(),
        name: "Hanako".to_string(),
        email: "hanako@example.com".to_string(),
        subject: format!("Subject {}", id),
        fields_json: r#"[{"name":"message","label":"Message","value":"こんにちは"}]"#.to_string(),
        status: status.to_string(),
        ip: Some("203.0.113.7".to_string()),
        user_agent: None,
        created_at: created_at.to_string(),
        updated_at: created_at.to_string(),
    }
}

async fn run_contact_storage_test<A: DatabaseAdapter>(adapter: &A) -> Result<()> {
    adapter.init().await?;
    adapter.init().await?;

    for m in [
        message("m1", "default", "new", "2026-10-01T00:00:00Z"),
        message("m2", "default", "read", "2026-10-02T00:00:00Z"),
        message("m3", "sales", "new", "2026-10-03T00:00:00Z"),
        message("m4", "default", "spam", "2026-10-04T00:00:00Z"),
    ] {
        adapter.insert_contact_message(&m).await?;
    }

    let got = adapter.get_contact_message("m1").await?.unwrap();
    assert_eq!(got, message("m1", "default", "new", "2026-10-01T00:00:00Z"));
    assert!(adapter.get_contact_message("missing").await?.is_none());

    let all = adapter
        .list_contact_messages(&ContactListQuery::default())
        .await?;
    assert_eq!(all.total, 4);
    let ids: Vec<&str> = all.messages.iter().map(|m| m.id.as_str()).collect();
    assert_eq!(ids, vec!["m4", "m3", "m2", "m1"]);

    let page = adapter
        .list_contact_messages(&ContactListQuery {
            status: Some("new".into()),
            limit: Some(1),
            offset: 1,
            ..Default::default()
        })
        .await?;
    assert_eq!(page.total, 2);
    assert_eq!(page.messages[0].id, "m1");

    let sales = adapter
        .list_contact_messages(&ContactListQuery {
            form: Some("sales".into()),
            ..Default::default()
        })
        .await?;
    assert_eq!(sales.total, 1);

    let mut counts = adapter.count_contact_messages_by_status().await?;
    counts.sort();
    assert_eq!(
        counts,
        vec![
            ("new".to_string(), 2),
            ("read".to_string(), 1),
            ("spam".to_string(), 1)
        ]
    );

    let changed = adapter
        .set_contact_messages_status(
            &["m1".to_string(), "m3".to_string(), "missing".to_string()],
            "done",
            "2026-10-05T00:00:00Z",
        )
        .await?;
    assert_eq!(changed, 2);
    let m1 = adapter.get_contact_message("m1").await?.unwrap();
    assert_eq!(m1.status, "done");
    assert_eq!(m1.updated_at, "2026-10-05T00:00:00Z");

    assert_eq!(
        adapter.delete_contact_messages(&["m4".to_string()]).await?,
        1
    );
    assert_eq!(adapter.delete_contact_messages(&[]).await?, 0);
    assert_eq!(
        adapter
            .delete_contact_messages_before("2026-10-03T00:00:00Z")
            .await?,
        2
    );
    let rest = adapter
        .list_contact_messages(&ContactListQuery::default())
        .await?;
    assert_eq!(rest.total, 1);
    assert_eq!(rest.messages[0].id, "m3");
    Ok(())
}

#[tokio::test]
async fn test_sqlite_contact_storage() {
    let adapter = SqliteAdapter::connect("sqlite::memory:", 1).await.unwrap();
    run_contact_storage_test(&adapter)
        .await
        .expect("SQLite contact storage test failed");
}

fn is_port_reachable(host_port: &str) -> bool {
    let Ok(addr) = host_port.parse() else {
        return false;
    };
    std::net::TcpStream::connect_timeout(&addr, std::time::Duration::from_millis(150)).is_ok()
}

#[tokio::test]
async fn test_postgres_contact_storage_real_db() {
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
    let _ = sqlx::query("DROP TABLE IF EXISTS _aegis_contact_messages;")
        .execute(adapter.pool())
        .await;
    run_contact_storage_test(&adapter)
        .await
        .expect("PostgreSQL contact storage test failed");
}

#[tokio::test]
async fn test_mysql_contact_storage_real_db() {
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
    let _ = sqlx::query("DROP TABLE IF EXISTS _aegis_contact_messages;")
        .execute(adapter.pool())
        .await;
    run_contact_storage_test(&adapter)
        .await
        .expect("MySQL contact storage test failed");
}
