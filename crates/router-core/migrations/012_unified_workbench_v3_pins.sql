-- Router-local presentation preference. Pinning never changes provider
-- selection, Endpoint routing, a Workstream lifecycle, or external data.
ALTER TABLE workstreams ADD COLUMN pinned_at INTEGER NULL;
CREATE INDEX workstreams_project_pin_idx
    ON workstreams(project_id, pinned_at DESC, updated_at DESC);
