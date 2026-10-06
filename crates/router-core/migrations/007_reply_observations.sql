-- MOBILE_WEB_PUSH_V1: exact, bounded ChatGPT observations are independent of
-- ProviderRun causality. Assistant identity is optional because an adapter
-- can read a complete reply without proving the producing Bridge request.
CREATE TABLE reply_observations (
  id TEXT PRIMARY KEY NOT NULL,
  workstream_id TEXT NOT NULL REFERENCES workstreams(id),
  endpoint_id TEXT NOT NULL REFERENCES endpoints(id),
  assistant_identity TEXT NULL,
  text TEXT NOT NULL,
  source_provider_run_id TEXT NULL REFERENCES provider_runs(id),
  observed_at INTEGER NOT NULL,
  read_at INTEGER NULL,
  handled_at INTEGER NULL
);
CREATE UNIQUE INDEX reply_observations_exact_identity_unique
  ON reply_observations(endpoint_id, assistant_identity)
  WHERE assistant_identity IS NOT NULL;
CREATE INDEX reply_observations_unread_idx
  ON reply_observations(workstream_id, read_at, observed_at DESC);
