-- Approved Hybrid Browser + Artifact Relay schema.  This file is only run by
-- the isolated preview maintenance runner; RouterStore::open_at never applies
-- it to the normal desktop database.
CREATE TABLE browser_endpoint_settings (
    endpoint_id TEXT PRIMARY KEY NOT NULL REFERENCES endpoints(id),
    owner_principal_key TEXT NOT NULL,
    observe_enabled INTEGER NOT NULL CHECK(observe_enabled IN (0,1)),
    settings_revision INTEGER NOT NULL CHECK(settings_revision > 0),
    paired_by TEXT NOT NULL,
    paired_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
);

CREATE TABLE relay_handoff_details (
    handoff_id TEXT PRIMARY KEY NOT NULL REFERENCES handoffs(id),
    owner_principal_key TEXT NOT NULL,
    source_run_id TEXT REFERENCES provider_runs(id),
    source_message_ref TEXT NOT NULL,
    capture_digest TEXT NOT NULL,
    source_completeness TEXT NOT NULL CHECK(source_completeness IN ('COMPLETE','PARTIAL','EXACT_SELECTION')),
    binding_revision INTEGER NOT NULL CHECK(binding_revision >= 0),
    root_revision INTEGER NOT NULL CHECK(root_revision > 0),
    destination_policy_json TEXT NOT NULL,
    destination_policy_hash TEXT NOT NULL,
    draft_revision INTEGER NOT NULL CHECK(draft_revision > 0),
    envelope_version TEXT NOT NULL,
    manifest_hash TEXT NOT NULL,
    client_request_id TEXT NOT NULL,
    review_nonce_hash TEXT UNIQUE,
    approved_principal_key TEXT,
    approval_expires_at INTEGER,
    consumed_at INTEGER,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,
    UNIQUE(owner_principal_key, client_request_id),
    CHECK((review_nonce_hash IS NULL AND approved_principal_key IS NULL AND approval_expires_at IS NULL AND consumed_at IS NULL)
       OR (review_nonce_hash IS NOT NULL AND approval_expires_at IS NOT NULL))
);

CREATE TRIGGER relay_handoff_details_requires_endpoint_source
BEFORE INSERT ON relay_handoff_details
WHEN (SELECT source_kind FROM handoffs WHERE id=NEW.handoff_id) != 'ENDPOINT'
  OR EXISTS(SELECT 1 FROM control_handoff_details WHERE handoff_id=NEW.handoff_id)
BEGIN SELECT RAISE(ABORT, 'RELAY_HANDOFF_REQUIRES_ENDPOINT_SOURCE'); END;

CREATE TRIGGER control_handoff_details_requires_control_source
BEFORE INSERT ON control_handoff_details
WHEN (SELECT source_kind FROM handoffs WHERE id=NEW.handoff_id) != 'CONTROL_CONTEXT'
  OR EXISTS(SELECT 1 FROM relay_handoff_details WHERE handoff_id=NEW.handoff_id)
BEGIN SELECT RAISE(ABORT, 'CONTROL_HANDOFF_REQUIRES_CONTROL_SOURCE'); END;

CREATE TABLE artifact_materializations (
    id TEXT PRIMARY KEY NOT NULL,
    source_endpoint_id TEXT NOT NULL REFERENCES endpoints(id),
    source_run_id TEXT REFERENCES provider_runs(id),
    source_message_ref TEXT NOT NULL,
    source_artifact_ref TEXT NOT NULL,
    kind TEXT NOT NULL CHECK(kind IN ('DOCUMENT','IMAGE')),
    filename TEXT NOT NULL,
    mime TEXT NOT NULL,
    size INTEGER NOT NULL CHECK(size >= 0),
    source_declared_sha256 TEXT,
    transfer_declared_sha256 TEXT NOT NULL,
    actual_sha256 TEXT,
    source_hash_status TEXT NOT NULL CHECK(source_hash_status IN ('DECLARED','UNAVAILABLE','MISMATCH')),
    integrity_status TEXT NOT NULL CHECK(integrity_status IN ('PENDING','VERIFIED','MISMATCH','FAILED')),
    storage_ref TEXT NOT NULL CHECK(storage_ref NOT LIKE '/%' AND storage_ref NOT LIKE '\\%' AND instr(storage_ref, ':') = 0 AND instr(storage_ref, '..') = 0),
    state TEXT NOT NULL CHECK(state IN ('MATERIALIZING','READY','FAILED','PURGED')),
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,
    UNIQUE(source_endpoint_id, source_message_ref, source_artifact_ref)
);

