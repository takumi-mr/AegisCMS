-- Baseline of the system tables (`_aegis_*`) as of v0.4.
--
-- Every statement is `IF NOT EXISTS`, so a database that v0.4 already created takes this
-- migration as applied without any change. New changes go into new numbered files; never edit
-- a file that has been released (sqlx records a checksum of each applied file and refuses to
-- start when it differs). See docs/versioning.md "6. テーブル定義の更新方針".

CREATE TABLE IF NOT EXISTS _aegis_content_versions (content_type TEXT NOT NULL, content_id TEXT NOT NULL, revision BIGINT NOT NULL, snapshot_json TEXT NOT NULL, created_at TEXT NOT NULL, created_by TEXT, change_note TEXT, PRIMARY KEY(content_type, content_id, revision));

CREATE TABLE IF NOT EXISTS _aegis_content_publications (content_type TEXT NOT NULL, content_id TEXT NOT NULL, revision BIGINT NOT NULL, slug TEXT NOT NULL, title TEXT NOT NULL, status TEXT NOT NULL, published_at TEXT, unpublish_at TEXT, created_at TEXT NOT NULL, updated_at TEXT NOT NULL, category_id TEXT, sort_at TEXT, PRIMARY KEY(content_type, content_id), UNIQUE(content_type, slug));

CREATE INDEX IF NOT EXISTS idx_content_publications_list ON _aegis_content_publications (content_type, status, created_at, content_id);

CREATE INDEX IF NOT EXISTS idx_content_publications_category ON _aegis_content_publications (content_type, status, category_id, created_at, content_id);

CREATE INDEX IF NOT EXISTS idx_content_publications_sort ON _aegis_content_publications (content_type, status, sort_at, content_id);

CREATE INDEX IF NOT EXISTS idx_content_publications_category_sort ON _aegis_content_publications (content_type, status, category_id, sort_at, content_id);

CREATE TABLE IF NOT EXISTS _aegis_audit_logs (
                id VARCHAR(64) PRIMARY KEY,
                occurred_at VARCHAR(32) NOT NULL,
                action VARCHAR(64) NOT NULL,
                outcome VARCHAR(16) NOT NULL,
                actor_id VARCHAR(255),
                actor_role VARCHAR(64),
                resource_type VARCHAR(64),
                resource_id VARCHAR(255),
                ip VARCHAR(64),
                user_agent TEXT,
                details_json TEXT NOT NULL DEFAULT '{}'
            );

CREATE INDEX IF NOT EXISTS idx_aegis_audit_logs_occurred_at ON _aegis_audit_logs (occurred_at);

CREATE INDEX IF NOT EXISTS idx_aegis_audit_logs_action ON _aegis_audit_logs (action, occurred_at);

CREATE INDEX IF NOT EXISTS idx_aegis_audit_logs_actor ON _aegis_audit_logs (actor_id, occurred_at);

CREATE INDEX IF NOT EXISTS idx_aegis_audit_logs_resource ON _aegis_audit_logs (resource_type, resource_id);

CREATE TABLE IF NOT EXISTS _aegis_users (
                id VARCHAR(64) PRIMARY KEY,
                username VARCHAR(255) NOT NULL UNIQUE,
                email VARCHAR(255),
                display_name VARCHAR(255),
                role VARCHAR(64) NOT NULL,
                password_hash TEXT NOT NULL,
                disabled BIGINT NOT NULL DEFAULT 0,
                created_at VARCHAR(64) NOT NULL,
                updated_at VARCHAR(64) NOT NULL,
                avatar_key TEXT,
                pending_approval BIGINT NOT NULL DEFAULT 0,
                totp_secret TEXT,
                totp_enabled BIGINT NOT NULL DEFAULT 0,
                backup_codes TEXT
            );

CREATE TABLE IF NOT EXISTS _aegis_user_identities (provider VARCHAR(255) NOT NULL, subject VARCHAR(255) NOT NULL, username VARCHAR(255) NOT NULL, created_at VARCHAR(255) NOT NULL, PRIMARY KEY (provider, subject));

CREATE TABLE IF NOT EXISTS _aegis_settings (name VARCHAR(255) PRIMARY KEY, value TEXT NOT NULL, updated_at VARCHAR(255) NOT NULL);

