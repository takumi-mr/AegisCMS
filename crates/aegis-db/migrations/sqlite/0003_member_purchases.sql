-- 記事・ページの単独購入の記録（docs/members.md 16）。`member_id` は退会で NULL にして、会計の記録として残す
CREATE TABLE IF NOT EXISTS _aegis_purchases (id TEXT PRIMARY KEY, member_id TEXT, content_type TEXT NOT NULL, content_id TEXT NOT NULL, amount INTEGER NOT NULL, currency TEXT NOT NULL, status TEXT NOT NULL, provider_ref TEXT NOT NULL UNIQUE, transaction_id TEXT, created_at TEXT NOT NULL, paid_at TEXT, updated_at TEXT NOT NULL);

CREATE INDEX IF NOT EXISTS idx_aegis_purchases_owner ON _aegis_purchases (member_id, content_type, content_id, status);

CREATE INDEX IF NOT EXISTS idx_aegis_purchases_transaction ON _aegis_purchases (transaction_id);
