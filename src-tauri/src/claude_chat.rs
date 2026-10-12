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
    let session=crate::claude_code::verified_session(core,sid,None)?;
    let title=crate::claude_code::catalog(core)?.sessions.into_iter().find(|s|s.session_id==sid).and_then(|s|s.title).unwrap_or_else(||"未命名 Claude Code 对话".into());
    core.store.enable_codex_watch(key,&title,&session.cwd,&snapshot(&session))
}
pub(crate) fn state(core:&RouterCore,key:&str)->Result<Value,String>{
    core.store.expire_claude_manual_offers()?;
    let mut watch=core.store.codex_chat_context(key)?;
    let session=crate::claude_code::verified_session(core,native_id(key)?,Some(&watch.cwd))?;
    watch.snapshot=snapshot(&session);watch.checked_at=session.checked_at;
    let ready=core.store.claude_manual_ready(&session.session_id,&session.instance_id)?;
    Ok(json!({"watch":watch,"provider":"CLAUDE_CODE","inputReady":ready,"host":std::env::var("COMPUTERNAME").unwrap_or_default(),"ownedTurnId":null,"controllableTurnId":null,"externalBusy":matches!(session.state.as_str(),"RUNNING"|"WAITING_DECISION"),"replies":core.store.watch_replies(key)?,"requests":[],"goal":null,"publicMessages":[],"activity":if session.state=="RUNNING"{Some("EXECUTING")}else{None},"checkedAt":session.checked_at}))
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
            if cursor.is_some(){return Err("CLAUDE_HISTORY_CURSOR_INVALID".into());}
            let current=state(core,&thread_id)?;let snap=&current["watch"]["snapshot"];
            // Only content actually received by this adapter. Never reads arbitrary
            // private transcript files, reasoning or native approvals.
            let mut messages=vec![];
            for r in core.store.watch_replies(&thread_id)?.into_iter().rev().filter(|r|r.status=="SENT"){
                messages.push(json!({"id":format!("receipt:{}",r.id),"turnId":format!("receipt:{}",r.id),"role":"user","text":r.text}));
            }
            if let (Some(id),Some(turn),Some(text))=(snap["itemId"].as_str(),snap["turnId"].as_str(),snap["text"].as_str()) {if !text.is_empty(){messages.push(json!({"id":id,"turnId":turn,"role":"assistant","text":text}));}}
            Ok(json!({"messages":messages.into_iter().rev().collect::<Vec<_>>(),"nextCursor":null,"coverage":"ADAPTER_RECEIVED_ONLY"}))
        },
        _=>Err("CLAUDE_OPERATION_NOT_SUPPORTED".into()),
    }
}
pub(crate) fn update_watch(core:&RouterCore,sid:&str)->Result<(),String>{
    let key=format!("{PREFIX}{sid}");
    let Ok(w)=core.store.registered_codex_watch(&key)else{return Ok(())};
    if !w.enabled{return Ok(());}
    let s=crate::claude_code::verified_session(core,sid,Some(&w.cwd))?;
    core.store.record_codex_watch(&key,w.generation,&snapshot(&s))?;Ok(())
}