CREATE TABLE IF NOT EXISTS _aegis_orders (id VARCHAR(255) PRIMARY KEY, number VARCHAR(255) NOT NULL UNIQUE, status VARCHAR(255) NOT NULL, currency VARCHAR(255) NOT NULL, subtotal BIGINT NOT NULL, total BIGINT NOT NULL, lines_json TEXT NOT NULL, adjustments_json TEXT NOT NULL, customer_email VARCHAR(255) NOT NULL, customer_name TEXT NOT NULL, shipping_json TEXT, coupon_code VARCHAR(255), metadata_json TEXT NOT NULL, payment_provider VARCHAR(255) NOT NULL, payment_ref VARCHAR(255), access_token_hash VARCHAR(255) NOT NULL, expires_at VARCHAR(255), paid_at VARCHAR(255), created_at VARCHAR(255) NOT NULL, updated_at VARCHAR(255) NOT NULL, note TEXT, transaction_id VARCHAR(255), refunded_amount BIGINT NOT NULL DEFAULT 0, refunds_json TEXT NOT NULL, member_id VARCHAR(255));

CREATE INDEX IF NOT EXISTS idx_aegis_orders_status ON _aegis_orders (status, created_at);

CREATE INDEX IF NOT EXISTS idx_aegis_orders_created ON _aegis_orders (created_at);

CREATE INDEX IF NOT EXISTS idx_aegis_orders_payment_ref ON _aegis_orders (payment_ref);

CREATE INDEX IF NOT EXISTS idx_aegis_orders_expires ON _aegis_orders (status, expires_at);

CREATE INDEX IF NOT EXISTS idx_aegis_orders_transaction ON _aegis_orders (transaction_id);

CREATE INDEX IF NOT EXISTS idx_aegis_orders_member ON _aegis_orders (member_id);

