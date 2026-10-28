-- Explicit list presentation only: never a message, read cursor or generation.
CREATE TABLE watch_visibility (
 thread_id TEXT PRIMARY KEY NOT NULL REFERENCES codex_watches(thread_id) ON DELETE CASCADE,
 shown_at INTEGER NOT NULL DEFAULT 0 CHECK(shown_at >= 0),
 changed_at INTEGER NOT NULL DEFAULT 0 CHECK(changed_at >= 0)
);
