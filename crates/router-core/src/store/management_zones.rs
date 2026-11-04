//! Owner-local responsibility metadata. Does not alter native parents or grants.
use super::*;
use serde::Deserialize;
use serde_json::json;

#[derive(Clone,Debug,Serialize)]
#[serde(rename_all="camelCase")]
pub struct Conversation {
    pub thread_id:String,pub label:String,pub provider:String,pub cwd:String,
    pub generation:i64,pub state:String,pub checked_at:i64,pub available:bool,pub watch_enabled:bool,
}
#[derive(Deserialize,Serialize)]
#[serde(rename_all="camelCase",deny_unknown_fields)]
pub struct Change {
    pub request_id:String,pub action:String,
    pub zone_id:Option<String>,pub revision:Option<i64>,pub name:Option<String>,
    pub manager_id:Option<String>,pub thread_id:Option<String>,
    pub generation:Option<i64>,pub from_revision:Option<i64>,pub from_zone_id:Option<String>,pub thread_ids:Option<Vec<String>>,
}
fn conversation(c:&Connection,id:&str)->Result<Conversation,String>{
    if let Some(sid)=id.strip_prefix("claude_code:"){
        if !crate::claude_code::native_id(sid){return Err("MANAGEMENT_CONVERSATION_INVALID".into());}
        return c.query_row("SELECT s.cwd,s.state,s.checked_at,s.lease_until,COALESCE(w.generation,1),COALESCE(w.label,''),COALESCE(w.enabled,0) FROM claude_sessions s LEFT JOIN codex_watches w ON w.thread_id=?1 WHERE s.session_id=?2",params![id,sid],|r|{
            let label:String=r.get(5)?;
            Ok(Conversation{thread_id:id.into(),label:if label.is_empty(){format!("Claude Code · {}",&sid[..8])}else{label},provider:"CLAUDE_CODE".into(),cwd:r.get(0)?,state:r.get(1)?,checked_at:r.get(2)?,available:r.get::<_,i64>(3)?>now(),generation:r.get(4)?,watch_enabled:r.get(6)?})
        }).map_err(|_|"MANAGEMENT_CONVERSATION_UNAVAILABLE".into());
    }
    let provider=if id.starts_with("pi:"){"PI"}else{"CODEX"};
    let native=id.strip_prefix("pi:").unwrap_or(id);
    if Uuid::parse_str(native).is_err(){return Err("MANAGEMENT_CONVERSATION_INVALID".into());}
    c.query_row("SELECT label,cwd,generation,COALESCE(json_extract(snapshot_json,'$.state'),'UNKNOWN'),checked_at,enabled,error_code FROM codex_watches WHERE thread_id=?1",[id],|r|Ok(Conversation{thread_id:id.into(),label:r.get(0)?,provider:provider.into(),cwd:r.get(1)?,generation:r.get(2)?,state:r.get(3)?,checked_at:r.get(4)?,available:r.get::<_,bool>(5)?&&r.get::<_,Option<String>>(6)?.is_none(),watch_enabled:r.get(5)?})).map_err(|_|"MANAGEMENT_CONVERSATION_UNAVAILABLE".into())
}
fn revision(c:&Connection,id:&str,expected:Option<i64>)->Result<i64,String>{
    let actual=c.query_row("SELECT revision FROM management_zones WHERE id=?1",[id],|r|r.get::<_,i64>(0)).optional().map_err(db_error)?.ok_or("MANAGEMENT_ZONE_NOT_FOUND")?;
    if Some(actual)!=expected{return Err("MANAGEMENT_CHANGED_REFRESH".into());}Ok(actual)
}
fn name(value:Option<&str>)->Result<&str,String>{
    let name=value.map(str::trim).filter(|n|!n.is_empty()&&n.chars().count()<=80&&!n.chars().any(char::is_control)).ok_or("MANAGEMENT_NAME_INVALID")?;Ok(name)
}
fn manager(c:&Connection,id:&str)->Result<(),String>{
    let row=conversation(c,id)?;
    if row.provider=="PI"{return Err("MANAGEMENT_MANAGER_UNSUPPORTED".into());}
    if c.query_row("SELECT EXISTS(SELECT 1 FROM management_members WHERE thread_id=?1)",[id],|r|r.get::<_,bool>(0)).map_err(db_error)?{return Err("MANAGEMENT_MANAGER_IS_EXECUTOR".into());}
    Ok(())
}
pub(super) fn assigned_manager(c:&Connection,thread:&str)->Result<Option<String>,String>{
    c.query_row("SELECT z.manager_id FROM management_members m JOIN management_zones z ON z.id=m.zone_id WHERE m.thread_id=?1",[thread],|r|r.get::<_,Option<String>>(0)).optional().map(Option::flatten).map_err(db_error)
}
pub(super) fn validate_subscription(c:&Connection,s:&codex_notices::Subscription)->Result<(),String>{
    if s.status!="ACTIVE"{return Err("MANAGEMENT_ROUTE_CHANGED_REFRESH".into());}
    let zone=s.management_zone_id.as_deref().ok_or("MANAGEMENT_ROUTE_INVALID")?;
    let(row,epoch):(String,i64)=c.query_row("SELECT z.manager_id,z.manager_epoch FROM management_zones z JOIN management_members m ON m.zone_id=z.id WHERE z.id=?1 AND m.thread_id=?2",params![zone,s.binding.thread_id],|r|Ok((r.get(0)?,r.get(1)?))).map_err(|_|"MANAGEMENT_ROUTE_CHANGED_REFRESH")?;
    let expected=if s.receiver_provider=="CLAUDE_CODE"{format!("claude_code:{}",s.binding.receiver_sid)}else{s.binding.receiver_sid.clone()};
    let manager=conversation(c,&row)?;let source=conversation(c,&s.binding.thread_id)?;
    if !source.watch_enabled{return Err("MANAGEMENT_SOURCE_NOT_WATCHED".into());}
    if row!=expected||Some(epoch)!=s.management_revision||manager.cwd!=s.binding.receiver_cwd||source.cwd!=s.binding.source_cwd||source.generation!=s.binding.generation{return Err("MANAGEMENT_ROUTE_CHANGED_REFRESH".into());}
    Ok(())
}
pub(super) fn validate_reply_route(c:&Connection,reply:&str)->Result<(),String>{
    let row=c.query_row("SELECT s.*,n.text AS fixed_text,n.status AS notice_status,w.text AS attempted_text,w.mode AS reply_mode,w.options_json AS reply_options FROM management_codex_attempts a JOIN claude_codex_notices n ON n.id=a.notice_id JOIN claude_codex_subscriptions s ON s.id=n.subscription_id JOIN watch_replies w ON w.id=a.reply_id WHERE a.reply_id=?1",[reply],|r|Ok((codex_notices::subscription(r)?,r.get::<_,String>("fixed_text")?,r.get::<_,String>("notice_status")?,r.get::<_,String>("attempted_text")?,r.get::<_,String>("reply_mode")?,r.get::<_,String>("reply_options")?))).optional().map_err(db_error)?;
    if let Some((subscription,fixed,status,attempted,mode,options))=row{
        validate_subscription(c,&subscription)?;
        let options:Value=serde_json::from_str(&options).map_err(|_|"MANAGEMENT_RECEIPT_INVALID")?;
        if subscription.receiver_provider!="CODEX"||fixed!=attempted||status!="UNKNOWN"||!matches!(mode.as_str(),"SEND"|"STEER")||options!=json!({"model":null,"effort":null,"attachments":[]}){return Err("MANAGEMENT_RECEIPT_INVALID".into());}
    }Ok(())
}
fn rehome_ready(c:&Connection)->Result<(),String>{
    let mut q=c.prepare("SELECT old.id,current.id FROM claude_codex_notices old JOIN claude_codex_subscriptions previous ON previous.id=old.subscription_id JOIN management_members m ON m.thread_id=previous.thread_id JOIN claude_codex_subscriptions current ON current.thread_id=m.thread_id AND current.management_zone_id=m.zone_id AND current.status='ACTIVE' AND current.generation=previous.generation AND current.source_cwd=previous.source_cwd WHERE old.status='READY' AND previous.status!='ACTIVE' AND previous.id!=current.id ORDER BY old.created_at LIMIT 512").map_err(db_error)?;
    let rows=q.query_map([],|r|Ok((r.get::<_,String>(0)?,r.get::<_,String>(1)?))).map_err(db_error)?.collect::<Result<Vec<_>,_>>().map_err(db_error)?;drop(q);
    for(old,current)in rows{
        let n=c.query_row("SELECT * FROM claude_codex_notices WHERE id=?1",[&old],codex_notices::notice).map_err(db_error)?;
        let s=c.query_row("SELECT * FROM claude_codex_subscriptions WHERE id=?1",[&current],codex_notices::subscription).map_err(db_error)?;
        if validate_subscription(c,&s).is_err(){continue;}
        codex_notices::enqueue(c,&s,&n.turn_id,&n.kind,&n.source_state,n.request_id.as_deref(),n.created_at)?;
        let copied:bool=c.query_row("SELECT EXISTS(SELECT 1 FROM claude_codex_notices WHERE subscription_id=?1 AND turn_id=?2 AND kind=?3 AND source_state=?4 AND request_id IS ?5)",params![current,n.turn_id,n.kind,n.source_state,n.request_id],|r|r.get(0)).map_err(db_error)?;
        if copied{c.execute("UPDATE claude_codex_notices SET status='OBSOLETE' WHERE id=?1 AND status='READY'",[old]).map_err(db_error)?;}
    }Ok(())
}
fn sync_zone(c:&Connection,zone:&str)->Result<(),String>{
    let target:Option<(Option<String>,i64)>=c.query_row("SELECT manager_id,manager_epoch FROM management_zones WHERE id=?1",[zone],|r|Ok((r.get(0)?,r.get(1)?))).optional().map_err(db_error)?;
    let Some((Some(target),epoch))=target else{return Ok(())};let Ok(receiver)=conversation(c,&target)else{return Ok(())};
    let sid=target.strip_prefix("claude_code:").unwrap_or(&target);
    let mut q=c.prepare("SELECT thread_id FROM management_members WHERE zone_id=?1 ORDER BY joined_at,thread_id").map_err(db_error)?;
    let sources=q.query_map([zone],|r|r.get::<_,String>(0)).map_err(db_error)?.collect::<Result<Vec<_>,_>>().map_err(db_error)?;drop(q);
    for thread in sources{
        let Ok(source)=conversation(c,&thread)else{continue};
        let id=length_prefixed_hash(&[b"management-route-v1",zone.as_bytes(),&epoch.to_be_bytes(),thread.as_bytes(),&source.generation.to_be_bytes(),source.cwd.as_bytes(),receiver.cwd.as_bytes()]);
        if c.query_row("SELECT EXISTS(SELECT 1 FROM claude_codex_subscriptions WHERE id=?1 AND status='ACTIVE')",[&id],|r|r.get::<_,bool>(0)).map_err(db_error)?{continue;}
        let active:i64=c.query_row("SELECT count(*) FROM claude_codex_subscriptions WHERE receiver_sid=?1 AND receiver_provider=?2 AND status='ACTIVE' AND thread_id!=?3",params![sid,receiver.provider,thread],|r|r.get(0)).map_err(db_error)?;
        if active>=128{return Err("MANAGEMENT_MANAGER_CAPACITY".into());}
        let mut q=c.prepare("SELECT n.* FROM claude_codex_notices n JOIN claude_codex_subscriptions s ON s.id=n.subscription_id WHERE s.thread_id=?1 AND s.generation=?2 AND s.source_cwd=?3 AND s.status='ACTIVE' AND n.status='READY' ORDER BY n.created_at").map_err(db_error)?;
        let waiting=q.query_map(params![thread,source.generation,source.cwd],codex_notices::notice).map_err(db_error)?.collect::<Result<Vec<_>,_>>().map_err(db_error)?;drop(q);
        // Retire only unattempted dispatch; UNKNOWN retains its original exact
        // recipient/claim and must settle before the same event is handed over.
        c.execute("UPDATE claude_codex_notices SET status='OBSOLETE' WHERE status='READY' AND subscription_id IN(SELECT id FROM claude_codex_subscriptions WHERE thread_id=?1 AND status='ACTIVE')",[&thread]).map_err(db_error)?;
        c.execute("UPDATE claude_codex_subscriptions SET status='CANCELLED' WHERE thread_id=?1 AND status='ACTIVE'",[&thread]).map_err(db_error)?;
        c.execute("INSERT INTO claude_codex_subscriptions(id,receiver_sid,receiver_cwd,thread_id,generation,source_cwd,anchor_turn,mode,expires_at,created_at,management_zone_id,management_revision,receiver_provider) VALUES(?1,?2,?3,?4,?5,?6,'await-first-turn','TASK',253402300799000,?7,?8,?9,?10) ON CONFLICT(id) DO UPDATE SET status='ACTIVE'",params![id,sid,receiver.cwd,thread,source.generation,source.cwd,now(),zone,epoch,receiver.provider]).map_err(db_error)?;
        c.execute("UPDATE management_members SET generation=?2 WHERE thread_id=?1",params![thread,source.generation]).map_err(db_error)?;
        let subscription=c.query_row("SELECT * FROM claude_codex_subscriptions WHERE id=?1",[&id],codex_notices::subscription).map_err(db_error)?;
        for n in waiting{codex_notices::enqueue(c,&subscription,&n.turn_id,&n.kind,&n.source_state,n.request_id.as_deref(),n.created_at)?;}
    }
    Ok(())
}
impl RouterStore {
    pub fn require_management_intent(&self,reply:&str)->Result<(),String>{self.with_connection(|c|{
        if !c.query_row("SELECT EXISTS(SELECT 1 FROM management_codex_attempts WHERE reply_id=?1)",[reply],|r|r.get::<_,bool>(0)).map_err(db_error)?{return Err("MANAGEMENT_RECEIPT_INVALID".into());}
        validate_reply_route(c,reply)
    })}
    pub fn management_active(&self)->Result<bool,String>{self.with_connection(|c|c.query_row("SELECT EXISTS(SELECT 1 FROM management_zones)",[],|r|r.get(0)).map_err(db_error))}
    pub fn management_assignment(&self,thread:&str)->Result<Option<Value>,String>{self.with_connection(|c|{
        let row:Option<(String,Option<String>,i64)>=c.query_row("SELECT z.id,z.manager_id,z.revision FROM management_members m JOIN management_zones z ON z.id=m.zone_id WHERE m.thread_id=?1",[thread],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).optional().map_err(db_error)?;
        Ok(row.map(|(id,manager,revision)|{let cwd=manager.as_deref().and_then(|id|conversation(c,id).ok()).map(|r|r.cwd);json!({"zoneId":id,"managerId":manager,"managerCwd":cwd,"revision":revision})}))
    })}
    pub fn management_codex_deliveries(&self)->Result<Vec<(codex_notices::Notice,codex_notices::Subscription,Option<String>,i64)>,String>{self.with_connection(|c|{
        let mut q=c.prepare("SELECT n.id,a.reply_id,COALESCE(a.attempt,1) FROM claude_codex_notices n JOIN claude_codex_subscriptions s ON s.id=n.subscription_id LEFT JOIN management_codex_attempts a ON a.notice_id=n.id WHERE s.receiver_provider='CODEX' AND s.management_zone_id IS NOT NULL AND ((n.status='READY' AND s.status='ACTIVE') OR n.status='UNKNOWN') AND COALESCE(a.retry_at,0)<=?1 ORDER BY CASE n.kind WHEN 'QUESTION' THEN 0 ELSE 1 END,n.created_at LIMIT 16").map_err(db_error)?;
        let rows=q.query_map([now()],|r|Ok((r.get::<_,String>(0)?,r.get::<_,Option<String>>(1)?,r.get::<_,i64>(2)?))).map_err(db_error)?.collect::<Result<Vec<_>,_>>().map_err(db_error)?;drop(q);
        rows.into_iter().map(|(id,reply,attempt)|{let n=c.query_row("SELECT * FROM claude_codex_notices WHERE id=?1",[id],codex_notices::notice).map_err(db_error)?;let s=c.query_row("SELECT * FROM claude_codex_subscriptions WHERE id=?1",[&n.subscription_id],codex_notices::subscription).map_err(db_error)?;Ok((n,s,reply,attempt))}).collect()
    })}
    pub fn claim_management_codex_notice(&self,id:&str,intent:&super::watch_reply::WatchReply)->Result<bool,String>{self.with_connection(|c|{
        if Uuid::parse_str(&intent.id).is_err(){return Err("MANAGEMENT_RECEIPT_INVALID".into());}
        let tx=c.transaction_with_behavior(TransactionBehavior::Immediate).map_err(db_error)?;
        let n=tx.query_row("SELECT * FROM claude_codex_notices WHERE id=?1",[id],codex_notices::notice).map_err(db_error)?;
        let s=tx.query_row("SELECT * FROM claude_codex_subscriptions WHERE id=?1",[&n.subscription_id],codex_notices::subscription).map_err(db_error)?;
        if s.receiver_provider!="CODEX"||n.status!="READY"{return Ok(false);}validate_subscription(&tx,&s)?;
        let receiver=conversation(&tx,&s.binding.receiver_sid)?;
        if !receiver.available||now()-receiver.checked_at>30000{return Ok(false);}
        if intent.thread_id!=s.binding.receiver_sid||intent.cwd!=receiver.cwd||intent.generation!=receiver.generation||intent.text!=n.text||!matches!(intent.mode.as_str(),"SEND"|"STEER")||intent.source_sequence.is_some(){return Err("MANAGEMENT_RECEIPT_INVALID".into());}
        super::watch_reply::prepare_watch_reply_at(&tx,intent)?;
        if tx.execute("UPDATE claude_codex_notices SET status='UNKNOWN' WHERE id=?1 AND status='READY'",[id]).map_err(db_error)?!=1{return Ok(false);}
        tx.execute("INSERT INTO management_codex_attempts(notice_id,reply_id) VALUES(?1,?2) ON CONFLICT(notice_id) DO UPDATE SET reply_id=excluded.reply_id",params![id,intent.id]).map_err(db_error)?;
        tx.commit().map_err(db_error)?;Ok(true)
    })}
    pub fn reconcile_management_codex_notice(&self,id:&str,reply:&str)->Result<(),String>{self.with_connection(|c|{
        let tx=c.transaction_with_behavior(TransactionBehavior::Immediate).map_err(db_error)?;
        let row:Option<(String,Option<String>)>=tx.query_row("SELECT status,turn_id FROM watch_replies WHERE id=?1",[reply],|r|Ok((r.get(0)?,r.get(1)?))).optional().map_err(db_error)?;
        if let Some((status,turn))=row{
            if status=="SENT"{tx.execute("UPDATE claude_codex_notices SET status='DELIVERED',native_message_id=?2 WHERE id=?1 AND status='UNKNOWN'",params![id,turn.unwrap_or_else(||reply.into())]).map_err(db_error)?;}
            else if matches!(status.as_str(),"CANCELLED"|"ACKNOWLEDGED"){tx.execute("UPDATE claude_codex_notices SET status='OBSOLETE' WHERE id=?1 AND status='UNKNOWN'",[id]).map_err(db_error)?;}
            else if status=="FAILED"{
                let attempt:i64=tx.query_row("SELECT attempt FROM management_codex_attempts WHERE notice_id=?1 AND reply_id=?2",params![id,reply],|r|r.get(0)).map_err(db_error)?;
                if attempt<3{
                    tx.execute("UPDATE management_codex_attempts SET attempt=attempt+1,retry_at=?2 WHERE notice_id=?1",params![id,now()+if attempt==1{5000}else{15000}]).map_err(db_error)?;
                    tx.execute("UPDATE claude_codex_notices SET status='READY' WHERE id=?1 AND status='UNKNOWN'",[id]).map_err(db_error)?;
                }else{
                    tx.execute("UPDATE claude_codex_notices SET status='FAILED' WHERE id=?1 AND status='UNKNOWN'",[id]).map_err(db_error)?;
                }
            }
            // Native SENDING/UNKNOWN is reconciled, never replayed.
        }
        tx.commit().map_err(db_error)
    })}
    pub fn management_queued_intent(&self,id:&str,reply:&str)->Result<Option<super::watch_reply::WatchReply>,String>{self.with_connection(|c|{
        let tx=c.transaction_with_behavior(TransactionBehavior::Immediate).map_err(db_error)?;
        let row:Option<String>=tx.query_row("SELECT w.status FROM management_codex_attempts a JOIN watch_replies w ON w.id=a.reply_id WHERE a.notice_id=?1 AND a.reply_id=?2",params![id,reply],|r|r.get(0)).optional().map_err(db_error)?;
        if row.as_deref()!=Some("QUEUED"){return Ok(None);}
        let n=tx.query_row("SELECT * FROM claude_codex_notices WHERE id=?1",[id],codex_notices::notice).map_err(db_error)?;
        let s=tx.query_row("SELECT * FROM claude_codex_subscriptions WHERE id=?1",[&n.subscription_id],codex_notices::subscription).map_err(db_error)?;
        if validate_subscription(&tx,&s).is_err(){
            tx.execute("UPDATE watch_replies SET status='FAILED',error_code='MANAGEMENT_ROUTE_CHANGED',updated_at=?2 WHERE id=?1 AND status='QUEUED'",params![reply,now()]).map_err(db_error)?;
            tx.execute("UPDATE claude_codex_notices SET status='READY' WHERE id=?1 AND status='UNKNOWN'",[id]).map_err(db_error)?;
            tx.commit().map_err(db_error)?;return Ok(None);
        }
        // The caller reads the immutable intent only after releasing this lock.
        tx.commit().map_err(db_error)?;Ok(Some(()))
    }).and_then(|ready|if ready.is_some(){self.watch_reply(reply)}else{Ok(None)})}
    pub fn management_has_source(&self,thread:&str)->Result<bool,String>{self.with_connection(|c|c.query_row("SELECT EXISTS(SELECT 1 FROM management_members WHERE thread_id=?1)",[thread],|r|r.get(0)).map_err(db_error))}
    pub fn publish_management_event(&self,thread:&str,generation:i64,turn:&str,kind:&str,state:&str,request:Option<&str>,at:i64)->Result<(),String>{self.with_connection(|c|{
        let mut q=c.prepare("SELECT * FROM claude_codex_subscriptions WHERE thread_id=?1 AND generation=?2 AND management_zone_id IS NOT NULL AND status='ACTIVE'").map_err(db_error)?;
        let rows=q.query_map(params![thread,generation],codex_notices::subscription).map_err(db_error)?.collect::<Result<Vec<_>,_>>().map_err(db_error)?;drop(q);
        for row in rows{codex_notices::enqueue(c,&row,turn,kind,state,request,at)?;}Ok(())
    })}
    pub fn sync_management_routes(&self)->Result<(),String>{self.with_connection(|c|{
        let tx=c.transaction_with_behavior(TransactionBehavior::Immediate).map_err(db_error)?;
        let mut q=tx.prepare("SELECT id FROM management_zones ORDER BY created_at LIMIT 32").map_err(db_error)?;
        let zones=q.query_map([],|r|r.get::<_,String>(0)).map_err(db_error)?.collect::<Result<Vec<_>,_>>().map_err(db_error)?;drop(q);
        for zone in zones{sync_zone(&tx,&zone)?;}rehome_ready(&tx)?;tx.commit().map_err(db_error)
    })}
    /// Scalar metadata only. Native Codex questions are observed by the existing
    /// adapter poll. Claude/Pi do not require an extra model or transcript read.
    pub fn observe_management_passive(&self)->Result<(),String>{self.with_connection(|c|{
        let mut q=c.prepare("SELECT m.thread_id,m.generation,COALESCE(json_extract(w.snapshot_json,'$.state'),'UNKNOWN'),json_extract(w.snapshot_json,'$.turnId'),w.checked_at FROM management_members m JOIN codex_watches w ON w.thread_id=m.thread_id WHERE w.enabled=1 AND (m.thread_id LIKE 'claude_code:%' OR m.thread_id LIKE 'pi:%') ORDER BY m.joined_at LIMIT 512").map_err(db_error)?;
        let sources=q.query_map([],|r|Ok((r.get::<_,String>(0)?,r.get::<_,i64>(1)?,r.get::<_,String>(2)?,r.get::<_,Option<String>>(3)?,r.get::<_,i64>(4)?))).map_err(db_error)?.collect::<Result<Vec<_>,_>>().map_err(db_error)?;drop(q);
        for(thread,generation,state,turn,at)in sources{
            let mut q=c.prepare("SELECT * FROM claude_codex_subscriptions WHERE thread_id=?1 AND generation=?2 AND management_zone_id IS NOT NULL AND status='ACTIVE'").map_err(db_error)?;
            let subscriptions=q.query_map(params![thread,generation],codex_notices::subscription).map_err(db_error)?.collect::<Result<Vec<_>,_>>().map_err(db_error)?;drop(q);
            for sub in subscriptions{
                if validate_subscription(c,&sub).is_err(){continue;}
                if let Some(native)=thread.strip_prefix("pi:"){
                    let record:Option<(String,Option<String>,i64)>=c.query_row("SELECT state,json_extract(result_json,'$.nativeTerminalReceiptId'),updated_at FROM pi_tasks WHERE native_session_id=?1 AND file_verified=1 AND generation=?2",params![native,generation],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).optional().map_err(db_error)?;
                    if let Some((state,Some(receipt),at))=record{if matches!(state.as_str(),"RESULT_READY"|"FAILED"|"PARTIAL"|"CANCELLED"){codex_notices::enqueue(c,&sub,&receipt,"PI_RESULT",&state,None,at)?;}}
                    let mut requests=c.prepare("SELECT q.runtime_epoch,q.request_id,q.observed_at FROM pi_owner_questions q JOIN pi_tasks t ON t.id=q.task_id WHERE t.native_session_id=?1 AND q.generation=?2 AND q.runtime_epoch=t.runtime_epoch ORDER BY q.observed_at LIMIT 32").map_err(db_error)?;
                    let requests=requests.query_map(params![native,generation],|r|Ok((r.get::<_,String>(0)?,r.get::<_,String>(1)?,r.get::<_,i64>(2)?))).map_err(db_error)?.collect::<Result<Vec<_>,_>>().map_err(db_error)?;
                    for(epoch,request,at)in requests{codex_notices::enqueue(c,&sub,&epoch,"PI_QUESTION","NATIVE_UI_UNAVAILABLE",Some(&request),at)?;}
                }else if let Some(sid)=thread.strip_prefix("claude_code:"){
                    if matches!(state.as_str(),"RESULT_READY"|"FAILED"|"INTERRUPTED"){if let Some(turn)=turn.as_deref().filter(|v|!v.is_empty()){codex_notices::enqueue(c,&sub,turn,"TURN_RESULT",&state,None,at)?;}}
                    let mut requests=c.prepare("SELECT id,tool_use_id,created_at FROM claude_owner_requests WHERE root=?1 AND generation=?2 AND status='PENDING' AND session_id IN(SELECT session_id FROM claude_sessions WHERE lease_until>?3) ORDER BY created_at LIMIT 32").map_err(db_error)?;
                    let requests=requests.query_map(params![thread,generation,now()],|r|Ok((r.get::<_,String>(0)?,r.get::<_,String>(1)?,r.get::<_,i64>(2)?))).map_err(db_error)?.collect::<Result<Vec<_>,_>>().map_err(db_error)?;
                    let _=sid;
                    for(id,native,at)in requests{codex_notices::enqueue(c,&sub,turn.as_deref().unwrap_or(""),"QUESTION","ACTION_REQUIRED",Some(if native.is_empty(){&id}else{&native}),at)?;}
                }
            }
        }Ok(())
    })}
    pub fn management_zones(&self)->Result<Value,String>{self.with_connection(|c|{
        let mut statement=c.prepare("SELECT id,name,manager_id,revision,updated_at FROM management_zones ORDER BY created_at,id").map_err(db_error)?;
        let zones=statement.query_map([],|r|Ok((r.get::<_,String>(0)?,r.get::<_,String>(1)?,r.get::<_,Option<String>>(2)?,r.get::<_,i64>(3)?,r.get::<_,i64>(4)?))).map_err(db_error)?.collect::<Result<Vec<_>,_>>().map_err(db_error)?;
        let mut result=Vec::new();
        for(id,name,manager_id,revision,updated_at)in zones{
            let mut members=c.prepare("SELECT thread_id,generation FROM management_members WHERE zone_id=?1 ORDER BY joined_at,thread_id").map_err(db_error)?;
            let members=members.query_map([&id],|r|Ok((r.get::<_,String>(0)?,r.get::<_,i64>(1)?))).map_err(db_error)?.collect::<Result<Vec<_>,_>>().map_err(db_error)?;
            let children:Vec<_>=members.into_iter().map(|(thread,generation)|match conversation(c,&thread){Ok(row)=>json!({"threadId":thread,"generation":generation,"conversation":row}),Err(_)=>json!({"threadId":thread,"generation":generation,"conversation":null})}).collect();
            let manager=manager_id.as_deref().and_then(|id|conversation(c,id).ok());
            result.push(json!({"id":id,"name":name,"managerId":manager_id,"manager":manager,"revision":revision,"updatedAt":updated_at,"members":children}));
        }
        let mut ids=c.prepare("SELECT thread_id FROM codex_watches WHERE enabled=1 UNION SELECT 'claude_code:'||session_id FROM claude_sessions WHERE state!='SESSION_END' ORDER BY 1 LIMIT 1024").map_err(db_error)?;
        let ids=ids.query_map([],|r|r.get::<_,String>(0)).map_err(db_error)?.collect::<Result<Vec<_>,_>>().map_err(db_error)?;
        let candidates:Vec<_>=ids.into_iter().filter_map(|id|conversation(c,&id).ok()).collect();
        // Existing subscriptions are proposals for adoption, not implicit ownership.
        let mut legacy=c.prepare("SELECT DISTINCT receiver_sid,thread_id FROM claude_codex_subscriptions WHERE status='ACTIVE' AND expires_at>?1 AND NOT EXISTS(SELECT 1 FROM management_members m WHERE m.thread_id=claude_codex_subscriptions.thread_id) ORDER BY receiver_sid,thread_id LIMIT 1024").map_err(db_error)?;
        let legacy=legacy.query_map([now()],|r|Ok(json!({"managerId":format!("claude_code:{}",r.get::<_,String>(0)?),"threadId":r.get::<_,String>(1)?}))).map_err(db_error)?.collect::<Result<Vec<_>,_>>().map_err(db_error)?;
        Ok(json!({"zones":result,"candidates":candidates,"legacyAssignments":legacy}))
    })}
    /// Call only after authenticating a desktop/PWA owner. Not an MCP operation.
    pub fn change_management_zone(&self,input:&Change)->Result<Value,String>{self.with_connection(|c|{
        if Uuid::parse_str(&input.request_id).is_err(){return Err("MANAGEMENT_REQUEST_INVALID".into());}
        let hash=format!("{:x}",Sha256::digest(serde_json::to_vec(input).map_err(|_|"MANAGEMENT_REQUEST_INVALID")?));
        let tx=c.transaction_with_behavior(TransactionBehavior::Immediate).map_err(db_error)?;
        if let Some((old,receipt))=tx.query_row("SELECT payload_hash,receipt_json FROM management_changes WHERE request_id=?1",[&input.request_id],|r|Ok((r.get::<_,String>(0)?,r.get::<_,String>(1)?))).optional().map_err(db_error)?{
            if old!=hash{return Err("MANAGEMENT_REQUEST_CONFLICT".into());}return serde_json::from_str(&receipt).map_err(|_|"MANAGEMENT_RECEIPT_INVALID".into());
        }
        let at=now();let mut source_zone=None;
        let id=if matches!(input.action.as_str(),"CREATE"|"ADOPT"){
            let label=name(input.name.as_deref())?;
            if tx.query_row("SELECT count(*) FROM management_zones",[],|r|r.get::<_,i64>(0)).map_err(db_error)?>=32{return Err("MANAGEMENT_ZONE_LIMIT".into());}
            manager(&tx,input.manager_id.as_deref().ok_or("MANAGEMENT_CONVERSATION_INVALID")?)?;
            let id=Uuid::new_v4().to_string();
            tx.execute("INSERT INTO management_zones(id,name,manager_id,created_at,updated_at) VALUES(?1,?2,?3,?4,?4)",params![id,label,input.manager_id,at]).map_err(db_error)?;
            if input.action=="ADOPT"{
                let manager=input.manager_id.as_deref().and_then(|id|id.strip_prefix("claude_code:")).ok_or("MANAGEMENT_ADOPTION_INVALID")?;
                let threads=input.thread_ids.as_ref().filter(|v|!v.is_empty()&&v.len()<=128).ok_or("MANAGEMENT_ADOPTION_INVALID")?;
                for thread in threads{
                    if tx.query_row("SELECT EXISTS(SELECT 1 FROM management_members WHERE thread_id=?1 UNION SELECT 1 FROM management_zones WHERE manager_id=?1)",[thread],|r|r.get::<_,bool>(0)).map_err(db_error)?{return Err("MANAGEMENT_ALREADY_ASSIGNED_USE_TRANSFER".into());}
                    if !tx.query_row("SELECT EXISTS(SELECT 1 FROM claude_codex_subscriptions WHERE receiver_sid=?1 AND thread_id=?2 AND status='ACTIVE' AND expires_at>?3)",params![manager,thread,at],|r|r.get::<_,bool>(0)).map_err(db_error)?{return Err("MANAGEMENT_ADOPTION_CHANGED_REFRESH".into());}
                    let source=conversation(&tx,thread)?;
                    tx.execute("INSERT INTO management_members(thread_id,zone_id,generation,joined_at) VALUES(?1,?2,?3,?4)",params![thread,id,source.generation,at]).map_err(db_error)?;
                }
            }
            id
        }else{
            let id=input.zone_id.as_deref().ok_or("MANAGEMENT_ZONE_NOT_FOUND")?;
            revision(&tx,id,input.revision)?;
            match input.action.as_str(){
                "RENAME"=>{tx.execute("UPDATE management_zones SET name=?2 WHERE id=?1",params![id,name(input.name.as_deref())?]).map_err(db_error)?;},
                "APPOINT"=>{let target=input.manager_id.as_deref().ok_or("MANAGEMENT_CONVERSATION_INVALID")?;manager(&tx,target)?;tx.execute("UPDATE management_zones SET manager_epoch=manager_epoch+CASE WHEN manager_id IS ?2 THEN 0 ELSE 1 END,manager_id=?2 WHERE id=?1",params![id,target]).map_err(db_error)?;},
                "ADD"|"MOVE"=>{
                    let thread=input.thread_id.as_deref().ok_or("MANAGEMENT_CONVERSATION_INVALID")?;
                    let source=conversation(&tx,thread)?;
                    if !source.watch_enabled{return Err("MANAGEMENT_ENABLE_WATCH_FIRST".into());}
                    if Some(source.generation)!=input.generation{return Err("MANAGEMENT_SOURCE_CHANGED_REFRESH".into());}
                    if tx.query_row("SELECT EXISTS(SELECT 1 FROM management_zones WHERE manager_id=?1)",[thread],|r|r.get::<_,bool>(0)).map_err(db_error)?{return Err("MANAGEMENT_EXECUTOR_IS_MANAGER".into());}
                    let old=tx.query_row("SELECT zone_id FROM management_members WHERE thread_id=?1",[thread],|r|r.get::<_,String>(0)).optional().map_err(db_error)?;
                    if input.action=="ADD"&&old.is_some(){return Err("MANAGEMENT_ALREADY_ASSIGNED_USE_TRANSFER".into());}
                    if input.action=="MOVE"{
                        let old=old.ok_or("MANAGEMENT_MEMBERSHIP_NOT_FOUND")?;
                        if input.from_zone_id.as_deref()!=Some(old.as_str()){return Err("MANAGEMENT_CHANGED_REFRESH".into());}
                        if old==id{return Err("MANAGEMENT_ALREADY_ASSIGNED".into());}
                        revision(&tx,&old,input.from_revision)?;source_zone=Some(old);
                    }
                    tx.execute("INSERT INTO management_members(thread_id,zone_id,generation,joined_at) VALUES(?1,?2,?3,?4) ON CONFLICT(thread_id) DO UPDATE SET zone_id=excluded.zone_id,generation=excluded.generation,joined_at=excluded.joined_at",params![thread,id,source.generation,at]).map_err(db_error)?;
                },
                "REMOVE"=>{let thread=input.thread_id.as_deref().ok_or("MANAGEMENT_CONVERSATION_INVALID")?;if tx.execute("DELETE FROM management_members WHERE thread_id=?1 AND zone_id=?2",params![thread,id]).map_err(db_error)?!=1{return Err("MANAGEMENT_MEMBERSHIP_NOT_FOUND".into());}tx.execute("UPDATE claude_codex_subscriptions SET status='CANCELLED' WHERE thread_id=?1 AND management_zone_id=?2",params![thread,id]).map_err(db_error)?;tx.execute("UPDATE claude_codex_notices SET status='OBSOLETE' WHERE status='READY' AND subscription_id IN(SELECT id FROM claude_codex_subscriptions WHERE thread_id=?1 AND management_zone_id=?2)",params![thread,id]).map_err(db_error)?;},
                "DELETE"=>{if tx.query_row("SELECT EXISTS(SELECT 1 FROM management_members WHERE zone_id=?1)",[id],|r|r.get::<_,bool>(0)).map_err(db_error)?{return Err("MANAGEMENT_ZONE_NOT_EMPTY".into());}tx.execute("DELETE FROM management_zones WHERE id=?1",[id]).map_err(db_error)?;},
                _=>return Err("MANAGEMENT_ACTION_INVALID".into()),
            }
            tx.execute("UPDATE management_zones SET revision=revision+1,updated_at=?2 WHERE id=?1",params![id,at]).map_err(db_error)?;
            if let Some(old)=&source_zone{tx.execute("UPDATE management_zones SET revision=revision+1,updated_at=?2 WHERE id=?1",params![old,at]).map_err(db_error)?;}
            id.to_string()
        };
        sync_zone(&tx,&id)?;
        if let Some(old)=&source_zone{sync_zone(&tx,old)?;}
        let receipt=json!({"requestId":input.request_id,"zoneId":id,"sourceZoneId":source_zone,"appliedAt":at});
        tx.execute("INSERT INTO management_changes(request_id,payload_hash,receipt_json,created_at) VALUES(?1,?2,?3,?4)",params![input.request_id,hash,receipt.to_string(),at]).map_err(db_error)?;
        tx.commit().map_err(db_error)?;Ok(receipt)
    })}
}

