//! Private adapter projection/journal in the existing Router SQLite authority.
use super::*;
use crate::claude_code::{CompleteReply, Event, Session};
use serde::Deserialize;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Pairing {
    pub connection_id: String,
    pub token: String,
    pub expires_at: i64,
    pub writes_enabled: bool,
}
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Catalog {
    pub sessions: Vec<Session>,
    pub complete: bool,
    pub writes_enabled: bool,
}
#[derive(Serialize)]
#[serde(rename_all="camelCase")]
pub struct ConnectionSummary {pub connection_id:String,pub expires_at:i64,pub revoked:bool,pub session_count:i64}

pub(super) fn migrate(c: &mut Connection, path: &Path) -> Result<(), String> {
    if c.query_row("SELECT EXISTS(SELECT 1 FROM router_feature_migrations WHERE key='claude-code-adapter-v1')", [], |r| r.get::<_, bool>(0)).map_err(db_error)? { return Ok(()); }
    let backup = path.with_extension(format!("before-claude-code-{}.db", id()));
    c.execute("VACUUM INTO ?1", params![backup.to_string_lossy().as_ref()]).map_err(db_error)?;
    let saved = Connection::open(&backup).map_err(db_error)?;
    if saved.query_row("PRAGMA integrity_check", [], |r| r.get::<_, String>(0)).map_err(db_error)? != "ok" { return Err("CLAUDE_MIGRATION_BACKUP_INVALID".into()); }
    drop(saved);
    // Preserve every column, index, trigger and row. No existing migration edited.
    c.execute_batch("PRAGMA foreign_keys=OFF;").map_err(db_error)?;
    let result = (|| {
        let tx = c.transaction_with_behavior(TransactionBehavior::Immediate).map_err(db_error)?;
        // Cross-table lifecycle triggers reference endpoints while it is absent.
        // Restore their exact SQL in this transaction before committing.
        let triggers = {
            let mut s=tx.prepare("SELECT name,sql FROM sqlite_master WHERE type='trigger' AND sql IS NOT NULL").map_err(db_error)?;
            let rows=s.query_map([],|r|Ok((r.get::<_,String>(0)?,r.get::<_,String>(1)?))).map_err(db_error)?.collect::<Result<Vec<_>,_>>().map_err(db_error)?;
            rows
        };
        for (name,_) in &triggers { tx.execute_batch(&format!("DROP TRIGGER \"{}\"",name.replace('"',"\"\""))).map_err(db_error)?; }
        for table in ["endpoints", "handoffs"] {
            let ddl: String = tx.query_row("SELECT sql FROM sqlite_master WHERE type='table' AND name=?1", [table], |r| r.get(0)).map_err(db_error)?;
            let old = if table == "endpoints" { "'CHATGPT', 'CODEX'" } else { "'CHATGPT_TO_CODEX', 'CODEX_TO_CHATGPT', 'CODEX_TO_CODEX'" };
            let new = if table == "endpoints" { "'CHATGPT', 'CODEX', 'CLAUDE_CODE'" } else { "'CHATGPT_TO_CODEX', 'CODEX_TO_CHATGPT', 'CODEX_TO_CODEX', 'CHATGPT_TO_CLAUDE_CODE', 'CODEX_TO_CLAUDE_CODE', 'CLAUDE_CODE_TO_CHATGPT', 'CLAUDE_CODE_TO_CODEX', 'CLAUDE_CODE_TO_CLAUDE_CODE'" };
            if !ddl.contains(old) { return Err("CLAUDE_MIGRATION_SCHEMA_UNEXPECTED".into()); }
            let objects = {
                let mut s = tx.prepare("SELECT sql FROM sqlite_master WHERE tbl_name=?1 AND type='index' AND sql IS NOT NULL").map_err(db_error)?;
                let rows = s.query_map([table], |r| r.get::<_, String>(0)).map_err(db_error)?.collect::<Result<Vec<_>, _>>().map_err(db_error)?;
                rows
            };
            let count: i64 = tx.query_row(&format!("SELECT count(*) FROM {table}"), [], |r| r.get(0)).map_err(db_error)?;
            let temp = format!("{table}_claude_upgrade");
            let columns=ddl.find('(').ok_or("CLAUDE_MIGRATION_SCHEMA_UNEXPECTED")?;
            let replacement = format!("CREATE TABLE {temp}{}", &ddl[columns..]).replace(old, new);
            tx.execute_batch(&replacement).map_err(db_error)?;
            tx.execute_batch(&format!("INSERT INTO {temp} SELECT * FROM {table}; DROP TABLE {table}; ALTER TABLE {temp} RENAME TO {table};")).map_err(db_error)?;
            for sql in objects { tx.execute_batch(&sql).map_err(db_error)?; }
            if tx.query_row(&format!("SELECT count(*) FROM {table}"), [], |r| r.get::<_, i64>(0)).map_err(db_error)? != count { return Err("CLAUDE_MIGRATION_ROW_COUNT_CHANGED".into()); }
        }
        tx.execute_batch("CREATE TABLE claude_connections(id TEXT PRIMARY KEY, token_hash TEXT NOT NULL UNIQUE, expires_at INTEGER NOT NULL, revoked_at INTEGER);
          CREATE TABLE claude_sessions(session_id TEXT PRIMARY KEY, connection_id TEXT NOT NULL REFERENCES claude_connections(id), instance_id TEXT NOT NULL, sequence INTEGER NOT NULL, cwd TEXT NOT NULL, version TEXT NOT NULL, state TEXT NOT NULL, turn_id TEXT, checked_at INTEGER NOT NULL, lease_until INTEGER NOT NULL, latest_reply TEXT);
          CREATE TABLE claude_event_journal(event_id TEXT PRIMARY KEY, connection_id TEXT NOT NULL REFERENCES claude_connections(id), session_id TEXT NOT NULL, instance_id TEXT NOT NULL, sequence INTEGER NOT NULL, digest TEXT NOT NULL, created_at INTEGER NOT NULL);") .map_err(db_error)?;
        for (_,sql) in triggers { tx.execute_batch(&sql).map_err(db_error)?; }
        if tx.prepare("PRAGMA foreign_key_check").map_err(db_error)?.exists([]).map_err(db_error)? { return Err("CLAUDE_MIGRATION_FOREIGN_KEY_INVALID".into()); }
        tx.execute("INSERT INTO router_feature_migrations(key,applied_at) VALUES('claude-code-adapter-v1',?1)", [now()]).map_err(db_error)?;
        tx.commit().map_err(db_error)
    })();
    c.execute_batch("PRAGMA foreign_keys=ON;").map_err(db_error)?;
    result
}
pub(super) fn auth(c: &Connection, token: &str, at: i64) -> Result<String, String> {
    if token.len() != 64 || !token.bytes().all(|b| b.is_ascii_hexdigit()) { return Err("CLAUDE_CONNECTION_UNAUTHORIZED".into()); }
    let hash = format!("{:x}", Sha256::digest(token.as_bytes()));
    c.query_row("SELECT id FROM claude_connections WHERE token_hash=?1 AND revoked_at IS NULL AND expires_at>?2", params![hash, at], |r| r.get(0)).optional().map_err(db_error)?.ok_or("CLAUDE_CONNECTION_UNAUTHORIZED".into())
}
fn session_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<Session> {
    let reply: Option<String> = r.get(9)?;
    Ok(Session { title: None, session_id: r.get(0)?, instance_id: r.get(1)?, cwd: r.get(2)?, version: r.get(3)?, state: r.get(4)?, turn_id: r.get(5)?, checked_at: r.get(6)?, lease_until: r.get(7)?, latest_reply: reply.map(|v| serde_json::from_str(&v)).transpose().map_err(|e| rusqlite::Error::FromSqlConversionFailure(9, rusqlite::types::Type::Text, Box::new(e)))? })
}
const SESSION_FIELDS: &str = "session_id,instance_id,cwd,version,state,turn_id,checked_at,lease_until,sequence,latest_reply";
impl RouterStore {
    pub fn claude_connections(&self)->Result<Vec<ConnectionSummary>,String>{
        self.with_connection(|c|{
            let mut s=c.prepare("SELECT a.id,a.expires_at,a.revoked_at IS NOT NULL,(SELECT count(*) FROM claude_sessions s WHERE s.connection_id=a.id) FROM claude_connections a ORDER BY a.expires_at DESC,a.id LIMIT 100").map_err(db_error)?;
            let rows=s.query_map([],|r|Ok(ConnectionSummary{connection_id:r.get(0)?,expires_at:r.get(1)?,revoked:r.get(2)?,session_count:r.get(3)?})).map_err(db_error)?.collect::<Result<Vec<_>,_>>().map_err(db_error)?;
            Ok(rows)
        })
    }
    /// Only an explicit native setup action creates this limited adapter credential.
    pub fn create_claude_connection(&self) -> Result<Pairing, String> {
        let token = format!("{}{}", Uuid::new_v4().simple(), Uuid::new_v4().simple());
        let connection_id = id(); let expires_at = now() + 30 * 24 * 3600 * 1000;
        self.with_connection(|c| { c.execute("INSERT INTO claude_connections(id,token_hash,expires_at) VALUES(?1,?2,?3)", params![connection_id,format!("{:x}",Sha256::digest(token.as_bytes())),expires_at]).map_err(db_error)?; Ok(Pairing {connection_id, token, expires_at,writes_enabled:false}) })
    }
    pub fn revoke_claude_connection(&self, connection: &str) -> Result<(), String> {
        self.with_connection(|c| { c.execute("UPDATE claude_connections SET revoked_at=?2 WHERE id=?1",params![connection,now()]).map_err(db_error)?; Ok(()) })
    }
    pub fn claude_catalog(&self) -> Result<Catalog, String> {
        self.with_connection(|c| {
            let fields=SESSION_FIELDS.replace("latest_reply","NULL AS latest_reply");
            let mut s=c.prepare(&format!("SELECT {fields} FROM claude_sessions s JOIN claude_connections a ON a.id=s.connection_id WHERE a.revoked_at IS NULL AND a.expires_at>?1 AND s.state!='SESSION_END' ORDER BY s.checked_at DESC,s.session_id LIMIT 201")).map_err(db_error)?;
            let mut sessions=s.query_map([now()],session_row).map_err(db_error)?.collect::<Result<Vec<_>,_>>().map_err(db_error)?;
            let complete=sessions.len()<=200; sessions.truncate(200);
            Ok(Catalog {sessions,complete,writes_enabled:false})
        })
    }
    pub fn claude_session(&self, session: &str) -> Result<Session, String> {
        self.with_connection(|c| {
            c.query_row(&format!("SELECT {SESSION_FIELDS} FROM claude_sessions s JOIN claude_connections a ON a.id=s.connection_id WHERE s.session_id=?1 AND a.revoked_at IS NULL AND a.expires_at>?2"),params![session,now()],session_row).optional().map_err(db_error)?.ok_or("CLAUDE_SESSION_UNAVAILABLE".into())
        })
    }
    /// Applies the state the session's own transcript shows. Returns true when a new
    /// completed reply appeared (a different turn, or the same turn newly finished).
    /// A session the plugin has not registered yet gets a row with no instance or lease.
    pub fn apply_claude_transcript(&self, session: &str, cwd: &str, state: &str, turn: Option<&str>, reply: Option<&CompleteReply>) -> Result<bool, String> {
        if !matches!(state, "RUNNING"|"COMPLETE"|"INTERRUPTED"|"EMPTY"|"FAILED") || !crate::claude_code::native_id(session) { return Err("CLAUDE_TRANSCRIPT_STATE_INVALID".into()); }
        let reply = reply.map(serde_json::to_string).transpose().map_err(|_| "CLAUDE_REPLY_INVALID")?;
        self.with_connection(|c| {
            let tx = c.transaction_with_behavior(TransactionBehavior::Immediate).map_err(db_error)?;
            let prior: Option<(String, Option<String>)> = tx.query_row("SELECT state,turn_id FROM claude_sessions WHERE session_id=?1", [session], |r| Ok((r.get(0)?, r.get(1)?))).optional().map_err(db_error)?;
            if prior.as_ref().is_some_and(|(s, t)| s == state && t.as_deref() == turn) { return Ok(false); }
            match prior {
                Some(_) => { tx.execute("UPDATE claude_sessions SET state=?2,turn_id=?3,latest_reply=CASE WHEN ?2='COMPLETE' THEN ?4 ELSE NULL END WHERE session_id=?1", params![session, state, turn, reply]).map_err(db_error)?; }
                None => {
                    let Some(connection): Option<String> = tx.query_row("SELECT id FROM claude_connections WHERE revoked_at IS NULL AND expires_at>?1 ORDER BY expires_at DESC LIMIT 1", [now()], |r| r.get(0)).optional().map_err(db_error)? else { return Ok(false) };
                    tx.execute("INSERT INTO claude_sessions(session_id,connection_id,instance_id,sequence,cwd,version,state,turn_id,checked_at,lease_until,latest_reply) VALUES(?1,?2,'',0,?3,'',?4,?5,?6,0,CASE WHEN ?4='COMPLETE' THEN ?7 ELSE NULL END)", params![session, connection, cwd, state, turn, now(), reply]).map_err(db_error)?;
                }
            }
            tx.commit().map_err(db_error)?;
            Ok(state == "COMPLETE")
        })
    }
    pub fn ingest_claude_event(&self, token: &str, event: &Event) -> Result<bool, String> { self.ingest_claude_event_owned(token, event, false) }
    /// `transcript_owned`: the session's transcript is the authority for state, turn
    /// and reply. Plugin shutdown expires its lease, not the native conversation.
    pub fn ingest_claude_event_owned(&self, token: &str, event: &Event, transcript_owned: bool) -> Result<bool, String> {
        let at=now(); event.validate(at)?;
        // The Adapter alone resolves native roots, not Domain titles or URL guesses.
        crate::claude_code::root_key(&event.cwd).ok_or("CLAUDE_ROOT_UNAVAILABLE")?;
        let digest=event.digest()?;
        self.with_connection(|c| {
            let tx=c.transaction_with_behavior(TransactionBehavior::Immediate).map_err(db_error)?;
            let connection=auth(&tx,token,at)?;
            let old:Option<(String,String)>=tx.query_row("SELECT connection_id,digest FROM claude_event_journal WHERE event_id=?1",[&event.event_id],|r|Ok((r.get(0)?,r.get(1)?))).optional().map_err(db_error)?;
            if let Some(old)=old { return if old==(connection,digest) {Ok(false)} else {Err("CLAUDE_EVENT_REPLAY_CONFLICT".into())}; }
            let prior:Option<(String,String,i64,String,i64)>=tx.query_row("SELECT connection_id,instance_id,sequence,cwd,lease_until FROM claude_sessions WHERE session_id=?1",[&event.session_id],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?))).optional().map_err(db_error)?;
            if let Some((owner,instance,sequence,cwd,lease))=&prior {
                let old_authorized=tx.query_row("SELECT revoked_at IS NULL AND expires_at>?2 FROM claude_connections WHERE id=?1",params![owner,at],|r|r.get::<_,bool>(0)).map_err(db_error)?;
                // A row the Host created from the transcript (no instance yet) is claimed by the first plugin.
                if owner!=&connection && !instance.is_empty() && (old_authorized || event.kind!="SESSION_START") { return Err("CLAUDE_SESSION_ALREADY_OWNED".into()); }
                // A spare session may move into its real project; any other root change is refused.
                // For a transcript-owned session the session id is the identity; its working
                // directory may move during a turn (for example into a subfolder).
                if !transcript_owned && !crate::claude_code::same_root(cwd,&event.cwd) && !crate::claude_code::spare_root(cwd) { return Err("CLAUDE_ROOT_CHANGED".into()); }
                if instance!=&event.instance_id {
                    // A delayed shutdown must never claim or terminate a newer publisher.
                    if event.kind=="SESSION_END" { return Err("CLAUDE_SESSION_INSTANCE_CHANGED".into()); }
                    // A reloaded plugin is a new instance. It waits for the old instance's
                    // lease to lapse, then any event re-registers it (its start event may
                    // have been acknowledged as rejected while the old lease was valid).
                    if *lease>at && old_authorized { return Err("CLAUDE_SESSION_INSTANCE_CHANGED".into()); }
                } else if event.sequence<=*sequence { return Err("CLAUDE_EVENT_OUT_OF_ORDER".into()); }
            } else if event.kind!="SESSION_START" { return Err("CLAUDE_SESSION_NOT_REGISTERED".into()); }
            if event.kind=="COMPLETE" && !transcript_owned {
                let (state,turn):(String,Option<String>)=tx.query_row("SELECT state,turn_id FROM claude_sessions WHERE session_id=?1",[&event.session_id],|r|Ok((r.get(0)?,r.get(1)?))).map_err(db_error)?;
                if !matches!(state.as_str(),"RUNNING"|"WAITING_DECISION") || turn!=event.turn_id {return Err("CLAUDE_COMPLETE_TURN_MISMATCH".into());}
            }
            let reply=if event.kind=="COMPLETE" { Some(serde_json::to_string(&CompleteReply {turn_id:event.turn_id.clone().unwrap(),text:event.text.clone().unwrap(),completed_at:event.completed_at.unwrap()}).map_err(|_|"CLAUDE_REPLY_INVALID")?) } else { None };
            let state=if event.kind=="HEARTBEAT" { let prior=tx.query_row("SELECT state FROM claude_sessions WHERE session_id=?1",[&event.session_id],|r|r.get::<_,String>(0)).map_err(db_error)?; if prior=="SESSION_END" { "SESSION_START".into() } else { prior } } else {event.kind.clone()};
            tx.execute("INSERT INTO claude_sessions(session_id,connection_id,instance_id,sequence,cwd,version,state,turn_id,checked_at,lease_until,latest_reply) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11) ON CONFLICT(session_id) DO UPDATE SET connection_id=excluded.connection_id,cwd=CASE WHEN ?13 AND NOT (claude_sessions.cwd LIKE '%.claude%spares%spare-%') THEN claude_sessions.cwd ELSE excluded.cwd END,instance_id=excluded.instance_id,sequence=excluded.sequence,version=excluded.version,state=CASE WHEN ?13 AND ?12!='SESSION_END' THEN CASE WHEN claude_sessions.state='SESSION_END' OR claude_sessions.instance_id!=excluded.instance_id THEN 'SESSION_START' ELSE claude_sessions.state END ELSE excluded.state END,turn_id=CASE WHEN ?13 OR ?12='HEARTBEAT' THEN claude_sessions.turn_id ELSE excluded.turn_id END,checked_at=excluded.checked_at,lease_until=excluded.lease_until,latest_reply=CASE WHEN ?13 THEN claude_sessions.latest_reply WHEN ?12 IN ('RUNNING','SESSION_START','SESSION_END') THEN NULL ELSE COALESCE(excluded.latest_reply,claude_sessions.latest_reply) END",params![event.session_id,connection,event.instance_id,event.sequence,event.cwd,event.version,state,event.turn_id,at,if event.kind=="SESSION_END"{at}else{at+45_000},reply,event.kind,transcript_owned]).map_err(db_error)?;
            tx.execute("INSERT INTO claude_event_journal(event_id,connection_id,session_id,instance_id,sequence,digest,created_at) VALUES(?1,?2,?3,?4,?5,?6,?7)",params![event.event_id,connection,event.session_id,event.instance_id,event.sequence,digest,at]).map_err(db_error)?;
            // Bounded dedupe history; older sequence fences survive pruning/restart.
            tx.execute("DELETE FROM claude_event_journal WHERE session_id=?1 AND sequence<?2 AND instance_id=?3",params![event.session_id,event.sequence-256,event.instance_id]).map_err(db_error)?;
            tx.commit().map_err(db_error)?; Ok(true)
        })
    }
}

#[cfg(test)] mod tests;
