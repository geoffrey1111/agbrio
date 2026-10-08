-- Additive: legacy grants keep their existing brief-based authority.
ALTER TABLE assistant_grants ADD COLUMN approval_mode TEXT NOT NULL DEFAULT 'BRIEF_RULES'
 CHECK(approval_mode IN ('BRIEF_RULES','CONVERSATION_REVIEW'));
-- A selected earlier result remains exact; revalidate against the source head
-- observed when it was reviewed, instead of requiring it to be the newest text.
ALTER TABLE assistant_drafts ADD COLUMN review_head_id TEXT;
UPDATE assistant_drafts SET review_head_id=observation_id;
