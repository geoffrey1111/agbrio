-- Additive normal-profile delegation; never rewrites historic approvals.
CREATE TABLE assistant_grants (
 id TEXT PRIMARY KEY, workstream_id TEXT NOT NULL REFERENCES workstreams(id),
 source_role TEXT NOT NULL CHECK(source_role IN ('DECISION','EXECUTION','BOTH')),
 binding_revision INTEGER NOT NULL, label TEXT NOT NULL, brief_json TEXT NOT NULL,
 created_at INTEGER NOT NULL, expires_at INTEGER NOT NULL, revoked_at INTEGER
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
