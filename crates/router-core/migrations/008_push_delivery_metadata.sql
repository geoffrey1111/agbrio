-- Raw PushSubscription capability material stays in local non-Git config.
-- SQLite retains only a one-way local fingerprint and non-secret state.
ALTER TABLE reply_observations ADD COLUMN push_state TEXT NOT NULL DEFAULT 'NOT_ATTEMPTED';
ALTER TABLE reply_observations ADD COLUMN push_attempted_at INTEGER NULL;
CREATE TABLE push_subscriptions (
  fingerprint TEXT PRIMARY KEY NOT NULL,
  status TEXT NOT NULL CHECK(status IN ('ACTIVE','REMOVED')),
  created_at INTEGER NOT NULL,
  updated_at INTEGER NOT NULL,
  last_push_at INTEGER NULL,
  last_push_state TEXT NULL
);
