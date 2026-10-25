//! コメントの保存（`CommentStorage`）を SQLite と、コンテナ稼働時は PostgreSQL / MySQL でも確かめる。
//! テーブル・索引の作成（MySQL の索引の長さ）、一覧の絞り込みと並び順、状態ごとの件数、
//! まとめての状態変更・削除、承認済みの投稿者の判定、返信通知の停止

use std::env;

use aegis_core::Result;
use aegis_db::{
    CommentListQuery, CommentRecord, DatabaseAdapter, MySqlAdapter, PostgresAdapter, SqliteAdapter,
};

fn comment(id: &str, content_id: &str, status: &str, created_at: &str) -> CommentRecord {
    CommentRecord {
        id: id.to_string(),
        content_type: "article".to_string(),
        content_id: content_id.to_string(),
        parent_id: None,
        author_name: "Hanako".to_string(),
        author_email: "hanako@example.com".to_string(),
        author_user_id: None,
        body: format!("Comment {}", id),
        status: status.to_string(),
        ip: Some("203.0.113.7".to_string()),
        user_agent: Some("test".to_string()),
        notify_replies: false,
        notify_token: None,
        created_at: created_at.to_string(),
        updated_at: created_at.to_string(),
    }
}

async fn run_comment_storage_test<A: DatabaseAdapter>(adapter: &A) -> Result<()> {
    adapter.init().await?;
    // 2 回目の init（再起動）でも失敗しない
    adapter.init().await?;

    let mut reply = comment("c2", "a1", "pending", "2026-10-25T00:00:02Z");
    reply.parent_id = Some("c1".to_string());
    reply.notify_replies = true;
    reply.notify_token = Some("token".to_string());
    for c in [
        comment("c1", "a1", "approved", "2026-10-25T00:00:01Z"),
        reply,
        comment("c3", "a1", "approved", "2026-10-25T00:00:03Z"),
        comment("c4", "a2", "spam", "2026-10-25T00:00:04Z"),
    ] {
        adapter.insert_comment(&c).await?;
    }

    let got = adapter.get_comment("c2").await?.expect("c2");
    assert_eq!(got.parent_id.as_deref(), Some("c1"));
    assert!(got.notify_replies);
    assert_eq!(got.notify_token.as_deref(), Some("token"));
    assert!(adapter.get_comment("missing").await?.is_none());

    // 公開ページ: 承認済みを古い順
    let page = adapter
        .list_comments(&CommentListQuery {
            content_type: Some("article".to_string()),
            content_id: Some("a1".to_string()),
            status: Some("approved".to_string()),
            oldest_first: true,
            ..Default::default()
        })
        .await?;
    let ids: Vec<_> = page.comments.iter().map(|c| c.id.as_str()).collect();
    assert_eq!(ids, ["c1", "c3"]);
    assert_eq!(page.total, 2);

    // Admin Studio: 新しい順・ページ送り
    let page = adapter
        .list_comments(&CommentListQuery {
            limit: Some(2),
            offset: 1,
            ..Default::default()
        })
        .await?;
    let ids: Vec<_> = page.comments.iter().map(|c| c.id.as_str()).collect();
    assert_eq!(ids, ["c3", "c2"]);
    assert_eq!(page.total, 4);

    let mut counts = adapter.count_comments_by_status().await?;
    counts.sort();
    assert_eq!(
        counts,
        vec![
            ("approved".to_string(), 2),
            ("pending".to_string(), 1),
            ("spam".to_string(), 1)
        ]
    );

    assert!(
        adapter
            .has_approved_comment("hanako@example.com", "Hanako")
            .await?
    );
    assert!(
        !adapter
            .has_approved_comment("hanako@example.com", "Someone else")
            .await?
    );

    let changed = adapter
        .set_comments_status(
            &["c2".to_string(), "c4".to_string(), "missing".to_string()],
            "trash",
            "2026-10-26T00:00:00Z",
        )
        .await?;
    assert_eq!(changed, 2);
    let got = adapter.get_comment("c4").await?.expect("c4");
    assert_eq!(got.status, "trash");
    assert_eq!(got.updated_at, "2026-10-26T00:00:00Z");
    assert_eq!(adapter.set_comments_status(&[], "trash", "x").await?, 0);

    assert!(adapter.set_comment_notify_replies("c2", false).await?);
    assert!(!adapter.get_comment("c2").await?.expect("c2").notify_replies);
    assert!(!adapter.set_comment_notify_replies("missing", false).await?);

    assert_eq!(adapter.delete_comments(&["c4".to_string()]).await?, 1);
    assert_eq!(adapter.delete_content_comments("article", "a1").await?, 3);
    assert_eq!(
        adapter
            .list_comments(&CommentListQuery::default())
            .await?
            .total,
        0
    );
    Ok(())
}

#[tokio::test]
async fn test_sqlite_comment_storage() {
    let adapter = SqliteAdapter::connect("sqlite::memory:", 1).await.unwrap();
    run_comment_storage_test(&adapter)
        .await
        .expect("SQLite comment storage test failed");
}

fn is_port_reachable(host_port: &str) -> bool {
    let Ok(addr) = host_port.parse() else {
        return false;
    };
    std::net::TcpStream::connect_timeout(&addr, std::time::Duration::from_millis(150)).is_ok()
}

#[tokio::test]
async fn test_postgres_comment_storage_real_db() {
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
    let _ = sqlx::query("DROP TABLE IF EXISTS _aegis_comments;")
        .execute(adapter.pool())
        .await;
    run_comment_storage_test(&adapter)
        .await
        .expect("PostgreSQL comment storage test failed");
}

#[tokio::test]
async fn test_mysql_comment_storage_real_db() {
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
    let _ = sqlx::query("DROP TABLE IF EXISTS _aegis_comments;")
        .execute(adapter.pool())
        .await;
    run_comment_storage_test(&adapter)
        .await
        .expect("MySQL comment storage test failed");
}
