-- A request that predates the current Bridge epoch but was never observed is
-- an explicit non-execution tombstone, not a fabricated assistant message.
CREATE TABLE management_bridge_stale_requests (
 chatgpt_endpoint_id TEXT NOT NULL REFERENCES endpoints(id),
 conversation_id TEXT NOT NULL,
 request_id TEXT NOT NULL,
 reason TEXT NOT NULL CHECK(reason IN ('HISTORY_UNOBSERVABLE')),
 recorded_at INTEGER NOT NULL,
 PRIMARY KEY(chatgpt_endpoint_id, request_id)
);
