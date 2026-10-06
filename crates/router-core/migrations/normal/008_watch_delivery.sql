CREATE TABLE watch_delivery_channels (
 channel TEXT PRIMARY KEY CHECK(channel IN ('WEB','WINDOWS','EMAIL')),
 enabled INTEGER NOT NULL DEFAULT 0 CHECK(enabled IN (0,1)),
 cursor INTEGER NOT NULL DEFAULT 0
);
INSERT INTO watch_delivery_channels(channel) VALUES ('WEB'),('WINDOWS'),('EMAIL');
CREATE TABLE watch_delivery_settings (
 id INTEGER PRIMARY KEY CHECK(id=1),
 host_id TEXT NOT NULL,
 account TEXT NOT NULL DEFAULT '',
 recipient TEXT NOT NULL DEFAULT ''
);
CREATE TABLE watch_deliveries (
 event_sequence INTEGER NOT NULL REFERENCES codex_watch_events(sequence),
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
