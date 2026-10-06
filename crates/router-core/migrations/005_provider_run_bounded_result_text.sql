-- MOBILE_AI_REVIEW_LOOPS_V1: retain only the current reviewable terminal
-- result for each Workstream/provider pair. ProviderRun history remains intact.
ALTER TABLE provider_runs ADD COLUMN result_text TEXT NULL;

CREATE UNIQUE INDEX provider_runs_current_result_unique
  ON provider_runs(workstream_id, provider)
  WHERE result_text IS NOT NULL;
