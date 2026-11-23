-- Reviewed MCP messages retain their grant/action provenance. They share the
-- native transport and independent receipts, never the owner's authorship.
CREATE TABLE claude_assistant_commands (
 request_id TEXT PRIMARY KEY REFERENCES claude_manual_commands(request_id),
 grant_id TEXT NOT NULL REFERENCES assistant_grants(id),
 action_hash TEXT NOT NULL,
 created_at INTEGER NOT NULL
);
