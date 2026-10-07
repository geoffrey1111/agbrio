-- Transactional table rebuild: retain every old capability as BRIDGE, never elevate it.
CREATE TEMP TABLE _instance_assistant_grants AS SELECT * FROM assistant_grants;
CREATE TEMP TABLE _instance_assistant_drafts AS SELECT * FROM assistant_drafts;
CREATE TEMP TABLE _instance_assistant_decisions AS SELECT * FROM assistant_decisions;
CREATE TEMP TABLE _instance_assistant_approvals AS SELECT * FROM assistant_approvals;
CREATE TEMP TABLE _instance_assistant_access AS SELECT * FROM assistant_access;
DROP TABLE assistant_access;
DROP TABLE assistant_approvals;
DROP TABLE assistant_decisions;
DROP TABLE assistant_drafts;
DROP TABLE assistant_grants;
-- Additive normal-profile delegation; never rewrites historic approvals.
CREATE TABLE assistant_grants (
 id TEXT PRIMARY KEY, workstream_id TEXT REFERENCES workstreams(id),
 source_role TEXT NOT NULL CHECK(source_role IN ('DECISION','EXECUTION','BOTH')),
 binding_revision INTEGER NOT NULL, label TEXT NOT NULL, brief_json TEXT NOT NULL,
 created_at INTEGER NOT NULL, expires_at INTEGER NOT NULL, revoked_at INTEGER,
 scope TEXT NOT NULL CHECK(scope IN ('BRIDGE','INSTANCE')),
 CHECK((scope='BRIDGE' AND workstream_id IS NOT NULL) OR (scope='INSTANCE' AND workstream_id IS NULL AND source_role='BOTH' AND binding_revision=0))
);
CREATE TABLE assistant_drafts (
 handoff_id TEXT PRIMARY KEY REFERENCES handoffs(id), grant_id TEXT NOT NULL REFERENCES assistant_grants(id),
 observation_id TEXT NOT NULL REFERENCES reply_observations(id),
 request_id TEXT, request_hash TEXT, UNIQUE(grant_id,request_id)
);
CREATE TABLE assistant_decisions (
 id TEXT PRIMARY KEY, handoff_id TEXT NOT NULL REFERENCES assistant_drafts(handoff_id),
 payload_hash TEXT NOT NULL, question TEXT NOT NULL, answer TEXT, answer_reference TEXT,
 created_at INTEGER NOT NULL, answered_at INTEGER
);
CREATE TABLE assistant_approvals (
 handoff_id TEXT PRIMARY KEY REFERENCES assistant_drafts(handoff_id), grant_id TEXT NOT NULL REFERENCES assistant_grants(id),
 payload_hash TEXT NOT NULL, basis TEXT NOT NULL, decision_reference TEXT NOT NULL,
 assessment TEXT NOT NULL, created_at INTEGER NOT NULL
);
CREATE TABLE assistant_access (
 token_hash TEXT PRIMARY KEY, grant_id TEXT NOT NULL REFERENCES assistant_grants(id),
 resource TEXT NOT NULL, expires_at INTEGER NOT NULL
);
INSERT INTO assistant_grants SELECT *, 'BRIDGE' FROM _instance_assistant_grants;
DROP TABLE _instance_assistant_grants;
INSERT INTO assistant_drafts SELECT * FROM _instance_assistant_drafts;
DROP TABLE _instance_assistant_drafts;
INSERT INTO assistant_decisions SELECT * FROM _instance_assistant_decisions;
DROP TABLE _instance_assistant_decisions;
INSERT INTO assistant_approvals SELECT * FROM _instance_assistant_approvals;
DROP TABLE _instance_assistant_approvals;
INSERT INTO assistant_access SELECT * FROM _instance_assistant_access;
DROP TABLE _instance_assistant_access;
CREATE TABLE assistant_actions (
 id TEXT PRIMARY KEY,grant_id TEXT NOT NULL REFERENCES assistant_grants(id),
 request_id TEXT NOT NULL,payload_hash TEXT NOT NULL,operation TEXT NOT NULL,input_json TEXT NOT NULL,
 status TEXT NOT NULL CHECK(status IN ('READY','EXECUTING','APPLIED','UNKNOWN')),
 question TEXT,answer TEXT,answer_reference TEXT,
 basis TEXT,assessment TEXT,result_json TEXT,error_code TEXT,created_at INTEGER NOT NULL,updated_at INTEGER NOT NULL,
 UNIQUE(grant_id,request_id)
);
