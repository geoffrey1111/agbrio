-- Approved design section 6.2. Applied only by the isolated 014 runner.
CREATE TABLE control_contexts (
 id TEXT PRIMARY KEY NOT NULL, principal_key TEXT NOT NULL,
 scope_key TEXT NOT NULL, key_version INTEGER NOT NULL CHECK(key_version>0),
 workstream_id TEXT REFERENCES workstreams(id),
 state TEXT NOT NULL CHECK(state IN ('UNBOUND','BOUND','REVOKED')),
 revision INTEGER NOT NULL DEFAULT 0 CHECK(revision>=0),
 created_at INTEGER NOT NULL, updated_at INTEGER NOT NULL,
 UNIQUE(principal_key,scope_key,key_version),
 CHECK((state='BOUND' AND workstream_id IS NOT NULL) OR state!='BOUND')
);
CREATE UNIQUE INDEX context_one_writer ON control_contexts(workstream_id)
 WHERE state='BOUND';
CREATE TABLE project_execution_roots (
 project_id TEXT PRIMARY KEY NOT NULL REFERENCES projects(id),
 canonical_path TEXT NOT NULL, path_identity_hash TEXT NOT NULL,
 revision INTEGER NOT NULL CHECK(revision>0),
 policy_json TEXT NOT NULL, policy_hash TEXT NOT NULL, updated_at INTEGER NOT NULL
);
-- policy_json is a server-produced closed PolicySnapshot DTO, not arbitrary model JSON.
CREATE TABLE control_handoff_details (
 handoff_id TEXT PRIMARY KEY NOT NULL REFERENCES handoffs(id),
 context_id TEXT NOT NULL REFERENCES control_contexts(id),
 client_request_id TEXT NOT NULL, request_hash TEXT NOT NULL,
 context_revision INTEGER NOT NULL, binding_revision INTEGER NOT NULL,
 root_revision INTEGER NOT NULL, draft_revision INTEGER NOT NULL CHECK(draft_revision>0),
 policy_json TEXT NOT NULL, policy_hash TEXT NOT NULL,
 previous_handoff_id TEXT REFERENCES handoffs(id),
 approved_principal_key TEXT, approved_channel TEXT CHECK(approved_channel='BROWSER_REVIEW'),
 review_nonce_hash TEXT UNIQUE, approval_expires_at INTEGER, consumed_at INTEGER,
 UNIQUE(context_id,client_request_id),
 CHECK((approved_principal_key IS NULL AND approved_channel IS NULL AND consumed_at IS NULL)
    OR (approved_principal_key IS NOT NULL AND approved_channel IS NOT NULL
        AND review_nonce_hash IS NOT NULL AND approval_expires_at IS NOT NULL))
);
CREATE TABLE handoff_dispatches (
 handoff_id TEXT PRIMARY KEY NOT NULL REFERENCES handoffs(id),
 run_id TEXT NOT NULL UNIQUE REFERENCES provider_runs(id),
 workstream_id TEXT NOT NULL REFERENCES workstreams(id),
 endpoint_id TEXT NOT NULL REFERENCES endpoints(id),
 dispatch_id TEXT NOT NULL UNIQUE, adapter_epoch TEXT NOT NULL,
 revision INTEGER NOT NULL DEFAULT 0 CHECK(revision>=0),
 native_request_id_json TEXT, write_intent_at INTEGER, epoch_sealed_at INTEGER,
 phase TEXT NOT NULL CHECK(phase IN ('CLAIMED','PREPARING','WRITE_INTENT','ACCEPTED','UNKNOWN','NOT_SENT','TERMINAL')),
 created_at INTEGER NOT NULL, updated_at INTEGER NOT NULL,
 last_code TEXT
);
CREATE UNIQUE INDEX dispatch_one_unresolved_per_workstream
 ON handoff_dispatches(workstream_id)
 WHERE phase IN ('CLAIMED','PREPARING','WRITE_INTENT','ACCEPTED','UNKNOWN');
