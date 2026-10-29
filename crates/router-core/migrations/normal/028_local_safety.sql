CREATE TABLE IF NOT EXISTS local_safety_settings (
  singleton INTEGER PRIMARY KEY CHECK(singleton=1), json TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS local_safety_attempts (
  id TEXT PRIMARY KEY, kind TEXT NOT NULL, thread_id TEXT NOT NULL,
  generation INTEGER NOT NULL, cwd TEXT NOT NULL, payload TEXT NOT NULL,
  status TEXT NOT NULL, created_at INTEGER NOT NULL, updated_at INTEGER NOT NULL,
  native_message_id TEXT
);
CREATE INDEX IF NOT EXISTS local_safety_recent ON local_safety_attempts(kind,thread_id,created_at);
