CREATE TABLE IF NOT EXISTS claude_codex_notice_claims (notice_id TEXT PRIMARY KEY REFERENCES claude_codex_notices(id), claim_id TEXT NOT NULL);
