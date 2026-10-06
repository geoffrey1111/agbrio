CREATE TABLE watch_replies (
 id TEXT PRIMARY KEY NOT NULL,
 thread_id TEXT NOT NULL,
 cwd TEXT NOT NULL,
 generation INTEGER NOT NULL,
 source_sequence INTEGER,
 expected_turn_id TEXT,
 mode TEXT NOT NULL CHECK(mode IN ('SEND','QUEUE','STEER')),
 text TEXT NOT NULL,
 options_json TEXT NOT NULL,
 payload_hash TEXT NOT NULL,
 status TEXT NOT NULL CHECK(status IN ('QUEUED','SENDING','SENT','UNKNOWN','ACKNOWLEDGED','CANCELLED','FAILED')),
 turn_id TEXT,
 error_code TEXT,
 created_at INTEGER NOT NULL,
 updated_at INTEGER NOT NULL
);
CREATE INDEX watch_replies_thread ON watch_replies(thread_id,created_at);
CREATE UNIQUE INDEX watch_replies_one_intent ON watch_replies(thread_id) WHERE status IN ('QUEUED','SENDING','UNKNOWN');
CREATE TABLE watch_reply_cancellations (id TEXT PRIMARY KEY NOT NULL,thread_id TEXT NOT NULL,created_at INTEGER NOT NULL);
