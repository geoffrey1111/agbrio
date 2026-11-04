ALTER TABLE claude_codex_notice_claims ADD COLUMN delivery_channel TEXT NOT NULL DEFAULT 'PROMPT';
ALTER TABLE claude_codex_notice_claims ADD COLUMN proof_nonce TEXT;
ALTER TABLE claude_codex_notice_claims ADD COLUMN released_at INTEGER;
ALTER TABLE claude_codex_notices ADD COLUMN offered_at INTEGER;
ALTER TABLE claude_codex_notices ADD COLUMN claimed_at INTEGER;
ALTER TABLE claude_codex_notices ADD COLUMN confirmed_at INTEGER;
ALTER TABLE claude_codex_notices ADD COLUMN delivered_via TEXT;
