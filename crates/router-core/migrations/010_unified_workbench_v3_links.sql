-- UNIFIED_WORKBENCH_V3: Router-owned project associations and local lifecycle.
-- External provider identities remain metadata only. This migration never
-- imports provider projects, conversations, credentials, or transcript data.
ALTER TABLE workstreams ADD COLUMN binding_revision INTEGER NOT NULL DEFAULT 0;
ALTER TABLE workstreams ADD COLUMN archived_at INTEGER NULL;
ALTER TABLE workstreams ADD COLUMN trashed_at INTEGER NULL;
ALTER TABLE workstreams ADD COLUMN lifecycle_previous_status TEXT NULL;

CREATE TABLE external_project_links (
    id TEXT PRIMARY KEY NOT NULL,
    project_id TEXT NOT NULL REFERENCES projects(id),
    provider TEXT NOT NULL CHECK (provider IN ('CHATGPT', 'CODEX')),
    external_project_id TEXT NOT NULL,
    canonical_url TEXT NULL,
    label TEXT NOT NULL,
    source_kind TEXT NOT NULL,
    source_version TEXT NULL,
    verified_at INTEGER NULL,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,
    UNIQUE(project_id, provider),
    UNIQUE(provider, external_project_id)
);

-- Only ChatGPT project links have a canonical provider URL. A URL is still
-- not a conversation binding and is deliberately stored separately from
-- `endpoints.external_id`.
CREATE UNIQUE INDEX external_project_links_provider_url_unique
    ON external_project_links(provider, canonical_url)
    WHERE canonical_url IS NOT NULL;
CREATE INDEX external_project_links_project_idx
    ON external_project_links(project_id, provider);

CREATE INDEX workstreams_trash_idx
    ON workstreams(project_id, trashed_at, updated_at DESC);
