-- OAuth のリフレッシュトークン（docs/mcp.md）。平文は保存せず、SHA-256 のハッシュだけを持つ。使うたびに作り直す（1 回しか使えない）
CREATE TABLE IF NOT EXISTS _aegis_oauth_refresh_tokens (id TEXT PRIMARY KEY, token_hash TEXT NOT NULL UNIQUE, username TEXT NOT NULL, client_hash TEXT NOT NULL, client_name TEXT NOT NULL, scope TEXT NOT NULL, role TEXT NOT NULL, access_token_id TEXT NOT NULL, created_at TEXT NOT NULL, expires_at TEXT NOT NULL);

CREATE INDEX IF NOT EXISTS idx_aegis_oauth_refresh_user ON _aegis_oauth_refresh_tokens (username);
