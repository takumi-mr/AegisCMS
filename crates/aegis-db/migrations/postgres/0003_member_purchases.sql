-- 記事・ページの単独購入の記録（docs/members.md 16）。`member_id` は退会で NULL にして、会計の記録として残す
CREATE TABLE IF NOT EXISTS _aegis_purchases (id VARCHAR(255) PRIMARY KEY, member_id VARCHAR(255), content_type VARCHAR(255) NOT NULL, content_id VARCHAR(255) NOT NULL, amount BIGINT NOT NULL, currency VARCHAR(255) NOT NULL, status VARCHAR(255) NOT NULL, provider_ref VARCHAR(255) NOT NULL UNIQUE, transaction_id VARCHAR(255), created_at VARCHAR(255) NOT NULL, paid_at VARCHAR(255), updated_at VARCHAR(255) NOT NULL);

CREATE INDEX IF NOT EXISTS idx_aegis_purchases_owner ON _aegis_purchases (member_id, content_type, content_id, status);

CREATE INDEX IF NOT EXISTS idx_aegis_purchases_transaction ON _aegis_purchases (transaction_id);
