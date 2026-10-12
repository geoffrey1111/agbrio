CREATE TABLE claude_control_leases (
 session_id TEXT PRIMARY KEY REFERENCES claude_sessions(session_id),
 instance_id TEXT NOT NULL,
 checked_at INTEGER NOT NULL
);
CREATE TABLE claude_manual_commands (
 request_id TEXT PRIMARY KEY REFERENCES watch_replies(id),
 session_id TEXT NOT NULL REFERENCES claude_sessions(session_id),
 instance_id TEXT NOT NULL,
 cwd TEXT NOT NULL,
 offered_at INTEGER,
 message_id TEXT
);
