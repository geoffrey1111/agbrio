//! Authenticated owner UI surface; deliberately absent from OAuth/MCP actions.
use crate::host_application::RouterCore;
use router_core::store::{claude_manual::{native_id,PREFIX},codex_watch::WatchSnapshot,watch_reply::WatchReply};
use serde_json::{json,Value};
use crate::watch_chat::ChatCommand;

#[derive(serde::Serialize)]
#[serde(rename_all="camelCase")]
pub(crate) struct WatchView {
    #[serde(flatten)] pub watch:router_core::store::codex_watch::CodexWatch,
    #[serde(skip_serializing_if="Option::is_none")] pub task_tree:Option<crate::claude_task_tree::TaskTree>,
    pub last_message_at:Option<i64>,
}
/// Directory projection; a child's activity is not a parent sending state.
pub(crate) fn list(core:&RouterCore)->Result<Vec<WatchView>,String>{
    Ok(core.store.codex_watches()?.into_iter().map(|mut watch|{
        let mut tree=None;
        if watch.enabled {if let Ok(sid)=native_id(&watch.thread_id){
            if let Ok(s)=crate::claude_code::observed_session(core,sid,Some(&watch.cwd)){watch.snapshot=snapshot(&s);watch.checked_at=s.checked_at;}
            tree=Some(crate::claude_task_tree::read(core,sid,&watch.cwd));
        }}
        let last_message_at=core.store.conversation_message_at(&watch.thread_id,&watch.cwd).ok().flatten();
        WatchView{watch,task_tree:tree,last_message_at}
    }).collect())
}

