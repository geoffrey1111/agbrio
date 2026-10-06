-- V0-006: execution lifecycle is separate from Handoff delivery lifecycle.
ALTER TABLE handoffs ADD COLUMN attention_acknowledged_at INTEGER NULL;

CREATE TABLE provider_runs (
  id TEXT PRIMARY KEY NOT NULL,
  workstream_id TEXT NOT NULL REFERENCES workstreams(id),
  endpoint_id TEXT NOT NULL REFERENCES endpoints(id),
  provider TEXT NOT NULL CHECK(provider IN ('CHATGPT','CODEX')),
  origin_handoff_id TEXT NULL REFERENCES handoffs(id),
  external_run_id TEXT NULL,
  status TEXT NOT NULL CHECK(status IN ('STARTING','RUNNING','COMPLETED','FAILED','CANCELLED','UNKNOWN')),
  result_identity TEXT NULL,
  terminal_code TEXT NULL,
  started_at INTEGER NULL,
  terminal_at INTEGER NULL,
  reviewed_at INTEGER NULL,
  created_at INTEGER NOT NULL,
  updated_at INTEGER NOT NULL,
  UNIQUE(origin_handoff_id)
);
CREATE UNIQUE INDEX provider_runs_provider_external_run_id_unique
  ON provider_runs(provider, external_run_id) WHERE external_run_id IS NOT NULL;
CREATE INDEX provider_runs_workstream_updated_idx ON provider_runs(workstream_id, updated_at DESC);
CREATE INDEX handoffs_attention_idx ON handoffs(status, attention_acknowledged_at, created_at DESC);
