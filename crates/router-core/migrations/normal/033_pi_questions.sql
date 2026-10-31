CREATE TABLE IF NOT EXISTS pi_owner_questions (
 task_id TEXT NOT NULL REFERENCES pi_tasks(id), generation INTEGER NOT NULL,
 runtime_epoch TEXT NOT NULL, request_id TEXT NOT NULL, method TEXT NOT NULL,
 title TEXT, message TEXT, observed_at INTEGER NOT NULL,
 disposition TEXT NOT NULL DEFAULT 'NATIVE_UI_UNAVAILABLE',
 PRIMARY KEY(task_id,generation,runtime_epoch,request_id)
);
