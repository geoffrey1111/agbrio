-- Owner-selected desktop identity only. This cannot authorize a plugin or a writer.
CREATE TABLE IF NOT EXISTS management_claude_desktop (
    session_id TEXT PRIMARY KEY,
    cwd TEXT NOT NULL,
    checked_at INTEGER NOT NULL
);
