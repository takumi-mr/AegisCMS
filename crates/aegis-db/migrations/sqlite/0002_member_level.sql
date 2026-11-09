-- 会員の権限レベル（有料メンバー限定の閲覧制御。docs/members.md 15）。既存の会員は free
ALTER TABLE _aegis_members ADD COLUMN level TEXT NOT NULL DEFAULT 'free';
