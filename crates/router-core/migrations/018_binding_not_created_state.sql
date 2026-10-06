-- A CREATE candidate can be proved absent only after the exact guarded
-- thread/read + thread/resume sequence. Preserve that durable distinction
-- from UNKNOWN without reopening the original one-shot CREATE.
CREATE TABLE binding_requests_v18 (
 id TEXT PRIMARY KEY NOT NULL, context_id TEXT NOT NULL REFERENCES control_contexts(id),
 client_request_id TEXT NOT NULL, request_hash TEXT NOT NULL,
 operation TEXT NOT NULL CHECK(operation IN ('SELECT','CREATE','REPLACE','TAKEOVER','UNBIND')),
 project_id TEXT REFERENCES projects(id), workstream_id TEXT REFERENCES workstreams(id),
 expected_context_revision INTEGER NOT NULL, expected_binding_revision INTEGER,
 root_revision INTEGER, candidate_thread_id TEXT, candidate_policy_hash TEXT,
 state TEXT NOT NULL CHECK(state IN ('REVIEW','CREATING','VERIFYING','READY','APPLIED','CANCELLED','UNKNOWN','NOT_CREATED')),
 approved_principal_key TEXT, review_nonce_hash TEXT UNIQUE,
 created_at INTEGER NOT NULL, expires_at INTEGER NOT NULL,
 UNIQUE(context_id,client_request_id)
);
INSERT INTO binding_requests_v18(
 id,context_id,client_request_id,request_hash,operation,project_id,workstream_id,
 expected_context_revision,expected_binding_revision,root_revision,candidate_thread_id,
 candidate_policy_hash,state,approved_principal_key,review_nonce_hash,created_at,expires_at
)
SELECT
 id,context_id,client_request_id,request_hash,operation,project_id,workstream_id,
 expected_context_revision,expected_binding_revision,root_revision,candidate_thread_id,
 candidate_policy_hash,state,approved_principal_key,review_nonce_hash,created_at,expires_at
FROM binding_requests;
DROP TABLE binding_requests;
ALTER TABLE binding_requests_v18 RENAME TO binding_requests;
