CREATE TABLE projects (
    id TEXT PRIMARY KEY NOT NULL,
    name TEXT NOT NULL,
    description TEXT,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
);

CREATE TABLE workstreams (
    id TEXT PRIMARY KEY NOT NULL,
    project_id TEXT NOT NULL REFERENCES projects(id),
    name TEXT NOT NULL,
    status TEXT NOT NULL CHECK (status IN ('ACTIVE', 'ARCHIVED')),
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
);

CREATE TABLE endpoints (
    id TEXT PRIMARY KEY NOT NULL,
    workstream_id TEXT NOT NULL REFERENCES workstreams(id),
    provider TEXT NOT NULL CHECK (provider IN ('CHATGPT', 'CODEX')),
    external_id TEXT NOT NULL,
    label TEXT NOT NULL,
    status TEXT NOT NULL CHECK (status IN ('ACTIVE', 'SUPERSEDED', 'ARCHIVED')),
    replaces_endpoint_id TEXT REFERENCES endpoints(id),
    created_at INTEGER NOT NULL,
    superseded_at INTEGER,
    UNIQUE(provider, external_id)
);

CREATE UNIQUE INDEX endpoints_one_active_provider_per_workstream
    ON endpoints(workstream_id, provider)
    WHERE status = 'ACTIVE';

CREATE TABLE handoffs (
    id TEXT PRIMARY KEY NOT NULL,
    workstream_id TEXT NOT NULL REFERENCES workstreams(id),
    source_endpoint_id TEXT NOT NULL REFERENCES endpoints(id),
    destination_endpoint_id TEXT NOT NULL REFERENCES endpoints(id),
    direction TEXT NOT NULL CHECK (direction IN ('CHATGPT_TO_CODEX', 'CODEX_TO_CHATGPT')),
    source_response_identity TEXT,
    original_text TEXT NOT NULL,
    approved_text TEXT NOT NULL,
    status TEXT NOT NULL CHECK (status IN ('DETECTED', 'READY', 'APPROVED', 'SENDING', 'SENT', 'FAILED', 'CANCELLED')),
    payload_hash TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    approved_at INTEGER,
    sent_at INTEGER,
    failed_at INTEGER,
    error_code TEXT,
    error_message TEXT
);

CREATE INDEX handoffs_workstream_created_at ON handoffs(workstream_id, created_at DESC);
CREATE INDEX handoffs_sent_dedup ON handoffs(source_response_identity, destination_endpoint_id, payload_hash) WHERE status = 'SENT';

CREATE TABLE handoff_attachments (
    id TEXT PRIMARY KEY NOT NULL,
    handoff_id TEXT NOT NULL REFERENCES handoffs(id),
    filename TEXT NOT NULL,
    original_path TEXT NOT NULL,
    size INTEGER,
    sha256 TEXT,
    integrity_status TEXT,
    created_at INTEGER NOT NULL
);

CREATE TABLE app_settings (
    key TEXT PRIMARY KEY NOT NULL CHECK (key IN ('last_project_id', 'last_workstream_id')),
    value TEXT NOT NULL,
    updated_at INTEGER NOT NULL
);
