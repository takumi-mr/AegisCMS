//! バックアップ → 復元を、DB の種類をまたいで確かめる（SQLite → PostgreSQL → MySQL → SQLite）。
//!
//! バックアップは DB の種類に依らない JSON なので、どの DB から取っても、どの DB に戻しても、
//! 同じ値になるはず。型の扱い（真偽値・数値・長い文字列・日本語や絵文字）が DB ごとに違うので、
//! SQLite だけのテストでは見つからない違いを、実機の PostgreSQL / MySQL で確かめる。
//!
//! コンテナが起動していない通常の開発（`task test`・CI）では、TCP の確認でスキップする。
//! `task docker:dbs:up` のあと `task test:dbs` で実行する。接続先は `POSTGRES_URL` / `MYSQL_URL`。

use std::collections::HashMap;
use std::env;
use std::io::Cursor;
use std::sync::Arc;

use aegis_backup::{dump_to_writer, restore_from_reader, DumpOptions, RestoreOptions};
use aegis_core::StorageBackend;
use aegis_db::{
    CommentRecord, ContentRecord, DatabaseAdapter, ForumCategoryRecord, MySqlAdapter,
    PostgresAdapter, SqliteAdapter, TagRecord, UserRecord,
};
use aegis_schema::SchemaDefinition;
use aegis_storage::LocalStorageBackend;
use serde_json::json;

const TS: &str = "2026-11-08T00:00:00Z";
/// 5 文字 × 1500 回。日本語と絵文字（4 バイト）を含む長い文字列
fn long_text() -> String {
    "長い文章🙂 ".repeat(1500)
}
const TITLE: &str = "タイトル🙂 with \"quotes\" & <tags>";
const COMMENT_BODY: &str = "コメント🙂\n二行目\t(タブ)";

fn is_port_reachable(host_port: &str) -> bool {
    let Ok(addr) = host_port.parse() else {
        return false;
    };
    std::net::TcpStream::connect_timeout(&addr, std::time::Duration::from_millis(150)).is_ok()
}

fn schemas() -> HashMap<String, SchemaDefinition> {
    let article: SchemaDefinition = serde_json::from_value(json!({
        "content_type": "article",
        "label": "Article",
        "fields": [
            { "name": "excerpt", "type": "text", "required": false },
            { "name": "views", "type": "integer", "required": false },
            { "name": "featured", "type": "boolean", "required": false }
        ]
    }))
    .unwrap();
    HashMap::from([("article".to_string(), article)])
}

fn storage(dir: &tempfile::TempDir) -> Arc<dyn StorageBackend> {
    Arc::new(LocalStorageBackend::new(dir.path(), "/uploads".to_string()))
}

/// 表を作り直した、空の DB（`init` でシステムテーブル、`run_migrations` でコンテンツの表）
async fn prepare(db: &Arc<dyn DatabaseAdapter>, schemas: &HashMap<String, SchemaDefinition>) {
    db.init().await.expect("init");
    db.run_migrations(schemas).await.expect("run_migrations");
}

async fn postgres() -> Option<Arc<dyn DatabaseAdapter>> {
    let custom = env::var("POSTGRES_URL").ok();
    if custom.is_none() && !is_port_reachable("127.0.0.1:5432") {
        eprintln!("Skipping PostgreSQL: 127.0.0.1:5432 is not reachable (task docker:dbs:up)");
        return None;
    }
    let url =
        custom.unwrap_or_else(|| "postgres://aegis:password@127.0.0.1:5432/aegis_test".to_string());
    let Ok(adapter) = PostgresAdapter::connect(&url, 5).await else {
        eprintln!("Skipping PostgreSQL: failed to connect");
        return None;
    };
    aegis_db::test_support::cleanup_postgres(&adapter)
        .await
        .ok()?;
    Some(Arc::new(adapter))
}

async fn mysql() -> Option<Arc<dyn DatabaseAdapter>> {
    let custom = env::var("MYSQL_URL").ok();
    if custom.is_none() && !is_port_reachable("127.0.0.1:3306") {
        eprintln!("Skipping MySQL: 127.0.0.1:3306 is not reachable (task docker:dbs:up)");
        return None;
    }
    let url =
        custom.unwrap_or_else(|| "mysql://aegis:password@127.0.0.1:3306/aegis_test".to_string());
    let Ok(adapter) = MySqlAdapter::connect(&url, 5).await else {
        eprintln!("Skipping MySQL: failed to connect");
        return None;
    };
    aegis_db::test_support::cleanup_mysql(&adapter).await.ok()?;
    Some(Arc::new(adapter))
}