pub(crate) fn snapshot(s:&router_core::claude_code::Session)->WatchSnapshot{
    let state=if s.state=="COMPLETE"&&s.turn_id.as_ref().is_some_and(|t|crate::claude_transcript::awaiting_reply(&s.session_id,t)){"ACTION_REQUIRED"}else{match s.state.as_str(){"COMPLETE"=>"RESULT_READY","WAITING_DECISION"=>"ACTION_REQUIRED","SESSION_START"=>"UNKNOWN","EMPTY"=>"IDLE","LIMITED"=>"INCOMPLETE",v @ ("RUNNING"|"FAILED"|"INTERRUPTED")=>v,_=>"UNKNOWN"}};
    let reply=s.latest_reply.as_ref().filter(|r|s.state=="COMPLETE"&&Some(&r.turn_id)==s.turn_id.as_ref());
    let native=crate::claude_transcript::latest(&s.session_id).filter(|p|p.cwd.as_ref().is_some_and(|cwd|router_core::claude_code::same_root(cwd,&s.cwd))).and_then(|p|p.turns.into_iter().find(|t|Some(&t.id)==s.turn_id.as_ref())).and_then(|t|t.messages.into_iter().rev().find(|m|m.role=="assistant"&&m.kind=="PUBLIC"));
    WatchSnapshot{state:state.into(),turn_id:s.turn_id.clone(),item_id:native.as_ref().map(|m|m.id.clone()).or_else(||reply.map(|r|format!("claude_reply:{}",r.turn_id))),text:native.map(|m|m.text).or_else(||reply.map(|r|r.text.clone())).unwrap_or_default()}
}
pub(crate) fn enable(core:&RouterCore,key:&str)->Result<(),String>{
    let sid=native_id(key)?;
    let title=crate::claude_code::catalog(core)?.sessions.into_iter().find(|s|s.session_id==sid).and_then(|s|s.title).unwrap_or_else(||"未命名 Claude Code 对话".into());
    match crate::claude_code::observed_session(core,sid,None) {
        Ok(session)=>core.store.enable_codex_watch(key,&title,&session.cwd,&snapshot(&session)),
        // Not reported yet: watch the exact desktop conversation; events fill it in.
        Err(_)=>core.store.enable_codex_watch(key,&title,&crate::claude_code::selectable_cwd(core,sid)?,&WatchSnapshot{state:"UNKNOWN".into(),turn_id:None,item_id:None,text:String::new()}),
    }
}
pub(crate) fn state(core:&RouterCore,key:&str)->Result<Value,String>{
    core.store.expire_claude_manual_offers()?;
    let mut watch=core.store.codex_chat_context(key)?;
    // Before the conversation reports, the watch shows what it has and input stays off.
    let session=crate::claude_code::observed_session(core,native_id(key)?,Some(&watch.cwd)).ok();
    if let Some(s)=&session {watch.snapshot=snapshot(s);watch.checked_at=s.checked_at;}
    let ready=match &session {Some(s)=>core.store.claude_manual_ready(&s.session_id,&s.instance_id)?,None=>false};
    let state=session.as_ref().map(|s|s.state.as_str()).unwrap_or("");
    let task_tree=crate::claude_task_tree::read(core,native_id(key)?,&watch.cwd);
    let page=crate::claude_transcript::latest(native_id(key)?).filter(|p|p.cwd.as_ref().is_some_and(|cwd|router_core::claude_code::same_root(cwd,&watch.cwd)));
    let messages:Vec<Value>=page.as_ref().map(|p|p.turns.iter().rev().flat_map(|t|t.messages.iter().map(|m|json!({"id":m.id,"turnId":t.id,"role":m.role,"text":m.text,"kind":m.kind,"card":m.card,"timestamp":m.at,"order":m.order,"revision":p.version}))).collect()).unwrap_or_default();
    let history_revision={use std::hash::{Hash,Hasher};let mut hash=std::collections::hash_map::DefaultHasher::new();json!(messages).to_string().hash(&mut hash);format!("{:x}",hash.finish())};
    let native_ids=core.store.claude_manual_message_ids(key)?;
    let replies=core.store.watch_replies(key)?.into_iter().map(|r|{let native=native_ids.get(&r.id);let mut row=json!(r);if let Some(id)=native{row["nativeMessageId"]=json!(id);}row}).collect::<Vec<_>>();
    Ok(json!({"watch":watch,"provider":"CLAUDE_CODE","inputReady":ready,"host":std::env::var("COMPUTERNAME").unwrap_or_default(),"ownedTurnId":null,"controllableTurnId":null,"externalBusy":matches!(state,"RUNNING"|"WAITING_DECISION"),"replies":replies,"requests":[],"goal":null,"publicMessages":messages,"historyRevision":history_revision,"taskTree":task_tree,"ownerRequests":core.store.claude_owner_requests(key,watch.generation,native_id(key)?)?,"childResultsEnabled":core.store.child_results_enabled(key)?,"activity":if state=="RUNNING"{Some("EXECUTING")}else{None},"checkedAt":session.as_ref().map(|s|s.checked_at).unwrap_or(watch.checked_at)}))
}
pub(crate) fn command(core:&RouterCore,c:ChatCommand)->Result<Value,String>{
    let encode=|r:WatchReply|serde_json::to_value(r).map_err(|_|"CLAUDE_RESPONSE_INVALID".to_string());
    match c {
        ChatCommand::Send{thread_id,id,generation,source_sequence,expected_turn_id,mode,text,options}=>{
            let w=core.store.codex_chat_context(&thread_id)?;
            let session=crate::claude_code::observed_session(core,native_id(&thread_id)?,Some(&w.cwd))?;
            if generation!=w.generation{return Err("REPLY_TARGET_CHANGED_REFRESH".into());}
            if !matches!(mode.as_str(),"SEND"|"QUEUE")||options.model.is_some()||options.effort.is_some()||!options.attachments.is_empty(){return Err("CLAUDE_TEXT_REPLY_ONLY".into());}
            let input=WatchReply{id,thread_id,cwd:w.cwd,generation,source_sequence,expected_turn_id,mode,text,options:serde_json::to_value(options).map_err(|_|"CLAUDE_RESPONSE_INVALID")?,payload_hash:String::new(),status:String::new(),turn_id:None,error_code:None,created_at:0,updated_at:0};
            // A duplicate ID checks the exact payload and returns its old receipt;
            // it never re-enqueues or grants a second physical submission.
            if core.store.watch_reply(&input.id)?.is_some(){return encode(core.store.prepare_watch_reply(&input)?);}
            if session.state=="WAITING_DECISION"{return Err("CLAUDE_NATIVE_APPROVAL_PENDING".into());}
            if input.expected_turn_id!=session.turn_id{return Err("REPLY_TURN_CHANGED_REFRESH".into());}
            if input.mode=="SEND"&&!matches!(session.state.as_str(),"COMPLETE"|"FAILED"|"INTERRUPTED"|"EMPTY"|"LIMITED"){return Err("REPLY_EXTERNAL_STATE_UNCONFIRMED".into());}
            if input.mode=="QUEUE"&&!matches!(session.state.as_str(),"RUNNING"|"SESSION_START"){return Err("REPLY_TURN_CHANGED_REFRESH".into());}
            let r=core.store.prepare_watch_reply(&input)?;
            if let Err(error)=core.store.enqueue_claude_manual(&r,&session.instance_id){
                let code=match error.as_str(){"CLAUDE_TARGET_CHANGED"=>"CLAUDE_TARGET_CHANGED",_=>"CLAUDE_PREWRITE_FAILED"};
                let _=core.store.finish_watch_reply(&r.id,"FAILED",None,Some(code));
                crate::claude_event_log::record(&session.session_id,"MANUAL_ENQUEUE",code);
                return Err(error);
            }
            encode(r)
        },
        ChatCommand::AsyncRespond{..}=>Err("CLAUDE_OPERATION_NOT_SUPPORTED".into()),
        ChatCommand::ChildOpen{thread_id,generation,child_id}=>crate::claude_child_contexts::open(core,&thread_id,generation,&child_id),
        ChatCommand::ClaudeDecision{thread_id,generation,child_id,id,revision,answer}=>crate::claude_owner_requests::answer(core,&thread_id,generation,child_id.as_deref(),&id,revision,&answer),
        ChatCommand::ChildHistory{thread_id,generation,child_id,cursor}=>{
            let w=core.store.codex_chat_context(&thread_id)?;
            if !w.enabled||w.generation!=generation{return Err("REPLY_TARGET_CHANGED_REFRESH".into())}
            let tree=crate::claude_task_tree::read(core,native_id(&thread_id)?,&w.cwd);
            let node=tree.nodes.iter().find(|n|n.session_id==child_id).ok_or("CLAUDE_CHILD_NOT_IN_TREE")?;
            let before=cursor.as_deref().map(|v|v.strip_prefix("claude:").and_then(|n|n.parse::<u64>().ok()).ok_or("CLAUDE_HISTORY_CURSOR_INVALID")).transpose()?;
            let path=crate::claude_transcript::projects_root().and_then(|r|crate::claude_transcript::find(&r,&child_id)).ok_or("CLAUDE_HISTORY_UNAVAILABLE")?;
            let page=crate::claude_transcript::page(&path,&child_id,before).map_err(|_|"CLAUDE_HISTORY_UNAVAILABLE")?;
            if !page.cwd.as_ref().is_some_and(|cwd|router_core::claude_code::same_root(cwd,&node.cwd)){return Err("REPLY_TARGET_CHANGED_REFRESH".into())}
            let messages:Vec<_>=page.turns.iter().flat_map(|turn|turn.messages.iter().map(|m|json!({"id":m.id,"turnId":turn.id,"role":m.role,"text":m.text,"kind":m.kind,"card":m.card,"timestamp":m.at,"order":m.order,"revision":page.version}))).collect();
            Ok(json!({"messages":messages,"ownerRequests":core.store.claude_owner_requests(&thread_id,generation,&child_id)?,"nextCursor":page.next.map(|n|format!("claude:{n}")),"coverage":"SESSION_TRANSCRIPT"}))
        },
        ChatCommand::ChildNotifications{thread_id,generation,enabled}=>{
            let w=core.store.codex_chat_context(&thread_id)?;
            if w.generation!=generation{return Err("REPLY_TARGET_CHANGED_REFRESH".into())}
            let tree=crate::claude_task_tree::read(core,native_id(&thread_id)?,&w.cwd);
            if enabled&&(tree.limited||tree.unknown>0){return Err("CLAUDE_TREE_UNCONFIRMED".into())}
            let baseline=tree.nodes.iter().filter_map(|node|child_snapshot(node).map(|s|(node.session_id.clone(),serde_json::to_string(&(s.state,s.turn_id,s.item_id)).unwrap()))).collect::<Vec<_>>();
            core.store.set_child_results(&thread_id,generation,enabled,&baseline)?;
            Ok(json!({"enabled":enabled}))
        },
        ChatCommand::Receipt{thread_id,id}=>{native_id(&thread_id)?;core.store.codex_chat_context(&thread_id)?;let r=core.store.watch_reply(&id)?;if r.as_ref().is_some_and(|r|r.thread_id!=thread_id){return Err("REPLY_TARGET_CHANGED_REFRESH".into());}Ok(json!(r))},
        ChatCommand::Cancel{thread_id,id}=>{native_id(&thread_id)?;core.store.codex_chat_context(&thread_id)?;core.store.cancel_watch_reply(&thread_id,&id)?;Ok(json!({"ok":true}))},
        ChatCommand::Abandon{thread_id,id}=>{native_id(&thread_id)?;core.store.codex_chat_context(&thread_id)?;Ok(json!({"cancelled":core.store.abandon_watch_reply(&thread_id,&id)?}))},
        ChatCommand::Options{thread_id}=>{native_id(&thread_id)?;core.store.codex_chat_context(&thread_id)?;Ok(json!({"models":[],"permissions":"INHERITED"}))},
        ChatCommand::History{thread_id,cursor}=>{
            let before=match cursor.as_deref(){None=>None,Some(c)=>Some(c.strip_prefix("claude:").and_then(|v|v.parse::<u64>().ok()).ok_or("CLAUDE_HISTORY_CURSOR_INVALID")?)};
            let current=state(core,&thread_id)?;let snap=&current["watch"]["snapshot"];
            // Owner-requested full flow: the exact session's own transcript, user
            // prompts and assistant text only (see claude_transcript).
            let sid=native_id(&thread_id)?;
            let path=crate::claude_transcript::projects_root().and_then(|root|crate::claude_transcript::find(&root,sid));
            let Some(path)=path else {
                let mut messages=vec![];
                if let (Some(id),Some(turn),Some(text))=(snap["itemId"].as_str(),snap["turnId"].as_str(),snap["text"].as_str()) {if !text.is_empty(){messages.push(json!({"id":id,"turnId":turn,"role":"assistant","text":text}));}}
                return Ok(json!({"messages":messages,"nextCursor":null,"coverage":"ADAPTER_RECEIVED_ONLY"}));
            };
            let page=if before.is_none(){crate::claude_transcript::latest(sid).ok_or("CLAUDE_HISTORY_UNAVAILABLE")?}else{crate::claude_transcript::page(&path,sid,before).map_err(|_|"CLAUDE_HISTORY_UNAVAILABLE")?};
            let mut messages=vec![];
            // Transcript ids are permanent; the client matches a notification of the
            // same reply by its text, so turns never change identity between pages.
            for turn in &page.turns {
                for m in &turn.messages {messages.push(json!({"id":m.id,"turnId":turn.id,"role":m.role,"text":m.text,"kind":m.kind,"card":m.card,"timestamp":m.at,"order":m.order,"revision":page.version}));}
            }
            Ok(json!({"messages":messages,"nextCursor":page.next.map(|n|format!("claude:{n}")),"coverage":"SESSION_TRANSCRIPT","order":"NEWEST_TURNS_FIRST"}))
        },
        _=>Err("CLAUDE_OPERATION_NOT_SUPPORTED".into()),
    }
}
/// A delivery whose receipt never arrived is settled as SENT only when the session's
/// own transcript holds the exact row the plugin submitted. Never resends anything.
pub(crate) fn reconcile_deliveries(core:&RouterCore,sid:&str)->Result<usize,String>{
    let pending=core.store.claude_unconfirmed_deliveries(sid)?;
    if pending.is_empty(){return Ok(0);}
    let Some(path)=crate::claude_transcript::projects_root().and_then(|root|crate::claude_transcript::find(&root,sid)) else {return Ok(0)};
    let mut settled=0;
    for delivery in pending {
        if let Some(message)=crate::claude_transcript::find_plugin_prompt(&path,sid,&delivery.text,delivery.offered_at) {
            if core.store.settle_claude_delivery_from_transcript(&delivery,&message)?{settled+=1;}
        }
    }
    Ok(settled)
}
pub(crate) fn update_watch(core:&RouterCore,sid:&str)->Result<(),String>{
    let key=format!("{PREFIX}{sid}");
    let Ok(w)=core.store.registered_codex_watch(&key)else{return Ok(())};
    if !w.enabled{return Ok(());}
    let s=crate::claude_code::observed_session(core,sid,Some(&w.cwd))?;
    core.store.record_codex_watch(&key,w.generation,&snapshot(&s))?;Ok(())
}

fn child_snapshot(node:&crate::claude_task_tree::Node)->Option<WatchSnapshot>{
 let page=crate::claude_transcript::latest(&node.session_id)?;
 if !page.cwd.as_ref().is_some_and(|cwd|router_core::claude_code::same_root(cwd,&node.cwd)){return None}
 let turn=page.turns.first()?;
 let message=turn.messages.iter().rev().find(|m|m.role=="assistant"&&m.kind=="PUBLIC");
 Some(WatchSnapshot{state:if node.state=="TURN_ENDED"{"RESULT_READY"}else{"RUNNING"}.into(),turn_id:Some(turn.id.clone()),item_id:message.map(|m|m.id.clone()),text:message.map(|m|m.text.clone()).unwrap_or_default()})
}
pub(crate) fn observe_children(core:&RouterCore,key:&str)->Result<(),String>{
 if !core.store.child_results_enabled(key)?{return Ok(())}
 let w=core.store.codex_chat_context(key)?;
 for node in crate::claude_task_tree::read(core,native_id(key)?,&w.cwd).nodes {
  if let Some(snapshot)=child_snapshot(&node){core.store.record_child_result(key,w.generation,&node.session_id,&node.title,&snapshot,node.updated_at)?;}
 }
 Ok(())
}
