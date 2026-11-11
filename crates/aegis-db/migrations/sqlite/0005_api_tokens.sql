-- API トークン（docs/authentication.md 13）。平文は保存せず、SHA-256 のハッシュだけを持つ。`role` はトークンに許すロールの上限
CREATE TABLE IF NOT EXISTS _aegis_api_tokens (id TEXT PRIMARY KEY, username TEXT NOT NULL, name TEXT NOT NULL, token_hash TEXT NOT NULL UNIQUE, token_prefix TEXT NOT NULL, role TEXT NOT NULL, created_at TEXT NOT NULL, expires_at TEXT, last_used_at TEXT);

CREATE INDEX IF NOT EXISTS idx_aegis_api_tokens_user ON _aegis_api_tokens (username);
