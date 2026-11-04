CREATE TABLE IF NOT EXISTS management_zones (
 id TEXT PRIMARY KEY, name TEXT NOT NULL, manager_id TEXT,
 revision INTEGER NOT NULL DEFAULT 1, manager_epoch INTEGER NOT NULL DEFAULT 1,
 created_at INTEGER NOT NULL, updated_at INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS management_members (
 thread_id TEXT PRIMARY KEY, zone_id TEXT NOT NULL REFERENCES management_zones(id),
 generation INTEGER NOT NULL, joined_at INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS management_members_zone ON management_members(zone_id);
CREATE TABLE IF NOT EXISTS management_changes (
 request_id TEXT PRIMARY KEY, payload_hash TEXT NOT NULL, receipt_json TEXT NOT NULL,
 created_at INTEGER NOT NULL
);
ALTER TABLE claude_codex_subscriptions ADD COLUMN management_zone_id TEXT;
ALTER TABLE claude_codex_subscriptions ADD COLUMN management_revision INTEGER;
ALTER TABLE claude_codex_subscriptions ADD COLUMN receiver_provider TEXT NOT NULL DEFAULT 'CLAUDE_CODE';
CREATE TABLE IF NOT EXISTS management_codex_attempts (
 notice_id TEXT PRIMARY KEY REFERENCES claude_codex_notices(id),
 reply_id TEXT NOT NULL UNIQUE, attempt INTEGER NOT NULL DEFAULT 1,
 retry_at INTEGER NOT NULL DEFAULT 0
);
