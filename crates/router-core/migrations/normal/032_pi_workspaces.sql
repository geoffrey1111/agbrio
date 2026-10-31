CREATE TABLE IF NOT EXISTS pi_scope_policies (
 scope_id TEXT PRIMARY KEY NOT NULL REFERENCES pi_directory_scopes(id),
 policy_json TEXT NOT NULL, updated_at INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS pi_task_access (
 task_id TEXT PRIMARY KEY NOT NULL REFERENCES pi_tasks(id),
 scope_generation INTEGER NOT NULL, access_json TEXT NOT NULL, audit_json TEXT
);
