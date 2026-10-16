CREATE TABLE assistant_oauth_refresh (
 token_hash TEXT PRIMARY KEY,
 grant_id TEXT NOT NULL REFERENCES assistant_grants(id),
 client_id TEXT NOT NULL,
 resource TEXT NOT NULL,
 expires_at INTEGER NOT NULL,
 family_id TEXT NOT NULL,
 access_hash TEXT NOT NULL,
 used_at INTEGER,
 revoked_at INTEGER
);
CREATE INDEX assistant_oauth_refresh_family ON assistant_oauth_refresh(family_id);
