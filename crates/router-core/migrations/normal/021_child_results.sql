-- Bridge-derived notifications have a parent context without an independent watch.
-- Preserve exact originals, sequence monotonicity and seen markers while removing
-- only the obsolete watch-row FK; context eligibility is checked at delivery.
CREATE TABLE child_migration_seen AS SELECT sequence,seen_at FROM watch_seen_events;
CREATE TABLE child_migration_sequence AS SELECT seq FROM sqlite_sequence WHERE name='codex_watch_events';
DROP TABLE watch_seen_events;
CREATE TABLE codex_watch_events_scoped (
 sequence INTEGER PRIMARY KEY AUTOINCREMENT,
 thread_id TEXT NOT NULL,
 label TEXT NOT NULL,
 cwd TEXT NOT NULL,
 snapshot_json TEXT NOT NULL,
 observed_at INTEGER NOT NULL
);
INSERT INTO codex_watch_events_scoped SELECT * FROM codex_watch_events;
DROP TABLE codex_watch_events;
ALTER TABLE codex_watch_events_scoped RENAME TO codex_watch_events;
UPDATE sqlite_sequence SET seq=MAX(seq,COALESCE((SELECT seq FROM child_migration_sequence),0)) WHERE name='codex_watch_events';
CREATE TABLE watch_seen_events(sequence INTEGER PRIMARY KEY REFERENCES codex_watch_events(sequence) ON DELETE CASCADE,seen_at INTEGER NOT NULL);
INSERT INTO watch_seen_events SELECT * FROM child_migration_seen;
DROP TABLE child_migration_seen;
DROP TABLE child_migration_sequence;
CREATE TABLE child_result_preferences(root TEXT PRIMARY KEY,cwd TEXT NOT NULL,generation INTEGER NOT NULL,enabled INTEGER NOT NULL,enabled_at INTEGER NOT NULL);
CREATE TABLE child_result_baselines(root TEXT NOT NULL,child TEXT NOT NULL,identity TEXT NOT NULL,PRIMARY KEY(root,child));
-- Retain origin fences with delivery audit even after notification-original pruning.
CREATE TABLE child_result_events(sequence INTEGER PRIMARY KEY,root TEXT NOT NULL,child TEXT NOT NULL,cwd TEXT NOT NULL,generation INTEGER NOT NULL);
