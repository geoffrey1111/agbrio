-- A ChatGPT → Codex mobile review must survive a Host restart just like the
-- opposite direction.  It is deliberately not a Handoff yet: the owner may
-- still edit or approve a revision without creating any provider write.
CREATE TABLE mobile_chatgpt_inbound_reviews (
    action_id TEXT PRIMARY KEY NOT NULL,
    workstream_id TEXT NOT NULL REFERENCES workstreams(id) ON DELETE CASCADE,
    source_run_id TEXT NULL REFERENCES provider_runs(id) ON DELETE SET NULL,
    source_reference_id TEXT NOT NULL,
    source_endpoint_id TEXT NOT NULL REFERENCES endpoints(id) ON DELETE RESTRICT,
    source_chatgpt_conversation_id TEXT NOT NULL,
    source_response_identity TEXT NOT NULL,
    destination_codex_thread_id TEXT NOT NULL,
    original_text TEXT NOT NULL,
    attachments_json TEXT NOT NULL,
    approved_text TEXT NULL,
    revision INTEGER NOT NULL,
    status TEXT NOT NULL CHECK(status IN ('READY','APPROVED','SENDING','SENT','FAILED')),
    handoff_id TEXT NULL REFERENCES handoffs(id) ON DELETE SET NULL,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
);
CREATE INDEX mobile_chatgpt_inbound_reviews_workstream_updated
ON mobile_chatgpt_inbound_reviews(workstream_id, updated_at DESC);
