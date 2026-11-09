//! 統合テストで実機 DB（PostgreSQL / MySQL）を初期化・クリーンアップするためのテスト支援。

use aegis_core::Result;

/// 統合テストで実機 DB（PostgreSQL / MySQL）を初期化する前に消すテーブル一覧。
/// `_sqlx_migrations` だけでなく、マイグレーションで管理される全システムテーブル（`_aegis_*`）を
/// 一緒に消すことで、マイグレーション 2（`level` 列の追加など）を再度流したときに重複エラーにならないようにする。
pub const CLEANUP_TABLES: &[&str] = &[
    "content_article",
    "content_product",
    "content_page",
    "_aegis_published_product",
    "_aegis_published_meta",
    "_aegis_content_versions",
    "_aegis_content_publications",
    "_aegis_content_tags",
    "_aegis_tags",
    "_aegis_comments",
    "_aegis_forum_reports",
    "_aegis_forum_posts",
    "_aegis_forum_threads",
    "_aegis_forum_categories",
    "_aegis_newsletter_issues",
    "_aegis_newsletter_subscribers",
    "_aegis_contact_messages",
    "_aegis_reviews",
    "_aegis_product_variants",
    "_aegis_orders",
    "_aegis_members",
    "_aegis_user_identities",
    "_aegis_users",
    "_aegis_settings",
    "_aegis_analytics",
    "_aegis_audit_logs",
    "_aegis_notification_queue",
    // 互換用の古い別名
    "_aegis_audit",
    "_aegis_user_links",
    // 適用済みの手順の記録（消したテーブルを init が作り直せるようにする）
    "_sqlx_migrations",
];

pub const POSTGRES_CLEANUP_SQL: &str = "\
DROP TABLE IF EXISTS \
content_article, content_product, content_page, \
_aegis_published_product, _aegis_published_meta, \
_aegis_content_versions, _aegis_content_publications, \
_aegis_content_tags, _aegis_tags, _aegis_comments, \
_aegis_forum_reports, _aegis_forum_posts, _aegis_forum_threads, _aegis_forum_categories, \
_aegis_newsletter_issues, _aegis_newsletter_subscribers, \
_aegis_contact_messages, _aegis_reviews, _aegis_product_variants, \
_aegis_orders, _aegis_members, _aegis_user_identities, \
_aegis_users, _aegis_settings, _aegis_analytics, _aegis_audit_logs, \
_aegis_notification_queue, _aegis_audit, _aegis_user_links, \
_sqlx_migrations CASCADE;";

pub const MYSQL_CLEANUP_SQL: &str = "\
DROP TABLE IF EXISTS \
content_article, content_product, content_page, \
_aegis_published_product, _aegis_published_meta, \
_aegis_content_versions, _aegis_content_publications, \
_aegis_content_tags, _aegis_tags, _aegis_comments, \
_aegis_forum_reports, _aegis_forum_posts, _aegis_forum_threads, _aegis_forum_categories, \
_aegis_newsletter_issues, _aegis_newsletter_subscribers, \
_aegis_contact_messages, _aegis_reviews, _aegis_product_variants, \
_aegis_orders, _aegis_members, _aegis_user_identities, \
_aegis_users, _aegis_settings, _aegis_analytics, _aegis_audit_logs, \
_aegis_notification_queue, _aegis_audit, _aegis_user_links, \
_sqlx_migrations;";

/// 実機の PostgreSQL DB 内の全システムテーブルおよびマイグレーション記録を消去する
pub async fn cleanup_postgres(adapter: &crate::PostgresAdapter) -> Result<()> {
    sqlx::query(POSTGRES_CLEANUP_SQL)
        .execute(adapter.pool())
        .await
        .map_err(|e| aegis_core::AegisError::Database(e.to_string()))?;
    Ok(())
}

/// 実機の MySQL DB 内の全システムテーブルおよびマイグレーション記録を消去する
pub async fn cleanup_mysql(adapter: &crate::MySqlAdapter) -> Result<()> {
    sqlx::query(MYSQL_CLEANUP_SQL)
        .execute(adapter.pool())
        .await
        .map_err(|e| aegis_core::AegisError::Database(e.to_string()))?;
    Ok(())
}