CREATE TABLE IF NOT EXISTS _aegis_product_variants (
    id VARCHAR(255) PRIMARY KEY,
    product_id VARCHAR(255) NOT NULL,
    sku VARCHAR(255),
    title VARCHAR(255) NOT NULL,
    price_override BIGINT,
    stock BIGINT,
    attributes_json TEXT NOT NULL,
    position INTEGER NOT NULL DEFAULT 0,
    created_at VARCHAR(255) NOT NULL,
    updated_at VARCHAR(255) NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_product_variants_product_id ON _aegis_product_variants (product_id);

CREATE INDEX IF NOT EXISTS idx_product_variants_sku ON _aegis_product_variants (sku);

CREATE TABLE IF NOT EXISTS _aegis_members (id VARCHAR(255) PRIMARY KEY, email VARCHAR(255) NOT NULL UNIQUE, password_hash TEXT NOT NULL, name TEXT NOT NULL, nickname VARCHAR(255), avatar_key VARCHAR(255), phone VARCHAR(255), status VARCHAR(255) NOT NULL, shipping_address_json TEXT, created_at VARCHAR(255) NOT NULL, updated_at VARCHAR(255) NOT NULL, last_login_at VARCHAR(255));

CREATE INDEX IF NOT EXISTS idx_aegis_members_email ON _aegis_members (email);

CREATE INDEX IF NOT EXISTS idx_aegis_members_created ON _aegis_members (created_at);

CREATE TABLE IF NOT EXISTS _aegis_reviews (id VARCHAR(255) PRIMARY KEY, product_id VARCHAR(255) NOT NULL, order_id VARCHAR(255) NOT NULL, member_id VARCHAR(255), author_name TEXT NOT NULL, author_email VARCHAR(255) NOT NULL, rating BIGINT NOT NULL, title TEXT NOT NULL, body TEXT NOT NULL, status VARCHAR(255) NOT NULL, verified_purchase BIGINT NOT NULL DEFAULT 1, reply TEXT, replied_at VARCHAR(255), rejection_reason TEXT, ip VARCHAR(255), user_agent TEXT, created_at VARCHAR(255) NOT NULL, updated_at VARCHAR(255) NOT NULL);

CREATE INDEX IF NOT EXISTS idx_aegis_reviews_product ON _aegis_reviews (product_id, status, created_at);

CREATE INDEX IF NOT EXISTS idx_aegis_reviews_order_product ON _aegis_reviews (order_id, product_id);

CREATE INDEX IF NOT EXISTS idx_aegis_reviews_status ON _aegis_reviews (status, created_at);

CREATE INDEX IF NOT EXISTS idx_aegis_reviews_member ON _aegis_reviews (member_id);

CREATE TABLE IF NOT EXISTS _aegis_analytics (day VARCHAR(10) NOT NULL, dimension VARCHAR(16) NOT NULL, value VARCHAR(255) NOT NULL, pageviews BIGINT NOT NULL DEFAULT 0, visitors BIGINT NOT NULL DEFAULT 0, PRIMARY KEY (day, dimension, value));

CREATE INDEX IF NOT EXISTS idx_aegis_analytics_dimension ON _aegis_analytics (dimension, day);

CREATE TABLE IF NOT EXISTS _aegis_contact_messages (id VARCHAR(255) PRIMARY KEY, form VARCHAR(255) NOT NULL, name TEXT NOT NULL, email VARCHAR(255) NOT NULL, subject TEXT NOT NULL, fields_json TEXT NOT NULL, status VARCHAR(255) NOT NULL, ip VARCHAR(255), user_agent TEXT, created_at VARCHAR(255) NOT NULL, updated_at VARCHAR(255) NOT NULL);

CREATE INDEX IF NOT EXISTS idx_aegis_contact_status ON _aegis_contact_messages (status, created_at);

CREATE INDEX IF NOT EXISTS idx_aegis_contact_created ON _aegis_contact_messages (created_at);

CREATE TABLE IF NOT EXISTS _aegis_newsletter_subscribers (id VARCHAR(255) PRIMARY KEY, email VARCHAR(255) NOT NULL UNIQUE, status VARCHAR(255) NOT NULL, token VARCHAR(255) NOT NULL UNIQUE, ip VARCHAR(255), created_at VARCHAR(255) NOT NULL, confirmed_at VARCHAR(255), unsubscribed_at VARCHAR(255), updated_at VARCHAR(255) NOT NULL);

CREATE TABLE IF NOT EXISTS _aegis_newsletter_issues (id VARCHAR(255) PRIMARY KEY, content_type VARCHAR(255) NOT NULL, content_id VARCHAR(255) NOT NULL, title VARCHAR(255) NOT NULL, url TEXT NOT NULL, recipients BIGINT NOT NULL, created_at VARCHAR(255) NOT NULL, created_by VARCHAR(255));

CREATE INDEX IF NOT EXISTS idx_aegis_newsletter_status ON _aegis_newsletter_subscribers (status, created_at);

CREATE INDEX IF NOT EXISTS idx_aegis_newsletter_issue_content ON _aegis_newsletter_issues (content_type, content_id);

CREATE TABLE IF NOT EXISTS _aegis_comments (id VARCHAR(255) PRIMARY KEY, content_type VARCHAR(255) NOT NULL, content_id VARCHAR(255) NOT NULL, parent_id VARCHAR(255), author_name TEXT NOT NULL, author_email VARCHAR(255) NOT NULL, author_user_id VARCHAR(255), body TEXT NOT NULL, status VARCHAR(255) NOT NULL, ip VARCHAR(255), user_agent TEXT, notify_replies BIGINT NOT NULL DEFAULT 0, notify_token VARCHAR(255), created_at VARCHAR(255) NOT NULL, updated_at VARCHAR(255) NOT NULL);

CREATE INDEX IF NOT EXISTS idx_aegis_comments_content ON _aegis_comments (content_id, status, created_at);

CREATE INDEX IF NOT EXISTS idx_aegis_comments_status ON _aegis_comments (status, created_at);

CREATE INDEX IF NOT EXISTS idx_aegis_comments_author ON _aegis_comments (author_email, status);

CREATE INDEX IF NOT EXISTS idx_aegis_comments_parent ON _aegis_comments (parent_id);

CREATE TABLE IF NOT EXISTS _aegis_tags (id VARCHAR(255) PRIMARY KEY, slug VARCHAR(255) NOT NULL UNIQUE, name VARCHAR(255) NOT NULL, description TEXT, created_at VARCHAR(255) NOT NULL, updated_at VARCHAR(255) NOT NULL);

CREATE TABLE IF NOT EXISTS _aegis_content_tags (content_type VARCHAR(255) NOT NULL, content_id VARCHAR(255) NOT NULL, tag_id VARCHAR(255) NOT NULL, created_at VARCHAR(255) NOT NULL, PRIMARY KEY (content_type, content_id, tag_id));

CREATE INDEX IF NOT EXISTS idx_aegis_content_tags_tag ON _aegis_content_tags (tag_id, content_type);

CREATE TABLE IF NOT EXISTS _aegis_notification_queue (id VARCHAR(255) PRIMARY KEY, kind VARCHAR(255) NOT NULL, channel VARCHAR(255) NOT NULL, message TEXT NOT NULL, status VARCHAR(255) NOT NULL, attempts BIGINT NOT NULL DEFAULT 0, next_attempt_at VARCHAR(255) NOT NULL, locked_until VARCHAR(255), last_error TEXT, created_at VARCHAR(255) NOT NULL, updated_at VARCHAR(255) NOT NULL);

CREATE INDEX IF NOT EXISTS idx_aegis_notification_queue_due ON _aegis_notification_queue (status, next_attempt_at);

CREATE INDEX IF NOT EXISTS idx_aegis_notification_queue_created ON _aegis_notification_queue (created_at);

CREATE TABLE IF NOT EXISTS _aegis_forum_categories (id VARCHAR(255) PRIMARY KEY, slug VARCHAR(255) NOT NULL UNIQUE, name TEXT NOT NULL, description TEXT NOT NULL, position BIGINT NOT NULL DEFAULT 0, visibility VARCHAR(255) NOT NULL DEFAULT 'public', created_at VARCHAR(255) NOT NULL, updated_at VARCHAR(255) NOT NULL);

CREATE TABLE IF NOT EXISTS _aegis_forum_threads (id VARCHAR(255) PRIMARY KEY, category_id VARCHAR(255) NOT NULL, title TEXT NOT NULL, author_id VARCHAR(255), author_kind VARCHAR(255), author_name TEXT NOT NULL, status VARCHAR(255) NOT NULL DEFAULT 'approved', is_pinned BIGINT NOT NULL DEFAULT 0, is_locked BIGINT NOT NULL DEFAULT 0, post_count BIGINT NOT NULL DEFAULT 0, last_post_at VARCHAR(255) NOT NULL, created_at VARCHAR(255) NOT NULL, updated_at VARCHAR(255) NOT NULL);

CREATE TABLE IF NOT EXISTS _aegis_forum_posts (id VARCHAR(255) PRIMARY KEY, thread_id VARCHAR(255) NOT NULL, post_number BIGINT NOT NULL, author_id VARCHAR(255), author_kind VARCHAR(255), author_name TEXT NOT NULL, body TEXT NOT NULL, status VARCHAR(255) NOT NULL DEFAULT 'approved', reply_to_post_id VARCHAR(255), reply_to_number BIGINT, is_deleted BIGINT NOT NULL DEFAULT 0, ip VARCHAR(255), user_agent TEXT, created_at VARCHAR(255) NOT NULL, updated_at VARCHAR(255) NOT NULL);

CREATE TABLE IF NOT EXISTS _aegis_forum_reports (id VARCHAR(255) PRIMARY KEY, target_type VARCHAR(255) NOT NULL, target_id VARCHAR(255) NOT NULL, thread_id VARCHAR(255) NOT NULL, reason TEXT NOT NULL, reporter_id VARCHAR(255), reporter_ip VARCHAR(255), status VARCHAR(255) NOT NULL DEFAULT 'open', resolved_by VARCHAR(255), created_at VARCHAR(255) NOT NULL, updated_at VARCHAR(255) NOT NULL);

CREATE INDEX IF NOT EXISTS idx_aegis_forum_cat_pos ON _aegis_forum_categories (position, created_at);

CREATE INDEX IF NOT EXISTS idx_aegis_forum_th_cat ON _aegis_forum_threads (category_id, is_pinned, last_post_at);

CREATE INDEX IF NOT EXISTS idx_aegis_forum_th_pinned ON _aegis_forum_threads (is_pinned, last_post_at);

CREATE INDEX IF NOT EXISTS idx_aegis_forum_post_th_num ON _aegis_forum_posts (thread_id, post_number);

CREATE INDEX IF NOT EXISTS idx_aegis_forum_post_th_created ON _aegis_forum_posts (thread_id, created_at);

CREATE INDEX IF NOT EXISTS idx_aegis_forum_rep_status ON _aegis_forum_reports (status, created_at);

CREATE INDEX IF NOT EXISTS idx_aegis_forum_rep_target ON _aegis_forum_reports (target_type, target_id);
