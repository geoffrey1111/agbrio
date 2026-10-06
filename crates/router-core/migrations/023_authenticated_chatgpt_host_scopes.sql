-- Router management contexts belong to one authenticated ChatGPT host
-- conversation.  The stable local IDs below deliberately retain only
-- deterministic, non-secret fingerprints of the externally asserted subject
-- and session; raw assertions and rotating credential material are never
-- stored here.
CREATE TABLE principals (
 principal_id TEXT PRIMARY KEY NOT NULL,
 provider TEXT NOT NULL,
 subject_fingerprint TEXT NOT NULL,
 created_at INTEGER NOT NULL,
 updated_at INTEGER NOT NULL,
 UNIQUE(provider, subject_fingerprint)
);

CREATE TABLE host_scopes (
 host_scope_id TEXT PRIMARY KEY NOT NULL,
 principal_id TEXT NOT NULL REFERENCES principals(principal_id),
 provider TEXT NOT NULL,
 session_fingerprint TEXT NOT NULL,
 created_at INTEGER NOT NULL,
 updated_at INTEGER NOT NULL,
 UNIQUE(principal_id, provider, session_fingerprint)
);

-- Old HMAC-derived rows intentionally remain unresolved history.  A new
-- authenticated host scope is created only after a fresh assertion; do not
-- infer a mapping from retired secrets.
ALTER TABLE control_contexts ADD COLUMN host_scope_id TEXT REFERENCES host_scopes(host_scope_id);
DROP INDEX context_one_writer;
CREATE UNIQUE INDEX context_one_writer ON control_contexts(workstream_id)
 WHERE state='BOUND' AND host_scope_id IS NOT NULL;
CREATE UNIQUE INDEX control_context_one_host_scope ON control_contexts(host_scope_id)
 WHERE host_scope_id IS NOT NULL;
