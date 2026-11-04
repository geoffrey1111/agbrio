-- External-vault reference intents and minimum retirement fences, never secrets.
CREATE TABLE mcp_native_secret_refs (
 reference BLOB PRIMARY KEY, subscription_id TEXT NOT NULL, grant_id TEXT NOT NULL,
 state TEXT NOT NULL CHECK(state IN ('RESERVED','IN_USE','RETIRED')),
 reserved_at INTEGER NOT NULL, creation_settled INTEGER NOT NULL DEFAULT 0, created INTEGER NOT NULL DEFAULT 0,
 revision INTEGER NOT NULL DEFAULT 0, delete_after INTEGER NOT NULL DEFAULT 0,
 delete_lease TEXT, delete_lease_until INTEGER, deleted_at INTEGER, last_error TEXT
);
CREATE UNIQUE INDEX mcp_native_secret_pending_sid ON mcp_native_secret_refs(subscription_id) WHERE state='RESERVED';
CREATE INDEX mcp_native_secret_due ON mcp_native_secret_refs(state,deleted_at,delete_after,delete_lease_until);
CREATE TRIGGER mcp_native_secret_rotation AFTER UPDATE OF secret,previous_secret ON mcp_event_subscriptions
BEGIN
 UPDATE mcp_native_secret_refs SET state='RETIRED',revision=revision+1,
  delete_after=0,delete_lease=NULL,delete_lease_until=NULL,deleted_at=NULL,last_error=NULL
 WHERE reference IN (OLD.secret,OLD.previous_secret)
   AND reference!=NEW.secret AND (NEW.previous_secret IS NULL OR reference!=NEW.previous_secret);
END;
CREATE TRIGGER mcp_native_secret_removed AFTER DELETE ON mcp_event_subscriptions
BEGIN
 UPDATE mcp_native_secret_refs SET state='RETIRED',revision=revision+1,
  delete_after=0,delete_lease=NULL,delete_lease_until=NULL,deleted_at=NULL,last_error=NULL
 WHERE reference IN (OLD.secret,OLD.previous_secret);
END;
