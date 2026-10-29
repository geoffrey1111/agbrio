CREATE TABLE IF NOT EXISTS claude_codex_subscriptions (
 id TEXT PRIMARY KEY, receiver_sid TEXT NOT NULL, receiver_cwd TEXT NOT NULL,
 thread_id TEXT NOT NULL, generation INTEGER NOT NULL, source_cwd TEXT NOT NULL,
 anchor_turn TEXT NOT NULL, mode TEXT NOT NULL, goal_key TEXT,
 expires_at INTEGER NOT NULL, status TEXT NOT NULL DEFAULT 'ACTIVE',
 last_state TEXT, created_at INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS codex_subscription_source ON claude_codex_subscriptions(thread_id,status,expires_at);
CREATE TABLE IF NOT EXISTS claude_codex_notices (
 id TEXT PRIMARY KEY, subscription_id TEXT NOT NULL REFERENCES claude_codex_subscriptions(id),
 turn_id TEXT NOT NULL, kind TEXT NOT NULL, source_state TEXT NOT NULL,
 request_id TEXT, text TEXT NOT NULL, status TEXT NOT NULL DEFAULT 'READY',
 created_at INTEGER NOT NULL, native_message_id TEXT
);
CREATE INDEX IF NOT EXISTS codex_notice_pending ON claude_codex_notices(subscription_id,status,created_at);
