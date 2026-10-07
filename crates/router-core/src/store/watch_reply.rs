//! Owner-authored follow-ups, separate from cross-provider Handoffs.
use super::*;
use serde::Deserialize;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WatchReply {
    pub id: String,
    pub thread_id: String,
    pub cwd: String,
    pub generation: i64,
    pub source_sequence: Option<i64>,
    pub expected_turn_id: Option<String>,
    pub mode: String,
    pub text: String,
    pub options: Value,
    pub payload_hash: String,
    pub status: String,
    pub turn_id: Option<String>,
    pub error_code: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
}
fn row(r: &rusqlite::Row<'_>) -> rusqlite::Result<WatchReply> {
    let raw: String = r.get(8)?;
    Ok(WatchReply { id:r.get(0)?,thread_id:r.get(1)?,cwd:r.get(2)?,generation:r.get(3)?,source_sequence:r.get(4)?,expected_turn_id:r.get(5)?,mode:r.get(6)?,text:r.get(7)?,options:serde_json::from_str(&raw).map_err(|e|rusqlite::Error::FromSqlConversionFailure(8,rusqlite::types::Type::Text,Box::new(e)))?,payload_hash:r.get(9)?,status:r.get(10)?,turn_id:r.get(11)?,error_code:r.get(12)?,created_at:r.get(13)?,updated_at:r.get(14)? })
}
const COLS:&str="id,thread_id,cwd,generation,source_sequence,expected_turn_id,mode,text,options_json,payload_hash,status,turn_id,error_code,created_at,updated_at";
impl RouterStore {
    pub fn watch_reply(&self,id:&str)->Result<Option<WatchReply>,String>{ self.with_connection(|c| c.query_row(&format!("SELECT {COLS} FROM watch_replies WHERE id=?1"),[id],row).optional().map_err(db_error)) }
    pub fn watch_replies(&self,thread:&str)->Result<Vec<WatchReply>,String>{self.with_connection(|c|{let mut s=c.prepare(&format!("SELECT {COLS} FROM watch_replies WHERE thread_id=?1 ORDER BY created_at DESC,id DESC LIMIT 20")).map_err(db_error)?;let out=s.query_map([thread],row).map_err(db_error)?.collect::<Result<Vec<_>,_>>().map_err(db_error)?;Ok(out)})}
    /// Record intent before any external mutation. Duplicate IDs must match bytes.
    pub fn prepare_watch_reply(&self,input:&WatchReply)->Result<WatchReply,String>{
        if Uuid::parse_str(&input.id).is_err()||input.text.trim().is_empty()||input.text.len()>100_000||!matches!(input.mode.as_str(),"SEND"|"QUEUE"|"STEER"){return Err("REPLY_INPUT_INVALID".into());}
        let raw=serde_json::to_string(&input.options).map_err(|_|"REPLY_OPTIONS_INVALID")?;
        if raw.len()>4096{return Err("REPLY_OPTIONS_INVALID".into());}
        let hash=length_prefixed_hash(&[&input.thread_id,&input.cwd,&input.generation.to_string(),&input.source_sequence.map(|n|n.to_string()).unwrap_or_default(),input.expected_turn_id.as_deref().unwrap_or_default(),&input.mode,&input.text,&raw].map(str::as_bytes));
        self.with_connection(|c|{
            let tx=c.transaction_with_behavior(TransactionBehavior::Immediate).map_err(db_error)?;
            if let Some(prior)=tx.query_row(&format!("SELECT {COLS} FROM watch_replies WHERE id=?1"),[&input.id],row).optional().map_err(db_error)?{if prior.payload_hash!=hash{return Err("REPLY_ID_REUSED_WITH_DIFFERENT_CONTENT".into());}return Ok(prior);}
            if tx.query_row("SELECT 1 FROM watch_reply_cancellations WHERE id=?1",[&input.id],|r|r.get::<_,i64>(0)).optional().map_err(db_error)?.is_some(){return Err("REPLY_CANCELLED".into());}
            let current:Option<(String,i64)>=tx.query_row("SELECT cwd,generation FROM codex_watches WHERE thread_id=?1",[&input.thread_id],|r|Ok((r.get(0)?,r.get(1)?))).optional().map_err(db_error)?;
            if current!=Some((input.cwd.clone(),input.generation)){return Err("REPLY_TARGET_CHANGED_REFRESH".into());}
            if let Some(seq)=input.source_sequence{let target:Option<String>=tx.query_row("SELECT thread_id FROM codex_watch_events WHERE sequence=?1",[seq],|r|r.get(0)).optional().map_err(db_error)?;if target.as_deref()!=Some(&input.thread_id){return Err("WATCH_EVENT_NOT_FOUND".into());}}
            let conflict:i64=tx.query_row("SELECT COUNT(*) FROM watch_replies WHERE thread_id=?1 AND status IN ('QUEUED','SENDING','UNKNOWN')",[&input.thread_id],|r|r.get(0)).map_err(db_error)?;if conflict>0{return Err("REPLY_PREVIOUS_PENDING_CHECK_FIRST".into());}
            let at=now();tx.execute("INSERT INTO watch_replies(id,thread_id,cwd,generation,source_sequence,expected_turn_id,mode,text,options_json,payload_hash,status,created_at,updated_at) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,'QUEUED',?11,?11)",params![input.id,input.thread_id,input.cwd,input.generation,input.source_sequence,input.expected_turn_id,input.mode,input.text,raw,hash,at]).map_err(db_error)?;
            let reply=tx.query_row(&format!("SELECT {COLS} FROM watch_replies WHERE id=?1"),[&input.id],row).map_err(db_error)?;tx.commit().map_err(db_error)?;Ok(reply)
        })
    }
    pub fn claim_watch_reply(&self,id:&str)->Result<WatchReply,String>{self.with_connection(|c|{let tx=c.transaction_with_behavior(TransactionBehavior::Immediate).map_err(db_error)?;let reply=tx.query_row(&format!("SELECT {COLS} FROM watch_replies WHERE id=?1"),[id],row).map_err(db_error)?;if reply.status!="QUEUED"{return Err("REPLY_ALREADY_ATTEMPTED".into());}
        let valid:i64=tx.query_row("SELECT COUNT(*) FROM codex_watches WHERE thread_id=?1 AND cwd=?2 AND generation=?3",params![reply.thread_id,reply.cwd,reply.generation],|r|r.get(0)).map_err(db_error)?;if valid!=1{return Err("REPLY_TARGET_CHANGED_REFRESH".into());}
        let busy:i64=tx.query_row("SELECT (SELECT COUNT(*) FROM provider_runs r JOIN endpoints e ON e.id=r.endpoint_id WHERE e.provider='CODEX' AND e.external_id=?1 AND r.status IN ('STARTING','RUNNING','UNKNOWN')) + (SELECT COUNT(*) FROM handoffs h JOIN endpoints e ON e.id=h.destination_endpoint_id WHERE e.provider='CODEX' AND e.external_id=?1 AND h.status='SENDING')",[&reply.thread_id],|r|r.get(0)).map_err(db_error)?;if busy>0{return Err("REPLY_OTHER_WRITER_PENDING".into());}
        // Instance-delegated direct replies, including delayed QUEUE dispatch,
        // revalidate authority in the physical writer's own claim transaction.
        let delegated:Option<(String,String)>=tx.query_row("SELECT grant_id,status FROM assistant_actions WHERE id=?1",[id],|r|Ok((r.get(0)?,r.get(1)?))).optional().map_err(db_error)?;
        if let Some((gid,status))=delegated{let g=assistant::active(&tx,&gid)?;if g.scope!="INSTANCE"||!matches!(status.as_str(),"EXECUTING"|"APPLIED"){return Err("ASSISTANT_ACTION_NOT_AUTHORIZED_FOR_SEND".into());}}
        tx.execute("UPDATE watch_replies SET status='SENDING',updated_at=?2 WHERE id=?1 AND status='QUEUED'",params![id,now()]).map_err(db_error)?;let out=tx.query_row(&format!("SELECT {COLS} FROM watch_replies WHERE id=?1"),[id],row).map_err(db_error)?;tx.commit().map_err(db_error)?;Ok(out)})}
    pub fn finish_watch_reply(&self,id:&str,status:&str,turn:Option<&str>,error:Option<&str>)->Result<(),String>{if !matches!(status,"SENT"|"UNKNOWN"|"FAILED"){return Err("REPLY_STATUS_INVALID".into());}self.with_connection(|c|{let changed=c.execute("UPDATE watch_replies SET status=?2,turn_id=?3,error_code=?4,updated_at=?5 WHERE id=?1 AND status IN ('QUEUED','SENDING') AND (?2!='FAILED' OR status='QUEUED')",params![id,status,turn,error,now()]).map_err(db_error)?;if changed!=1{return Err("REPLY_ALREADY_ATTEMPTED".into());}Ok(())})}
    pub fn cancel_watch_reply(&self,thread:&str,id:&str)->Result<(),String>{self.with_connection(|c|{if c.execute("UPDATE watch_replies SET status='CANCELLED',updated_at=?3 WHERE id=?2 AND thread_id=?1 AND status='QUEUED'",params![thread,id,now()]).map_err(db_error)?!=1{return Err("REPLY_ALREADY_ATTEMPTED".into());}Ok(())})}
    pub fn recover_watch_replies(&self)->Result<usize,String>{self.with_connection(|c|c.execute("UPDATE watch_replies SET status='UNKNOWN',error_code='HOST_RESTART_ACK_UNKNOWN',updated_at=?1 WHERE status='SENDING'",[now()]).map_err(db_error))}
    pub fn require_no_watch_reply_writer(&self,thread:&str)->Result<(),String>{self.with_connection(|c|{let count:i64=c.query_row("SELECT COUNT(*) FROM watch_replies WHERE thread_id=?1 AND status IN ('QUEUED','SENDING','UNKNOWN')",[thread],|r|r.get(0)).map_err(db_error)?;if count>0{return Err("REPLY_PREVIOUS_PENDING_CHECK_FIRST".into());}Ok(())})}
    pub fn acknowledge_watch_reply_unknown(&self,thread:&str,id:&str,confirmed:bool)->Result<(),String>{if !confirmed{return Err("REPLY_OWNER_CONFIRMATION_REQUIRED".into());}self.with_connection(|c|{if c.execute("UPDATE watch_replies SET status='ACKNOWLEDGED',error_code='OWNER_CHECKED_DELIVERY_STILL_UNKNOWN',updated_at=?3 WHERE id=?2 AND thread_id=?1 AND status='UNKNOWN'",params![thread,id,now()]).map_err(db_error)?!=1{return Err("REPLY_STATUS_CHANGED_REFRESH".into());}Ok(())})}
    /// Explicitly revoke an unacknowledged request ID, including a delayed HTTP
    /// request that has not reached this Host. Never withdraw an attempted write.
    pub fn abandon_watch_reply(&self,thread:&str,id:&str)->Result<bool,String>{if Uuid::parse_str(id).is_err(){return Err("REPLY_INPUT_INVALID".into());}self.with_connection(|c|{let tx=c.transaction_with_behavior(TransactionBehavior::Immediate).map_err(db_error)?;let prior=tx.query_row(&format!("SELECT {COLS} FROM watch_replies WHERE id=?1"),[id],row).optional().map_err(db_error)?;if let Some(r)=prior{if r.thread_id!=thread{return Err("REPLY_TARGET_CHANGED_REFRESH".into());}if matches!(r.status.as_str(),"SENT"|"SENDING"|"UNKNOWN"|"ACKNOWLEDGED"){return Ok(false);}tx.execute("UPDATE watch_replies SET status='CANCELLED',updated_at=?2 WHERE id=?1 AND status='QUEUED'",params![id,now()]).map_err(db_error)?;}tx.execute("INSERT OR IGNORE INTO watch_reply_cancellations(id,thread_id,created_at) VALUES(?1,?2,?3)",params![id,thread,now()]).map_err(db_error)?;tx.commit().map_err(db_error)?;Ok(true)})}
}

