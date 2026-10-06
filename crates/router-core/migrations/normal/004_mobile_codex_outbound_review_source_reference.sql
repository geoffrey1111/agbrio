-- V1 reviews created before this migration retained the source Endpoint and
-- text, but not the exact observation/result record that was selected.  Keep
-- historical drafts readable without inventing a source, and require every
-- new review to preserve one exact source reference for restart-safe revisit.
ALTER TABLE mobile_codex_outbound_reviews
ADD COLUMN source_reference_id TEXT NOT NULL DEFAULT '';
