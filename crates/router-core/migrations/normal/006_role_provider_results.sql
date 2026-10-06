-- Two Codex roles must not erase each other's retained terminal results.
-- Legacy workstreams keep provider-scoped cleanup in the application service.
DROP INDEX provider_runs_current_result_unique;
CREATE UNIQUE INDEX provider_runs_current_result_unique
    ON provider_runs(workstream_id, endpoint_id)
    WHERE result_text IS NOT NULL;
