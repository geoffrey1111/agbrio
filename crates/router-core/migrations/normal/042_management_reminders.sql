CREATE TABLE management_reminder_evidence (
    notice_id TEXT PRIMARY KEY REFERENCES claude_codex_notices(id),
    source_at INTEGER NOT NULL,
    verified_at INTEGER NOT NULL
);
