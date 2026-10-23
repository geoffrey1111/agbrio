CREATE TABLE conversation_message_times (
 thread_id TEXT NOT NULL,
 cwd TEXT NOT NULL,
 message_at INTEGER NOT NULL CHECK(message_at > 0),
 PRIMARY KEY(thread_id,cwd)
);
