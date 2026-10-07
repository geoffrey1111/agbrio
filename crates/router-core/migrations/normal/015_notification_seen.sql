-- Viewing a card changes unread state, not list deletion or subscription state.
CREATE TABLE watch_seen_events (
 sequence INTEGER PRIMARY KEY REFERENCES codex_watch_events(sequence) ON DELETE CASCADE,
 seen_at INTEGER NOT NULL
);