#[cfg(test)] mod tests {
use super::*;
fn setup()->(tempfile::TempDir,RouterStore,WatchReply){let d=tempfile::tempdir().unwrap();let s=RouterStore::open_at(d.path().join("router.db")).unwrap();let root=d.path().to_string_lossy().to_string();s.enable_codex_watch("exact","name",&root,&crate::store::codex_watch::WatchSnapshot{state:"IDLE".into(),turn_id:None,item_id:None,text:String::new()}).unwrap();let i=WatchReply{id:Uuid::new_v4().to_string(),thread_id:"exact".into(),cwd:root,generation:s.codex_watches().unwrap()[0].generation,source_sequence:None,expected_turn_id:None,mode:"SEND".into(),text:"original bytes".into(),options:serde_json::json!({}),payload_hash:String::new(),status:String::new(),turn_id:None,error_code:None,created_at:0,updated_at:0};(d,s,i)}
#[test]fn replay_and_unknown_never_claim_again(){let(_d,s,mut i)=setup();let a=s.prepare_watch_reply(&i).unwrap();s.claim_watch_reply(&a.id).unwrap();assert_eq!(s.recover_watch_replies().unwrap(),1);assert_eq!(s.prepare_watch_reply(&i).unwrap().status,"UNKNOWN");assert!(s.claim_watch_reply(&i.id).is_err());i.text.push('!');assert!(s.prepare_watch_reply(&i).unwrap_err().contains("DIFFERENT_CONTENT"));}
#[test]fn concurrent_intent_and_target_change_fail_closed(){let(_d,s,mut i)=setup();let first=s.prepare_watch_reply(&i).unwrap();i.id=Uuid::new_v4().to_string();assert!(s.prepare_watch_reply(&i).is_err());s.pause_codex_watch("exact").unwrap();assert!(s.claim_watch_reply(&first.id).is_err());s.cancel_watch_reply("exact",&first.id).unwrap();assert!(s.prepare_watch_reply(&i).is_err());}
#[test]fn delayed_http_attempt_is_revoked_and_uncertainty_needs_owner_check(){let(_d,s,mut i)=setup();assert!(s.abandon_watch_reply("exact",&i.id).unwrap());assert_eq!(s.prepare_watch_reply(&i).unwrap_err(),"REPLY_CANCELLED");i.id=Uuid::new_v4().to_string();s.prepare_watch_reply(&i).unwrap();s.claim_watch_reply(&i.id).unwrap();assert!(s.finish_watch_reply(&i.id,"FAILED",None,None).is_err());s.finish_watch_reply(&i.id,"UNKNOWN",None,Some("ACK_LOST")).unwrap();assert!(!s.abandon_watch_reply("exact",&i.id).unwrap());assert!(s.acknowledge_watch_reply_unknown("exact",&i.id,false).is_err());s.acknowledge_watch_reply_unknown("exact",&i.id,true).unwrap();assert_eq!(s.prepare_watch_reply(&i).unwrap().status,"ACKNOWLEDGED");assert!(s.claim_watch_reply(&i.id).is_err());i.id=Uuid::new_v4().to_string();assert!(s.prepare_watch_reply(&i).is_ok());}
}