CREATE TABLE binding_requests (
 id TEXT PRIMARY KEY NOT NULL, context_id TEXT NOT NULL REFERENCES control_contexts(id),
 client_request_id TEXT NOT NULL, request_hash TEXT NOT NULL,
 operation TEXT NOT NULL CHECK(operation IN ('SELECT','CREATE','REPLACE','TAKEOVER','UNBIND')),
 project_id TEXT REFERENCES projects(id), workstream_id TEXT REFERENCES workstreams(id),
 expected_context_revision INTEGER NOT NULL, expected_binding_revision INTEGER,
 root_revision INTEGER, candidate_thread_id TEXT, candidate_policy_hash TEXT,
 state TEXT NOT NULL CHECK(state IN ('REVIEW','CREATING','VERIFYING','READY','APPLIED','CANCELLED','UNKNOWN')),
 approved_principal_key TEXT, review_nonce_hash TEXT UNIQUE,
 created_at INTEGER NOT NULL, expires_at INTEGER NOT NULL,
 UNIQUE(context_id,client_request_id)
);

CREATE TABLE handoffs_v14 (
    id TEXT PRIMARY KEY NOT NULL,
    workstream_id TEXT NOT NULL REFERENCES workstreams(id),
    source_endpoint_id TEXT REFERENCES endpoints(id),
    destination_endpoint_id TEXT NOT NULL REFERENCES endpoints(id),
    direction TEXT NOT NULL CHECK (direction IN ('CHATGPT_TO_CODEX', 'CODEX_TO_CHATGPT', 'CONTROL_TO_CODEX')),
    source_response_identity TEXT,
    original_text TEXT NOT NULL,
    approved_text TEXT NOT NULL,
    status TEXT NOT NULL CHECK (status IN ('DETECTED', 'READY', 'APPROVED', 'SENDING', 'SENT', 'FAILED', 'CANCELLED')),
    payload_hash TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    approved_at INTEGER,
    sent_at INTEGER,
    failed_at INTEGER,
    error_code TEXT,
    error_message TEXT,
    bridge_request_id TEXT,
    bridge_turn_key TEXT,
    bridge_correlation_observed_at INTEGER,
    attention_acknowledged_at INTEGER,
    source_kind TEXT NOT NULL DEFAULT 'ENDPOINT' CHECK(source_kind IN ('ENDPOINT','CONTROL_CONTEXT')),
    source_context_id TEXT REFERENCES control_contexts(id),
    CHECK ((source_kind='ENDPOINT' AND source_endpoint_id IS NOT NULL AND source_context_id IS NULL
            AND direction IN ('CHATGPT_TO_CODEX','CODEX_TO_CHATGPT'))
        OR (source_kind='CONTROL_CONTEXT' AND source_endpoint_id IS NULL AND source_context_id IS NOT NULL
            AND direction='CONTROL_TO_CODEX'))
);
INSERT INTO handoffs_v14 (id,workstream_id,source_endpoint_id,destination_endpoint_id,direction,source_response_identity,original_text,approved_text,status,payload_hash,created_at,approved_at,sent_at,failed_at,error_code,error_message,bridge_request_id,bridge_turn_key,bridge_correlation_observed_at,attention_acknowledged_at,source_kind,source_context_id) SELECT id,workstream_id,source_endpoint_id,destination_endpoint_id,direction,source_response_identity,original_text,approved_text,status,payload_hash,created_at,approved_at,sent_at,failed_at,error_code,error_message,bridge_request_id,bridge_turn_key,bridge_correlation_observed_at,attention_acknowledged_at,'ENDPOINT',NULL FROM handoffs ORDER BY rowid;
DROP TABLE handoffs;
ALTER TABLE handoffs_v14 RENAME TO handoffs;
CREATE INDEX handoffs_workstream_created_at ON handoffs(workstream_id, created_at DESC);
CREATE INDEX handoffs_sent_dedup ON handoffs(source_response_identity, destination_endpoint_id, payload_hash) WHERE status = 'SENT';
CREATE UNIQUE INDEX handoffs_unique_bridge_request_id
    ON handoffs(bridge_request_id)
    WHERE bridge_request_id IS NOT NULL;
