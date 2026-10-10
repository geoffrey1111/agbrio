-- Webhook metadata/outbox only. Full replies remain in their existing source store.
CREATE TABLE mcp_event_initial_observations (observation_id TEXT PRIMARY KEY REFERENCES reply_observations(id) ON DELETE CASCADE);
INSERT INTO mcp_event_initial_observations SELECT id FROM reply_observations;
CREATE TABLE mcp_events (
 sequence INTEGER PRIMARY KEY AUTOINCREMENT, event_id TEXT NOT NULL UNIQUE,
 name TEXT NOT NULL, workstream_id TEXT NOT NULL REFERENCES workstreams(id) ON DELETE CASCADE,
 binding_revision INTEGER NOT NULL, endpoint_id TEXT NOT NULL,
 observation_id TEXT NOT NULL, source_role TEXT NOT NULL,
 occurred_at INTEGER NOT NULL, root_event_id TEXT NOT NULL, hop_count INTEGER NOT NULL DEFAULT 0,
 reason TEXT, actor_grant_id TEXT,
 UNIQUE(name,workstream_id,binding_revision,observation_id,source_role)
);
CREATE TABLE mcp_event_subscriptions (
 id TEXT PRIMARY KEY, grant_id TEXT NOT NULL REFERENCES assistant_grants(id) ON DELETE CASCADE,
 name TEXT NOT NULL, workstream_id TEXT NOT NULL REFERENCES workstreams(id) ON DELETE CASCADE,
 binding_revision INTEGER NOT NULL, source_role TEXT NOT NULL, callback_url TEXT NOT NULL,
 secret BLOB NOT NULL, previous_secret BLOB, rotation_until INTEGER,
 verified_at INTEGER NOT NULL, expires_at INTEGER NOT NULL, status TEXT NOT NULL,
 created_at INTEGER NOT NULL, updated_at INTEGER NOT NULL, start_sequence INTEGER NOT NULL
);
CREATE TABLE mcp_event_deliveries (
 subscription_id TEXT NOT NULL REFERENCES mcp_event_subscriptions(id) ON DELETE CASCADE,
 event_id TEXT NOT NULL REFERENCES mcp_events(event_id) ON DELETE CASCADE,
 status TEXT NOT NULL, attempts INTEGER NOT NULL DEFAULT 0,
 next_attempt_at INTEGER NOT NULL, lease_until INTEGER, delivered_at INTEGER, http_status INTEGER,
 PRIMARY KEY(subscription_id,event_id)
);
CREATE INDEX mcp_events_due ON mcp_event_deliveries(status,next_attempt_at,lease_until);
CREATE TABLE mcp_event_actions (
 grant_id TEXT NOT NULL REFERENCES assistant_grants(id) ON DELETE CASCADE, event_id TEXT NOT NULL REFERENCES mcp_events(event_id) ON DELETE CASCADE,
 handoff_id TEXT NOT NULL REFERENCES handoffs(id) ON DELETE CASCADE, request_hash TEXT NOT NULL, source_key TEXT NOT NULL,
 PRIMARY KEY(grant_id,event_id), UNIQUE(handoff_id), UNIQUE(grant_id,source_key)
);
CREATE TABLE mcp_event_native_causes (
 thread_id TEXT NOT NULL, turn_id TEXT NOT NULL, handoff_id TEXT NOT NULL REFERENCES handoffs(id) ON DELETE CASCADE,
 root_event_id TEXT NOT NULL, hop_count INTEGER NOT NULL, actor_grant_id TEXT NOT NULL, ambiguous INTEGER NOT NULL DEFAULT 0,
 PRIMARY KEY(thread_id,turn_id)
);

-- Native questions are typed source observations, never completed replies.
CREATE TABLE mcp_event_request_sources (
 id TEXT PRIMARY KEY, workstream_id TEXT NOT NULL REFERENCES workstreams(id) ON DELETE CASCADE,
 binding_revision INTEGER NOT NULL, endpoint_id TEXT NOT NULL REFERENCES endpoints(id) ON DELETE CASCADE,
 source_role TEXT NOT NULL, thread_id TEXT NOT NULL, turn_id TEXT NOT NULL,
 raw_request_id TEXT NOT NULL, public_request TEXT NOT NULL,
 created_at INTEGER NOT NULL, resolved_at INTEGER,
 UNIQUE(workstream_id,binding_revision,endpoint_id,turn_id,raw_request_id)
);

CREATE TABLE mcp_event_request_tombstones (
 thread_id TEXT NOT NULL, turn_id TEXT NOT NULL, raw_request_id TEXT NOT NULL,
 resolved_at INTEGER NOT NULL, PRIMARY KEY(thread_id,turn_id,raw_request_id)
);
