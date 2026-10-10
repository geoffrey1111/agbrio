//! Deleted Workstreams release native claims; historical Endpoint IDs never move.
use super::*;

const KEY: &str = "endpoint-trash-claims-v1";

pub(super) fn claimed_elsewhere(c: &Connection, workstream: &str, provider: &str, external: &str) -> Result<bool, String> {
    c.query_row("SELECT EXISTS(SELECT 1 FROM endpoints e JOIN workstreams w ON w.id=e.workstream_id WHERE e.provider=?1 AND e.external_id=?2 AND e.workstream_id!=?3 AND w.trashed_at IS NULL)", params![provider, external, workstream], |r| r.get(0)).map_err(db_error)
}

pub(super) fn require_restore(c: &Connection, workstream: &str) -> Result<(), String> {
    let conflict: bool = c.query_row("SELECT EXISTS(SELECT 1 FROM endpoints e JOIN endpoints other ON other.provider=e.provider AND other.external_id=e.external_id JOIN workstreams w ON w.id=other.workstream_id WHERE e.workstream_id=?1 AND other.workstream_id!=?1 AND w.trashed_at IS NULL)", [workstream], |r| r.get(0)).map_err(db_error)?;
    if conflict { return Err("BRIDGE_NATIVE_TARGET_ALREADY_BOUND".into()); }
    Ok(())
}