async fn sqlite() -> Arc<dyn DatabaseAdapter> {
    Arc::new(
        SqliteAdapter::connect("sqlite::memory:", 1)
            .await
            .expect("sqlite"),
    )
}

/// DB の種類で扱いが違いそうな値を入れる
async fn seed(db: &Arc<dyn DatabaseAdapter>, schemas: &HashMap<String, SchemaDefinition>) {
    db.set_setting("site.title", "往復テスト🙂").await.unwrap();
    db.set_setting("x.long", &long_text()).await.unwrap();
    db.insert_user(&UserRecord {
        id: "u-1".to_string(),
        username: "alice".to_string(),
        email: Some("alice@example.com".to_string()),
        display_name: Some("アリス🙂".to_string()),
        role: "admin".to_string(),
        password_hash: "$argon2id$v=19$m=19456,t=2,p=1$hash".to_string(),
        disabled: false,
        avatar_key: Some("avatars/alice.png".to_string()),
        created_at: TS.to_string(),
        updated_at: TS.to_string(),
        pending_approval: true,
        totp_secret: Some("JBSWY3DPEHPK3PXP".to_string()),
        totp_enabled: true,
        backup_codes: Some(r#"["h1","h2"]"#.to_string()),
    })
    .await
    .unwrap();

    let mut custom = HashMap::new();
    custom.insert("excerpt".to_string(), json!(long_text()));
    custom.insert("views".to_string(), json!(42));
    custom.insert("featured".to_string(), json!(true));
    db.insert_content(
        &schemas["article"],
        &ContentRecord {
            id: "art-1".to_string(),
            content_type: "article".to_string(),
            title: TITLE.to_string(),
            slug: "hello".to_string(),
            status: "draft".to_string(),
            blocks_json: r#"[{"name":"core/paragraph","text":"本文🙂"}]"#.to_string(),
            author_id: Some("alice".to_string()),
            published_at: None,
            unpublish_at: None,
            created_at: TS.to_string(),
            updated_at: TS.to_string(),
            custom_fields: custom,
        },
    )
    .await
    .unwrap();

    db.insert_comment(&CommentRecord {
        id: "cmt-1".to_string(),
        content_type: "article".to_string(),
        content_id: "art-1".to_string(),
        parent_id: None,
        author_name: "読者".to_string(),
        author_email: "reader@example.com".to_string(),
        author_user_id: None,
        body: COMMENT_BODY.to_string(),
        status: "approved".to_string(),
        ip: Some("203.0.113.7".to_string()),
        user_agent: None,
        notify_replies: true,
        notify_token: Some("tok".to_string()),
        created_at: TS.to_string(),
        updated_at: TS.to_string(),
    })
    .await
    .unwrap();
    db.insert_tag(&TagRecord {
        id: "tag-1".to_string(),
        slug: "rust".to_string(),
        name: "Rust 🦀".to_string(),
        description: Some("説明".to_string()),
        created_at: TS.to_string(),
        updated_at: TS.to_string(),
    })
    .await
    .unwrap();
    db.insert_category(&ForumCategoryRecord {
        id: "fc-1".to_string(),
        slug: "members-only".to_string(),
        name: "会員だけ".to_string(),
        description: String::new(),
        position: 3,
        visibility: "members".to_string(),
        created_at: TS.to_string(),
        updated_at: TS.to_string(),
    })
    .await
    .unwrap();
}

/// `seed` で入れた値が、そのまま読めること（バイト単位で同じ文字列、真偽値・数値の型も）
async fn verify(
    db: &Arc<dyn DatabaseAdapter>,
    schemas: &HashMap<String, SchemaDefinition>,
    at: &str,
) {
    let settings = db.list_settings().await.unwrap();
    assert_eq!(
        settings.get("site.title").map(String::as_str),
        Some("往復テスト🙂"),
        "{at}"
    );
    assert_eq!(
        settings.get("x.long"),
        Some(&long_text()),
        "{at}: long text"
    );

    let user = db.get_user_by_username("alice").await.unwrap().expect(at);
    assert_eq!(user.display_name.as_deref(), Some("アリス🙂"), "{at}");
    assert!(
        user.pending_approval && user.totp_enabled && !user.disabled,
        "{at}: booleans"
    );
    assert_eq!(user.backup_codes.as_deref(), Some(r#"["h1","h2"]"#), "{at}");

    let article = db
        .get_content(&schemas["article"], "art-1")
        .await
        .unwrap()
        .expect(at);
    assert_eq!(article.title, TITLE, "{at}");
    assert_eq!(
        article.blocks_json, r#"[{"name":"core/paragraph","text":"本文🙂"}]"#,
        "{at}"
    );
    assert_eq!(
        article.custom_fields.get("excerpt"),
        Some(&json!(long_text())),
        "{at}: text field"
    );
    assert_eq!(
        article.custom_fields.get("views"),
        Some(&json!(42)),
        "{at}: integer field"
    );
    // 真偽値は、どの DB でも `true` で読める（SQLite も `1` ではなく `true`）
    assert_eq!(
        article.custom_fields.get("featured"),
        Some(&json!(true)),
        "{at}: boolean field"
    );

    let tag = db.get_tag("tag-1").await.unwrap().expect(at);
    assert_eq!(
        (tag.name.as_str(), tag.description.as_deref()),
        ("Rust 🦀", Some("説明")),
        "{at}"
    );

    let category = db.get_category("fc-1").await.unwrap().expect(at);
    assert_eq!(
        (category.visibility.as_str(), category.position),
        ("members", 3),
        "{at}"
    );

    let comments = db
        .list_comments(&aegis_db::CommentListQuery::default())
        .await
        .unwrap();
    let comment = comments
        .comments
        .iter()
        .find(|c| c.id == "cmt-1")
        .expect(at);
    assert_eq!(comment.body, COMMENT_BODY, "{at}");
    assert!(comment.notify_replies, "{at}: notify_replies");
}

/// `from` から取ったバックアップを `to`（空の DB）に戻し、中身が同じことを確かめる
async fn hop(
    from: &Arc<dyn DatabaseAdapter>,
    to: &Arc<dyn DatabaseAdapter>,
    schemas: &HashMap<String, SchemaDefinition>,
    label: &str,
) {
    let from_dir = tempfile::tempdir().unwrap();
    let from_storage = storage(&from_dir);
    from_storage
        .put("2026/11/a.png", b"PNGDATA", "image/png")
        .await
        .unwrap();
    let mut zip = Vec::new();
    dump_to_writer(
        from,
        &from_storage,
        schemas,
        &DumpOptions::default(),
        Cursor::new(&mut zip),
    )
    .await
    .unwrap_or_else(|e| panic!("{label}: dump failed: {e}"));

    prepare(to, schemas).await;
    let to_dir = tempfile::tempdir().unwrap();
    let to_storage = storage(&to_dir);
    let summary = restore_from_reader(
        to,
        &to_storage,
        schemas,
        &RestoreOptions::default(),
        Cursor::new(&zip),
    )
    .await
    .unwrap_or_else(|e| panic!("{label}: restore failed: {e}"));
    assert!(
        summary.warnings.is_empty(),
        "{label}: {:?}",
        summary.warnings
    );
    assert_eq!(summary.storage_files_restored, 1, "{label}");
    assert_eq!(
        to_storage.get("2026/11/a.png").await.unwrap(),
        b"PNGDATA".to_vec(),
        "{label}"
    );
    verify(to, schemas, label).await;
}

/// SQLite → PostgreSQL → MySQL → SQLite と、バックアップを渡していく
#[tokio::test]
async fn test_backup_restores_across_database_kinds() {
    let (pg, my) = (postgres().await, mysql().await);
    if pg.is_none() && my.is_none() {
        return;
    }
    let schemas = schemas();
    let source = sqlite().await;
    prepare(&source, &schemas).await;
    seed(&source, &schemas).await;
    verify(&source, &schemas, "sqlite (source)").await;

    // それぞれ、動いている DB だけを通す（片方しか無ければ、その DB と往復する）
    let mut current = source.clone();
    let mut current_label = "sqlite";
    for (db, label) in [(pg, "postgres"), (my, "mysql")] {
        if let Some(db) = db {
            hop(
                &current,
                &db,
                &schemas,
                &format!("{current_label} -> {label}"),
            )
            .await;
            current = db;
            current_label = label;
        }
    }
    let back = sqlite().await;
    hop(
        &current,
        &back,
        &schemas,
        &format!("{current_label} -> sqlite"),
    )
    .await;
}
