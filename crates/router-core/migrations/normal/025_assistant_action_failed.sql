CREATE TABLE assistant_actions_next (
 id TEXT PRIMARY KEY,grant_id TEXT NOT NULL REFERENCES assistant_grants(id),
 request_id TEXT NOT NULL,payload_hash TEXT NOT NULL,operation TEXT NOT NULL,input_json TEXT NOT NULL,
 status TEXT NOT NULL CHECK(status IN ('READY','EXECUTING','APPLIED','UNKNOWN','FAILED')),
 question TEXT,answer TEXT,answer_reference TEXT,
 basis TEXT,assessment TEXT,result_json TEXT,error_code TEXT,created_at INTEGER NOT NULL,updated_at INTEGER NOT NULL,
 UNIQUE(grant_id,request_id)
);
INSERT INTO assistant_actions_next SELECT * FROM assistant_actions;
DROP TABLE assistant_actions;
ALTER TABLE assistant_actions_next RENAME TO assistant_actions;
-- This exact rejection occurs before a new reply row or native writer claim.
-- Preserve the request identity and never turn it back into a retryable action.
UPDATE assistant_actions SET status='FAILED',
 result_json='{"delivery":"NOT_SENT","reason":"PREVIOUS_REPLY_PENDING","automaticRetry":false}'
 WHERE operation='SEND_CHAT' AND status='UNKNOWN'
 AND error_code='REPLY_PREVIOUS_PENDING_CHECK_FIRST'
 AND NOT EXISTS(SELECT 1 FROM watch_replies WHERE id=assistant_actions.id);
