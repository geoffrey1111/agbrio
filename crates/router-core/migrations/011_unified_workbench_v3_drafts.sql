-- Router-owned drafts are local, revisioned presentation state. They never
-- create a provider turn or authorize a Handoff.
CREATE TABLE workstream_drafts (
    workstream_id TEXT PRIMARY KEY NOT NULL REFERENCES workstreams(id),
    text TEXT NOT NULL,
    revision INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
);
