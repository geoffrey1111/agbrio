//! Manual Claude input only. OAuth/MCP never creates these commands.
use super::*;
use crate::claude_code::Event;
use super::watch_reply::WatchReply;
use serde::Deserialize;

pub const PREFIX: &str = "claude_code:";
pub fn native_id(key: &str) -> Result<&str, String> {
    let value=key.strip_prefix(PREFIX).ok_or("CLAUDE_TARGET_INVALID")?;
    if !Uuid::parse_str(value).is_ok_and(|id|id.to_string()==value) {return Err("CLAUDE_TARGET_INVALID".into());}
    Ok(value)
}
pub(super) fn migrate(c:&mut Connection)->Result<(),String>{
    let tx=c.transaction_with_behavior(TransactionBehavior::Immediate).map_err(db_error)?;
    let done:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM router_feature_migrations WHERE key='claude-manual-input-v1')",[],|r|r.get(0)).map_err(db_error)?;
    if !done {
        tx.execute_batch(include_str!("../../migrations/normal/018_claude_manual_commands.sql")).map_err(db_error)?;
        tx.execute("INSERT INTO router_feature_migrations(key,applied_at) VALUES('claude-manual-input-v1',?1)",[now()]).map_err(db_error)?;
    }
    let bridge:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM router_feature_migrations WHERE key='claude-bridge-input-v1')",[],|r|r.get(0)).map_err(db_error)?;
    if !bridge {
        tx.execute_batch(include_str!("../../migrations/normal/020_claude_bridge_commands.sql")).map_err(db_error)?;
        tx.execute("INSERT INTO router_feature_migrations(key,applied_at) VALUES('claude-bridge-input-v1',?1)",[now()]).map_err(db_error)?;
    }
    tx.commit().map_err(db_error)
}

