//! 通知の送信待ち（`NotificationQueueStorage`）を SQLite と、コンテナ稼働時は PostgreSQL / MySQL でも確かめる。
//! テーブル・索引の作成、送る時刻を迎えたものの取り出し、送信の権利の取り合い（1 台だけが取れる・期限切れは
//! 取り直せる）、試行の結果の記録、失敗したものの再送・一覧・古いものの削除

use std::env;

use aegis_core::Result;
use aegis_db::{
    DatabaseAdapter, MySqlAdapter, NotificationListQuery, NotificationRelease, PostgresAdapter,
    QueuedNotification, SqliteAdapter, QUEUE_FAILED, QUEUE_PENDING,
};

fn queued(id: &str, next_attempt_at: &str) -> QueuedNotification {
    QueuedNotification {
        id: id.to_string(),
        kind: "order.placed".to_string(),
        channel: "email".to_string(),
        message: format!(r#"{{"subject":"{}"}}"#, id),
        status: QUEUE_PENDING.to_string(),
        attempts: 0,
        next_attempt_at: next_attempt_at.to_string(),
        locked_until: None,
        last_error: None,
        created_at: next_attempt_at.to_string(),
        updated_at: next_attempt_at.to_string(),
    }
}

const T0: &str = "2026-10-25T00:00:00.000000Z";
const T1: &str = "2026-10-25T00:00:01.000000Z";
const T2: &str = "2026-10-25T00:00:02.000000Z";
const T5: &str = "2026-10-25T00:00:05.000000Z";
const T9: &str = "2026-10-25T00:00:09.000000Z";

async fn run_notification_queue_test<A: DatabaseAdapter>(adapter: &A) -> Result<()> {
    adapter.init().await?;
    // 2 回目の init（再起動）でも失敗しない
    adapter.init().await?;

    adapter.enqueue_notifications(&[]).await?;
    adapter
        .enqueue_notifications(&[queued("n1", T0), queued("n2", T1), queued("n3", T9)])
        .await?;

    // T2 の時点で送るのは n1, n2（古い順）。n3 はまだ
    let due = adapter.due_notifications(T2, 10).await?;
    let ids: Vec<_> = due.iter().map(|n| n.id.as_str()).collect();
    assert_eq!(ids, ["n1", "n2"]);
    assert_eq!(due[0].message, r#"{"subject":"n1"}"#);
    assert_eq!(adapter.due_notifications(T2, 1).await?.len(), 1);

    // 送信の権利は 1 台だけが取れる。取ったものは期限まで取り出されない
    assert!(adapter.claim_notification("n1", T2, T5).await?);
    assert!(!adapter.claim_notification("n1", T2, T5).await?);
    let ids: Vec<_> = adapter
        .due_notifications(T2, 10)
        .await?
        .into_iter()
        .map(|n| n.id)
        .collect();
    assert_eq!(ids, ["n2"]);
    // 期限が過ぎれば（送っていたサーバーが止まった）ほかのサーバーが取り直せる
    assert!(adapter.claim_notification("n1", T9, T9).await?);
    assert!(!adapter.claim_notification("missing", T2, T5).await?);

    // 一時的な失敗: 次の試行の日時へ
    adapter
        .release_notification(&NotificationRelease {
            id: "n1".to_string(),
            status: QUEUE_PENDING.to_string(),
            attempts: 1,
            next_attempt_at: T5.to_string(),
            last_error: Some("SMTP: connection refused".to_string()),
            updated_at: T2.to_string(),
        })
        .await?;
    let n1 = adapter.get_notification("n1").await?.expect("n1");
    assert_eq!(n1.attempts, 1);
    assert_eq!(n1.locked_until, None);
    assert_eq!(n1.last_error.as_deref(), Some("SMTP: connection refused"));
    assert!(adapter
        .due_notifications(T2, 10)
        .await?
        .iter()
        .all(|n| n.id != "n1"));
    assert!(adapter
        .due_notifications(T5, 10)
        .await?
        .iter()
        .any(|n| n.id == "n1"));

    // 送れた: 消す
    assert!(adapter.claim_notification("n2", T2, T5).await?);
    assert!(adapter.delete_notification("n2").await?);
    assert!(!adapter.delete_notification("n2").await?);

    // 諦めた: failed は取り出されず、権利も取れない
    adapter
        .release_notification(&NotificationRelease {
            id: "n1".to_string(),
            status: QUEUE_FAILED.to_string(),
            attempts: 3,
            next_attempt_at: T5.to_string(),
            last_error: Some("SMTP: 550 mailbox unavailable".to_string()),
            updated_at: T5.to_string(),
        })
        .await?;
    let ids: Vec<_> = adapter
        .due_notifications(T9, 10)
        .await?
        .into_iter()
        .map(|n| n.id)
        .collect();
    assert_eq!(ids, ["n3"]);
    assert!(!adapter.claim_notification("n1", T9, T9).await?);

    let mut counts = adapter.count_notifications_by_status().await?;
    counts.sort();
    assert_eq!(
        counts,
        vec![
            (QUEUE_FAILED.to_string(), 1),
            (QUEUE_PENDING.to_string(), 1)
        ]
    );
    let page = adapter
        .list_notifications(&NotificationListQuery {
            status: Some(QUEUE_FAILED.to_string()),
            limit: 10,
            offset: 0,
        })
        .await?;
    assert_eq!(page.total, 1);
    assert_eq!(page.items[0].id, "n1");
    let page = adapter
        .list_notifications(&NotificationListQuery {
            status: None,
            limit: 1,
            offset: 0,
        })
        .await?;
    assert_eq!(page.total, 2);
    assert_eq!(page.items[0].id, "n3", "newest first");

    // 再送: 最初の試行から。pending のものは対象外
    assert!(!adapter.retry_notification("n3", T9).await?);
    assert!(adapter.retry_notification("n1", T9).await?);
    let n1 = adapter.get_notification("n1").await?.expect("n1");
    assert_eq!(n1.status, QUEUE_PENDING);
    assert_eq!(n1.attempts, 0);
    assert_eq!(n1.next_attempt_at, T9);

    // 古い失敗だけを消す
    adapter
        .release_notification(&NotificationRelease {
            id: "n1".to_string(),
            status: QUEUE_FAILED.to_string(),
            attempts: 1,
            next_attempt_at: T9.to_string(),
            last_error: None,
            updated_at: T1.to_string(),
        })
        .await?;
    assert_eq!(adapter.delete_failed_notifications_before(T0).await?, 0);
    assert_eq!(adapter.delete_failed_notifications_before(T2).await?, 1);
    assert!(adapter.get_notification("n1").await?.is_none());
    assert!(adapter.get_notification("n3").await?.is_some());

    // 大きな本文（HTML メール）も保存できる（MySQL の TEXT は 64KB まで）
    let mut big = queued("n4", T0);
    big.message = "x".repeat(200_000);
    adapter.enqueue_notifications(&[big]).await?;
    assert_eq!(
        adapter
            .get_notification("n4")
            .await?
            .expect("n4")
            .message
            .len(),
        200_000
    );
    Ok(())
}

#[tokio::test]
async fn test_sqlite_notification_queue() {
    let adapter = SqliteAdapter::connect("sqlite::memory:", 1).await.unwrap();
    run_notification_queue_test(&adapter)
        .await
        .expect("SQLite notification queue test failed");
}

fn is_port_reachable(host_port: &str) -> bool {
    let Ok(addr) = host_port.parse() else {
        return false;
    };
    std::net::TcpStream::connect_timeout(&addr, std::time::Duration::from_millis(150)).is_ok()
}

#[tokio::test]
async fn test_postgres_notification_queue_real_db() {
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
    let _ = sqlx::query("DROP TABLE IF EXISTS _aegis_notification_queue;")
        .execute(adapter.pool())
        .await;
    run_notification_queue_test(&adapter)
        .await
        .expect("PostgreSQL notification queue test failed");
}

#[tokio::test]
async fn test_mysql_notification_queue_real_db() {
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
    let _ = sqlx::query("DROP TABLE IF EXISTS _aegis_notification_queue;")
        .execute(adapter.pool())
        .await;
    run_notification_queue_test(&adapter)
        .await
        .expect("MySQL notification queue test failed");
}
