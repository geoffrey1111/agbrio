-- A Workstream may have one unresolved native writer and one unresolved
-- browser writer, because their physical destination endpoints differ.  An
-- UNKNOWN browser write must still block every later write to that same
-- ChatGPT endpoint; it must not block an independently approved Codex run.
DROP INDEX dispatch_one_unresolved_per_workstream;
CREATE UNIQUE INDEX dispatch_one_unresolved_per_destination_endpoint
 ON handoff_dispatches(workstream_id, endpoint_id)
 WHERE phase IN ('CLAIMED','PREPARING','WRITE_INTENT','ACCEPTED')
    OR (phase = 'UNKNOWN' AND last_code IS NOT 'OBSERVATION_FAILED');
