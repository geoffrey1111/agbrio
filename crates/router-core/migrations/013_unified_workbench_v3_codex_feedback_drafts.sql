-- A direct Codex correction is Router-local presentation state until the
-- user explicitly sends it. It is scoped to the exact retained source result
-- so feedback for one completed result cannot prefill another result.
CREATE TABLE codex_feedback_drafts (
    workstream_id TEXT NOT NULL REFERENCES workstreams(id),
    source_run_id TEXT NOT NULL REFERENCES provider_runs(id),
    text TEXT NOT NULL,
    revision INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,
    PRIMARY KEY (workstream_id, source_run_id)
);
