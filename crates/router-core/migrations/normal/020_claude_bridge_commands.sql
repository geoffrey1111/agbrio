CREATE TABLE claude_bridge_commands (
 handoff_id TEXT PRIMARY KEY REFERENCES handoffs(id),
 session_id TEXT NOT NULL REFERENCES claude_sessions(session_id),
 instance_id TEXT NOT NULL,
 cwd TEXT NOT NULL,
 enqueued_at INTEGER NOT NULL,
 offered_at INTEGER,
 message_id TEXT
);
