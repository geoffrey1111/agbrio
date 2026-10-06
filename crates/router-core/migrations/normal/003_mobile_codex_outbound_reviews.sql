-- Mobile review drafts must outlive a Router Host restart. They are not
-- Handoffs yet: a durable Handoff begins only after the owner approves text.
CREATE TABLE mobile_codex_outbound_reviews (
    action_id TEXT PRIMARY KEY NOT NULL,
    workstream_id TEXT NOT NULL REFERENCES workstreams(id) ON DELETE CASCADE,
    source_run_id TEXT NULL REFERENCES provider_runs(id) ON DELETE SET NULL,
    source_endpoint_id TEXT NOT NULL REFERENCES endpoints(id) ON DELETE RESTRICT,
    source_codex_thread_id TEXT NOT NULL,
    destination_chatgpt_conversation_id TEXT NOT NULL,
    original_text TEXT NOT NULL,
    attachments_json TEXT NOT NULL,
    approved_text TEXT NULL,
    revision INTEGER NOT NULL,
    status TEXT NOT NULL CHECK(status IN ('READY','APPROVED','SENDING','SENT','FAILED')),
    handoff_id TEXT NULL REFERENCES handoffs(id) ON DELETE SET NULL,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
);
CREATE INDEX mobile_codex_outbound_reviews_workstream_updated
ON mobile_codex_outbound_reviews(workstream_id, updated_at DESC);
