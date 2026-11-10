-- 会員のサブスクの記録（docs/members.md 16）。`member_id` は退会で NULL にして、記録として残す
CREATE TABLE IF NOT EXISTS _aegis_subscriptions (id VARCHAR(255) PRIMARY KEY, member_id VARCHAR(255), plan_id VARCHAR(255) NOT NULL, level VARCHAR(255) NOT NULL, status VARCHAR(255) NOT NULL, stripe_customer_id VARCHAR(255) NOT NULL, stripe_subscription_id VARCHAR(255) NOT NULL UNIQUE, current_period_end VARCHAR(255) NOT NULL, cancel_at_period_end BIGINT NOT NULL DEFAULT 0, created_at VARCHAR(255) NOT NULL, updated_at VARCHAR(255) NOT NULL);

CREATE INDEX IF NOT EXISTS idx_aegis_subscriptions_member ON _aegis_subscriptions (member_id, status);
