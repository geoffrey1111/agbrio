CREATE TABLE IF NOT EXISTS local_safety_resource_events (
 event_id TEXT PRIMARY KEY, payload TEXT NOT NULL, created_at INTEGER NOT NULL,
 status TEXT NOT NULL DEFAULT 'ACTIVE'
);
CREATE TABLE IF NOT EXISTS local_safety_receiver_leases (
 session_id TEXT PRIMARY KEY, instance_id TEXT NOT NULL, cwd TEXT NOT NULL,
 checked_at INTEGER NOT NULL
);
