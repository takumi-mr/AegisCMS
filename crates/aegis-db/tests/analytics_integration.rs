//! アクセス解析の集計（`AnalyticsStorage`）を SQLite と、コンテナ稼働時は PostgreSQL / MySQL でも確かめる。
//! 加算（同じ行への足し込み）、日ごとの合計、値ごとの上位、長い値の切り詰め、保存期間による削除

use std::env;

use aegis_core::Result;
use aegis_db::{
    AnalyticsCount, AnalyticsTotal, DatabaseAdapter, MySqlAdapter, PostgresAdapter, SqliteAdapter,
};

fn count(day: &str, dimension: &str, value: &str, pageviews: u64, visitors: u64) -> AnalyticsCount {
    AnalyticsCount {
        day: day.to_string(),
        dimension: dimension.to_string(),
        value: value.to_string(),
        pageviews,
        visitors,
    }
}

fn total(value: &str, pageviews: u64, visitors: u64) -> AnalyticsTotal {
    AnalyticsTotal {
        value: value.to_string(),
        pageviews,
        visitors,
    }
}

async fn run_analytics_storage_test<A: DatabaseAdapter>(adapter: &A) -> Result<()> {
    adapter.init().await?;
    adapter.init().await?;

    adapter
        .add_analytics_counts(&[
            count("2026-10-01", "site", "", 3, 2),
            count("2026-10-01", "path", "/", 2, 2),
            count("2026-10-01", "path", "/about", 1, 1),
            count("2026-10-02", "site", "", 1, 1),
            count("2026-10-02", "path", "/about", 1, 1),
        ])
        .await?;
    // 同じ日・切り口・値には足し込む
    adapter
        .add_analytics_counts(&[
            count("2026-10-02", "site", "", 4, 1),
            count("2026-10-02", "path", "/about", 4, 1),
            count("2026-10-03", "path", &format!("/{}", "x".repeat(300)), 1, 1),
        ])
        .await?;
    adapter.add_analytics_counts(&[]).await?;

    let daily = adapter
        .list_analytics_daily("site", "2026-10-01", "2026-10-02")
        .await?;
    assert_eq!(
        daily
            .iter()
            .map(|c| (c.day.as_str(), c.pageviews, c.visitors))
            .collect::<Vec<_>>(),
        vec![("2026-10-01", 3, 2), ("2026-10-02", 5, 2)]
    );

    let top = adapter
        .top_analytics_values("path", "2026-10-01", "2026-10-02", 10)
        .await?;
    assert_eq!(top, vec![total("/about", 6, 3), total("/", 2, 2)]);
    let top = adapter
        .top_analytics_values("path", "2026-10-01", "2026-10-02", 1)
        .await?;
    assert_eq!(top, vec![total("/about", 6, 3)]);

    // 長い値は切り詰めて保存する
    let long = adapter
        .top_analytics_values("path", "2026-10-03", "2026-10-03", 10)
        .await?;
    assert_eq!(
        long[0].value.chars().count(),
        aegis_db::analytics::ANALYTICS_VALUE_MAX_CHARS
    );

    assert_eq!(adapter.delete_analytics_before("2026-10-02").await?, 3);
    assert!(adapter
        .list_analytics_daily("site", "2026-10-01", "2026-10-01")
        .await?
        .is_empty());
    Ok(())
}

#[tokio::test]
async fn test_sqlite_analytics_storage() {
    let adapter = SqliteAdapter::connect("sqlite::memory:", 1).await.unwrap();
    run_analytics_storage_test(&adapter)
        .await
        .expect("SQLite analytics storage test failed");
}

fn is_port_reachable(host_port: &str) -> bool {
    let Ok(addr) = host_port.parse() else {
        return false;
    };
    std::net::TcpStream::connect_timeout(&addr, std::time::Duration::from_millis(150)).is_ok()
}

#[tokio::test]
async fn test_postgres_analytics_storage_real_db() {
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
    let _ = sqlx::query("DROP TABLE IF EXISTS _aegis_analytics;")
        .execute(adapter.pool())
        .await;
    run_analytics_storage_test(&adapter)
        .await
        .expect("PostgreSQL analytics storage test failed");
}

#[tokio::test]
async fn test_mysql_analytics_storage_real_db() {
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
    let _ = sqlx::query("DROP TABLE IF EXISTS _aegis_analytics;")
        .execute(adapter.pool())
        .await;
    run_analytics_storage_test(&adapter)
        .await
        .expect("MySQL analytics storage test failed");
}
