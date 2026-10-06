CREATE TABLE codex_watches (
 thread_id TEXT PRIMARY KEY NOT NULL,
 label TEXT NOT NULL,
 cwd TEXT NOT NULL,
 enabled INTEGER NOT NULL CHECK(enabled IN (0,1)),
 generation INTEGER NOT NULL DEFAULT 1,
 snapshot_json TEXT NOT NULL,
 checked_at INTEGER NOT NULL,
 error_code TEXT,
 retry_after INTEGER NOT NULL DEFAULT 0
);
CREATE TABLE codex_watch_events (
 sequence INTEGER PRIMARY KEY AUTOINCREMENT,
 thread_id TEXT NOT NULL REFERENCES codex_watches(thread_id),
 label TEXT NOT NULL,
 cwd TEXT NOT NULL,
 snapshot_json TEXT NOT NULL,
 observed_at INTEGER NOT NULL
);
