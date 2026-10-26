-- Queue intent is not a physical writer. Preserve every existing row and its
-- payload/receipt identity while allowing several explicit Codex queue intents.
DROP INDEX watch_replies_one_intent;
CREATE UNIQUE INDEX watch_replies_one_writer ON watch_replies(thread_id)
WHERE status IN ('SENDING','UNKNOWN');