#[derive(Debug,Serialize)]
#[serde(rename_all="camelCase")]
pub struct ManualCommand {
    pub request_id:String, pub session_id:String, pub instance_id:String,
    pub cwd:String, pub text:String, pub text_sha256:String,
}
#[derive(Clone,Deserialize)]
#[serde(rename_all="camelCase",deny_unknown_fields)]
pub struct ManualReceipt {
    pub request_id:String,pub session_id:String,pub instance_id:String,pub cwd:String,
    pub status:String,pub message_id:Option<String>,pub text_sha256:String,
}
pub(super) fn verify_publisher(c:&Connection,token:&str,sid:&str,instance:&str,cwd:&str)->Result<(),String>{
    let owner=super::claude_code::auth(c,token,now())?;
    // Token, session and plugin instance prove the publisher; the working directory may
    // differ from the session's root during a turn.
    let _=cwd;
    let matched:bool=c.query_row("SELECT EXISTS(SELECT 1 FROM claude_sessions WHERE session_id=?1 AND instance_id=?2 AND connection_id=?3 AND lease_until>?4 AND state!='SESSION_END')",params![sid,instance,owner,now()],|r|r.get(0)).map_err(db_error)?;
    if !matched {return Err("CLAUDE_COMMAND_PUBLISHER_CHANGED".into());} Ok(())
}
impl RouterStore {
    /// Only independently acknowledged native rows identify a rendered owner prompt.
    pub fn claude_manual_message_ids(&self,key:&str)->Result<std::collections::HashMap<String,String>,String>{self.with_connection(|c|{let mut q=c.prepare("SELECT r.id,m.message_id FROM claude_manual_commands m JOIN watch_replies r ON r.id=m.request_id WHERE r.thread_id=?1 AND r.status='SENT' AND m.message_id IS NOT NULL ORDER BY r.created_at DESC LIMIT 100").map_err(db_error)?;let rows=q.query_map([key],|r|Ok((r.get(0)?,r.get(1)?))).map_err(db_error)?.collect::<Result<std::collections::HashMap<_,_>,_>>().map_err(db_error)?;Ok(rows)})}
    pub fn claude_manual_ready(&self,sid:&str,instance:&str)->Result<bool,String>{self.with_connection(|c|c.query_row("SELECT EXISTS(SELECT 1 FROM claude_control_leases WHERE session_id=?1 AND instance_id=?2 AND checked_at>?3)",params![sid,instance,now()-30_000],|r|r.get(0)).map_err(db_error))}
    pub fn enqueue_claude_manual(&self,r:&WatchReply,instance:&str)->Result<(),String>{
        let sid=native_id(&r.thread_id)?;
        self.with_connection(|c|{
            let tx=c.transaction_with_behavior(TransactionBehavior::Immediate).map_err(db_error)?;
            // Accept the owner's durable intent before the plugin loads. Native
            // transcripts are often readable several seconds before its first
            // heartbeat. A live publisher is required at offer time, not enqueue.
            // Never infer a destination from titles or silently change its root.
            let cwd:Option<String>=tx.query_row("SELECT s.cwd FROM claude_sessions s JOIN claude_connections a ON a.id=s.connection_id WHERE s.session_id=?1 AND a.revoked_at IS NULL AND a.expires_at>?2",params![sid,now()],|r|r.get(0)).optional().map_err(db_error)?;
            if !cwd.is_some_and(|cwd|crate::claude_code::same_root(&cwd,&r.cwd)){return Err("CLAUDE_TARGET_CHANGED".into());}
            tx.execute("INSERT INTO claude_manual_commands(request_id,session_id,instance_id,cwd) VALUES(?1,?2,?3,?4)",params![r.id,sid,instance,r.cwd]).map_err(db_error)?;
            tx.commit().map_err(db_error)
        })
    }
    /// Piggyback the existing authenticated heartbeat. No provider/model polling.
    /// An offer is claimed BEFORE the HTTP reply; it is never offered twice.
    pub fn offer_claude_manual(&self,token:&str,event:&Event)->Result<Option<ManualCommand>,String>{self.with_connection(|c|{
        let tx=c.transaction_with_behavior(TransactionBehavior::Immediate).map_err(db_error)?;
        verify_publisher(&tx,token,&event.session_id,&event.instance_id,&event.cwd)?;
        tx.execute("INSERT INTO claude_control_leases(session_id,instance_id,checked_at) VALUES(?1,?2,?3) ON CONFLICT(session_id) DO UPDATE SET instance_id=excluded.instance_id,checked_at=excluded.checked_at",params![event.session_id,event.instance_id,now()]).map_err(db_error)?;
        if event.kind!="HEARTBEAT" {tx.commit().map_err(db_error)?;return Ok(None);}
        let candidate:Option<(String,String,String)>=tx.query_row("SELECT r.id,r.thread_id,r.text FROM claude_manual_commands m JOIN watch_replies r ON r.id=m.request_id WHERE m.session_id=?1 AND r.status='QUEUED' AND m.offered_at IS NULL ORDER BY r.created_at LIMIT 1",[&event.session_id],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).optional().map_err(db_error)?;
        let Some((request,key,text))=candidate else{let offer=bridge_offer(&tx,event)?;tx.commit().map_err(db_error)?;return Ok(offer)};
        let (instance,cwd,generation,mode,expected):(String,String,i64,String,Option<String>)=tx.query_row("SELECT m.instance_id,m.cwd,r.generation,r.mode,r.expected_turn_id FROM claude_manual_commands m JOIN watch_replies r ON r.id=m.request_id WHERE r.id=?1",[&request],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?))).map_err(db_error)?;
        let context=codex_watch::chat_context(&tx,&key);
        // Same exact session and root; a reloaded plugin (new instance) still delivers.
        let _=instance;
        let valid=context.is_ok_and(|w|w.generation==generation&&crate::claude_code::same_root(&w.cwd,&cwd))&&crate::claude_code::within_root(&cwd,&event.cwd);
        if !valid{
            tx.execute("UPDATE watch_replies SET status='FAILED',error_code='CLAUDE_TARGET_CHANGED',updated_at=?2 WHERE id=?1 AND status='QUEUED'",params![request,now()]).map_err(db_error)?;
            tx.commit().map_err(db_error)?;return Ok(None);
        }
        let (state,turn):(String,Option<String>)=tx.query_row("SELECT state,turn_id FROM claude_sessions WHERE session_id=?1",[&event.session_id],|r|Ok((r.get(0)?,r.get(1)?))).map_err(db_error)?;
        // After reload, SESSION_START proves identity, not idle. Explicit QUEUE
        // may use the native prompt queue (which never aborts a running turn).
        // It is not an idle/goal-recovery claim. Known approvals never dispatch.
        if !matches!(state.as_str(),"SESSION_START"|"COMPLETE"|"FAILED"|"INTERRUPTED"|"EMPTY"|"LIMITED") {tx.commit().map_err(db_error)?;return Ok(None);}
        if mode=="QUEUE"&&!matches!(state.as_str(),"SESSION_START"|"COMPLETE"|"EMPTY") {tx.commit().map_err(db_error)?;return Ok(None);}
        if mode=="SEND"&&state=="SESSION_START" {tx.commit().map_err(db_error)?;return Ok(None);}
        if mode=="SEND" && expected!=turn {
            tx.execute("UPDATE watch_replies SET status='FAILED',error_code='CLAUDE_TURN_CHANGED',updated_at=?2 WHERE id=?1",params![request,now()]).map_err(db_error)?;
            tx.commit().map_err(db_error)?;return Ok(None);
        }
        let busy:i64=tx.query_row("SELECT (SELECT count(*) FROM provider_runs r JOIN endpoints e ON e.id=r.endpoint_id WHERE e.provider='CLAUDE_CODE' AND e.external_id=?1 AND r.status IN ('STARTING','RUNNING','UNKNOWN'))+(SELECT count(*) FROM handoffs h JOIN endpoints e ON e.id=h.destination_endpoint_id WHERE e.provider='CLAUDE_CODE' AND e.external_id=?1 AND h.status IN ('SENDING','UNKNOWN','APPROVED'))",[&event.session_id],|r|r.get(0)).map_err(db_error)?;
        if busy>0{tx.commit().map_err(db_error)?;return Ok(None);}
        tx.execute("UPDATE watch_replies SET status='SENDING',updated_at=?2 WHERE id=?1 AND status='QUEUED'",params![request,now()]).map_err(db_error)?;
        // Bind the command to the plugin instance that receives it; receipts match it.
        tx.execute("UPDATE claude_manual_commands SET offered_at=?2,instance_id=?3,cwd=?4 WHERE request_id=?1",params![request,now(),event.instance_id,event.cwd]).map_err(db_error)?;
        tx.commit().map_err(db_error)?;
        Ok(Some(ManualCommand{request_id:request,session_id:event.session_id.clone(),instance_id:event.instance_id.clone(),cwd:event.cwd.clone(),text_sha256:format!("{:x}",Sha256::digest(text.as_bytes())),text}))
    })}
    pub fn accept_claude_manual_receipt(&self,token:&str,input:&ManualReceipt)->Result<(),String>{self.with_connection(|c|{
        if !matches!(input.status.as_str(),"SENT"|"NOT_SENT"|"UNKNOWN") || (input.status=="SENT"&&input.message_id.as_deref().is_none_or(|v|Uuid::parse_str(v).is_err())) {return Err("CLAUDE_RECEIPT_INVALID".into());}
        let tx=c.transaction_with_behavior(TransactionBehavior::Immediate).map_err(db_error)?;
        verify_publisher(&tx,token,&input.session_id,&input.instance_id,&input.cwd)?;
        let bridge:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM claude_bridge_commands WHERE handoff_id=?1)",[&input.request_id],|r|r.get(0)).map_err(db_error)?;
        if bridge {bridge_receipt(&tx,input)?;return tx.commit().map_err(db_error);}
        let (sid,instance,cwd,text,status,offered,message):(String,String,String,String,String,Option<i64>,Option<String>)=tx.query_row("SELECT m.session_id,m.instance_id,m.cwd,r.text,r.status,m.offered_at,m.message_id FROM claude_manual_commands m JOIN watch_replies r ON r.id=m.request_id WHERE m.request_id=?1",[&input.request_id],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?,r.get(5)?,r.get(6)?))).map_err(db_error)?;
        if sid!=input.session_id||instance!=input.instance_id||cwd!=input.cwd||offered.is_none()||format!("{:x}",Sha256::digest(text.as_bytes()))!=input.text_sha256{return Err("CLAUDE_RECEIPT_TARGET_MISMATCH".into());}
        let final_status=if input.status=="NOT_SENT"{"FAILED"}else{&input.status};
        if status==final_status&&message==input.message_id{tx.commit().map_err(db_error)?;return Ok(());}
        if !matches!(status.as_str(),"SENDING"|"UNKNOWN"){return Err("CLAUDE_RECEIPT_CONFLICT".into());}
        tx.execute("UPDATE watch_replies SET status=?2,error_code=?3,updated_at=?4 WHERE id=?1",params![input.request_id,final_status,if input.status=="SENT"{None}else{Some("CLAUDE_DELIVERY_UNCONFIRMED")},now()]).map_err(db_error)?;
        tx.execute("UPDATE claude_manual_commands SET message_id=?2 WHERE request_id=?1",params![input.request_id,input.message_id]).map_err(db_error)?;
        tx.commit().map_err(db_error)
    })}
    pub fn expire_claude_manual_offers(&self)->Result<usize,String>{self.with_connection(|c|{
        let manual=c.execute("UPDATE watch_replies SET status='UNKNOWN',error_code='CLAUDE_RECEIPT_NOT_OBSERVED',updated_at=?1 WHERE status='SENDING' AND EXISTS(SELECT 1 FROM claude_manual_commands m WHERE m.request_id=watch_replies.id AND m.offered_at<?2)",params![now(),now()-30_000]).map_err(db_error)?;
        // An offered relay without a receipt is UNKNOWN (reconcile only, never resent);
        // one never offered because the session stayed busy is a proven pre-write failure.
        // Handoffs have no UNKNOWN status: like a lost Codex acknowledgement, the
        // relay stays SENDING (unresolved) with an error code; only a receipt settles it.
        let offered=c.execute("UPDATE handoffs SET error_code='CLAUDE_RECEIPT_NOT_OBSERVED',error_message='Claude Code 是否收到尚未确认，不会自动重发。' WHERE status='SENDING' AND error_code IS NULL AND id IN (SELECT handoff_id FROM claude_bridge_commands WHERE offered_at<?1)",params![now()-30_000]).map_err(db_error)?;
        let idle=c.execute("UPDATE handoffs SET status='FAILED',failed_at=?1,error_code='CLAUDE_TARGET_NOT_IDLE',error_message='Claude Code 对话一直没有空下来，本次没有发送，可以重新转发。' WHERE status='SENDING' AND id IN (SELECT handoff_id FROM claude_bridge_commands WHERE offered_at IS NULL AND enqueued_at<?2)",params![now(),now()-CLAUDE_BRIDGE_WAIT_MS]).map_err(db_error)?;
        Ok(manual+offered+idle)
    })}
}