#[cfg(test)]mod tests{
    use super::*;
    fn change(s:&RouterStore,value:Value)->Result<Value,String>{s.change_management_zone(&serde_json::from_value(value).unwrap())}
    fn fixture()->(tempfile::TempDir,RouterStore,String,String,String){
        let d=tempfile::tempdir().unwrap();let s=RouterStore::open_at(d.path().join("zones.db")).unwrap();
        let root=d.path().to_string_lossy().to_string();let pair=s.create_claude_connection().unwrap();
        let sid=Uuid::new_v4().to_string();let instance=Uuid::new_v4().to_string();
        s.ingest_claude_event(&pair.token,&crate::claude_code::Event{event_id:Uuid::new_v4().to_string(),session_id:sid.clone(),instance_id:instance,sequence:1,cwd:root.clone(),version:"2.1.317".into(),kind:"SESSION_START".into(),turn_id:None,text:None,completed_at:None}).unwrap();
        let manager=Uuid::new_v4().to_string();let source=Uuid::new_v4().to_string();
        for(id,label)in [(&manager,"Codex manager"),(&source,"Executor")]{s.enable_codex_watch(id,label,&root,&codex_watch::WatchSnapshot{state:"IDLE".into(),turn_id:None,item_id:None,text:"must not enter topology".into()}).unwrap();}
        (d,s,format!("claude_code:{sid}"),manager,source)
    }
    fn create(s:&RouterStore,label:&str,manager:&str)->String{change(s,json!({"requestId":Uuid::new_v4().to_string(),"action":"CREATE","name":label,"managerId":manager})).unwrap()["zoneId"].as_str().unwrap().into()}
    fn add(s:&RouterStore,zone:&str,source:&str,revision:i64)->Result<Value,String>{change(s,json!({"requestId":Uuid::new_v4().to_string(),"action":"ADD","zoneId":zone,"revision":revision,"threadId":source,"generation":1}))}
    #[test]fn ownership_is_unique_and_transfer_updates_both_zones_atomically(){
        let(_d,s,claude,codex,source)=fixture();let a=create(&s,"Art",&claude);let b=create(&s,"Engineering",&codex);
        add(&s,&a,&source,1).unwrap();assert_eq!(add(&s,&b,&source,1).unwrap_err(),"MANAGEMENT_ALREADY_ASSIGNED_USE_TRANSFER");
        let mut transfer=json!({"requestId":Uuid::new_v4().to_string(),"action":"MOVE","zoneId":b,"revision":1,"fromZoneId":a,"fromRevision":1,"threadId":source,"generation":1});
        assert_eq!(change(&s,transfer.clone()).unwrap_err(),"MANAGEMENT_CHANGED_REFRESH");
        assert_eq!(s.management_zones().unwrap()["zones"][0]["members"][0]["threadId"],source);
        transfer["fromRevision"]=json!(2);change(&s,transfer).unwrap();
        let view=s.management_zones().unwrap();assert_eq!(view["zones"][0]["revision"],3);assert_eq!(view["zones"][1]["revision"],2);
        assert_eq!(view["zones"][0]["members"],json!([]));assert_eq!(view["zones"][1]["members"][0]["threadId"],source);
        assert!(!view.to_string().contains("must not enter topology"));
    }
    #[test]fn requests_are_idempotent_but_changed_payloads_conflict(){
        let(_d,s,claude,_,_)=fixture();let mut input=json!({"requestId":Uuid::new_v4().to_string(),"action":"CREATE","name":"Zone","managerId":claude});
        let first=change(&s,input.clone()).unwrap();assert_eq!(change(&s,input.clone()).unwrap(),first);
        input["name"]=json!("Another");assert_eq!(change(&s,input).unwrap_err(),"MANAGEMENT_REQUEST_CONFLICT");
        assert_eq!(s.management_zones().unwrap()["zones"].as_array().unwrap().len(),1);
    }
    #[test]fn appointment_keeps_unknown_attempt_fenced_and_retargets_unattempted_questions(){
        let(_d,s,claude,codex,source)=fixture();let zone=create(&s,"Zone",&claude);add(&s,&zone,&source,1).unwrap();
        s.publish_management_event(&source,1,"","QUESTION","ACTION_REQUIRED",Some("question-a"),now()).unwrap();
        s.publish_management_event(&source,1,"turn-b","TURN_RESULT","completed",None,now()).unwrap();
        s.with_connection(|c|{c.execute("UPDATE claude_codex_notices SET status='UNKNOWN' WHERE kind='QUESTION'",[]).map_err(db_error)?;Ok(())}).unwrap();
        change(&s,json!({"requestId":Uuid::new_v4().to_string(),"action":"APPOINT","zoneId":zone,"revision":2,"managerId":codex})).unwrap();
        s.publish_management_event(&source,1,"","QUESTION","ACTION_REQUIRED",Some("question-a"),now()).unwrap();
        let deliveries=s.management_codex_deliveries().unwrap();assert_eq!(deliveries.len(),1);assert_eq!(deliveries[0].0.kind,"TURN_RESULT");
        let unknown=s.with_connection(|c|c.query_row("SELECT id FROM claude_codex_notices WHERE status='UNKNOWN'",[],|r|r.get::<_,String>(0)).map_err(db_error)).unwrap();
        s.reconcile_codex_notice(&unknown,&Uuid::new_v4().to_string()).unwrap();
        s.publish_management_event(&source,1,"","QUESTION","ACTION_REQUIRED",Some("question-a"),now()).unwrap();
        assert_eq!(s.management_codex_deliveries().unwrap().len(),2);
        let topology=s.management_zones().unwrap();assert_eq!(topology["zones"][0]["managerId"],codex);
        assert_eq!(s.registered_codex_watch(&source).unwrap().generation,1);
    }
    #[test]fn cannot_make_an_executor_a_manager_or_form_a_notification_cycle(){
        let(_d,s,claude,codex,source)=fixture();let a=create(&s,"One",&claude);let b=create(&s,"Two",&codex);
        add(&s,&a,&source,1).unwrap();
        assert_eq!(change(&s,json!({"requestId":Uuid::new_v4().to_string(),"action":"APPOINT","zoneId":b,"revision":1,"managerId":source})).unwrap_err(),"MANAGEMENT_MANAGER_IS_EXECUTOR");
        assert_eq!(add(&s,&a,&codex,2).unwrap_err(),"MANAGEMENT_EXECUTOR_IS_MANAGER");
    }
    #[test]fn responsibility_and_pending_notices_survive_host_reopen(){
        let(d,s,claude,_,source)=fixture();let zone=create(&s,"Durable",&claude);add(&s,&zone,&source,1).unwrap();
        s.publish_management_event(&source,1,"turn","TURN_RESULT","completed",None,now()).unwrap();drop(s);
        let s=RouterStore::open_at(d.path().join("zones.db")).unwrap();assert_eq!(s.management_zones().unwrap()["zones"][0]["members"].as_array().unwrap().len(),1);
        let subs=s.codex_notice_sources(Some(&source)).unwrap();assert_eq!(subs.len(),1);assert_eq!(subs[0].management_zone_id.as_deref(),Some(zone.as_str()));
    }
    #[test]fn new_claude_manager_offers_claims_and_acks_the_original_pending_question_once(){
        let(d,s,original,_,source)=fixture();let zone=create(&s,"Handover",&original);add(&s,&zone,&source,1).unwrap();
        s.publish_management_event(&source,1,"","QUESTION","ACTION_REQUIRED",Some("exact-request"),now()).unwrap();
        let pair=s.create_claude_connection().unwrap();let sid=Uuid::new_v4().to_string();let instance=Uuid::new_v4().to_string();let root=d.path().to_string_lossy().to_string();
        s.ingest_claude_event(&pair.token,&crate::claude_code::Event{event_id:Uuid::new_v4().to_string(),session_id:sid.clone(),instance_id:instance.clone(),sequence:1,cwd:root.clone(),version:"2.1.317".into(),kind:"SESSION_START".into(),turn_id:None,text:None,completed_at:None}).unwrap();
        change(&s,json!({"requestId":Uuid::new_v4().to_string(),"action":"APPOINT","zoneId":zone,"revision":2,"managerId":format!("claude_code:{sid}")})).unwrap();
        let mut command:codex_notices::Command=serde_json::from_value(json!({"sessionId":sid,"instanceId":instance,"cwd":root,"action":"OFFER"})).unwrap();
        let offered=s.codex_notice_sync(&pair.token,&command).unwrap();assert_eq!(offered["notice"]["requestId"],"exact-request");assert_eq!(offered["notice"]["turnId"],"");
        command.notice_id=offered["notice"]["id"].as_str().map(str::to_owned);command.claim_id=Some(Uuid::new_v4().to_string());command.action="CLAIM".into();
        assert_eq!(s.codex_notice_sync(&pair.token,&command).unwrap()["accepted"],true);
        command.message_id=Some(Uuid::new_v4().to_string());command.action="ACK".into();s.codex_notice_sync(&pair.token,&command).unwrap();
        command.action="OFFER".into();assert!(s.codex_notice_sync(&pair.token,&command).unwrap()["notice"].is_null());
    }
    #[test]fn restart_resumes_only_exact_unadmitted_metadata_and_handover_fences_its_writer(){
        let(d,s,claude,codex,source)=fixture();let zone=create(&s,"Durable notices",&codex);add(&s,&zone,&source,1).unwrap();
        s.publish_management_event(&source,1,"source-turn","TURN_RESULT","completed",None,now()).unwrap();
        let notice=s.management_codex_deliveries().unwrap()[0].0.clone();
        let context=s.registered_codex_watch(&codex).unwrap();let reply=Uuid::new_v4().to_string();
        let intent=super::super::watch_reply::WatchReply{id:reply.clone(),thread_id:codex,cwd:context.cwd,generation:context.generation,source_sequence:None,expected_turn_id:None,mode:"SEND".into(),text:notice.text,options:json!({"model":null,"effort":null,"attachments":[]}),payload_hash:String::new(),status:String::new(),turn_id:None,error_code:None,created_at:0,updated_at:0};
        assert!(s.claim_management_codex_notice(&notice.id,&intent).unwrap());drop(s);
        let s=RouterStore::open_at(d.path().join("zones.db")).unwrap();
        assert_eq!(s.management_queued_intent(&notice.id,&reply).unwrap().unwrap().id,reply);
        change(&s,json!({"requestId":Uuid::new_v4().to_string(),"action":"APPOINT","zoneId":zone,"revision":2,"managerId":claude})).unwrap();
        assert_eq!(s.claim_watch_reply(&reply).unwrap_err(),"MANAGEMENT_ROUTE_CHANGED_REFRESH");
        assert!(s.management_queued_intent(&notice.id,&reply).unwrap().is_none());
        s.sync_management_routes().unwrap();
        assert_eq!(s.watch_reply(&reply).unwrap().unwrap().status,"FAILED");
        let carried=s.with_connection(|c|c.query_row("SELECT count(*) FROM claude_codex_notices n JOIN claude_codex_subscriptions r ON r.id=n.subscription_id WHERE n.status='READY' AND r.status='ACTIVE' AND r.receiver_provider='CLAUDE_CODE'",[],|r|r.get::<_,i64>(0)).map_err(db_error)).unwrap();assert_eq!(carried,1);
    }
}
