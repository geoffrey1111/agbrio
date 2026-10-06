CREATE TABLE provider_run_feedback_sources (
    feedback_run_id TEXT PRIMARY KEY REFERENCES provider_runs(id),
    source_run_id TEXT NOT NULL REFERENCES provider_runs(id),
    created_at INTEGER NOT NULL
);

CREATE INDEX idx_provider_run_feedback_sources_source
    ON provider_run_feedback_sources(source_run_id);