CREATE TABLE handoff_attachment_details (
    attachment_id TEXT PRIMARY KEY NOT NULL REFERENCES handoff_attachments(id),
    handoff_id TEXT NOT NULL REFERENCES handoffs(id),
    artifact_id TEXT NOT NULL REFERENCES artifact_materializations(id),
    ordinal INTEGER NOT NULL CHECK(ordinal >= 0),
    mime TEXT NOT NULL,
    approved_sha256 TEXT NOT NULL,
    approved_size INTEGER NOT NULL CHECK(approved_size >= 0),
    manifest_entry_json TEXT NOT NULL,
    target_relative_path TEXT CHECK(target_relative_path IS NULL OR (target_relative_path NOT LIKE '/%' AND target_relative_path NOT LIKE '\\%' AND instr(target_relative_path, ':') = 0 AND instr(target_relative_path, '..') = 0)),
    UNIQUE(handoff_id, ordinal)
);

CREATE TRIGGER handoff_attachment_details_handoff_matches
BEFORE INSERT ON handoff_attachment_details
WHEN (SELECT handoff_id FROM handoff_attachments WHERE id=NEW.attachment_id) != NEW.handoff_id
BEGIN SELECT RAISE(ABORT, 'RELAY_ATTACHMENT_HANDOFF_MISMATCH'); END;

CREATE TABLE bridge_dispatch_evidence (
    id TEXT PRIMARY KEY NOT NULL,
    dispatch_id TEXT NOT NULL REFERENCES handoff_dispatches(dispatch_id),
    kind TEXT NOT NULL CHECK(kind IN ('USER_ACCEPTED','ASSISTANT_TERMINAL','PREWRITE_STOP','CONFLICT')),
    endpoint_id TEXT NOT NULL REFERENCES endpoints(id),
    conversation_id TEXT NOT NULL,
    source_message_id TEXT NOT NULL,
    request_id TEXT NOT NULL,
    turn_key TEXT,
    adapter_epoch TEXT NOT NULL,
    bundle_identity TEXT NOT NULL,
    payload_hash TEXT NOT NULL,
    manifest_hash TEXT NOT NULL,
    evidence_digest TEXT NOT NULL,
    integrity_mac TEXT NOT NULL,
    recorded_at INTEGER NOT NULL,
    UNIQUE(dispatch_id, kind, source_message_id, request_id)
);

-- 014 only allowed ControlContext resolutions and native terminal evidence.
-- Preserve every historical field verbatim while adding a tagged source for
-- the endpoint-origin relay path.
ALTER TABLE provider_result_details RENAME TO provider_result_details_v14;
CREATE TABLE provider_result_details (
    run_id TEXT PRIMARY KEY NOT NULL REFERENCES provider_runs(id),
    result_state TEXT NOT NULL CHECK(result_state IN ('PENDING','AVAILABLE','NOT_RETAINED','UNAVAILABLE','TOO_LARGE','NOT_APPLICABLE')),
    source TEXT CHECK(source IN ('LIVE_TERMINAL','LIVE_ITEM_AND_TERMINAL','HISTORY_PAGED')),
    final_item_id TEXT,
    final_item_phase TEXT,
    result_hash TEXT,
    result_bytes INTEGER,
    terminal_evidence_id TEXT REFERENCES dispatch_native_evidence(id),
    bridge_evidence_id TEXT REFERENCES bridge_dispatch_evidence(id),
    terminal_source TEXT NOT NULL DEFAULT 'NATIVE' CHECK(terminal_source IN ('NATIVE','BRIDGE_TERMINAL')),
    result_error_code TEXT,
    updated_at INTEGER NOT NULL,
    CHECK((terminal_source='NATIVE' AND bridge_evidence_id IS NULL)
       OR (terminal_source='BRIDGE_TERMINAL' AND terminal_evidence_id IS NULL AND bridge_evidence_id IS NOT NULL))
);
INSERT INTO provider_result_details(run_id,result_state,source,final_item_id,final_item_phase,result_hash,result_bytes,terminal_evidence_id,bridge_evidence_id,terminal_source,result_error_code,updated_at)
SELECT run_id,result_state,source,final_item_id,final_item_phase,result_hash,result_bytes,terminal_evidence_id,NULL,'NATIVE',result_error_code,updated_at
FROM provider_result_details_v14;
DROP TABLE provider_result_details_v14;