CREATE INDEX handoffs_attention_idx ON handoffs(status, attention_acknowledged_at, created_at DESC);

CREATE TABLE dispatch_native_evidence (
 id TEXT PRIMARY KEY NOT NULL,
 dispatch_id TEXT NOT NULL REFERENCES handoff_dispatches(dispatch_id),
 kind TEXT NOT NULL CHECK(kind IN ('ACK_LINK','PREWRITE_STOP','TERMINAL_LINK','CONFLICT')),
 backend_id TEXT NOT NULL, native_version TEXT NOT NULL,
 proof_source TEXT NOT NULL CHECK(proof_source IN
  ('STDIO_ACK','PREWRITE_GATE','STDIO_TERMINAL','HISTORY_PAGED','CONFLICT')),
 adapter_epoch TEXT NOT NULL, request_id_json TEXT NOT NULL,
 thread_id TEXT NOT NULL, turn_id TEXT, item_id TEXT,
 payload_hash TEXT NOT NULL, native_status TEXT,
 event_sequence INTEGER, evidence_digest TEXT NOT NULL, integrity_mac TEXT NOT NULL,
 recorded_at INTEGER NOT NULL, UNIQUE(dispatch_id,kind)
);
CREATE TABLE provider_result_details (
 run_id TEXT PRIMARY KEY NOT NULL REFERENCES provider_runs(id),
 result_state TEXT NOT NULL CHECK(result_state IN
  ('PENDING','AVAILABLE','NOT_RETAINED','UNAVAILABLE','TOO_LARGE','NOT_APPLICABLE')),
 source TEXT CHECK(source IN ('LIVE_TERMINAL','LIVE_ITEM_AND_TERMINAL','HISTORY_PAGED')),
 final_item_id TEXT, final_item_phase TEXT, result_hash TEXT, result_bytes INTEGER,
 terminal_evidence_id TEXT REFERENCES dispatch_native_evidence(id),
 result_error_code TEXT, updated_at INTEGER NOT NULL
);
CREATE TABLE operator_resolutions (
 id TEXT PRIMARY KEY NOT NULL,
 dispatch_id TEXT NOT NULL REFERENCES handoff_dispatches(dispatch_id),
 client_resolution_id TEXT NOT NULL, request_hash TEXT NOT NULL,
 principal_key TEXT NOT NULL, context_id TEXT NOT NULL REFERENCES control_contexts(id),
 context_revision INTEGER NOT NULL, binding_revision INTEGER NOT NULL,
 expected_dispatch_revision INTEGER NOT NULL, resulting_dispatch_revision INTEGER NOT NULL,
 action TEXT NOT NULL CHECK(action IN ('ATTACH_RUN','CONFIRM_NOT_SENT','KEEP_UNKNOWN')),
 ack_or_stop_evidence_id TEXT REFERENCES dispatch_native_evidence(id),
 terminal_evidence_id TEXT REFERENCES dispatch_native_evidence(id),
 evidence_digest TEXT NOT NULL, nonce_hash TEXT NOT NULL UNIQUE,
 old_state_json TEXT NOT NULL, new_state_json TEXT NOT NULL,
 reason_code TEXT NOT NULL CHECK(reason_code IN
  ('ACK_PROVEN','TERMINAL_PROVEN','PREWRITE_PROVEN','INSUFFICIENT','CONFLICT')),
 channel TEXT NOT NULL CHECK(channel='BROWSER_REVIEW'), created_at INTEGER NOT NULL,
 UNIQUE(dispatch_id,client_resolution_id)
);
CREATE TABLE runtime_exit_state (
 singleton INTEGER PRIMARY KEY CHECK(singleton=1), revision INTEGER NOT NULL,
 adapter_epoch TEXT NOT NULL, phase TEXT NOT NULL CHECK(phase IN ('RUNNING','QUIESCED','EXIT_PENDING','SEALED')),
 unresolved_set_hash TEXT NOT NULL, operator_principal_key TEXT,
 nonce_hash TEXT UNIQUE, quiesced_at INTEGER, sealed_at INTEGER
);
