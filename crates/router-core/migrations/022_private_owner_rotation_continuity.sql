-- A credential rotation must not rewrite a READY control Handoff's immutable
-- payload hash.  This marker is written only by offline private maintenance
-- while the preview writer lease is held.  It carries no retired key and is
-- scoped to the exact surviving Handoff and its replacement owner epoch.
CREATE TABLE private_owner_rotation_continuity (
 handoff_id TEXT PRIMARY KEY NOT NULL REFERENCES handoffs(id),
 payload_hash TEXT NOT NULL,
 principal_key TEXT NOT NULL,
 created_at INTEGER NOT NULL
);
