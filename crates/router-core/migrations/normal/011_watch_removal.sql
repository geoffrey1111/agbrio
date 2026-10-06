CREATE TABLE IF NOT EXISTS watch_removed_items (
    kind TEXT NOT NULL CHECK(kind IN ('WATCH','EVENT')),
    id TEXT NOT NULL,
    was_enabled INTEGER,
    removed_at INTEGER NOT NULL,
    PRIMARY KEY(kind,id)
);