#[cfg(test)] mod tests {
    use super::*;
    fn event(root:&Path,kind:&str,n:i64,turn:Option<String>)->Event{Event{event_id:id(),session_id:"10000000-0000-4000-8000-000000000001".into(),instance_id:"20000000-0000-4000-8000-000000000001".into(),sequence:n,cwd:root.to_string_lossy().into(),version:"2.1.295".into(),kind:kind.into(),turn_id:turn,text:if kind=="COMPLETE"{Some("original complete answer".into())}else{None},completed_at:if kind=="COMPLETE"{Some(now())}else{None}}}
    fn intent(store:&RouterStore,start:&Event,turn:&str)->WatchReply {
        let key=format!("{PREFIX}{}",start.session_id);
        let w=store.codex_chat_context(&key).unwrap();
        WatchReply{id:id(),thread_id:key,cwd:start.cwd.clone(),generation:w.generation,source_sequence:None,expected_turn_id:Some(turn.into()),mode:"SEND".into(),text:"Approved exact text\n中文\n```code```".into(),options:serde_json::json!({"attachments":[]}),payload_hash:String::new(),status:String::new(),turn_id:None,error_code:None,created_at:0,updated_at:0}
    }
    #[test] fn manual_offer_receipt_restart_and_replays_are_at_most_once(){
        let temp=tempfile::tempdir().unwrap();let path=temp.path().join("router.db");let store=RouterStore::open_at(&path).unwrap();let pair=store.create_claude_connection().unwrap();let start=event(temp.path(),"SESSION_START",1,None);let turn=id();
        store.ingest_claude_event(&pair.token,&start).unwrap();let active=event(temp.path(),"RUNNING",2,Some(turn.clone()));store.ingest_claude_event(&pair.token,&active).unwrap();
        let end=event(temp.path(),"COMPLETE",3,Some(turn.clone()));store.ingest_claude_event(&pair.token,&end).unwrap();
        store.enable_codex_watch(&format!("{PREFIX}{}",start.session_id),"Claude",&start.cwd,&codex_watch::WatchSnapshot{state:"RESULT_READY".into(),turn_id:Some(turn.clone()),item_id:None,text:String::new()}).unwrap();
        let beat=event(temp.path(),"HEARTBEAT",4,Some(turn.clone()));store.ingest_claude_event(&pair.token,&beat).unwrap();assert!(store.offer_claude_manual(&pair.token,&beat).unwrap().is_none());
        let r=store.prepare_watch_reply(&intent(&store,&start,&turn)).unwrap();store.enqueue_claude_manual(&r,&start.instance_id).unwrap();let cmd=store.offer_claude_manual(&pair.token,&beat).unwrap().unwrap();assert_eq!(cmd.text,r.text);assert!(store.offer_claude_manual(&pair.token,&beat).unwrap().is_none());
        let receipt=ManualReceipt{request_id:r.id.clone(),session_id:start.session_id.clone(),instance_id:start.instance_id.clone(),cwd:start.cwd.clone(),status:"SENT".into(),message_id:Some(id()),text_sha256:cmd.text_sha256.clone()};
        let mut forged=receipt.clone();forged.text_sha256="0".repeat(64);assert!(store.accept_claude_manual_receipt(&pair.token,&forged).is_err());forged=receipt.clone();forged.instance_id=id();assert!(store.accept_claude_manual_receipt(&pair.token,&forged).is_err());
        drop(store);let store=RouterStore::open_at(path).unwrap();store.recover_watch_replies().unwrap();assert_eq!(store.watch_reply(&r.id).unwrap().unwrap().status,"UNKNOWN");assert!(store.offer_claude_manual(&pair.token,&beat).unwrap().is_none());
        store.accept_claude_manual_receipt(&pair.token,&receipt).unwrap();store.accept_claude_manual_receipt(&pair.token,&receipt).unwrap();assert_eq!(store.watch_reply(&r.id).unwrap().unwrap().status,"SENT");assert!(store.offer_claude_manual(&pair.token,&beat).unwrap().is_none());
        let mut conflict=r.clone();conflict.text.push('!');assert!(store.prepare_watch_reply(&conflict).is_err());
        store.revoke_claude_connection(&pair.connection_id).unwrap();assert!(store.accept_claude_manual_receipt(&pair.token,&receipt).is_err());
    }
    #[test]fn pause_or_pending_native_approval_never_dispatches_a_command(){
        let temp=tempfile::tempdir().unwrap();let store=RouterStore::open_at(temp.path().join("router.db")).unwrap();let pair=store.create_claude_connection().unwrap();let start=event(temp.path(),"SESSION_START",1,None);store.ingest_claude_event(&pair.token,&start).unwrap();let turn=id();let active=event(temp.path(),"RUNNING",2,Some(turn.clone()));store.ingest_claude_event(&pair.token,&active).unwrap();let key=format!("{PREFIX}{}",start.session_id);
        store.enable_codex_watch(&key,"Claude",&start.cwd,&codex_watch::WatchSnapshot{state:"RUNNING".into(),turn_id:Some(turn.clone()),item_id:None,text:String::new()}).unwrap();
        let beat=event(temp.path(),"HEARTBEAT",3,Some(turn.clone()));store.ingest_claude_event(&pair.token,&beat).unwrap();store.offer_claude_manual(&pair.token,&beat).unwrap();let mut r=intent(&store,&start,&turn);r.mode="QUEUE".into();let r=store.prepare_watch_reply(&r).unwrap();store.enqueue_claude_manual(&r,&start.instance_id).unwrap();assert!(store.offer_claude_manual(&pair.token,&beat).unwrap().is_none());
        let decision=event(temp.path(),"WAITING_DECISION",4,Some(turn.clone()));store.ingest_claude_event(&pair.token,&decision).unwrap();assert!(store.offer_claude_manual(&pair.token,&beat).unwrap().is_none());store.pause_codex_watch(&key).unwrap();let end=event(temp.path(),"COMPLETE",5,Some(turn));store.ingest_claude_event(&pair.token,&end).unwrap();assert!(store.offer_claude_manual(&pair.token,&beat).unwrap().is_none());assert_eq!(store.watch_reply(&r.id).unwrap().unwrap().status,"FAILED");
    }
    #[test]fn queue_survives_a_new_turn_and_plugin_reload_but_send_keeps_turn_fence(){
        let temp=tempfile::tempdir().unwrap();let store=RouterStore::open_at(temp.path().join("router.db")).unwrap();let pair=store.create_claude_connection().unwrap();let start=event(temp.path(),"SESSION_START",1,None);
        store.ingest_claude_event_owned(&pair.token,&start,true).unwrap();let key=format!("{PREFIX}{}",start.session_id);
        store.apply_claude_transcript(&start.session_id,&start.cwd,"RUNNING",Some("claude_turn:p1"),None).unwrap();
        store.enable_codex_watch(&key,"Claude",&start.cwd,&codex_watch::WatchSnapshot{state:"RUNNING".into(),turn_id:Some("claude_turn:p1".into()),item_id:None,text:String::new()}).unwrap();
        let mut beat=event(temp.path(),"HEARTBEAT",2,None);store.ingest_claude_event_owned(&pair.token,&beat,true).unwrap();store.offer_claude_manual(&pair.token,&beat).unwrap();
        let mut queued=intent(&store,&start,"claude_turn:p1");queued.mode="QUEUE".into();let queued=store.prepare_watch_reply(&queued).unwrap();store.enqueue_claude_manual(&queued,&start.instance_id).unwrap();
        assert!(store.offer_claude_manual(&pair.token,&beat).unwrap().is_none());
        store.ingest_claude_event_owned(&pair.token,&event(temp.path(),"SESSION_END",3,None),true).unwrap();
        let mut next=event(temp.path(),"SESSION_START",1,None);next.instance_id=id();store.ingest_claude_event_owned(&pair.token,&next,true).unwrap();
        let reply=crate::claude_code::CompleteReply{turn_id:"claude_turn:p2".into(),text:"answer".into(),completed_at:now()};
        store.apply_claude_transcript(&start.session_id,&start.cwd,"COMPLETE",Some(&reply.turn_id),Some(&reply)).unwrap();
        beat.instance_id=next.instance_id.clone();beat.event_id=id();store.ingest_claude_event_owned(&pair.token,&beat,true).unwrap();
        let offered=store.offer_claude_manual(&pair.token,&beat).unwrap().unwrap();assert_eq!(offered.request_id,queued.id);assert!(store.offer_claude_manual(&pair.token,&beat).unwrap().is_none());
        let receipt=ManualReceipt{request_id:queued.id.clone(),session_id:start.session_id.clone(),instance_id:next.instance_id.clone(),cwd:start.cwd.clone(),status:"SENT".into(),message_id:Some(id()),text_sha256:offered.text_sha256};
        store.accept_claude_manual_receipt(&pair.token,&receipt).unwrap();assert_eq!(store.watch_reply(&queued.id).unwrap().unwrap().status,"SENT");
        let stale=store.prepare_watch_reply(&intent(&store,&start,"claude_turn:p1")).unwrap();store.enqueue_claude_manual(&stale,&next.instance_id).unwrap();assert!(store.offer_claude_manual(&pair.token,&beat).unwrap().is_none());
        assert_eq!(store.watch_reply(&stale.id).unwrap().unwrap().error_code.as_deref(),Some("CLAUDE_TURN_CHANGED"));
    }
    #[test]fn newly_reloaded_exact_session_uses_explicit_native_queue_without_claiming_idle(){
        let temp=tempfile::tempdir().unwrap();let store=RouterStore::open_at(temp.path().join("router.db")).unwrap();let pair=store.create_claude_connection().unwrap();let start=event(temp.path(),"SESSION_START",1,None);store.ingest_claude_event(&pair.token,&start).unwrap();let key=format!("{PREFIX}{}",start.session_id);
        store.enable_codex_watch(&key,"Claude",&start.cwd,&codex_watch::WatchSnapshot{state:"UNKNOWN".into(),turn_id:None,item_id:None,text:String::new()}).unwrap();
        let beat=event(temp.path(),"HEARTBEAT",2,None);store.ingest_claude_event(&pair.token,&beat).unwrap();store.offer_claude_manual(&pair.token,&beat).unwrap();
        let mut r=intent(&store,&start,"unconfirmed");r.mode="QUEUE".into();r.expected_turn_id=None;let r=store.prepare_watch_reply(&r).unwrap();store.enqueue_claude_manual(&r,&start.instance_id).unwrap();assert!(store.offer_claude_manual(&pair.token,&beat).unwrap().is_some());assert!(store.offer_claude_manual(&pair.token,&beat).unwrap().is_none());assert_eq!(store.claude_session(&start.session_id).unwrap().state,"SESSION_START");
    }
}

