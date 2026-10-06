-- V0-014: retain the final local mutation check as temporal evidence.
-- Existing rows deliberately remain NULL; no historical dispatch hash is inferred.
ALTER TABLE handoff_attachments ADD COLUMN send_sha256 TEXT NULL;
ALTER TABLE handoff_attachments ADD COLUMN send_verified_at INTEGER NULL;
