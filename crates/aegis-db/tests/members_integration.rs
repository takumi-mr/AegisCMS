//! SQLite（インメモリ）/ PostgreSQL / MySQL での会員の更新（`MemberStorage`）の検証テスト。
//! SQLite は常に実行し、PostgreSQL / MySQL はコンテナ稼働時（無ければスキップ）に実行する。
//!
//! 会員の更新は、変える列だけを書き、前提の状態を `WHERE` に入れる（監査 A-06・B-04・A-13）。
//! 条件に合わないときに書かないこと、ほかの列を書かないこと、変えたかどうかの戻り値（MySQL は、
//! 値が同じでも「当てはまった行」を数えること）を、3 つの方言で確かめる。

use std::env;

use aegis_core::Result;
use aegis_db::{
    DatabaseAdapter, MemberProfileUpdate, MemberRecord, MySqlAdapter, PostgresAdapter,
    SqliteAdapter,
};

const T: &str = "2026-11-28T00:00:00Z";

fn member(id: &str, status: &str) -> MemberRecord {
    MemberRecord {
        id: id.to_string(),
        email: format!("{id}@example.com"),
        password_hash: "hash-1".to_string(),
        name: "Hanako".to_string(),
        nickname: None,
        avatar_key: None,
        phone: None,
        status: status.to_string(),
        level: "free".to_string(),
        shipping_address_json: None,
        created_at: T.to_string(),
        updated_at: T.to_string(),
        last_login_at: None,
    }
}

async fn get(adapter: &impl DatabaseAdapter, id: &str) -> Result<MemberRecord> {
    Ok(adapter.get_member_by_id(id).await?.expect("member exists"))
}