ALTER TABLE operator_resolutions RENAME TO operator_resolutions_v14;
CREATE TABLE operator_resolutions (
    id TEXT PRIMARY KEY NOT NULL,
    dispatch_id TEXT NOT NULL REFERENCES handoff_dispatches(dispatch_id),
    client_resolution_id TEXT NOT NULL,
    request_hash TEXT NOT NULL,
    principal_key TEXT NOT NULL,
    context_id TEXT REFERENCES control_contexts(id),
    source_endpoint_id TEXT REFERENCES endpoints(id),
    source_kind TEXT NOT NULL DEFAULT 'CONTROL_CONTEXT' CHECK(source_kind IN ('CONTROL_CONTEXT','ENDPOINT')),
    context_revision INTEGER NOT NULL,
    binding_revision INTEGER NOT NULL,
    expected_dispatch_revision INTEGER NOT NULL,
    resulting_dispatch_revision INTEGER NOT NULL,
    action TEXT NOT NULL CHECK(action IN ('ATTACH_RUN','CONFIRM_NOT_SENT','KEEP_UNKNOWN')),
    ack_or_stop_evidence_id TEXT REFERENCES dispatch_native_evidence(id),
    terminal_evidence_id TEXT REFERENCES dispatch_native_evidence(id),
    evidence_digest TEXT NOT NULL,
    nonce_hash TEXT NOT NULL UNIQUE,
    old_state_json TEXT NOT NULL,
    new_state_json TEXT NOT NULL,
    reason_code TEXT NOT NULL CHECK(reason_code IN ('ACK_PROVEN','TERMINAL_PROVEN','PREWRITE_PROVEN','INSUFFICIENT','CONFLICT')),
    channel TEXT NOT NULL CHECK(channel='BROWSER_REVIEW'),
    created_at INTEGER NOT NULL,
    UNIQUE(dispatch_id,client_resolution_id),
    CHECK((source_kind='CONTROL_CONTEXT' AND context_id IS NOT NULL AND source_endpoint_id IS NULL)
       OR (source_kind='ENDPOINT' AND context_id IS NULL AND source_endpoint_id IS NOT NULL))
);
INSERT INTO operator_resolutions(id,dispatch_id,client_resolution_id,request_hash,principal_key,context_id,source_endpoint_id,source_kind,context_revision,binding_revision,expected_dispatch_revision,resulting_dispatch_revision,action,ack_or_stop_evidence_id,terminal_evidence_id,evidence_digest,nonce_hash,old_state_json,new_state_json,reason_code,channel,created_at)
SELECT id,dispatch_id,client_resolution_id,request_hash,principal_key,context_id,NULL,'CONTROL_CONTEXT',context_revision,binding_revision,expected_dispatch_revision,resulting_dispatch_revision,action,ack_or_stop_evidence_id,terminal_evidence_id,evidence_digest,nonce_hash,old_state_json,new_state_json,reason_code,channel,created_at
FROM operator_resolutions_v14;
DROP TABLE operator_resolutions_v14;
