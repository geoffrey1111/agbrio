-- Passive Bridge observation retains only tiny exact identities/cursors, not
-- a provider transcript or raw SSE payload.  The identities seed bootstrap
-- and discontinuity reconciliation so pre-existing history never floods Push.
CREATE TABLE reply_observer_stream_cursor (
  id INTEGER PRIMARY KEY NOT NULL CHECK(id = 1),
  stream_epoch TEXT NOT NULL,
  last_sequence INTEGER NOT NULL,
  updated_at INTEGER NOT NULL
);
CREATE TABLE reply_observer_seen_identities (
  endpoint_id TEXT NOT NULL REFERENCES endpoints(id),
  assistant_identity TEXT NOT NULL,
  seen_at INTEGER NOT NULL,
  PRIMARY KEY(endpoint_id, assistant_identity)
);
CREATE INDEX reply_observer_seen_identities_recent_idx
  ON reply_observer_seen_identities(endpoint_id, seen_at DESC);
