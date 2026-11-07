-- Baseline of the system tables (`_aegis_*`) as of v0.4.
--
-- Every statement is `IF NOT EXISTS`, so a database that v0.4 already created takes this
-- migration as applied without any change. New changes go into new numbered files; never edit
-- a file that has been released (sqlx records a checksum of each applied file and refuses to
-- start when it differs). See docs/versioning.md "6. テーブル定義の更新方針".

CREATE TABLE IF NOT EXISTS _aegis_content_versions (content_type TEXT NOT NULL, content_id TEXT NOT NULL, revision INTEGER NOT NULL, snapshot_json TEXT NOT NULL, created_at TEXT NOT NULL, created_by TEXT, change_note TEXT, PRIMARY KEY(content_type, content_id, revision));

CREATE TABLE IF NOT EXISTS _aegis_content_publications (content_type TEXT NOT NULL, content_id TEXT NOT NULL, revision INTEGER NOT NULL, slug TEXT NOT NULL, title TEXT NOT NULL, status TEXT NOT NULL, published_at TEXT, unpublish_at TEXT, created_at TEXT NOT NULL, updated_at TEXT NOT NULL, category_id TEXT, sort_at TEXT, PRIMARY KEY(content_type, content_id), UNIQUE(content_type, slug));

CREATE INDEX IF NOT EXISTS idx_content_publications_list ON _aegis_content_publications (content_type, status, created_at, content_id);

CREATE INDEX IF NOT EXISTS idx_content_publications_category ON _aegis_content_publications (content_type, status, category_id, created_at, content_id);

CREATE INDEX IF NOT EXISTS idx_content_publications_sort ON _aegis_content_publications (content_type, status, sort_at, content_id);

CREATE INDEX IF NOT EXISTS idx_content_publications_category_sort ON _aegis_content_publications (content_type, status, category_id, sort_at, content_id);

CREATE TABLE IF NOT EXISTS _aegis_audit_logs (
                id TEXT PRIMARY KEY,
                occurred_at TEXT NOT NULL,
                action TEXT NOT NULL,
                outcome TEXT NOT NULL,
                actor_id TEXT,
                actor_role TEXT,
                resource_type TEXT,
                resource_id TEXT,
                ip TEXT,
                user_agent TEXT,
                details_json TEXT NOT NULL DEFAULT '{}'
            );

CREATE INDEX IF NOT EXISTS idx_aegis_audit_logs_occurred_at ON _aegis_audit_logs (occurred_at);

CREATE INDEX IF NOT EXISTS idx_aegis_audit_logs_action ON _aegis_audit_logs (action, occurred_at);

CREATE INDEX IF NOT EXISTS idx_aegis_audit_logs_actor ON _aegis_audit_logs (actor_id, occurred_at);

CREATE INDEX IF NOT EXISTS idx_aegis_audit_logs_resource ON _aegis_audit_logs (resource_type, resource_id);

CREATE TABLE IF NOT EXISTS _aegis_users (
                id TEXT PRIMARY KEY,
                username TEXT NOT NULL UNIQUE,
                email TEXT,
                display_name TEXT,
                role TEXT NOT NULL,
                password_hash TEXT NOT NULL,
                disabled INTEGER NOT NULL DEFAULT 0,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL,
                avatar_key TEXT,
                pending_approval INTEGER NOT NULL DEFAULT 0,
                totp_secret TEXT,
                totp_enabled INTEGER NOT NULL DEFAULT 0,
                backup_codes TEXT
            );

CREATE TABLE IF NOT EXISTS _aegis_user_identities (provider TEXT NOT NULL, subject TEXT NOT NULL, username TEXT NOT NULL, created_at TEXT NOT NULL, PRIMARY KEY (provider, subject));

CREATE TABLE IF NOT EXISTS _aegis_settings (name TEXT PRIMARY KEY, value TEXT NOT NULL, updated_at TEXT NOT NULL);

CREATE TABLE IF NOT EXISTS _aegis_orders (id TEXT PRIMARY KEY, number TEXT NOT NULL UNIQUE, status TEXT NOT NULL, currency TEXT NOT NULL, subtotal INTEGER NOT NULL, total INTEGER NOT NULL, lines_json TEXT NOT NULL, adjustments_json TEXT NOT NULL, customer_email TEXT NOT NULL, customer_name TEXT NOT NULL, shipping_json TEXT, coupon_code TEXT, metadata_json TEXT NOT NULL, payment_provider TEXT NOT NULL, payment_ref TEXT, access_token_hash TEXT NOT NULL, expires_at TEXT, paid_at TEXT, created_at TEXT NOT NULL, updated_at TEXT NOT NULL, note TEXT, transaction_id TEXT, refunded_amount INTEGER NOT NULL DEFAULT 0, refunds_json TEXT NOT NULL, member_id TEXT);

CREATE INDEX IF NOT EXISTS idx_aegis_orders_status ON _aegis_orders (status, created_at);

CREATE INDEX IF NOT EXISTS idx_aegis_orders_created ON _aegis_orders (created_at);

CREATE INDEX IF NOT EXISTS idx_aegis_orders_payment_ref ON _aegis_orders (payment_ref);

