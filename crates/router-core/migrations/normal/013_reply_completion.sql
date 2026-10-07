ALTER TABLE reply_observations ADD COLUMN completed_at INTEGER;
ALTER TABLE reply_observations ADD COLUMN completion_checked INTEGER NOT NULL DEFAULT 0;