async fn run_member_update_tests<A: DatabaseAdapter>(adapter: &A) -> Result<()> {
    adapter.init().await?;

    // --- 確認待ちの会員を有効にする（確認待ちのときだけ。登録の内容を反映する） ---
    adapter
        .insert_member(&member("m-pending", "unverified"))
        .await?;
    assert!(
        adapter
            .activate_member("m-pending", "hash-2", "Taro", Some("090"), T)
            .await?
    );
    let m = get(adapter, "m-pending").await?;
    assert_eq!(
        (
            m.status.as_str(),
            m.password_hash.as_str(),
            m.name.as_str(),
            m.phone.as_deref()
        ),
        ("active", "hash-2", "Taro", Some("090"))
    );
    // もう確認待ちではない: 何もしない
    assert!(
        !adapter
            .activate_member("m-pending", "hash-3", "Other", None, T)
            .await?
    );
    assert_eq!(get(adapter, "m-pending").await?.password_hash, "hash-2");

    // --- 状態の変更（`from` のどれかのときだけ） ---
    adapter.insert_member(&member("m-status", "active")).await?;
    assert!(
        adapter
            .set_member_status("m-status", "deleting", T, Some(&["active"]))
            .await?
    );
    assert!(
        !adapter
            .set_member_status("m-status", "deleting", T, Some(&["active"]))
            .await?
    );
    assert!(
        !adapter
            .set_member_status(
                "m-status",
                "active",
                T,
                Some(&["active", "suspended", "unverified"])
            )
            .await?
    );
    assert_eq!(get(adapter, "m-status").await?.status, "deleting");
    // 条件なし（同じ値でも、当てはまった行を変えたものとして返す）
    assert!(
        adapter
            .set_member_status("m-status", "deleting", T, None)
            .await?
    );
    assert!(
        !adapter
            .set_member_status("nobody", "active", T, None)
            .await?
    );

    // --- プロフィール（`active` のときだけ。ほかの列は書かない） ---
    adapter
        .insert_member(&member("m-profile", "active"))
        .await?;
    adapter.set_member_level("m-profile", "vip", T).await?;
    let profile = MemberProfileUpdate {
        name: "Jiro".to_string(),
        nickname: Some("jj".to_string()),
        phone: Some("080".to_string()),
        shipping_address_json: Some("{}".to_string()),
        updated_at: "2026-11-28T01:00:00Z".to_string(),
    };
    assert!(adapter.update_member_profile("m-profile", &profile).await?);
    // 同じ内容でもう一度（MySQL でも、当てはまった行を数える）
    assert!(adapter.update_member_profile("m-profile", &profile).await?);
    let m = get(adapter, "m-profile").await?;
    assert_eq!(
        (
            m.name.as_str(),
            m.nickname.as_deref(),
            m.level.as_str(),
            m.status.as_str()
        ),
        ("Jiro", Some("jj"), "vip", "active")
    );
    adapter
        .set_member_status("m-profile", "suspended", T, None)
        .await?;
    assert!(!adapter.update_member_profile("m-profile", &profile).await?);
    assert_eq!(get(adapter, "m-profile").await?.status, "suspended");

    // --- パスワードの変更（`active` で、今のハッシュが確かめたもののときだけ） ---
    adapter
        .insert_member(&member("m-password", "active"))
        .await?;
    assert!(
        !adapter
            .update_member_password("m-password", "hash-new", "stale-hash", T)
            .await?
    );
    assert!(
        adapter
            .update_member_password("m-password", "hash-new", "hash-1", T)
            .await?
    );
    // 2 回目は、もう確かめたハッシュではない
    assert!(
        !adapter
            .update_member_password("m-password", "hash-other", "hash-1", T)
            .await?
    );
    assert_eq!(get(adapter, "m-password").await?.password_hash, "hash-new");
    adapter
        .set_member_status("m-password", "suspended", T, None)
        .await?;
    assert!(
        !adapter
            .update_member_password("m-password", "hash-x", "hash-new", T)
            .await?
    );

    // --- パスワードの再設定（`active` / `unverified` のときだけ。`unverified` は `active` になる） ---
    adapter
        .insert_member(&member("m-reset", "unverified"))
        .await?;
    assert!(
        adapter
            .reset_member_password("m-reset", "hash-r", T)
            .await?
    );
    let m = get(adapter, "m-reset").await?;
    assert_eq!(
        (m.status.as_str(), m.password_hash.as_str()),
        ("active", "hash-r")
    );
    for status in ["suspended", "deleting"] {
        adapter
            .set_member_status("m-reset", status, T, None)
            .await?;
        assert!(
            !adapter
                .reset_member_password("m-reset", "hash-s", T)
                .await?
        );
    }
    assert_eq!(get(adapter, "m-reset").await?.password_hash, "hash-r");

    // --- アバター（会員自身: 今のキーと `active` が条件。管理者: 条件なし） ---
    adapter.insert_member(&member("m-avatar", "active")).await?;
    assert!(
        adapter
            .update_member_avatar("m-avatar", Some("avatars/a.png"), T, Some(None))
            .await?
    );
    // 同時の差し替え: 片方は、もう今のキーが無い（null）ではない
    assert!(
        !adapter
            .update_member_avatar("m-avatar", Some("avatars/b.png"), T, Some(None))
            .await?
    );
    assert!(
        adapter
            .update_member_avatar(
                "m-avatar",
                Some("avatars/c.png"),
                T,
                Some(Some("avatars/a.png"))
            )
            .await?
    );
    adapter
        .set_member_status("m-avatar", "deleting", T, None)
        .await?;
    assert!(
        !adapter
            .update_member_avatar("m-avatar", None, T, Some(Some("avatars/c.png")))
            .await?
    );
    assert!(
        adapter
            .update_member_avatar("m-avatar", None, T, None)
            .await?
    );
    assert_eq!(get(adapter, "m-avatar").await?.avatar_key, None);

    // --- レベル（同じ値でも、当てはまった行を変えたものとして返す） ---
    assert!(adapter.set_member_level("m-avatar", "vip", T).await?);
    assert!(adapter.set_member_level("m-avatar", "vip", T).await?);
    assert!(!adapter.set_member_level("nobody", "vip", T).await?);
    let m = get(adapter, "m-avatar").await?;
    assert_eq!((m.level.as_str(), m.status.as_str()), ("vip", "deleting"));
    Ok(())
}

fn is_port_reachable(host_port: &str) -> bool {
    let Ok(addr) = host_port.parse() else {
        return false;
    };
    std::net::TcpStream::connect_timeout(&addr, std::time::Duration::from_millis(150)).is_ok()
}

#[tokio::test]
async fn test_sqlite_member_updates() {
    let adapter = SqliteAdapter::connect("sqlite::memory:", 1).await.unwrap();
    run_member_update_tests(&adapter)
        .await
        .expect("SQLite member update test failed");
}

#[tokio::test]
async fn test_postgres_member_updates_real_db() {
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
    aegis_db::test_support::cleanup_postgres(&adapter)
        .await
        .expect("cleanup postgres");
    run_member_update_tests(&adapter)
        .await
        .expect("PostgreSQL member update test failed");
}

#[tokio::test]
async fn test_mysql_member_updates_real_db() {
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
    aegis_db::test_support::cleanup_mysql(&adapter)
        .await
        .expect("cleanup mysql");
    run_member_update_tests(&adapter)
        .await
        .expect("MySQL member update test failed");
}
