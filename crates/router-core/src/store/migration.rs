//! 014 and 015 are never added to the legacy automatic migration list.
use super::*;
use crate::runtime::PreviewProfile;
use rusqlite::types::ValueRef;
use serde::{Deserialize, Serialize};

const SQL_014: &str = include_str!("../../migrations/014_control_context_handoffs.sql");
const SQL_015: &str = include_str!("../../migrations/015_hybrid_relay_artifacts.sql");
const SQL_016: &str = include_str!("../../migrations/016_dispatch_observation_failed_index.sql");
const SQL_017: &str = include_str!("../../migrations/017_dispatch_destination_writer_index.sql");
const SQL_018: &str = include_str!("../../migrations/018_binding_not_created_state.sql");
const SQL_019: &str =
    include_str!("../../migrations/019_restore_workstream_dispatch_writer_lock.sql");
const SQL_020: &str = include_str!("../../migrations/020_management_bridge_transport.sql");
const SQL_021: &str = include_str!("../../migrations/021_management_bridge_stale_requests.sql");
const SQL_022: &str = include_str!("../../migrations/022_private_owner_rotation_continuity.sql");
const SQL_023: &str = include_str!("../../migrations/023_authenticated_chatgpt_host_scopes.sql");
pub(super) const PREVIEW_SCHEMA: i64 = 23;

#[derive(Deserialize, Serialize)]
struct MaintenanceMarker {
    phase: String,
    from: i64,
    to: i64,
}

fn write_marker(profile: &PreviewProfile, phase: &str, from: i64) -> Result<(), String> {
    let marker = MaintenanceMarker {
        phase: phase.into(),
        from,
        to: PREVIEW_SCHEMA,
    };
    fs::write(
        profile.migration_marker_path(),
        serde_json::to_vec(&marker).map_err(|_| "PREVIEW_MIGRATION_MARKER_INVALID")?,
    )
    .map_err(|_| "PREVIEW_MIGRATION_MARKER_WRITE_FAILED".into())
}

fn marker_state(profile: &PreviewProfile) -> Result<Option<MaintenanceMarker>, String> {
    let path = profile.migration_marker_path();
    if !path.exists() {
        return Ok(None);
    }
    serde_json::from_slice(&fs::read(path).map_err(|_| "PREVIEW_MIGRATION_INTERRUPTED")?)
        .map(Some)
        .map_err(|_| "PREVIEW_MIGRATION_INTERRUPTED".into())
}

pub(super) fn preflight(conn: &Connection, maximum: i64) -> Result<i64, String> {
    let exists: bool = conn.query_row("SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='schema_migrations')", [], |r|r.get(0)).map_err(db_error)?;
    if !exists {
        return Ok(0);
    }
    let version: i64 = conn
        .query_row(
            "SELECT COALESCE(MAX(version),0) FROM schema_migrations",
            [],
            |r| r.get(0),
        )
        .map_err(db_error)?;
    if version > maximum {
        return Err("SCHEMA_TOO_NEW".into());
    }
    Ok(version)
}

impl RouterStore {
    /// Explicit offline maintenance operation; the lease remains owned throughout.
    pub fn initialize_preview(profile: &PreviewProfile) -> Result<(), String> {
        let mut conn = Connection::open(profile.database_path()).map_err(db_error)?;
        conn.execute_batch("PRAGMA foreign_keys=ON; PRAGMA busy_timeout=5000;")
            .map_err(db_error)?;
        let version = preflight(&conn, PREVIEW_SCHEMA)?;
        if let Some(marker) = marker_state(profile)? {
            if marker.phase != "COMPLETED" {
                if marker.phase != "PREPARING"
                    || marker.from != version
                    || marker.to != PREVIEW_SCHEMA
                    || marker.from >= marker.to
                {
                    return Err("PREVIEW_MIGRATION_INTERRUPTED".into());
                }
                check_integrity(&conn)?;
            } else {
                if version != marker.to || marker.from > marker.to || marker.to > PREVIEW_SCHEMA {
                    return Err("PREVIEW_MIGRATION_MARKER_INVALID".into());
                }
                if version == PREVIEW_SCHEMA {
                    check_integrity(&conn)?;
                    return Ok(());
                }
            }
        }
        write_marker(profile, "PREPARING", version)?;
        if version < 14 {
            run_migrations(&mut conn)?;
            upgrade(&mut conn, &profile.backup_path(14), 13, 14, SQL_014, false)?;
        }
        if preflight(&conn, PREVIEW_SCHEMA)? < 15 {
            upgrade(&mut conn, &profile.backup_path(15), 14, 15, SQL_015, false)?;
        }
        if preflight(&conn, PREVIEW_SCHEMA)? < 16 {
            upgrade(&mut conn, &profile.backup_path(16), 15, 16, SQL_016, true)?;
        }
        if preflight(&conn, PREVIEW_SCHEMA)? < 17 {
            upgrade(&mut conn, &profile.backup_path(17), 16, 17, SQL_017, true)?;
        }
        if preflight(&conn, PREVIEW_SCHEMA)? < 18 {
            upgrade(&mut conn, &profile.backup_path(18), 17, 18, SQL_018, true)?;
        }
        if preflight(&conn, PREVIEW_SCHEMA)? < 19 {
            upgrade(&mut conn, &profile.backup_path(19), 18, 19, SQL_019, true)?;
        }
        if preflight(&conn, PREVIEW_SCHEMA)? < 20 {
            upgrade(&mut conn, &profile.backup_path(20), 19, 20, SQL_020, true)?;
        }
        if preflight(&conn, PREVIEW_SCHEMA)? < 21 {
            upgrade(&mut conn, &profile.backup_path(21), 20, 21, SQL_021, true)?;
        }
        if preflight(&conn, PREVIEW_SCHEMA)? < 22 {
            upgrade(&mut conn, &profile.backup_path(22), 21, 22, SQL_022, true)?;
        }
        if preflight(&conn, PREVIEW_SCHEMA)? < 23 {
            upgrade(&mut conn, &profile.backup_path(23), 22, 23, SQL_023, true)?;
        }
        check_integrity(&conn)?;
        write_marker(profile, "COMPLETED", version)?;
        Ok(())
    }

