-- 017 weakened the approved 014 invariant by allowing a separate unresolved
-- writer per destination endpoint and by exempting OBSERVATION_FAILED.  Keep
-- immutable pre-existing evidence intact, but make every workstream a single
-- writer from this point forward.
--
-- Some isolated-preview databases already contain multiple historical UNKNOWN
-- records in a workstream.  A replacement partial UNIQUE index cannot be
-- installed without changing that evidence.  This forward-compatible lock
-- table records those workstreams as occupied and the triggers enforce the
-- original invariant for inserts and all phase transitions.  Thus a legacy
-- conflict is fail-closed: it cannot admit any new writer.
DROP INDEX dispatch_one_unresolved_per_destination_endpoint;

CREATE TABLE dispatch_unresolved_workstream_locks (
    workstream_id TEXT PRIMARY KEY NOT NULL REFERENCES workstreams(id),
    acquired_at INTEGER NOT NULL
);

INSERT INTO dispatch_unresolved_workstream_locks(workstream_id, acquired_at)
SELECT workstream_id, MIN(created_at)
FROM handoff_dispatches
WHERE phase IN ('CLAIMED','PREPARING','WRITE_INTENT','ACCEPTED','UNKNOWN')
GROUP BY workstream_id;

CREATE TRIGGER dispatch_one_unresolved_per_workstream_insert
BEFORE INSERT ON handoff_dispatches
WHEN NEW.phase IN ('CLAIMED','PREPARING','WRITE_INTENT','ACCEPTED','UNKNOWN')
 AND EXISTS (
    SELECT 1 FROM handoff_dispatches
    WHERE workstream_id=NEW.workstream_id
      AND phase IN ('CLAIMED','PREPARING','WRITE_INTENT','ACCEPTED','UNKNOWN')
 )
BEGIN SELECT RAISE(ABORT, 'DISPATCH_WORKSTREAM_WRITER_BUSY'); END;

CREATE TRIGGER dispatch_one_unresolved_per_workstream_update
BEFORE UPDATE OF phase, workstream_id ON handoff_dispatches
WHEN NEW.phase IN ('CLAIMED','PREPARING','WRITE_INTENT','ACCEPTED','UNKNOWN')
 AND EXISTS (
    SELECT 1 FROM handoff_dispatches
    WHERE workstream_id=NEW.workstream_id
      AND handoff_id!=OLD.handoff_id
      AND phase IN ('CLAIMED','PREPARING','WRITE_INTENT','ACCEPTED','UNKNOWN')
 )
BEGIN SELECT RAISE(ABORT, 'DISPATCH_WORKSTREAM_WRITER_BUSY'); END;

CREATE TRIGGER dispatch_unresolved_workstream_lock_insert
AFTER INSERT ON handoff_dispatches
WHEN NEW.phase IN ('CLAIMED','PREPARING','WRITE_INTENT','ACCEPTED','UNKNOWN')
BEGIN
    INSERT OR IGNORE INTO dispatch_unresolved_workstream_locks(workstream_id, acquired_at)
    VALUES(NEW.workstream_id, NEW.created_at);
END;

CREATE TRIGGER dispatch_unresolved_workstream_lock_update
AFTER UPDATE OF phase, workstream_id ON handoff_dispatches
BEGIN
    INSERT OR IGNORE INTO dispatch_unresolved_workstream_locks(workstream_id, acquired_at)
    SELECT NEW.workstream_id, NEW.created_at
    WHERE NEW.phase IN ('CLAIMED','PREPARING','WRITE_INTENT','ACCEPTED','UNKNOWN');
    DELETE FROM dispatch_unresolved_workstream_locks
    WHERE workstream_id=OLD.workstream_id
      AND NOT EXISTS (
          SELECT 1 FROM handoff_dispatches
          WHERE workstream_id=OLD.workstream_id
            AND phase IN ('CLAIMED','PREPARING','WRITE_INTENT','ACCEPTED','UNKNOWN')
      );
END;
