-- OBSERVATION_FAILED means the prior native launch never obtained a
-- provider observation.  It remains durable UNKNOWN evidence, but cannot be
-- the one live writer that blocks a later, independently approved handoff.
DROP INDEX dispatch_one_unresolved_per_workstream;
CREATE UNIQUE INDEX dispatch_one_unresolved_per_workstream
 ON handoff_dispatches(workstream_id)
 WHERE phase IN ('CLAIMED','PREPARING','WRITE_INTENT','ACCEPTED')
    OR (phase = 'UNKNOWN' AND last_code IS NOT 'OBSERVATION_FAILED');
