CREATE TABLE claude_owner_requests(
 id TEXT PRIMARY KEY,session_id TEXT NOT NULL,instance_id TEXT NOT NULL,cwd TEXT NOT NULL,
 root TEXT NOT NULL,generation INTEGER NOT NULL,tool_use_id TEXT NOT NULL,kind TEXT NOT NULL,
 payload_json TEXT NOT NULL,payload_hash TEXT NOT NULL,status TEXT NOT NULL,
 answer_json TEXT,receipt_message_id TEXT,created_at INTEGER NOT NULL,updated_at INTEGER NOT NULL,
 UNIQUE(session_id,instance_id,tool_use_id,kind)
);
