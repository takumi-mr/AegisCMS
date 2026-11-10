-- 会員のサブスクの記録（docs/members.md 16）。`member_id` は退会で NULL にして、記録として残す
CREATE TABLE IF NOT EXISTS _aegis_subscriptions (id TEXT PRIMARY KEY, member_id TEXT, plan_id TEXT NOT NULL, level TEXT NOT NULL, status TEXT NOT NULL, stripe_customer_id TEXT NOT NULL, stripe_subscription_id TEXT NOT NULL UNIQUE, current_period_end TEXT NOT NULL, cancel_at_period_end INTEGER NOT NULL DEFAULT 0, created_at TEXT NOT NULL, updated_at TEXT NOT NULL);

CREATE INDEX IF NOT EXISTS idx_aegis_subscriptions_member ON _aegis_subscriptions (member_id, status);
