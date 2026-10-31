CREATE TABLE IF NOT EXISTS pi_directory_scopes (
 id TEXT PRIMARY KEY, root TEXT NOT NULL UNIQUE, writable INTEGER NOT NULL,
 terminal_confirmation INTEGER NOT NULL, enabled INTEGER NOT NULL DEFAULT 1,
 generation INTEGER NOT NULL DEFAULT 1, created_at INTEGER NOT NULL, updated_at INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS pi_tasks (
 id TEXT PRIMARY KEY, create_action_id TEXT NOT NULL UNIQUE, creator_grant_id TEXT NOT NULL,
 scope_id TEXT NOT NULL REFERENCES pi_directory_scopes(id), scope_generation INTEGER NOT NULL,
 directory TEXT NOT NULL, label TEXT NOT NULL,
 parent_provider TEXT NOT NULL, parent_external_id TEXT NOT NULL, parent_cwd TEXT NOT NULL,
 native_session_id TEXT UNIQUE, native_session_file TEXT, file_verified INTEGER NOT NULL DEFAULT 0,
 generation INTEGER NOT NULL DEFAULT 1, state TEXT NOT NULL DEFAULT 'CREATING',
 desired_json TEXT, selected_json TEXT, selection_source TEXT NOT NULL DEFAULT 'PENDING',
 runtime_epoch TEXT, result_json TEXT, error_code TEXT,native_leaf_id TEXT,branch_revision INTEGER NOT NULL DEFAULT 1,
 created_at INTEGER NOT NULL, updated_at INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS pi_tasks_parent ON pi_tasks(parent_provider,parent_external_id,parent_cwd);
CREATE TABLE IF NOT EXISTS pi_commands (
 id TEXT PRIMARY KEY, task_id TEXT NOT NULL REFERENCES pi_tasks(id), request_id TEXT NOT NULL,
 generation INTEGER NOT NULL, operation TEXT NOT NULL, payload_hash TEXT NOT NULL,
 input_json TEXT NOT NULL, status TEXT NOT NULL DEFAULT 'READY',
 native_entry_id TEXT, result_json TEXT, error_code TEXT,
 created_at INTEGER NOT NULL, updated_at INTEGER NOT NULL, UNIQUE(task_id,request_id)
);
CREATE TABLE IF NOT EXISTS pi_public_items (
 task_id TEXT NOT NULL REFERENCES pi_tasks(id), id TEXT NOT NULL, native_parent_id TEXT,
 ordinal INTEGER NOT NULL, role TEXT NOT NULL, kind TEXT NOT NULL,
 content_json TEXT NOT NULL, at INTEGER NOT NULL, active INTEGER NOT NULL DEFAULT 0, PRIMARY KEY(task_id,id)
);
CREATE TABLE IF NOT EXISTS pi_command_tombstones (
 task_id TEXT NOT NULL REFERENCES pi_tasks(id),request_id TEXT NOT NULL,created_at INTEGER NOT NULL,
 PRIMARY KEY(task_id,request_id)
);
CREATE TABLE IF NOT EXISTS pi_tool_decisions (
 id TEXT PRIMARY KEY, task_id TEXT NOT NULL REFERENCES pi_tasks(id), native_request_id TEXT NOT NULL,
 generation INTEGER NOT NULL, payload_hash TEXT NOT NULL, input_json TEXT NOT NULL,
 status TEXT NOT NULL DEFAULT 'PENDING', answer INTEGER, native_receipt_id TEXT,
 created_at INTEGER NOT NULL, updated_at INTEGER NOT NULL, UNIQUE(task_id,native_request_id)
);