    /// Runtime open never migrates. It consumes the lease used by maintenance.
    pub fn open_preview(profile: PreviewProfile) -> Result<Self, String> {
        let conn = Connection::open_with_flags(
            profile.database_path(),
            rusqlite::OpenFlags::SQLITE_OPEN_READ_WRITE,
        )
        .map_err(db_error)?;
        conn.execute_batch("PRAGMA foreign_keys=ON; PRAGMA busy_timeout=5000;")
            .map_err(db_error)?;
        if let Some(marker) = marker_state(&profile)? {
            if marker.phase != "COMPLETED" || marker.to != PREVIEW_SCHEMA {
                return Err("PREVIEW_MIGRATION_INTERRUPTED".into());
            }
        }
        if preflight(&conn, PREVIEW_SCHEMA)? != PREVIEW_SCHEMA {
            return Err("EXPLICIT_PREVIEW_MIGRATION_REQUIRED".into());
        }
        check_integrity(&conn)?;
        Ok(Self {
            connection: Mutex::new(conn),
            _preview_profile: Some(profile),
        })
    }
}

fn check_integrity(conn: &Connection) -> Result<(), String> {
    let quick: String = conn
        .query_row("PRAGMA quick_check", [], |r| r.get(0))
        .map_err(db_error)?;
    if quick != "ok" {
        return Err("DB_QUICK_CHECK_FAILED".into());
    }
    let result: String = conn
        .query_row("PRAGMA integrity_check", [], |r| r.get(0))
        .map_err(db_error)?;
    if result != "ok" {
        return Err("DB_INTEGRITY_FAILED".into());
    }
    if conn
        .prepare("PRAGMA foreign_key_check")
        .map_err(db_error)?
        .query([])
        .map_err(db_error)?
        .next()
        .map_err(db_error)?
        .is_some()
    {
        return Err("DB_FOREIGN_KEY_FAILED".into());
    }
    Ok(())
}

pub(super) fn require_foreign_keys(conn: &Connection) -> Result<(), String> {
    let enabled: i64 = conn
        .query_row("PRAGMA foreign_keys", [], |row| row.get(0))
        .map_err(db_error)?;
    if enabled != 1 {
        return Err("FOREIGN_KEYS_REQUIRED".into());
    }
    Ok(())
}

/// Hash every original column, ordered by original primary/row identity. Streaming
/// hashes avoid building a second copy of historical texts in memory.
fn legacy_digests(
    conn: &Connection,
    tables: &[(String, Vec<String>)],
) -> Result<Vec<(String, u64, String)>, String> {
    let mut out = Vec::new();
    for (table, columns) in tables {
        let query = format!(
            "SELECT {} FROM \"{}\" ORDER BY rowid",
            columns
                .iter()
                .map(|c| format!("\"{c}\""))
                .collect::<Vec<_>>()
                .join(","),
            table
        );
        let mut stmt = conn.prepare(&query).map_err(db_error)?;
        let mut rows = stmt.query([]).map_err(db_error)?;
        let mut sha = Sha256::new();
        let mut count = 0;
        while let Some(row) = rows.next().map_err(db_error)? {
            count += 1;
            for i in 0..columns.len() {
                match row.get_ref(i).map_err(db_error)? {
                    ValueRef::Null => sha.update([0]),
                    ValueRef::Integer(v) => {
                        sha.update([1]);
                        sha.update(v.to_le_bytes());
                    }
                    ValueRef::Real(v) => {
                        sha.update([2]);
                        sha.update(v.to_bits().to_le_bytes());
                    }
                    ValueRef::Text(v) => {
                        sha.update([3]);
                        sha.update((v.len() as u64).to_le_bytes());
                        sha.update(v);
                    }
                    ValueRef::Blob(v) => {
                        sha.update([4]);
                        sha.update((v.len() as u64).to_le_bytes());
                        sha.update(v);
                    }
                }
            }
        }
        out.push((table.clone(), count, format!("{:x}", sha.finalize())));
    }
    Ok(out)
}

