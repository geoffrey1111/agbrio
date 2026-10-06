CREATE TABLE endpoint_canonical_urls (
    endpoint_id TEXT PRIMARY KEY NOT NULL REFERENCES endpoints(id) ON DELETE CASCADE,
    canonical_url TEXT NOT NULL,
    updated_at INTEGER NOT NULL
);