/// An offered relay or manual reply whose receipt never arrived.
#[derive(Clone, Debug, PartialEq)]
pub struct UnconfirmedDelivery { pub id: String, pub bridge: bool, pub text: String, pub offered_at: i64 }
impl RouterStore {
    /// Offered deliveries into one session that are still unconfirmed.
    pub fn claude_unconfirmed_deliveries(&self, session: &str) -> Result<Vec<UnconfirmedDelivery>, String> { self.with_connection(|c| {
        let mut out = Vec::new();
        let mut q = c.prepare("SELECT r.id,r.text,m.offered_at FROM claude_manual_commands m JOIN watch_replies r ON r.id=m.request_id WHERE m.session_id=?1 AND m.offered_at IS NOT NULL AND r.status IN ('SENDING','UNKNOWN')").map_err(db_error)?;
        for row in q.query_map([session], |r| Ok(UnconfirmedDelivery { id: r.get(0)?, bridge: false, text: r.get(1)?, offered_at: r.get(2)? })).map_err(db_error)? { out.push(row.map_err(db_error)?); }
        let mut q = c.prepare("SELECT b.handoff_id,h.approved_text,b.offered_at FROM claude_bridge_commands b JOIN handoffs h ON h.id=b.handoff_id WHERE b.session_id=?1 AND b.offered_at IS NOT NULL AND h.status='SENDING'").map_err(db_error)?;
        for row in q.query_map([session], |r| Ok(UnconfirmedDelivery { id: r.get(0)?, bridge: true, text: r.get(1)?, offered_at: r.get(2)? })).map_err(db_error)? { out.push(row.map_err(db_error)?); }
        Ok(out)
    })}
    /// Settles an unconfirmed delivery as SENT from the native transcript row the
    /// plugin's submission produced (exact session, plugin origin, exact text).
    pub fn settle_claude_delivery_from_transcript(&self, delivery: &UnconfirmedDelivery, message_id: &str) -> Result<bool, String> { self.with_connection(|c| {
        let tx = c.transaction_with_behavior(TransactionBehavior::Immediate).map_err(db_error)?;
        let changed = if delivery.bridge {
            let n = tx.execute("UPDATE handoffs SET status='SENT',sent_at=?2,error_code=NULL,error_message=NULL WHERE id=?1 AND status='SENDING'", params![delivery.id, now()]).map_err(db_error)?;
            if n > 0 { tx.execute("UPDATE claude_bridge_commands SET message_id=?2 WHERE handoff_id=?1", params![delivery.id, message_id]).map_err(db_error)?; super::bridge_notifications::mark_sent_source_handled(&tx, &delivery.id, now())?; }
            n
        } else {
            let n = tx.execute("UPDATE watch_replies SET status='SENT',error_code=NULL,updated_at=?2 WHERE id=?1 AND status IN ('SENDING','UNKNOWN')", params![delivery.id, now()]).map_err(db_error)?;
            if n > 0 { tx.execute("UPDATE claude_manual_commands SET message_id=?2 WHERE request_id=?1", params![delivery.id, message_id]).map_err(db_error)?; }
            n
        };
        tx.commit().map_err(db_error)?; Ok(changed > 0)
    })}
}
/// A relay waits at most this long for the Claude Code session to become idle.
pub const CLAUDE_BRIDGE_WAIT_MS: i64 = 15 * 60_000;
fn fail_bridge(c:&Connection,handoff:&str,code:&str,message:&str)->Result<(),String>{
    c.execute("UPDATE handoffs SET status='FAILED',failed_at=?2,error_code=?3,error_message=?4 WHERE id=?1 AND status='SENDING'",params![handoff,now(),code,message]).map_err(db_error)?;Ok(())
}
/// Owner-approved Bridge relays ride the same authenticated heartbeat as manual
/// replies, only when the exact session is idle and no manual reply is open.
fn bridge_offer(c:&Connection,event:&Event)->Result<Option<ManualCommand>,String>{
    let row:Option<(String,String,String,String)>=c.query_row("SELECT b.handoff_id,h.approved_text,b.instance_id,b.cwd FROM claude_bridge_commands b JOIN handoffs h ON h.id=b.handoff_id WHERE b.session_id=?1 AND h.status='SENDING' AND b.offered_at IS NULL ORDER BY b.enqueued_at LIMIT 1",[&event.session_id],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?))).optional().map_err(db_error)?;
    let Some((handoff,text,instance,cwd))=row else {return Ok(None)};
    let _=&instance;
    if !crate::claude_code::within_root(&cwd,&event.cwd) {fail_bridge(c,&handoff,"CLAUDE_TARGET_CHANGED","Claude Code 会话已变化，本次没有发送。")?;return Ok(None);}
    let state:String=c.query_row("SELECT state FROM claude_sessions WHERE session_id=?1",[&event.session_id],|r|r.get(0)).map_err(db_error)?;
    if !matches!(state.as_str(),"COMPLETE"|"FAILED"|"INTERRUPTED"|"EMPTY"|"LIMITED") {return Ok(None);}
    let manual:bool=c.query_row("SELECT EXISTS(SELECT 1 FROM claude_manual_commands m JOIN watch_replies r ON r.id=m.request_id WHERE m.session_id=?1 AND r.status IN ('SENDING','UNKNOWN'))",[&event.session_id],|r|r.get(0)).map_err(db_error)?;
    if manual {return Ok(None);}
    c.execute("UPDATE claude_bridge_commands SET offered_at=?2,instance_id=?3,cwd=?4 WHERE handoff_id=?1 AND offered_at IS NULL",params![handoff,now(),event.instance_id,event.cwd]).map_err(db_error)?;
    Ok(Some(ManualCommand{request_id:handoff,session_id:event.session_id.clone(),instance_id:event.instance_id.clone(),cwd:event.cwd.clone(),text_sha256:format!("{:x}",Sha256::digest(text.as_bytes())),text}))
}
fn bridge_receipt(c:&Connection,input:&ManualReceipt)->Result<(),String>{
    let (sid,instance,cwd,text,status,offered,message):(String,String,String,String,String,Option<i64>,Option<String>)=c.query_row("SELECT b.session_id,b.instance_id,b.cwd,h.approved_text,h.status,b.offered_at,b.message_id FROM claude_bridge_commands b JOIN handoffs h ON h.id=b.handoff_id WHERE b.handoff_id=?1",[&input.request_id],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?,r.get(5)?,r.get(6)?))).map_err(db_error)?;
    if sid!=input.session_id||instance!=input.instance_id||cwd!=input.cwd||offered.is_none()||format!("{:x}",Sha256::digest(text.as_bytes()))!=input.text_sha256{return Err("CLAUDE_RECEIPT_TARGET_MISMATCH".into());}
    let target=match input.status.as_str(){"SENT"=>"SENT","NOT_SENT"=>"FAILED",_=>"SENDING"};
    if status!="SENDING"{return if status==target&&message==input.message_id{Ok(())}else{Err("CLAUDE_RECEIPT_CONFLICT".into())};}
    match target {
        "SENT"=>{c.execute("UPDATE handoffs SET status='SENT',sent_at=?2,error_code=NULL,error_message=NULL WHERE id=?1",params![input.request_id,now()]).map_err(db_error)?;super::bridge_notifications::mark_sent_source_handled(c,&input.request_id,now())?;}
        "FAILED"=>{c.execute("UPDATE handoffs SET status='FAILED',failed_at=?2,error_code='CLAUDE_NOT_SENT',error_message='Claude Code 没有接收这条转发，可以重新转发。' WHERE id=?1",params![input.request_id,now()]).map_err(db_error)?;}
        _=>{c.execute("UPDATE handoffs SET error_code='CLAUDE_DELIVERY_UNCONFIRMED',error_message='Claude Code 是否收到尚未确认，不会自动重发。' WHERE id=?1",params![input.request_id]).map_err(db_error)?;}
    }
    c.execute("UPDATE claude_bridge_commands SET message_id=?2 WHERE handoff_id=?1",params![input.request_id,input.message_id]).map_err(db_error)?;
    Ok(())
}