fn upgrade(
    conn: &mut Connection,
    backup: &Path,
    required_version: i64,
    target_version: i64,
    sql: &str,
    allow_existing_unknown: bool,
) -> Result<(), String> {
    require_foreign_keys(conn)?;
    if preflight(conn, PREVIEW_SCHEMA)? != required_version {
        return Err(format!("SCHEMA_{required_version:03}_REQUIRED"));
    }
    let active: i64 = if allow_existing_unknown {
        conn.query_row(
            "SELECT COUNT(*) FROM provider_runs WHERE status IN ('STARTING','RUNNING')",
            [],
            |r| r.get(0),
        )
    } else {
        conn.query_row(
            "SELECT COUNT(*) FROM provider_runs WHERE status IN ('STARTING','RUNNING','UNKNOWN')",
            [],
            |r| r.get(0),
        )
    }
    .map_err(db_error)?;
    // An index-only migration can preserve a durable UNKNOWN dispatch: it
    // performs no provider I/O and does not reinterpret that dispatch.  It
    // must still refuse a genuinely active or pre-dispatch handoff.
    let sending: i64 = if allow_existing_unknown {
        conn.query_row(
            "SELECT COUNT(*) FROM handoffs h
             LEFT JOIN handoff_dispatches d ON d.handoff_id=h.id
             WHERE h.status='APPROVED'
                OR (h.status='SENDING' AND (d.phase IS NULL OR d.phase!='UNKNOWN'))",
            [],
            |r| r.get(0),
        )
    } else {
        conn.query_row(
            "SELECT COUNT(*) FROM handoffs WHERE status IN ('APPROVED','SENDING')",
            [],
            |r| r.get(0),
        )
    }
    .map_err(db_error)?;
    if active + sending != 0 {
        return Err("MIGRATION_UNRESOLVED_WORK".into());
    }
    check_integrity(conn)?;
    if backup.exists() {
        return Err("BACKUP_MUST_NOT_EXIST".into());
    }
    conn.execute("VACUUM INTO ?1", params![backup.to_string_lossy()])
        .map_err(db_error)?;
    let copy = Connection::open(backup).map_err(db_error)?;
    check_integrity(&copy)?;
    if preflight(&copy, PREVIEW_SCHEMA)? != required_version {
        return Err("BACKUP_SCHEMA_MISMATCH".into());
    }
    let digest = file_sha256(backup)?;
    let bytes = fs::metadata(backup).map_err(|e| e.to_string())?.len();
    // Sidecar contains backup identity only, never contents/paths/credentials.
    fs::write(
        backup.with_extension("json"),
        serde_json::to_vec(
            &serde_json::json!({"schema":required_version,"sha256":digest,"bytes":bytes}),
        )
        .map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    let names:Vec<String>=conn.prepare("SELECT name FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%' AND name!='schema_migrations' ORDER BY name").map_err(db_error)?.query_map([],|r|r.get(0)).map_err(db_error)?.collect::<Result<_,_>>().map_err(db_error)?;
    let mut tables = Vec::new();
    for name in names {
        let columns = conn
            .prepare(&format!("PRAGMA table_info(\"{name}\")"))
            .map_err(db_error)?
            .query_map([], |r| r.get(1))
            .map_err(db_error)?
            .collect::<Result<Vec<String>, _>>()
            .map_err(db_error)?;
        tables.push((name, columns));
    }
    let before = legacy_digests(conn, &tables)?;
    conn.execute_batch("PRAGMA foreign_keys=OFF;")
        .map_err(db_error)?;
    let result = (|| {
        let tx = conn
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(db_error)?;
        // SQLite parent-table rebuilds need this offline, pre-BEGIN FK-off
        // window. foreign_key_check below remains mandatory before COMMIT.
        tx.execute_batch(sql).map_err(db_error)?;
        check_integrity(&tx)?;
        if before != legacy_digests(&tx, &tables)? {
            return Err("MIGRATION_LEGACY_CHANGED".into());
        }
        tx.execute(
            "INSERT INTO schema_migrations(version,applied_at) VALUES(?1,?2)",
            params![target_version, now()],
        )
        .map_err(db_error)?;
        tx.commit().map_err(db_error)
    })();
    conn.execute_batch("PRAGMA foreign_keys=ON;")
        .map_err(db_error)?;
    let enabled: i64 = conn
        .query_row("PRAGMA foreign_keys", [], |r| r.get(0))
        .map_err(db_error)?;
    if enabled != 1 {
        return Err("FOREIGN_KEYS_DISABLED".into());
    }
    result?;
    check_integrity(conn)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn assert_fk_on(conn: &Connection) {
        assert_eq!(
            conn.query_row("PRAGMA foreign_keys", [], |row| row.get::<_, i64>(0))
                .unwrap(),
            1
        );
    }
    fn assert_backup_identity(backup: &Path, expected_schema: i64) {
        assert_eq!(
            preflight(&Connection::open(backup).unwrap(), PREVIEW_SCHEMA).unwrap(),
            expected_schema
        );
        let sidecar: serde_json::Value =
            serde_json::from_slice(&fs::read(backup.with_extension("json")).unwrap()).unwrap();
        assert_eq!(sidecar["schema"], expected_schema);
        assert_eq!(sidecar["sha256"], file_sha256(backup).unwrap());
        assert_eq!(sidecar["bytes"], fs::metadata(backup).unwrap().len());
    }

    #[test]
    fn workstream_dispatch_writer_lock_blocks_all_second_unresolved_writers() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE workstreams (id TEXT PRIMARY KEY);
             INSERT INTO workstreams(id) VALUES('legacy'),('fresh'),('observation');
             CREATE TABLE handoff_dispatches (handoff_id TEXT PRIMARY KEY, workstream_id TEXT NOT NULL, endpoint_id TEXT NOT NULL, dispatch_id TEXT NOT NULL, phase TEXT NOT NULL, last_code TEXT, created_at INTEGER NOT NULL);
             CREATE UNIQUE INDEX dispatch_one_unresolved_per_destination_endpoint
              ON handoff_dispatches(workstream_id,endpoint_id)
              WHERE phase IN ('CLAIMED','PREPARING','WRITE_INTENT','ACCEPTED')
                 OR (phase = 'UNKNOWN' AND last_code IS NOT 'OBSERVATION_FAILED');
             INSERT INTO handoff_dispatches VALUES('legacy-a','legacy','chatgpt','legacy-a','UNKNOWN','BRIDGE_ACCEPTANCE_UNKNOWN',1);
             INSERT INTO handoff_dispatches VALUES('legacy-b','legacy','codex','legacy-b','UNKNOWN','BRIDGE_ACCEPTANCE_UNKNOWN',2);",
        )
        .unwrap();
        conn.execute_batch(SQL_019).unwrap();
        conn.execute(
            "INSERT INTO handoff_dispatches VALUES('fresh-a','fresh','chatgpt','fresh-a','CLAIMED',NULL,3)",
            [],
        )
        .unwrap();
        assert!(conn
            .execute(
                "INSERT INTO handoff_dispatches VALUES('fresh-b','fresh','codex','fresh-b','CLAIMED',NULL,4)",
                [],
            )
            .is_err());
        assert!(conn
            .execute(
                "INSERT INTO handoff_dispatches VALUES('legacy-c','legacy','new','legacy-c','CLAIMED',NULL,5)",
                [],
            )
            .is_err());
        conn.execute(
            "UPDATE handoff_dispatches SET phase='TERMINAL' WHERE handoff_id='fresh-a'",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO handoff_dispatches VALUES('fresh-c','fresh','codex','fresh-c','UNKNOWN','OBSERVATION_FAILED',6)",
            [],
        )
        .unwrap();
        assert!(conn
            .execute(
                "INSERT INTO handoff_dispatches VALUES('observation-a','observation','codex','observation-a','UNKNOWN','OBSERVATION_FAILED',7)",
                [],
            )
            .is_ok());
        assert!(conn
            .execute(
                "INSERT INTO handoff_dispatches VALUES('observation-b','observation','chatgpt','observation-b','CLAIMED',NULL,8)",
                [],
            )
            .is_err());
        check_integrity(&conn).unwrap();
    }

    #[test]
    fn management_request_journal_rejects_a_second_source_message_for_the_same_exact_request() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE workstreams (id TEXT PRIMARY KEY);
             CREATE TABLE endpoints (id TEXT PRIMARY KEY);
             INSERT INTO workstreams(id) VALUES('workstream');
             INSERT INTO endpoints(id) VALUES('endpoint');",
        )
        .unwrap();
        conn.execute_batch(SQL_020).unwrap();
        let insert = "INSERT INTO management_bridge_calls(id,owner_principal_key,workstream_id,chatgpt_endpoint_id,conversation_id,source_client_id,source_message_id,source_turn_key,source_user_turn_key,request_id,operation,request_hash,state,created_at,updated_at) VALUES(?1,'owner','workstream','endpoint','conversation','client',?2,'turn','user-turn','7dbe0c6b-64bc-492f-9fbc-b4105d0b19b4','GET_CONTEXT','hash','OBSERVED',1,1)";
        conn.execute(insert, params!["first", "message-one"])
            .unwrap();
        assert!(conn
            .execute(insert, params!["second", "message-two"])
            .is_err());
    }
    #[test]
    fn binding_not_created_is_a_durable_schema_state() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE projects (id TEXT PRIMARY KEY);
             CREATE TABLE workstreams (id TEXT PRIMARY KEY);
             CREATE TABLE control_contexts (id TEXT PRIMARY KEY);
             CREATE TABLE binding_requests (
               id TEXT PRIMARY KEY NOT NULL, context_id TEXT NOT NULL REFERENCES control_contexts(id),
               client_request_id TEXT NOT NULL, request_hash TEXT NOT NULL,
               operation TEXT NOT NULL CHECK(operation IN ('SELECT','CREATE','REPLACE','TAKEOVER','UNBIND')),
               project_id TEXT, workstream_id TEXT, expected_context_revision INTEGER NOT NULL,
               expected_binding_revision INTEGER, root_revision INTEGER, candidate_thread_id TEXT,
               candidate_policy_hash TEXT,
               state TEXT NOT NULL CHECK(state IN ('REVIEW','CREATING','VERIFYING','READY','APPLIED','CANCELLED','UNKNOWN')),
               approved_principal_key TEXT, review_nonce_hash TEXT UNIQUE,
               created_at INTEGER NOT NULL, expires_at INTEGER NOT NULL,
               UNIQUE(context_id,client_request_id)
             );
             CREATE TABLE schema_migrations (version INTEGER PRIMARY KEY, applied_at INTEGER NOT NULL);
             INSERT INTO schema_migrations(version,applied_at) VALUES(17,1);
             INSERT INTO control_contexts(id) VALUES('context');
             INSERT INTO binding_requests(id,context_id,client_request_id,request_hash,operation,expected_context_revision,state,created_at,expires_at)
             VALUES('binding','context','request','hash','CREATE',1,'VERIFYING',1,2);",
        )
        .unwrap();
        conn.execute_batch(SQL_018).unwrap();
        conn.execute(
            "UPDATE binding_requests SET state='NOT_CREATED' WHERE id='binding'",
            [],
        )
        .unwrap();
        assert_eq!(
            conn.query_row(
                "SELECT state FROM binding_requests WHERE id='binding'",
                [],
                |r| r.get::<_, String>(0)
            )
            .unwrap(),
            "NOT_CREATED"
        );
        check_integrity(&conn).unwrap();
    }
    fn history(conn: &Connection) {
        conn.execute_batch("INSERT INTO projects(id,name,created_at,updated_at) VALUES('p','synthetic',1,1);
          INSERT INTO workstreams(id,project_id,name,status,created_at,updated_at) VALUES('w','p','history','ACTIVE',1,1);
          INSERT INTO endpoints(id,workstream_id,provider,external_id,label,status,created_at) VALUES('c','w','CHATGPT','chat','chat','ACTIVE',1),('e','w','CODEX','thread','thread','ACTIVE',1);
          INSERT INTO handoffs(id,workstream_id,source_endpoint_id,destination_endpoint_id,direction,original_text,approved_text,status,payload_hash,created_at,bridge_request_id,attention_acknowledged_at) VALUES('h','w','c','e','CHATGPT_TO_CODEX','original','edited','SENT','hash',1,'bridge',2);
          INSERT INTO provider_runs(id,workstream_id,endpoint_id,provider,origin_handoff_id,external_run_id,status,created_at,updated_at,result_text) VALUES('r','w','e','CODEX','h','turn','COMPLETED',1,1,'old result');
          INSERT INTO handoff_attachments(id,handoff_id,filename,original_path,created_at) VALUES('a','h','fixture.txt','fixture',1);
          INSERT INTO workstream_drafts(workstream_id,text,revision,updated_at) VALUES('w','draft',2,1);
          INSERT INTO codex_feedback_drafts(workstream_id,source_run_id,text,revision,updated_at) VALUES('w','r','feedback',3,1);
          INSERT INTO external_project_links(id,project_id,provider,external_project_id,label,source_kind,created_at,updated_at) VALUES('link','p','CODEX','native-project','label','fixture',1,1);
          INSERT INTO reply_observations(id,workstream_id,endpoint_id,text,observed_at) VALUES('reply','w','c','observed',1);
          INSERT INTO reply_observer_seen_identities(endpoint_id,assistant_identity,seen_at) VALUES('c','reply-id',1);
          INSERT INTO push_subscriptions(fingerprint,status,created_at,updated_at) VALUES('hash','ACTIVE',1,1);").unwrap();
    }
    fn old_db() -> (tempfile::TempDir, Connection) {
        let dir = tempfile::tempdir().unwrap();
        let mut conn = Connection::open(dir.path().join("fixture.db")).unwrap();
        conn.execute_batch("PRAGMA foreign_keys=ON;").unwrap();
        run_migrations(&mut conn).unwrap();
        history(&conn);
        (dir, conn)
    }
    #[test]
    fn upgrade_preserves_history_indexes_fk_and_backup() {
        let (dir, mut conn) = old_db();
        let backup = dir.path().join("backup.db");
        assert_fk_on(&conn);
        upgrade(&mut conn, &backup, 13, 14, SQL_014, false).unwrap();
        assert_fk_on(&conn);
        assert_eq!(preflight(&conn, PREVIEW_SCHEMA).unwrap(), 14);
        assert_backup_identity(&backup, 13);
        for index in [
            "handoffs_workstream_created_at",
            "handoffs_sent_dedup",
            "handoffs_unique_bridge_request_id",
            "handoffs_attention_idx",
        ] {
            assert_eq!(
                conn.query_row(
                    "SELECT COUNT(*) FROM sqlite_master WHERE type='index' AND name=?1",
                    params![index],
                    |r| r.get::<_, i64>(0)
                )
                .unwrap(),
                1
            );
        }
        assert_eq!(conn.query_row("SELECT original_text||':'||approved_text||':'||source_kind FROM handoffs WHERE id='h'",[],|r|r.get::<_,String>(0)).unwrap(),"original:edited:ENDPOINT");
        assert!(conn
            .execute("DELETE FROM handoffs WHERE id='h'", [])
            .is_err());
        assert!(conn
            .execute(
                "UPDATE handoffs SET source_kind='CONTROL_CONTEXT' WHERE id='h'",
                []
            )
            .is_err());
        assert!(conn
            .execute(
                "UPDATE handoffs SET source_context_id='missing' WHERE id='h'",
                []
            )
            .is_err());
        assert!(conn.execute("INSERT INTO control_contexts(id,principal_key,scope_key,key_version,state,created_at,updated_at) VALUES('invalid','p','s',1,'BOUND',1,1)",[]).is_err());
        conn.execute_batch("INSERT INTO control_contexts(id,principal_key,scope_key,key_version,workstream_id,state,created_at,updated_at) VALUES('ctx','principal','scope',1,'w','BOUND',1,1);
          INSERT INTO handoffs(id,workstream_id,source_kind,source_context_id,destination_endpoint_id,direction,original_text,approved_text,status,payload_hash,created_at) VALUES('new','w','CONTROL_CONTEXT','ctx','e','CONTROL_TO_CODEX','draft','draft','READY','digest',1);").unwrap();
        check_integrity(&conn).unwrap();
        let projected = list_handoffs(&conn, "w").unwrap();
        assert_eq!(projected.len(), 2);
        assert!(
            matches!(&projected.iter().find(|h|h.id=="new").unwrap().source,HandoffSource::ControlContext {source_context_id} if source_context_id=="ctx")
        );
        assert_eq!(
            projected
                .iter()
                .find(|h| h.id == "h")
                .unwrap()
                .endpoint_source()
                .unwrap()
                .id,
            "c"
        );
        let store = RouterStore {
            connection: Mutex::new(conn),
            _preview_profile: None,
        };
        assert_eq!(
            store
                .transition_handoff("new", "APPROVED", None)
                .unwrap_err(),
            "CONTROL_CONTEXT_REQUIRES_REVIEW_SERVICE"
        );
        let conn = store.connection.into_inner().unwrap();
        drop(conn);
        check_integrity(&Connection::open(dir.path().join("fixture.db")).unwrap()).unwrap();
        assert!(backup.with_extension("json").exists());
    }
    #[test]
    fn failed_copy_rolls_back_and_restores_fk_without_erasing_backup() {
        let (dir, mut conn) = old_db();
        assert!(upgrade(
            &mut conn,
            &dir.path().join("backup.db"),
            13,
            14,
            &format!("{SQL_014}\nUPDATE handoffs SET approved_text='tampered';"),
            false,
        )
        .is_err());
        assert_eq!(preflight(&conn, 13).unwrap(), 13);
        assert_fk_on(&conn);
        assert_eq!(
            conn.query_row("SELECT approved_text FROM handoffs WHERE id='h'", [], |r| r
                .get::<_, String>(0))
                .unwrap(),
            "edited"
        );
        assert!(dir.path().join("backup.db").exists());
    }
    #[test]
    fn upgrade_fifteen_preserves_014_rows_and_enforces_relay_source_boundaries() {
        let (dir, mut conn) = old_db();
        upgrade(
            &mut conn,
            &dir.path().join("pre-014.db"),
            13,
            14,
            SQL_014,
            false,
        )
        .unwrap();
        conn.execute_batch("INSERT INTO control_contexts(id,principal_key,scope_key,key_version,workstream_id,state,created_at,updated_at) VALUES('ctx','principal','scope',1,'w','BOUND',1,1);
          INSERT INTO handoffs(id,workstream_id,source_kind,source_context_id,destination_endpoint_id,direction,original_text,approved_text,status,payload_hash,created_at) VALUES('control','w','CONTROL_CONTEXT','ctx','e','CONTROL_TO_CODEX','control original','control approved','READY','control-hash',1);
          INSERT INTO provider_runs(id,workstream_id,endpoint_id,provider,origin_handoff_id,status,created_at,updated_at) VALUES('r2','w','e','CODEX','control','FAILED',1,1);
          INSERT INTO handoff_dispatches(handoff_id,run_id,workstream_id,endpoint_id,dispatch_id,adapter_epoch,phase,created_at,updated_at) VALUES('control','r2','w','e','dispatch', 'epoch','TERMINAL',1,1);
          INSERT INTO control_handoff_details(handoff_id,context_id,client_request_id,request_hash,context_revision,binding_revision,root_revision,draft_revision,policy_json,policy_hash) VALUES('control','ctx','client','request',1,1,1,1,'{}','policy');
          INSERT INTO operator_resolutions(id,dispatch_id,client_resolution_id,request_hash,principal_key,context_id,context_revision,binding_revision,expected_dispatch_revision,resulting_dispatch_revision,action,evidence_digest,nonce_hash,old_state_json,new_state_json,reason_code,channel,created_at) VALUES('resolution','dispatch','resolution-client','request','principal','ctx',1,1,0,1,'KEEP_UNKNOWN','evidence','nonce','{}','{}','INSUFFICIENT','BROWSER_REVIEW',1);")
            .unwrap();
        conn.execute(
            "INSERT INTO provider_result_details(run_id,result_state,updated_at) VALUES('r','PENDING',2)",
            [],
        )
        .unwrap();
        let backup = dir.path().join("pre-015.db");
        assert_fk_on(&conn);
        upgrade(&mut conn, &backup, 14, 15, SQL_015, false).unwrap();
        assert_fk_on(&conn);
        assert_eq!(preflight(&conn, PREVIEW_SCHEMA).unwrap(), 15);
        assert_backup_identity(&backup, 14);
        assert_eq!(
            conn.query_row(
                "SELECT terminal_source || ':' || COALESCE(bridge_evidence_id,'') FROM provider_result_details WHERE run_id='r'",
                [],
                |row| row.get::<_, String>(0),
            )
            .unwrap(),
            "NATIVE:"
        );
        assert_eq!(
            conn.query_row("SELECT source_kind || ':' || context_id || ':' || COALESCE(source_endpoint_id,'') FROM operator_resolutions WHERE id='resolution'", [], |row| row.get::<_, String>(0)).unwrap(),
            "CONTROL_CONTEXT:ctx:"
        );
        for table in [
            "browser_endpoint_settings",
            "relay_handoff_details",
            "artifact_materializations",
            "handoff_attachment_details",
            "bridge_dispatch_evidence",
        ] {
            assert_eq!(
                conn.query_row(
                    "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name=?1",
                    params![table],
                    |row| row.get::<_, i64>(0),
                )
                .unwrap(),
                1
            );
        }
        conn.execute(
            "INSERT INTO relay_handoff_details(handoff_id,owner_principal_key,source_message_ref,capture_digest,source_completeness,binding_revision,root_revision,destination_policy_json,destination_policy_hash,draft_revision,envelope_version,manifest_hash,client_request_id,created_at,updated_at) VALUES('h','owner','message','capture','COMPLETE',1,1,'{}','policy',1,'relay-envelope-v1','manifest','request',1,1)",
            [],
        )
        .unwrap();
        assert!(conn.execute("INSERT INTO relay_handoff_details(handoff_id,owner_principal_key,source_message_ref,capture_digest,source_completeness,binding_revision,root_revision,destination_policy_json,destination_policy_hash,draft_revision,envelope_version,manifest_hash,client_request_id,created_at,updated_at) VALUES('control','owner','message','capture','COMPLETE',1,1,'{}','policy',1,'relay-envelope-v1','manifest','control-request',1,1)", []).is_err());
        assert!(conn.execute("INSERT INTO control_handoff_details(handoff_id,context_id,client_request_id,request_hash,context_revision,binding_revision,root_revision,draft_revision,policy_json,policy_hash) VALUES('h','ctx','endpoint-client','request',1,1,1,1,'{}','policy')", []).is_err());
        conn.execute("INSERT INTO artifact_materializations(id,source_endpoint_id,source_message_ref,source_artifact_ref,kind,filename,mime,size,transfer_declared_sha256,actual_sha256,source_hash_status,integrity_status,storage_ref,state,created_at,updated_at) VALUES('artifact','c','message','artifact','DOCUMENT','x.txt','text/plain',1,'hash','hash','DECLARED','VERIFIED','artifacts/artifact/blob','READY',1,1)", []).unwrap();
        assert!(conn.execute("INSERT INTO handoff_attachment_details(attachment_id,handoff_id,artifact_id,ordinal,mime,approved_sha256,approved_size,manifest_entry_json) VALUES('a','control','artifact',0,'text/plain','hash',1,'{}')", []).is_err());
        assert!(conn.execute("INSERT INTO provider_result_details(run_id,result_state,bridge_evidence_id,terminal_source,updated_at) VALUES('r2','PENDING','missing','BRIDGE_TERMINAL',1)", []).is_err());
        assert!(conn
            .execute(
                "INSERT INTO artifact_materializations(id,source_endpoint_id,source_message_ref,source_artifact_ref,kind,filename,mime,size,transfer_declared_sha256,source_hash_status,integrity_status,storage_ref,state,created_at,updated_at) VALUES('bad','c','message','artifact','DOCUMENT','x.txt','text/plain',1,'hash','DECLARED','PENDING','C:/not-allowed','MATERIALIZING',1,1)",
                [],
            )
            .is_err());
        check_integrity(&conn).unwrap();
    }
    #[test]
    fn unresolved_or_newer_schema_refuses_migration() {
        let (dir, mut conn) = old_db();
        conn.execute("UPDATE provider_runs SET status='UNKNOWN'", [])
            .unwrap();
        assert_eq!(
            upgrade(
                &mut conn,
                &dir.path().join("backup.db"),
                13,
                14,
                SQL_014,
                false
            )
            .unwrap_err(),
            "MIGRATION_UNRESOLVED_WORK"
        );
        assert!(!dir.path().join("backup.db").exists());
        assert_fk_on(&conn);
        conn.execute("INSERT INTO schema_migrations VALUES(99,1)", [])
            .unwrap();
        assert_eq!(
            preflight(&conn, PREVIEW_SCHEMA).unwrap_err(),
            "SCHEMA_TOO_NEW"
        );
    }
    #[test]
    fn migration_fk_violation_and_backup_collision_fail_closed() {
        let (dir, mut conn) = old_db();
        let backup = dir.path().join("backup.db");
        assert!(upgrade(
            &mut conn,
            &backup,
            13,
            14,
            &format!("{SQL_014}\nINSERT INTO handoff_attachments(id,handoff_id,filename,original_path,created_at) VALUES('orphan','missing','x','x',1);"),
            false,
        ).is_err());
        assert_eq!(preflight(&conn, 13).unwrap(), 13);
        assert_fk_on(&conn);
        check_integrity(&conn).unwrap();
        assert!(backup.exists());
        assert_eq!(
            upgrade(&mut conn, &backup, 13, 14, SQL_014, false).unwrap_err(),
            "BACKUP_MUST_NOT_EXIST"
        );
        assert_fk_on(&conn);
    }
    #[test]
    fn preview_profile_excludes_other_writer_and_legacy_auto_open() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("mcp-preview");
        assert!(PreviewProfile::acquire(dir.path().join("data")).is_err());
        let profile = PreviewProfile::acquire(&root).unwrap();
        assert!(PreviewProfile::acquire(&root).is_err());
        RouterStore::initialize_preview(&profile).unwrap();
        let store = RouterStore::open_preview(profile).unwrap();
        assert!(RouterStore::open_at(root.join("router.db")).is_err());
        drop(store);
        RouterStore::open_preview(PreviewProfile::acquire(&root).unwrap()).unwrap();
    }
    #[test]
    fn preview_maintenance_marker_is_idempotent_and_interruption_fails_closed() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("mcp-preview");
        let profile = PreviewProfile::acquire(&root).unwrap();
        RouterStore::initialize_preview(&profile).unwrap();
        let marker: MaintenanceMarker =
            serde_json::from_slice(&fs::read(profile.migration_marker_path()).unwrap()).unwrap();
        assert_eq!(
            (marker.phase.as_str(), marker.to),
            ("COMPLETED", PREVIEW_SCHEMA)
        );
        // A completed maintenance run is a read-only no-op when invoked again.
        RouterStore::initialize_preview(&profile).unwrap();
        drop(profile);
        fs::write(
            root.join("schema-maintenance.json"),
            format!(r#"{{"phase":"PREPARING","from":19,"to":{PREVIEW_SCHEMA}}}"#),
        )
        .unwrap();
        match RouterStore::open_preview(PreviewProfile::acquire(&root).unwrap()) {
            Err(error) => assert_eq!(error, "PREVIEW_MIGRATION_INTERRUPTED"),
            Ok(_) => panic!("interrupted maintenance must not open a writer"),
        }
    }
}