pub(super) fn migrate(c: &mut Connection, path: &Path) -> Result<(), String> {
    if c.query_row("SELECT EXISTS(SELECT 1 FROM router_feature_migrations WHERE key=?1)", [KEY], |r| r.get::<_, bool>(0)).map_err(db_error)? { return Ok(()); }
    migration::require_foreign_keys(c)?;
    let ddl: String = c.query_row("SELECT sql FROM sqlite_master WHERE type='table' AND name='endpoints'", [], |r| r.get(0)).map_err(db_error)?;
    let old = "UNIQUE(provider, external_id)";
    if ddl.matches(old).count() != 1 || !ddl.starts_with("CREATE TABLE endpoints") { return Err("ENDPOINT_CLAIM_SCHEMA_UNEXPECTED".into()); }
    let objects = {
        let mut q = c.prepare("SELECT sql FROM sqlite_master WHERE tbl_name='endpoints' AND type IN ('index','trigger') AND sql IS NOT NULL").map_err(db_error)?;
        let objects = q.query_map([], |r| r.get::<_, String>(0)).map_err(db_error)?.collect::<rusqlite::Result<Vec<_>>>().map_err(db_error)?;
        objects
    };
    let backup = path.with_extension(format!("before-endpoint-claims-{}.db", id()));
    c.execute("VACUUM INTO ?1", params![backup.to_string_lossy().as_ref()]).map_err(db_error)?;
    let saved = Connection::open(&backup).map_err(db_error)?;
    if saved.query_row("PRAGMA integrity_check", [], |r| r.get::<_, String>(0)).map_err(db_error)? != "ok" { return Err("ENDPOINT_CLAIM_BACKUP_INVALID".into()); }
    drop(saved);
    // As in the existing role migration, disable FK enforcement only around a
    // transactional parent-table rebuild; child rows are neither dropped nor moved.
    c.execute_batch("PRAGMA foreign_keys=OFF;").map_err(db_error)?;
    let result = (|| {
        let tx = c.transaction_with_behavior(TransactionBehavior::Immediate).map_err(db_error)?;
        let count: i64 = tx.query_row("SELECT COUNT(*) FROM endpoints", [], |r| r.get(0)).map_err(db_error)?;
        tx.execute_batch(&ddl.replacen("CREATE TABLE endpoints", "CREATE TABLE endpoints_claim_upgrade", 1).replace(old, "UNIQUE(workstream_id, provider, external_id)")).map_err(db_error)?;
        tx.execute_batch("INSERT INTO endpoints_claim_upgrade SELECT * FROM endpoints ORDER BY rowid; DROP TABLE endpoints; ALTER TABLE endpoints_claim_upgrade RENAME TO endpoints;").map_err(db_error)?;
        for sql in objects { tx.execute_batch(&sql).map_err(db_error)?; }
        tx.execute_batch("CREATE TRIGGER endpoint_claims_live_insert BEFORE INSERT ON endpoints WHEN EXISTS(SELECT 1 FROM workstreams WHERE id=NEW.workstream_id AND trashed_at IS NULL) AND EXISTS(SELECT 1 FROM endpoints e JOIN workstreams w ON w.id=e.workstream_id WHERE e.provider=NEW.provider AND e.external_id=NEW.external_id AND e.workstream_id!=NEW.workstream_id AND w.trashed_at IS NULL) BEGIN SELECT RAISE(ABORT,'BRIDGE_NATIVE_TARGET_ALREADY_BOUND'); END;
            CREATE TRIGGER endpoint_claims_live_update BEFORE UPDATE OF workstream_id,provider,external_id ON endpoints WHEN EXISTS(SELECT 1 FROM workstreams WHERE id=NEW.workstream_id AND trashed_at IS NULL) AND EXISTS(SELECT 1 FROM endpoints e JOIN workstreams w ON w.id=e.workstream_id WHERE e.id!=NEW.id AND e.provider=NEW.provider AND e.external_id=NEW.external_id AND e.workstream_id!=NEW.workstream_id AND w.trashed_at IS NULL) BEGIN SELECT RAISE(ABORT,'BRIDGE_NATIVE_TARGET_ALREADY_BOUND'); END;
            CREATE TRIGGER endpoint_claims_restore BEFORE UPDATE OF trashed_at ON workstreams WHEN OLD.trashed_at IS NOT NULL AND NEW.trashed_at IS NULL AND EXISTS(SELECT 1 FROM endpoints e JOIN endpoints other ON other.provider=e.provider AND other.external_id=e.external_id JOIN workstreams w ON w.id=other.workstream_id WHERE e.workstream_id=NEW.id AND other.workstream_id!=NEW.id AND w.trashed_at IS NULL) BEGIN SELECT RAISE(ABORT,'BRIDGE_NATIVE_TARGET_ALREADY_BOUND'); END;").map_err(db_error)?;
        if tx.query_row("SELECT COUNT(*) FROM endpoints", [], |r| r.get::<_, i64>(0)).map_err(db_error)? != count { return Err("ENDPOINT_CLAIM_ROW_COUNT_CHANGED".into()); }
        if tx.prepare("PRAGMA foreign_key_check").map_err(db_error)?.exists([]).map_err(db_error)? { return Err("ENDPOINT_CLAIM_FOREIGN_KEY_INVALID".into()); }
        tx.execute("INSERT INTO router_feature_migrations(key,applied_at) VALUES(?1,?2)", params![KEY, now()]).map_err(db_error)?;
        tx.commit().map_err(db_error)
    })();
    c.execute_batch("PRAGMA foreign_keys=ON;").map_err(db_error)?;
    migration::require_foreign_keys(c)?;
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::role_bridge::RoleBindingInput;
    fn binding(name: &str) -> RoleBindingInput { RoleBindingInput { provider: "CODEX".into(), external_id: name.into(), label: name.into(), cwd: Some("D:\\fixture".into()) } }

    #[test]
    fn endpoint_claim_migration_preserves_populated_legacy_history_and_reopens() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("router.db");
        let s = RouterStore::open_at(&path).unwrap();
        let p = s.create_project("fixture".into(), None).unwrap();
        let w = s.create_workstream(&p.id, "old".into()).unwrap();
        let b = s.bind_role_bridge(&w.id, 0, binding("legacy-source"), binding("legacy-target")).unwrap();
        let source = b.decision.as_ref().unwrap().endpoint.id.clone();
        s.record_reply_observation(&w.id, &source, Some("old-identity"), "Retained source", None).unwrap();
        let obs = s.reply_observations_for_workstream(&w.id).unwrap().remove(0);
        let h = s.prepare_role_handoff(&w.id, "DECISION", &obs.id, "Retained text").unwrap();
        let before = serde_json::to_value(s.role_handoff(&h.id).unwrap()).unwrap();
        // Reconstruct the previous endpoint constraint on this disposable DB.
        // All child data and endpoint columns stay populated for the real upgrade.
        s.with_connection(|c| {
            let ddl: String = c.query_row("SELECT sql FROM sqlite_master WHERE name='endpoints'", [], |r| r.get(0)).map_err(db_error)?;
            let legacy = ddl.replace("CREATE TABLE \"endpoints\"", "CREATE TABLE endpoints").replace("UNIQUE(workstream_id, provider, external_id)", "UNIQUE(provider, external_id)");
            let objects = {
                let mut q = c.prepare("SELECT sql FROM sqlite_master WHERE tbl_name='endpoints' AND type='index' AND sql IS NOT NULL").map_err(db_error)?;
                let rows = q.query_map([], |r| r.get::<_, String>(0)).map_err(db_error)?.collect::<rusqlite::Result<Vec<_>>>().map_err(db_error)?;
                rows
            };
            c.execute_batch("PRAGMA foreign_keys=OFF; BEGIN IMMEDIATE; DROP TRIGGER endpoint_claims_restore; CREATE TEMP TABLE saved_fixture_endpoints AS SELECT * FROM endpoints; DROP TABLE endpoints;").map_err(db_error)?;
            c.execute_batch(&legacy).map_err(db_error)?;
            c.execute_batch("INSERT INTO endpoints SELECT * FROM saved_fixture_endpoints; DROP TABLE saved_fixture_endpoints;").map_err(db_error)?;
            for sql in objects { c.execute_batch(&sql).map_err(db_error)?; }
            c.execute("DELETE FROM router_feature_migrations WHERE key=?1", [KEY]).map_err(db_error)?;
            c.execute_batch("COMMIT; PRAGMA foreign_keys=ON;").map_err(db_error)
        }).unwrap();
        drop(s);
        let migrated = RouterStore::open_at(&path).unwrap();
        assert_eq!(serde_json::to_value(migrated.role_handoff(&h.id).unwrap()).unwrap(), before);
        assert_eq!(serde_json::to_value(migrated.role_bridge(&w.id).unwrap()).unwrap(), serde_json::to_value(b).unwrap());
        assert_eq!(migrated.reply_observations_for_workstream(&w.id).unwrap()[0].id, obs.id);
        migrated.with_connection(|c| {
            assert!(!c.prepare("PRAGMA foreign_key_check").unwrap().exists([]).unwrap());
            assert_eq!(c.query_row("PRAGMA foreign_keys", [], |r| r.get::<_, i64>(0)).unwrap(), 1);
            Ok(())
        }).unwrap();
        drop(migrated);
        RouterStore::open_at(&path).unwrap();
    }

    #[test]
    fn endpoint_claim_sql_constraints_reject_restore_and_insert_bypasses() {
        let dir = tempfile::tempdir().unwrap();
        let s = RouterStore::open_at(dir.path().join("router.db")).unwrap();
        let p = s.create_project("fixture".into(), None).unwrap();
        let a = s.create_workstream(&p.id, "a".into()).unwrap();
        let b = s.create_workstream(&p.id, "b".into()).unwrap();
        s.bind_role_bridge(&a.id, 0, binding("claim-source"), binding("claim-target")).unwrap();
        s.trash_workstream(&a.id).unwrap();
        s.bind_role_bridge(&b.id, 0, binding("claim-source"), binding("claim-target")).unwrap();
        let c = s.create_workstream(&p.id, "c".into()).unwrap();
        s.with_connection(|db| {
            assert!(db.execute("UPDATE workstreams SET trashed_at=NULL WHERE id=?1", [&a.id]).is_err());
            assert!(db.execute("INSERT INTO endpoints(id,workstream_id,provider,external_id,label,status,created_at) VALUES('bypass',?1,'CODEX','claim-source','fixture','ACTIVE',1)", [&c.id]).is_err());
            assert!(!db.prepare("PRAGMA foreign_key_check").unwrap().exists([]).unwrap());
            Ok(())
        }).unwrap();
    }
}
