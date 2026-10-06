ALTER TABLE handoffs ADD COLUMN bridge_request_id TEXT;
ALTER TABLE handoffs ADD COLUMN bridge_turn_key TEXT;
ALTER TABLE handoffs ADD COLUMN bridge_correlation_observed_at INTEGER;

CREATE UNIQUE INDEX handoffs_unique_bridge_request_id
    ON handoffs(bridge_request_id)
    WHERE bridge_request_id IS NOT NULL;
