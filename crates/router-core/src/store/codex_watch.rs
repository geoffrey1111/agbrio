//! Independent read-only subscriptions. They never bind an Endpoint or claim a writer.
use super::*;
use serde::Deserialize;

pub const NOTIFICATION_RETENTION_LIMIT: i64 = 20;

fn prune_events(tx: &rusqlite::Transaction<'_>) -> Result<usize, String> {
    let cutoff: Option<i64> = tx
        .query_row(
            "SELECT sequence FROM codex_watch_events ORDER BY sequence DESC LIMIT 1 OFFSET ?1",
            [NOTIFICATION_RETENTION_LIMIT - 1],
            |r| r.get(0),
        )
        .optional()
        .map_err(db_error)?;
    let Some(cutoff) = cutoff else { return Ok(0) };
    // Preserve semantic send intent even when its old original is discarded.
    // A previously claimed/in-flight send retains its own materialized event.
    tx.execute("UPDATE watch_deliveries SET status='SKIPPED',error_code='NOTIFICATION_EXPIRED',updated_at=?2 WHERE event_sequence<?1 AND status IN ('PENDING','FAILED')",params![cutoff,now()]).map_err(db_error)?;
    let removed=tx.execute("DELETE FROM codex_watch_events WHERE sequence<?1", [cutoff]).map_err(db_error)?;
    tx.execute("DELETE FROM watch_removed_items WHERE kind='EVENT' AND NOT EXISTS(SELECT 1 FROM codex_watch_events WHERE CAST(sequence AS TEXT)=watch_removed_items.id)",[]).map_err(db_error)?;
    Ok(removed)
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct WatchSnapshot {
    pub state: String,
    pub turn_id: Option<String>,
    pub item_id: Option<String>,
    pub text: String,
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CodexWatch {
    pub thread_id: String,
    pub label: String,
    pub cwd: String,
    pub enabled: bool,
    pub generation: i64,
    pub snapshot: WatchSnapshot,
    pub checked_at: i64,
    pub error_code: Option<String>,
    pub retry_after: i64,
    pub send_count: i64,
}
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WatchEvent {
    pub sequence: i64,
    pub thread_id: String,
    pub label: String,
    pub cwd: String,
    pub snapshot: WatchSnapshot,
    pub observed_at: i64,
    pub seen_at:Option<i64>,
}
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WatchFeed {
    pub events: Vec<WatchEvent>,
    pub next_cursor: i64,
    pub has_more: bool,
    pub hidden_sequences:Vec<i64>,
}
fn encoded(snapshot: &WatchSnapshot) -> Result<String, String> {
    if !matches!(
        snapshot.state.as_str(),
        "IDLE"
            | "RUNNING"
            | "ACTION_REQUIRED"
            | "RESULT_READY"
            | "FAILED"
            | "INTERRUPTED"
            | "INCOMPLETE"
            | "UNKNOWN"
            | "RESULT_PENDING"
    ) || snapshot.text.len() > MAX_REPLY_OBSERVATION_BYTES
        || [&snapshot.turn_id, &snapshot.item_id]
            .into_iter()
            .flatten()
            .any(|id| id.is_empty() || id.len() > 256)
    {
        return Err("WATCH_SNAPSHOT_INVALID".into());
    }
    serde_json::to_string(snapshot).map_err(|_| "WATCH_SNAPSHOT_INVALID".into())
}
fn watch_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<CodexWatch> {
    let raw: String = row.get(5)?;
    let snapshot = serde_json::from_str(&raw).map_err(|e| {
        rusqlite::Error::FromSqlConversionFailure(5, rusqlite::types::Type::Text, Box::new(e))
    })?;
    Ok(CodexWatch {
        thread_id: row.get(0)?,
        label: row.get(1)?,
        cwd: row.get(2)?,
        enabled: row.get(3)?,
        generation: row.get(4)?,
        snapshot,
        checked_at: row.get(6)?,
        error_code: row.get(7)?,
        retry_after: row.get(8)?,
        send_count: if row.as_ref().column_count()>9 {row.get(9)?}else{0},
    })
}
/// A bound Bridge is already an exact conversation authority. Derive its reply
/// context without registering a separate watch or consuming the watch limit.
/// Negative generation fingerprints pin Bridge/revision/endpoint/cwd; they cannot
/// collide with standalone watch generations, which are positive. Keep the
/// fingerprint within 52 bits so desktop/PWA JSON numbers remain exact.
pub(super) fn chat_context(c:&Connection,id:&str)->Result<CodexWatch,String>{
 let binding:Option<(String,String,String,String,i64)>=c.query_row("SELECT e.id,e.label,d.cwd,w.id,w.binding_revision FROM endpoints e JOIN workstreams w ON w.id=e.workstream_id JOIN endpoint_role_details d ON d.endpoint_id=e.id WHERE e.provider='CODEX' AND e.external_id=?1 AND e.status='ACTIVE' AND w.trashed_at IS NULL AND w.archived_at IS NULL AND d.cwd IS NOT NULL ORDER BY w.id,e.id LIMIT 1",[id],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?))).optional().map_err(db_error)?;
 if let Some((endpoint,label,cwd,bridge,revision))=binding{
  if !Path::new(&cwd).is_absolute(){return Err("WATCH_PROJECT_UNAVAILABLE".into());}
  let hash=length_prefixed_hash(&[bridge.as_bytes(),revision.to_string().as_bytes(),endpoint.as_bytes(),cwd.as_bytes()]);
  let generation=-(i64::from_str_radix(&hash[..13],16).map_err(|_|"REPLY_CONTEXT_INVALID")?+1);
  return Ok(CodexWatch{thread_id:id.into(),label,cwd,enabled:true,generation,snapshot:WatchSnapshot{state:"UNKNOWN".into(),turn_id:None,item_id:None,text:String::new()},checked_at:0,error_code:None,retry_after:0,send_count:0});
 }
 c.query_row("SELECT thread_id,label,cwd,enabled,generation,snapshot_json,checked_at,error_code,retry_after FROM codex_watches WHERE thread_id=?1 AND NOT EXISTS(SELECT 1 FROM watch_removed_items WHERE kind='WATCH' AND id=codex_watches.thread_id)",[id],watch_row).optional().map_err(db_error)?.ok_or("WATCH_NOT_FOUND".into())
}
impl RouterStore {
    /// Reading a notification acknowledges this exact chat through the selected
    /// sequence, including older retained events not loaded on the client yet.
    /// Newer events and other chats stay unread; originals and watches remain.
    pub fn mark_codex_watch_event_seen(&self,sequence:i64)->Result<(),String>{
        if sequence<=0{return Err("WATCH_SEQUENCE_INVALID".into());}
        self.with_connection(|c|{c.execute("INSERT OR IGNORE INTO watch_seen_events(sequence,seen_at) SELECT sequence,?2 FROM codex_watch_events WHERE sequence=?1",params![sequence,now()]).map_err(db_error)?;Ok(())})
    }
    pub fn acknowledge_codex_watch_event(&self,sequence:i64)->Result<(),String>{
        if sequence<=0{return Err("WATCH_SEQUENCE_INVALID".into());}
        self.with_connection(|c|{
            let tx=c.transaction_with_behavior(TransactionBehavior::Immediate).map_err(db_error)?;
            tx.execute("INSERT OR IGNORE INTO watch_removed_items(kind,id,removed_at) SELECT 'EVENT',CAST(sequence AS TEXT),?2 FROM codex_watch_events WHERE sequence<=?1 AND thread_id=(SELECT thread_id FROM codex_watch_events WHERE sequence=?1)",params![sequence,now()]).map_err(db_error)?;
            tx.commit().map_err(db_error)
        })
    }
    /// Reversible local list removal; never deletes/resumes/stops a provider thread.
    pub fn set_watch_item_removed(&self,kind:&str,id:&str,removed:bool)->Result<(),String>{
        if !matches!(kind,"WATCH"|"EVENT")||id.is_empty()||id.len()>256{return Err("WATCH_REMOVAL_INVALID".into());}
        if kind=="EVENT"&&id.parse::<i64>().ok().filter(|v|*v>0).is_none(){return Err("WATCH_REMOVAL_INVALID".into());}
        self.with_connection(|c|{
            let tx=c.transaction_with_behavior(TransactionBehavior::Immediate).map_err(db_error)?;
            let exists:bool=if kind=="WATCH"{tx.query_row("SELECT EXISTS(SELECT 1 FROM codex_watches WHERE thread_id=?1)",[id],|r|r.get(0)).map_err(db_error)?}else{tx.query_row("SELECT EXISTS(SELECT 1 FROM codex_watch_events WHERE sequence=?1)",[id],|r|r.get(0)).map_err(db_error)?};
            if !exists{return Err("WATCH_ITEM_NOT_FOUND".into());}
            if removed{
                let enabled:Option<bool>=if kind=="WATCH"{Some(tx.query_row("SELECT enabled FROM codex_watches WHERE thread_id=?1",[id],|r|r.get(0)).map_err(db_error)?)}else{None};
                let changed=tx.execute("INSERT OR IGNORE INTO watch_removed_items(kind,id,was_enabled,removed_at) VALUES(?1,?2,?3,?4)",params![kind,id,enabled,now()]).map_err(db_error)?;
                if kind=="WATCH"&&changed>0{tx.execute("UPDATE codex_watches SET enabled=0,generation=generation+1 WHERE thread_id=?1",[id]).map_err(db_error)?;}
            }else{
                if kind=="WATCH"{
                    let prior:Option<bool>=tx.query_row("SELECT was_enabled FROM watch_removed_items WHERE kind='WATCH' AND id=?1",[id],|r|r.get(0)).optional().map_err(db_error)?;
                    if let Some(enabled)=prior{
                        let count:i64=tx.query_row("SELECT COUNT(*) FROM codex_watches WHERE enabled=1 AND thread_id!=?1",[id],|r|r.get(0)).map_err(db_error)?;
                        if enabled&&count>=20{return Err("WATCH_LIMIT_REACHED".into());}
                        tx.execute("UPDATE codex_watches SET enabled=?2,generation=generation+1,checked_at=0 WHERE thread_id=?1",params![id,enabled]).map_err(db_error)?;
                    }
                }
                tx.execute("DELETE FROM watch_removed_items WHERE kind=?1 AND id=?2",params![kind,id]).map_err(db_error)?;
            }
            tx.commit().map_err(db_error)?;Ok(())
        })
    }
    pub fn prune_codex_watch_events(&self) -> Result<usize, String> {
        self.with_connection(|c| {
            let tx = c
                .transaction_with_behavior(TransactionBehavior::Immediate)
                .map_err(db_error)?;
            let removed = prune_events(&tx)?;
            tx.commit().map_err(db_error)?;
            Ok(removed)
        })
    }
    /// The Host has verified metadata and taken the baseline before this transaction.
    pub fn enable_codex_watch(
        &self,
        id: &str,
        label: &str,
        cwd: &str,
        baseline: &WatchSnapshot,
    ) -> Result<(), String> {
        if id.is_empty()
            || id.len() > 256
            || label.is_empty()
            || label.len() > 500
            || !Path::new(cwd).is_absolute()
        {
            return Err("WATCH_IDENTITY_INVALID".into());
        }
        let raw = encoded(baseline)?;
        self.with_connection(|c| {
            let bound:bool=c.query_row("SELECT EXISTS(SELECT 1 FROM endpoints e JOIN workstreams w ON w.id=e.workstream_id WHERE e.provider='CODEX' AND e.external_id=?1 AND e.status='ACTIVE' AND w.trashed_at IS NULL AND w.archived_at IS NULL)",[id],|r|r.get(0)).map_err(db_error)?;if bound{return Err("WATCH_ALREADY_IN_BRIDGE".into());}
            let count:i64=c.query_row("SELECT COUNT(*) FROM codex_watches WHERE enabled=1 AND thread_id!=?1",[id],|r|r.get(0)).map_err(db_error)?;
            if count>=20 {return Err("WATCH_LIMIT_REACHED".into());}
            // Repeated enable of an active subscription does not reset its cursor.
            c.execute("INSERT INTO codex_watches(thread_id,label,cwd,enabled,snapshot_json,checked_at) VALUES(?1,?2,?3,1,?4,?5) ON CONFLICT(thread_id) DO UPDATE SET label=excluded.label,cwd=excluded.cwd,enabled=1,generation=codex_watches.generation+1,snapshot_json=excluded.snapshot_json,checked_at=excluded.checked_at,error_code=NULL,retry_after=0 WHERE codex_watches.enabled=0", params![id,label,cwd,raw,now()]).map_err(db_error)?;
            c.execute("DELETE FROM watch_removed_items WHERE kind='WATCH' AND id=?1",[id]).map_err(db_error)?;
            Ok(())
        })
    }
    pub fn pause_codex_watch(&self, id: &str) -> Result<(), String> {
        self.with_connection(|c| {
            if c.execute(
                "UPDATE codex_watches SET enabled=0,generation=generation+1 WHERE thread_id=?1",
                [id],
            )
            .map_err(db_error)?
                == 0
            {
                return Err("WATCH_NOT_FOUND".into());
            }
            Ok(())
        })
    }
    pub fn registered_codex_watch(&self,id:&str)->Result<CodexWatch,String>{self.with_connection(|c|c.query_row("SELECT thread_id,label,cwd,enabled,generation,snapshot_json,checked_at,error_code,retry_after FROM codex_watches WHERE thread_id=?1",[id],watch_row).optional().map_err(db_error)?.ok_or("WATCH_CONVERSATION_NOT_REGISTERED".into()))}
    pub fn removed_watch_items(&self)->Result<Vec<Value>,String>{self.with_connection(|c|{
        let mut statement=c.prepare("SELECT kind,id,removed_at,(SELECT generation FROM codex_watches WHERE thread_id=watch_removed_items.id) FROM watch_removed_items ORDER BY removed_at DESC LIMIT 100").map_err(db_error)?;
        let out=statement.query_map([],|r|Ok(serde_json::json!({"kind":r.get::<_,String>(0)?,"id":r.get::<_,String>(1)?,"removedAt":r.get::<_,i64>(2)?,"generation":r.get::<_,Option<i64>>(3)?}))).map_err(db_error)?.collect::<Result<Vec<_>,_>>().map_err(db_error)?;Ok(out)
    })}
    pub fn bridge_codex_thread_ids(&self)->Result<Vec<String>,String>{self.with_connection(|c|{let mut q=c.prepare("SELECT DISTINCT e.external_id FROM endpoints e JOIN workstreams w ON w.id=e.workstream_id WHERE e.provider='CODEX' AND e.status='ACTIVE' AND w.trashed_at IS NULL AND w.archived_at IS NULL").map_err(db_error)?;let rows=q.query_map([],|r|r.get(0)).map_err(db_error)?.collect::<Result<Vec<_>,_>>().map_err(db_error)?;Ok(rows)})}
    pub fn codex_chat_context(&self,id:&str)->Result<CodexWatch,String>{self.with_connection(|c|chat_context(c,id))}
    pub fn codex_watches(&self) -> Result<Vec<CodexWatch>, String> {
        self.with_connection(|c| { let mut s=c.prepare("SELECT thread_id,label,cwd,enabled,generation,snapshot_json,checked_at,error_code,retry_after,(SELECT COUNT(*) FROM watch_replies r WHERE r.thread_id=codex_watches.thread_id AND r.source_sequence IS NULL AND r.generation>0 AND r.status!='FAILED' AND (r.mode='QUEUE' OR r.status!='QUEUED') AND NOT EXISTS(SELECT 1 FROM assistant_actions a WHERE a.id=r.id)) AS send_count FROM codex_watches WHERE NOT EXISTS(SELECT 1 FROM watch_removed_items WHERE kind='WATCH' AND id=codex_watches.thread_id) AND NOT EXISTS(SELECT 1 FROM endpoints e JOIN workstreams w ON w.id=e.workstream_id WHERE e.provider='CODEX' AND e.external_id=codex_watches.thread_id AND e.status='ACTIVE' AND w.trashed_at IS NULL AND w.archived_at IS NULL) ORDER BY send_count DESC,thread_id").map_err(db_error)?; let rows=s.query_map([],watch_row).map_err(db_error)?; rows.collect::<Result<Vec<_>,_>>().map_err(db_error) })
    }
    /// Generation prevents a read begun before Pause/Resume from writing a stale event.
    pub fn record_codex_watch(
        &self,
        id: &str,
        generation: i64,
        snapshot: &WatchSnapshot,
    ) -> Result<bool, String> {
        let raw = encoded(snapshot)?;
        self.with_connection(|c| {
            let tx=c.transaction_with_behavior(TransactionBehavior::Immediate).map_err(db_error)?;
            let old=tx.query_row("SELECT thread_id,label,cwd,enabled,generation,snapshot_json,checked_at,error_code,retry_after FROM codex_watches WHERE thread_id=?1",[id],watch_row).optional().map_err(db_error)?;
            let Some(old)=old.filter(|w|w.enabled && w.generation==generation) else {return Ok(false);};
            let changed=old.snapshot!=*snapshot;
            if changed {tx.execute("INSERT INTO codex_watch_events(thread_id,label,cwd,snapshot_json,observed_at) VALUES(?1,?2,?3,?4,?5)",params![id,old.label,old.cwd,raw,now()]).map_err(db_error)?;}
            tx.execute("UPDATE codex_watches SET snapshot_json=?2,checked_at=?3,error_code=NULL,retry_after=0 WHERE thread_id=?1",params![id,raw,now()]).map_err(db_error)?;
            if changed {prune_events(&tx)?;}
            tx.commit().map_err(db_error)?;Ok(changed)
        })
    }
    pub fn codex_watch_unavailable(&self, id: &str, generation: i64) -> Result<(), String> {
        self.with_connection(|c|{c.execute("UPDATE codex_watches SET error_code='WATCH_READ_UNAVAILABLE',retry_after=?3 WHERE thread_id=?1 AND generation=?2 AND enabled=1",params![id,generation,now()+30_000]).map_err(db_error)?;Ok(())})
    }
    /// Feed summaries are bounded. Fetch an exact sequence separately for the original text.
    pub fn codex_watch_feed(&self, after: i64) -> Result<WatchFeed, String> {
        if after < 0 {
            return Err("WATCH_CURSOR_INVALID".into());
        }
        self.with_connection(|c| {
            let mut s=c.prepare("SELECT sequence,thread_id,label,cwd,snapshot_json,observed_at,(SELECT seen_at FROM watch_seen_events WHERE watch_seen_events.sequence=codex_watch_events.sequence) FROM codex_watch_events WHERE sequence>?1 AND NOT EXISTS(SELECT 1 FROM watch_removed_items WHERE kind='EVENT' AND id=CAST(codex_watch_events.sequence AS TEXT)) ORDER BY sequence LIMIT 21").map_err(db_error)?;
            let mut events=s.query_map([after], event_row).map_err(db_error)?.collect::<Result<Vec<_>,_>>().map_err(db_error)?;
            let has_more=events.len()>20;events.truncate(20);
            for e in &mut events {e.snapshot.text=e.snapshot.text.chars().take(400).collect();}
            let max:i64=c.query_row("SELECT COALESCE(MAX(sequence),0) FROM codex_watch_events",[],|r|r.get(0)).map_err(db_error)?;
            let next_cursor=if has_more{events.last().map(|e|e.sequence).unwrap_or(after)}else{after.max(max)};
            let mut hidden=c.prepare("SELECT CAST(id AS INTEGER) FROM watch_removed_items WHERE kind='EVENT' ORDER BY CAST(id AS INTEGER)").map_err(db_error)?;
            let hidden_sequences=hidden.query_map([],|r|r.get(0)).map_err(db_error)?.collect::<Result<Vec<_>,_>>().map_err(db_error)?;
            Ok(WatchFeed{events,next_cursor,has_more,hidden_sequences})
        })
    }
    pub fn codex_watch_event(&self, sequence: i64) -> Result<WatchEvent, String> {
        self.with_connection(|c| c.query_row("SELECT sequence,thread_id,label,cwd,snapshot_json,observed_at,(SELECT seen_at FROM watch_seen_events WHERE watch_seen_events.sequence=codex_watch_events.sequence) FROM codex_watch_events WHERE sequence=?1",[sequence],event_row).optional().map_err(db_error)?.ok_or("WATCH_EVENT_NOT_FOUND".into()))
    }
}
pub(super) fn event_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<WatchEvent> {
    let raw: String = row.get(4)?;
    let snapshot = serde_json::from_str(&raw).map_err(|e| {
        rusqlite::Error::FromSqlConversionFailure(4, rusqlite::types::Type::Text, Box::new(e))
    })?;
    Ok(WatchEvent {
        sequence: row.get(0)?,
        thread_id: row.get(1)?,
        label: row.get(2)?,
        cwd: row.get(3)?,
        snapshot,
        observed_at: row.get(5)?,
        seen_at:row.get(6)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]fn reading_acknowledges_only_the_exact_chat_through_selected_event_and_persists(){
        let d=tempfile::tempdir().unwrap();let path=d.path().join("read.db");let s=RouterStore::open_at(&path).unwrap();let cwd=d.path().to_str().unwrap();
        for id in ["a","b"]{s.enable_codex_watch(id,id,cwd,&snapshot("IDLE","baseline","baseline")).unwrap();}
        for (id,text) in [("a","old"),("b","other chat"),("a","selected"),("a","newer")]{s.record_codex_watch(id,1,&snapshot("RESULT_READY",text,text)).unwrap();}
        let selected=s.codex_watch_feed(0).unwrap().events.into_iter().find(|e|e.snapshot.text=="selected").unwrap().sequence;
        s.acknowledge_codex_watch_event(selected).unwrap();s.acknowledge_codex_watch_event(selected).unwrap();
        drop(s);let s=RouterStore::open_at(path).unwrap();let feed=s.codex_watch_feed(0).unwrap();
        assert_eq!(feed.events.len(),2);assert!(feed.events.iter().any(|e|e.snapshot.text=="other chat"));assert!(feed.events.iter().any(|e|e.snapshot.text=="newer"));
        assert_eq!(s.codex_watch_event(selected).unwrap().snapshot.text,"selected");assert!(s.codex_watches().unwrap().iter().all(|w|w.enabled));
        s.acknowledge_codex_watch_event(99999).unwrap();assert!(s.acknowledge_codex_watch_event(0).is_err());
    }
    fn snapshot(state: &str, turn: &str, text: &str) -> WatchSnapshot {
        WatchSnapshot {
            state: state.into(),
            turn_id: Some(turn.into()),
            item_id: Some("item".into()),
            text: text.into(),
        }
    }
    #[test]
    fn baseline_restart_dedupe_pause_generation_and_original_text() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("w.db");
        let s = RouterStore::open_at(&p).unwrap();
        let cwd = d.path().to_str().unwrap();
        let old = snapshot("RESULT_READY", "old", "old result");
        s.enable_codex_watch("thread-a", "A", cwd, &old).unwrap();
        assert!(s.codex_watch_feed(0).unwrap().events.is_empty());
        let running = snapshot("RUNNING", "new", "progress");
        assert!(s.record_codex_watch("thread-a", 1, &running).unwrap());
        assert!(!s.record_codex_watch("thread-a", 1, &running).unwrap());
        s.pause_codex_watch("thread-a").unwrap();
        assert!(!s.record_codex_watch("thread-a", 1, &old).unwrap());
        s.enable_codex_watch("thread-a", "A", cwd, &running)
            .unwrap();
        assert!(!s.record_codex_watch("thread-a", 1, &old).unwrap());
        let final_reply = snapshot("RESULT_READY", "new", &"结果\n".repeat(600));
        assert!(s.record_codex_watch("thread-a", 3, &final_reply).unwrap());
        drop(s);
        let s = RouterStore::open_at(&p).unwrap();
        assert!(!s.record_codex_watch("thread-a", 3, &final_reply).unwrap());
        let f = s.codex_watch_feed(0).unwrap();
        assert_eq!(f.events.len(), 2);
        assert_eq!(f.events[1].snapshot.text.chars().count(), 400);
        assert_eq!(
            s.codex_watch_event(f.next_cursor).unwrap().snapshot.text,
            final_reply.text
        );
        assert!(s.codex_watch_feed(f.next_cursor).unwrap().events.is_empty());
        s.codex_watch_unavailable("thread-a", 3).unwrap();
        let after_failure = s.codex_watches().unwrap().remove(0);
        assert!(after_failure.checked_at <= now() && now() - after_failure.checked_at < 10_000);
        assert!(
            after_failure.retry_after >= now() + 29_000
                && after_failure.retry_after <= now() + 30_000
        );
        assert_eq!(s.codex_watches().unwrap()[0].snapshot.state, "RESULT_READY");
        assert_eq!(
            s.codex_watches().unwrap()[0].error_code.as_deref(),
            Some("WATCH_READ_UNAVAILABLE")
        );
        assert!(!s.record_codex_watch("thread-a", 3, &final_reply).unwrap());
        assert!(s.codex_watches().unwrap()[0].error_code.is_none());
        assert!(s.snapshot().unwrap().endpoint_lineage.is_empty());
    }
    #[test]
    fn local_removal_is_reversible_and_invalidates_inflight_observations(){
        let d=tempfile::tempdir().unwrap();let s=RouterStore::open_at(d.path().join("w.db")).unwrap();
        s.enable_codex_watch("exact","A",d.path().to_str().unwrap(),&snapshot("IDLE","none","")).unwrap();
        s.record_codex_watch("exact",1,&snapshot("RUNNING","t","public")).unwrap();let seq=s.codex_watch_feed(0).unwrap().next_cursor;
        s.set_watch_item_removed("EVENT",&seq.to_string(),true).unwrap();let f=s.codex_watch_feed(0).unwrap();assert!(f.events.is_empty());assert_eq!(f.next_cursor,seq);assert_eq!(f.hidden_sequences,vec![seq]);
        s.set_watch_item_removed("EVENT",&seq.to_string(),false).unwrap();assert_eq!(s.codex_watch_feed(0).unwrap().events.len(),1);
        s.set_watch_item_removed("WATCH","exact",true).unwrap();s.set_watch_item_removed("WATCH","exact",true).unwrap();assert!(s.codex_watches().unwrap().is_empty());assert!(!s.record_codex_watch("exact",1,&snapshot("RESULT_READY","t","late")).unwrap());
        s.set_watch_item_removed("WATCH","exact",false).unwrap();let w=&s.codex_watches().unwrap()[0];assert!(w.enabled);assert_eq!(w.generation,3);assert_eq!(w.snapshot.text,"public");
        s.pause_codex_watch("exact").unwrap();s.set_watch_item_removed("WATCH","exact",true).unwrap();s.set_watch_item_removed("WATCH","exact",false).unwrap();assert!(!s.codex_watches().unwrap()[0].enabled);
        assert!(s.set_watch_item_removed("WATCH","other",true).is_err());assert!(s.set_watch_item_removed("EVENT","0",true).is_err());assert!(s.set_watch_item_removed("INVALID","exact",true).is_err());
    }
    #[test]
    fn retention_keeps_latest_twenty_distinct_turns_and_monotonic_cursor() {
        let d = tempfile::tempdir().unwrap();
        let s = RouterStore::open_at(d.path().join("w.db")).unwrap();
        s.enable_codex_watch(
            "a",
            "A",
            d.path().to_str().unwrap(),
            &snapshot("IDLE", "none", ""),
        )
        .unwrap();
        for n in 0..25 {
            s.record_codex_watch(
                "a",
                1,
                &snapshot("RESULT_READY", &n.to_string(), "same text"),
            )
            .unwrap();
        }
        let first = s.codex_watch_feed(0).unwrap();
        assert!(!first.has_more);
        assert_eq!(first.events.len(), 20);
        assert_eq!(first.events.first().unwrap().sequence, 6);
        assert_eq!(first.next_cursor, 25);
        assert!(s.codex_watch_event(5).is_err());
        let next = s.codex_watch_feed(first.next_cursor).unwrap();
        assert!(next.events.is_empty());
        assert!(!next.has_more);
        assert!(s.codex_watch_feed(-1).is_err());
        s.record_codex_watch("a", 1, &snapshot("RESULT_READY", "26", "next"))
            .unwrap();
        let next = s.codex_watch_feed(25).unwrap();
        assert_eq!(next.events.len(), 1);
        assert_eq!(next.next_cursor, 26);
    }
}
