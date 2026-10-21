//! Authenticated owner UI surface; deliberately absent from OAuth/MCP actions.
use crate::host_application::RouterCore;
use router_core::store::{claude_manual::{native_id,PREFIX},codex_watch::WatchSnapshot,watch_reply::WatchReply};
use serde_json::{json,Value};
use crate::watch_chat::ChatCommand;

pub(crate) fn snapshot(s:&router_core::claude_code::Session)->WatchSnapshot{
    let state=match s.state.as_str(){"COMPLETE"=>"RESULT_READY","WAITING_DECISION"=>"ACTION_REQUIRED","SESSION_START"=>"UNKNOWN","EMPTY"|"LIMITED"=>"INCOMPLETE",v=>v};
    let reply=s.latest_reply.as_ref().filter(|r|s.state=="COMPLETE"&&Some(&r.turn_id)==s.turn_id.as_ref());
    WatchSnapshot{state:state.into(),turn_id:s.turn_id.clone(),item_id:reply.map(|r|format!("claude_reply:{}",r.turn_id)),text:reply.map(|r|r.text.clone()).unwrap_or_default()}
}
pub(crate) fn enable(core:&RouterCore,key:&str)->Result<(),String>{
    let sid=native_id(key)?;
    let title=crate::claude_code::catalog(core)?.sessions.into_iter().find(|s|s.session_id==sid).and_then(|s|s.title).unwrap_or_else(||"未命名 Claude Code 对话".into());
    match crate::claude_code::verified_session(core,sid,None) {
        Ok(session)=>core.store.enable_codex_watch(key,&title,&session.cwd,&snapshot(&session)),
        // Not reported yet: watch the exact desktop conversation; events fill it in.
        Err(_)=>core.store.enable_codex_watch(key,&title,&crate::claude_code::selectable_cwd(core,sid)?,&WatchSnapshot{state:"UNKNOWN".into(),turn_id:None,item_id:None,text:String::new()}),
    }
}
pub(crate) fn state(core:&RouterCore,key:&str)->Result<Value,String>{
    core.store.expire_claude_manual_offers()?;
    let mut watch=core.store.codex_chat_context(key)?;
    // Before the conversation reports, the watch shows what it has and input stays off.
    let session=crate::claude_code::verified_session(core,native_id(key)?,Some(&watch.cwd)).ok();
    if let Some(s)=&session {watch.snapshot=snapshot(s);watch.checked_at=s.checked_at;}
    let ready=match &session {Some(s)=>core.store.claude_manual_ready(&s.session_id,&s.instance_id)?,None=>false};
    let state=session.as_ref().map(|s|s.state.as_str()).unwrap_or("");
    Ok(json!({"watch":watch,"provider":"CLAUDE_CODE","inputReady":ready,"host":std::env::var("COMPUTERNAME").unwrap_or_default(),"ownedTurnId":null,"controllableTurnId":null,"externalBusy":matches!(state,"RUNNING"|"WAITING_DECISION"),"replies":core.store.watch_replies(key)?,"requests":[],"goal":null,"publicMessages":[],"activity":if state=="RUNNING"{Some("EXECUTING")}else{None},"checkedAt":session.as_ref().map(|s|s.checked_at).unwrap_or(watch.checked_at)}))
}
pub(crate) fn command(core:&RouterCore,c:ChatCommand)->Result<Value,String>{
    let encode=|r:WatchReply|serde_json::to_value(r).map_err(|_|"CLAUDE_RESPONSE_INVALID".to_string());
    match c {
        ChatCommand::Send{thread_id,id,generation,source_sequence,expected_turn_id,mode,text,options}=>{
            let w=core.store.codex_chat_context(&thread_id)?;
            let session=crate::claude_code::verified_session(core,native_id(&thread_id)?,Some(&w.cwd))?;
            if generation!=w.generation{return Err("REPLY_TARGET_CHANGED_REFRESH".into());}
            if !matches!(mode.as_str(),"SEND"|"QUEUE")||options.model.is_some()||options.effort.is_some()||!options.attachments.is_empty(){return Err("CLAUDE_TEXT_REPLY_ONLY".into());}
            let input=WatchReply{id,thread_id,cwd:w.cwd,generation,source_sequence,expected_turn_id,mode,text,options:serde_json::to_value(options).map_err(|_|"CLAUDE_RESPONSE_INVALID")?,payload_hash:String::new(),status:String::new(),turn_id:None,error_code:None,created_at:0,updated_at:0};
            // A duplicate ID checks the exact payload and returns its old receipt;
            // it never re-enqueues or grants a second physical submission.
            if core.store.watch_reply(&input.id)?.is_some(){return encode(core.store.prepare_watch_reply(&input)?);}
            if session.state=="WAITING_DECISION"{return Err("CLAUDE_NATIVE_APPROVAL_PENDING".into());}
            if input.expected_turn_id!=session.turn_id{return Err("REPLY_TURN_CHANGED_REFRESH".into());}
            if !core.store.claude_manual_ready(&session.session_id,&session.instance_id)?{return Err("CLAUDE_INPUT_PLUGIN_NOT_READY".into());}
            if input.mode=="SEND"&&!matches!(session.state.as_str(),"COMPLETE"|"FAILED"|"INTERRUPTED"|"EMPTY"|"LIMITED"){return Err("REPLY_EXTERNAL_STATE_UNCONFIRMED".into());}
            if input.mode=="QUEUE"&&!matches!(session.state.as_str(),"RUNNING"|"SESSION_START"){return Err("REPLY_TURN_CHANGED_REFRESH".into());}
            let r=core.store.prepare_watch_reply(&input)?;
            if let Err(error)=core.store.enqueue_claude_manual(&r,&session.instance_id){let _=core.store.finish_watch_reply(&r.id,"FAILED",None,Some("CLAUDE_PREWRITE_FAILED"));return Err(error);}
            encode(r)
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
            let page=crate::claude_transcript::page(&path,sid,before).map_err(|_|"CLAUDE_HISTORY_UNAVAILABLE")?;
            let mut messages=vec![];
            // Transcript ids are permanent; the client matches a notification of the
            // same reply by its text, so turns never change identity between pages.
            for turn in &page.turns {
                for m in &turn.messages {messages.push(json!({"id":m.id,"turnId":turn.id,"role":m.role,"text":m.text}));}
            }
            Ok(json!({"messages":messages,"nextCursor":page.next.map(|n|format!("claude:{n}")),"coverage":"SESSION_TRANSCRIPT"}))
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
    let s=crate::claude_code::verified_session(core,sid,Some(&w.cwd))?;
    core.store.record_codex_watch(&key,w.generation,&snapshot(&s))?;Ok(())
}
