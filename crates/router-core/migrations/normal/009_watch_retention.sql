-- Delivery dedupe/audit outlives the retained notification original.
CREATE TABLE watch_deliveries_retained (
 event_sequence INTEGER NOT NULL,
 channel TEXT NOT NULL REFERENCES watch_delivery_channels(channel),
 notification_key TEXT NOT NULL,
 status TEXT NOT NULL CHECK(status IN ('PENDING','SENDING','SENT','FAILED','UNKNOWN','SKIPPED')),
 attempts INTEGER NOT NULL DEFAULT 0,
 retry_at INTEGER NOT NULL DEFAULT 0,
 error_code TEXT,
 updated_at INTEGER NOT NULL,
 PRIMARY KEY(event_sequence,channel),
 UNIQUE(notification_key,channel)
);
INSERT INTO watch_deliveries_retained SELECT * FROM watch_deliveries;
DROP TABLE watch_deliveries;
ALTER TABLE watch_deliveries_retained RENAME TO watch_deliveries;
