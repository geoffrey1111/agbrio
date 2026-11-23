//! Reviewed instance-assistant text sends through the existing native transport.
use crate::{host_application::RouterCore,watch_chat::{ChatCommand,ReplyOptions}};
use serde_json::{json,Value};

pub(crate) fn validate(core:&RouterCore,thread:&str,generation:i64,mode:&str,queue:bool,expected:Option<&str>,options:&ReplyOptions)->Result<(),String>{
 let watch=core.store.codex_chat_context(thread)?;
 if watch.generation!=generation||!watch.enabled{return Err("REPLY_TARGET_CHANGED_REFRESH".into());}
 if !matches!(mode,"AUTO"|"SEND"|"QUEUE")||queue!=(mode=="QUEUE"){return Err("CLAUDE_ASSISTANT_USE_SEND_OR_EXPLICIT_QUEUE".into());}
 if options.model.is_some()||options.effort.is_some()||!options.attachments.is_empty(){return Err("CLAUDE_ASSISTANT_TEXT_ONLY".into());}
 let sid=router_core::store::claude_manual::native_id(thread)?;
 let session=crate::claude_code::observed_session(core,sid,Some(&watch.cwd))?;
 if session.state=="WAITING_DECISION"{return Err("CLAUDE_NATIVE_APPROVAL_PENDING".into());}
 if expected.is_some_and(|turn|session.turn_id.as_deref()!=Some(turn)){return Err("REPLY_TURN_CHANGED_REFRESH".into());}
 if mode!="QUEUE"{
  if !core.store.claude_manual_ready(sid,&session.instance_id)?{return Err("CLAUDE_INPUT_PLUGIN_NOT_READY".into());}
  if !matches!(session.state.as_str(),"COMPLETE"|"FAILED"|"INTERRUPTED"|"EMPTY"|"LIMITED"){return Err("CLAUDE_BUSY_USE_EXPLICIT_QUEUE".into());}
 }else if !matches!(session.state.as_str(),"RUNNING"|"SESSION_START"|"COMPLETE"|"EMPTY"){return Err("REPLY_TURN_CHANGED_REFRESH".into());}
 Ok(())
}
pub(crate) fn send(core:&RouterCore,id:&str,thread:String,generation:i64,sequence:Option<i64>,mode:String,queue:bool,expected:Option<String>,text:String,options:ReplyOptions)->Result<Value,String>{
 validate(core,&thread,generation,&mode,queue,expected.as_deref(),&options)?;
 let watch=core.store.codex_chat_context(&thread)?;
 let session=crate::claude_code::observed_session(core,router_core::store::claude_manual::native_id(&thread)?,Some(&watch.cwd))?;
 crate::claude_chat::assistant_send(core,ChatCommand::Send{thread_id:thread,id:id.into(),generation,source_sequence:sequence,expected_turn_id:expected.or(session.turn_id),mode:if mode=="AUTO"{"SEND".into()}else{mode},queue_requested:queue,text,options})
}

#[cfg(test)]
#[path="assistant_claude_tests.rs"]
mod tests;
pub(crate) fn read(core:&RouterCore,thread:String,cursor:Option<String>)->Result<Value,String>{
 let state=crate::claude_chat::state(core,&thread)?;
 let history=crate::claude_chat::command(core,ChatCommand::History{thread_id:thread.clone(),cursor})?;
 Ok(json!({"state":state,"history":history,"sendContext":{"threadId":thread,"generation":state["watch"]["generation"],"sourceSequence":null,"mode":"AUTO","expectedTurnId":state["watch"]["snapshot"]["turnId"]},"sendCapabilities":{"text":true,"steer":false,"queueRequiresExplicitRequest":true,"nativeApprovalBypass":false}}))
}