CREATE INDEX IF NOT EXISTS idx_aegis_orders_expires ON _aegis_orders (status, expires_at);

CREATE INDEX IF NOT EXISTS idx_aegis_orders_transaction ON _aegis_orders (transaction_id);

CREATE INDEX IF NOT EXISTS idx_aegis_orders_member ON _aegis_orders (member_id);

CREATE TABLE IF NOT EXISTS _aegis_product_variants (
    id TEXT PRIMARY KEY,
    product_id TEXT NOT NULL,
    sku TEXT,
    title TEXT NOT NULL,
    price_override INTEGER,
    stock INTEGER,
    attributes_json TEXT NOT NULL,
    position INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_product_variants_product_id ON _aegis_product_variants (product_id);

CREATE INDEX IF NOT EXISTS idx_product_variants_sku ON _aegis_product_variants (sku);

CREATE TABLE IF NOT EXISTS _aegis_members (id TEXT PRIMARY KEY, email TEXT NOT NULL UNIQUE, password_hash TEXT NOT NULL, name TEXT NOT NULL, nickname TEXT, avatar_key TEXT, phone TEXT, status TEXT NOT NULL, shipping_address_json TEXT, created_at TEXT NOT NULL, updated_at TEXT NOT NULL, last_login_at TEXT);

CREATE INDEX IF NOT EXISTS idx_aegis_members_email ON _aegis_members (email);

CREATE INDEX IF NOT EXISTS idx_aegis_members_created ON _aegis_members (created_at);

CREATE TABLE IF NOT EXISTS _aegis_reviews (id TEXT PRIMARY KEY, product_id TEXT NOT NULL, order_id TEXT NOT NULL, member_id TEXT, author_name TEXT NOT NULL, author_email TEXT NOT NULL, rating INTEGER NOT NULL, title TEXT NOT NULL, body TEXT NOT NULL, status TEXT NOT NULL, verified_purchase INTEGER NOT NULL DEFAULT 1, reply TEXT, replied_at TEXT, rejection_reason TEXT, ip TEXT, user_agent TEXT, created_at TEXT NOT NULL, updated_at TEXT NOT NULL);

CREATE INDEX IF NOT EXISTS idx_aegis_reviews_product ON _aegis_reviews (product_id, status, created_at);

CREATE INDEX IF NOT EXISTS idx_aegis_reviews_order_product ON _aegis_reviews (order_id, product_id);

CREATE INDEX IF NOT EXISTS idx_aegis_reviews_status ON _aegis_reviews (status, created_at);

CREATE INDEX IF NOT EXISTS idx_aegis_reviews_member ON _aegis_reviews (member_id);

CREATE TABLE IF NOT EXISTS _aegis_analytics (day VARCHAR(10) NOT NULL, dimension VARCHAR(16) NOT NULL, value TEXT NOT NULL, pageviews BIGINT NOT NULL DEFAULT 0, visitors BIGINT NOT NULL DEFAULT 0, PRIMARY KEY (day, dimension, value));

CREATE INDEX IF NOT EXISTS idx_aegis_analytics_dimension ON _aegis_analytics (dimension, day);

CREATE TABLE IF NOT EXISTS _aegis_contact_messages (id TEXT PRIMARY KEY, form TEXT NOT NULL, name TEXT NOT NULL, email TEXT NOT NULL, subject TEXT NOT NULL, fields_json TEXT NOT NULL, status TEXT NOT NULL, ip TEXT, user_agent TEXT, created_at TEXT NOT NULL, updated_at TEXT NOT NULL);

CREATE INDEX IF NOT EXISTS idx_aegis_contact_status ON _aegis_contact_messages (status, created_at);

CREATE INDEX IF NOT EXISTS idx_aegis_contact_created ON _aegis_contact_messages (created_at);

CREATE TABLE IF NOT EXISTS _aegis_newsletter_subscribers (id TEXT PRIMARY KEY, email TEXT NOT NULL UNIQUE, status TEXT NOT NULL, token TEXT NOT NULL UNIQUE, ip TEXT, created_at TEXT NOT NULL, confirmed_at TEXT, unsubscribed_at TEXT, updated_at TEXT NOT NULL);

CREATE TABLE IF NOT EXISTS _aegis_newsletter_issues (id TEXT PRIMARY KEY, content_type TEXT NOT NULL, content_id TEXT NOT NULL, title TEXT NOT NULL, url TEXT NOT NULL, recipients INTEGER NOT NULL, created_at TEXT NOT NULL, created_by TEXT);

CREATE INDEX IF NOT EXISTS idx_aegis_newsletter_status ON _aegis_newsletter_subscribers (status, created_at);

CREATE INDEX IF NOT EXISTS idx_aegis_newsletter_issue_content ON _aegis_newsletter_issues (content_type, content_id);

CREATE TABLE IF NOT EXISTS _aegis_comments (id TEXT PRIMARY KEY, content_type TEXT NOT NULL, content_id TEXT NOT NULL, parent_id TEXT, author_name TEXT NOT NULL, author_email TEXT NOT NULL, author_user_id TEXT, body TEXT NOT NULL, status TEXT NOT NULL, ip TEXT, user_agent TEXT, notify_replies INTEGER NOT NULL DEFAULT 0, notify_token TEXT, created_at TEXT NOT NULL, updated_at TEXT NOT NULL);

CREATE INDEX IF NOT EXISTS idx_aegis_comments_content ON _aegis_comments (content_id, status, created_at);

CREATE INDEX IF NOT EXISTS idx_aegis_comments_status ON _aegis_comments (status, created_at);

CREATE INDEX IF NOT EXISTS idx_aegis_comments_author ON _aegis_comments (author_email, status);

CREATE INDEX IF NOT EXISTS idx_aegis_comments_parent ON _aegis_comments (parent_id);

CREATE TABLE IF NOT EXISTS _aegis_tags (id TEXT PRIMARY KEY, slug TEXT NOT NULL UNIQUE, name TEXT NOT NULL, description TEXT, created_at TEXT NOT NULL, updated_at TEXT NOT NULL);

CREATE TABLE IF NOT EXISTS _aegis_content_tags (content_type TEXT NOT NULL, content_id TEXT NOT NULL, tag_id TEXT NOT NULL, created_at TEXT NOT NULL, PRIMARY KEY (content_type, content_id, tag_id));

CREATE INDEX IF NOT EXISTS idx_aegis_content_tags_tag ON _aegis_content_tags (tag_id, content_type);

CREATE TABLE IF NOT EXISTS _aegis_notification_queue (id TEXT PRIMARY KEY, kind TEXT NOT NULL, channel TEXT NOT NULL, message TEXT NOT NULL, status TEXT NOT NULL, attempts INTEGER NOT NULL DEFAULT 0, next_attempt_at TEXT NOT NULL, locked_until TEXT, last_error TEXT, created_at TEXT NOT NULL, updated_at TEXT NOT NULL);

CREATE INDEX IF NOT EXISTS idx_aegis_notification_queue_due ON _aegis_notification_queue (status, next_attempt_at);

CREATE INDEX IF NOT EXISTS idx_aegis_notification_queue_created ON _aegis_notification_queue (created_at);

CREATE TABLE IF NOT EXISTS _aegis_forum_categories (id TEXT PRIMARY KEY, slug TEXT NOT NULL UNIQUE, name TEXT NOT NULL, description TEXT NOT NULL, position INTEGER NOT NULL DEFAULT 0, visibility TEXT NOT NULL DEFAULT 'public', created_at TEXT NOT NULL, updated_at TEXT NOT NULL);

CREATE TABLE IF NOT EXISTS _aegis_forum_threads (id TEXT PRIMARY KEY, category_id TEXT NOT NULL, title TEXT NOT NULL, author_id TEXT, author_kind TEXT, author_name TEXT NOT NULL, status TEXT NOT NULL DEFAULT 'approved', is_pinned INTEGER NOT NULL DEFAULT 0, is_locked INTEGER NOT NULL DEFAULT 0, post_count INTEGER NOT NULL DEFAULT 0, last_post_at TEXT NOT NULL, created_at TEXT NOT NULL, updated_at TEXT NOT NULL);

CREATE TABLE IF NOT EXISTS _aegis_forum_posts (id TEXT PRIMARY KEY, thread_id TEXT NOT NULL, post_number INTEGER NOT NULL, author_id TEXT, author_kind TEXT, author_name TEXT NOT NULL, body TEXT NOT NULL, status TEXT NOT NULL DEFAULT 'approved', reply_to_post_id TEXT, reply_to_number INTEGER, is_deleted INTEGER NOT NULL DEFAULT 0, ip TEXT, user_agent TEXT, created_at TEXT NOT NULL, updated_at TEXT NOT NULL);

CREATE TABLE IF NOT EXISTS _aegis_forum_reports (id TEXT PRIMARY KEY, target_type TEXT NOT NULL, target_id TEXT NOT NULL, thread_id TEXT NOT NULL, reason TEXT NOT NULL, reporter_id TEXT, reporter_ip TEXT, status TEXT NOT NULL DEFAULT 'open', resolved_by TEXT, created_at TEXT NOT NULL, updated_at TEXT NOT NULL);

CREATE INDEX IF NOT EXISTS idx_aegis_forum_cat_pos ON _aegis_forum_categories (position, created_at);

CREATE INDEX IF NOT EXISTS idx_aegis_forum_th_cat ON _aegis_forum_threads (category_id, is_pinned, last_post_at);

CREATE INDEX IF NOT EXISTS idx_aegis_forum_th_pinned ON _aegis_forum_threads (is_pinned, last_post_at);

CREATE INDEX IF NOT EXISTS idx_aegis_forum_post_th_num ON _aegis_forum_posts (thread_id, post_number);

CREATE INDEX IF NOT EXISTS idx_aegis_forum_post_th_created ON _aegis_forum_posts (thread_id, created_at);

CREATE INDEX IF NOT EXISTS idx_aegis_forum_rep_status ON _aegis_forum_reports (status, created_at);

CREATE INDEX IF NOT EXISTS idx_aegis_forum_rep_target ON _aegis_forum_reports (target_type, target_id);
