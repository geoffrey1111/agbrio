-- Narrow, local audit state for the Browser-Bridge management transport.
-- This is not a Handoff, ProviderRun, transcript, or task queue.
CREATE TABLE management_bridge_calls (
 id TEXT PRIMARY KEY NOT NULL,
 owner_principal_key TEXT NOT NULL,
 workstream_id TEXT NOT NULL REFERENCES workstreams(id),
 chatgpt_endpoint_id TEXT NOT NULL REFERENCES endpoints(id),
 conversation_id TEXT NOT NULL,
 source_client_id TEXT NOT NULL,
 source_message_id TEXT NOT NULL,
 source_turn_key TEXT NOT NULL,
 source_user_turn_key TEXT NOT NULL,
 request_id TEXT NOT NULL,
 operation TEXT NOT NULL CHECK(operation IN ('GET_CONTEXT')),
 request_hash TEXT NOT NULL,
 state TEXT NOT NULL CHECK(state IN ('OBSERVED','EXECUTED','RETURNING','RETURNED','WAITING_SAFE_RETURN','UNKNOWN','REJECTED','CONSUMED')),
 result_json TEXT,
 result_user_turn_key TEXT,
 consumed_message_id TEXT,
 created_at INTEGER NOT NULL,
 updated_at INTEGER NOT NULL,
 UNIQUE(chatgpt_endpoint_id, source_message_id),
 UNIQUE(owner_principal_key, chatgpt_endpoint_id, request_id),
 UNIQUE(result_user_turn_key)
);
CREATE INDEX management_bridge_calls_endpoint_state ON management_bridge_calls(chatgpt_endpoint_id,state,updated_at);
CREATE TABLE management_bridge_cursors (
 chatgpt_endpoint_id TEXT PRIMARY KEY NOT NULL REFERENCES endpoints(id),
 stream_epoch TEXT NOT NULL,
 sequence INTEGER NOT NULL CHECK(sequence>=0),
 updated_at INTEGER NOT NULL
);
