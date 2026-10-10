//! One exact native Goal, shared by desktop/PWA and reviewed assistant actions.
use crate::host_application::{RouterCore,Session,MobileCodexGoal,read_codex_goal_from_session};
use router_core::store::goal_control::GoalControlReceipt;
use serde::{Deserialize,Serialize};
use serde_json::{json,Value};
use sha2::{Digest,Sha256};

#[derive(Clone,Debug,Serialize,Deserialize,PartialEq)]
#[serde(rename_all="camelCase",deny_unknown_fields)]
pub(crate) struct GoalTarget{pub thread_id:String,pub generation:i64,pub workstream_id:Option<String>,pub binding_revision:Option<i64>,pub role:Option<String>,pub endpoint_id:Option<String>}
#[derive(Clone,Serialize,Deserialize)]
#[serde(rename_all="camelCase",deny_unknown_fields)]
pub(crate) struct GoalInput{pub target:GoalTarget,pub expected_goal_fingerprint:String}
#[derive(Clone,Serialize)]#[serde(rename_all="camelCase")]
pub(crate) struct GoalControls{pub target:GoalTarget,pub can_pause:bool,pub can_resume:bool,pub resume_requires_owner_answer:bool,pub blocked_reason:Option<String>}

// Native accounting advances updatedAt without replacing the Goal. Pin the
// task contract/instance; keep accounting timestamps as display data.
pub(crate) fn fingerprint(g:&MobileCodexGoal)->String{
 format!("{:x}",Sha256::digest(serde_json::to_vec(&json!([g.thread_id,g.objective,g.status,g.created_at,g.token_budget])).unwrap_or_default()))
}
pub(crate) fn target(core:&RouterCore,thread:&str)->Result<GoalTarget,String>{target_for_store(&core.store,thread)}
fn target_for_store(store:&crate::RouterStore,thread:&str)->Result<GoalTarget,String>{
 let context=store.codex_chat_context(thread)?;
 let mut t=GoalTarget{thread_id:thread.into(),generation:context.generation,workstream_id:None,binding_revision:None,role:None,endpoint_id:None};
 if context.generation<0{
  for w in store.active_workstreams()?{let b=store.role_bridge(&w.id)?;
   if let Some(side)=[b.decision,b.execution].into_iter().flatten().find(|s|s.endpoint.provider=="CODEX"&&s.endpoint.external_id==thread){
    t.workstream_id=Some(w.id);t.binding_revision=Some(b.binding_revision);t.role=Some(side.role);t.endpoint_id=Some(side.endpoint.id);return Ok(t);
   }
  }return Err("GOAL_TARGET_CHANGED".into());
 }Ok(t)
}
fn require_target(core:&RouterCore,t:&GoalTarget)->Result<(),String>{if target(core,&t.thread_id)?!=*t{return Err("GOAL_TARGET_CHANGED".into());}Ok(())}
pub(crate) fn transition(status:&str,operation:&str)->Result<&'static str,String>{
 match (status,operation){("active","PAUSE_GOAL")=>Ok("paused"),("paused"|"blocked"|"usageLimited","RESUME_GOAL")=>Ok("active"),_=>Err("GOAL_TRANSITION_UNAVAILABLE".into())}
}
pub(crate) fn controls(core:&RouterCore,g:&MobileCodexGoal,busy:bool,pending:bool)->Result<GoalControls,String>{controls_for_store(&core.store,g,busy,pending)}
pub(crate) fn controls_for_store(store:&crate::RouterStore,g:&MobileCodexGoal,busy:bool,pending:bool)->Result<GoalControls,String>{
 let identity_complete=g.created_at.as_ref().and_then(Value::as_i64).is_some()&&g.updated_at.as_ref().and_then(Value::as_i64).is_some()&&!g.objective.trim().is_empty();
 let blocked=if !identity_complete{Some("GOAL_IDENTITY_INCOMPLETE".into())}else if pending{Some("GOAL_PENDING_DECISION".into())}else{store.require_goal_control_clear(&g.thread_id).err()};
 Ok(GoalControls{target:target_for_store(store,&g.thread_id)?,can_pause:blocked.is_none()&&g.status=="active",can_resume:blocked.is_none()&&!busy&&matches!(g.status.as_str(),"paused"|"blocked"|"usageLimited"),resume_requires_owner_answer:g.status=="usageLimited",blocked_reason:blocked})
}
fn checked(core:&RouterCore,s:&mut Session,input:&GoalInput,operation:&str)->Result<MobileCodexGoal,String>{
 require_target(core,&input.target)?;core.store.require_goal_control_clear(&input.target.thread_id)?;
 if s.pending_codex_requests.values().any(|r|r.thread_id==input.target.thread_id&&!r.responded){return Err("GOAL_PENDING_DECISION".into());}
 let g=read_codex_goal_from_session(s,&input.target.thread_id)?.ok_or("GOAL_NO_LONGER_EXISTS")?;
 if g.created_at.as_ref().and_then(Value::as_i64).is_none()||g.updated_at.as_ref().and_then(Value::as_i64).is_none()||g.objective.trim().is_empty(){return Err("GOAL_IDENTITY_INCOMPLETE".into());}
 if g.fingerprint!=input.expected_goal_fingerprint{return Err("GOAL_CHANGED_REFRESH".into());}
 transition(&g.status,operation)?;
 let w=core.store.codex_chat_context(&input.target.thread_id)?;let a=s.adapter.as_mut().ok_or("GOAL_BACKEND_UNAVAILABLE")?;
 let meta=crate::watch_chat::metadata(a,&w)?;
 if operation=="RESUME_GOAL"&&a.latest_turn_diagnostic(&g.thread_id).is_some_and(|d|d.will_retry==Some(true)){return Err("GOAL_NATIVE_RETRY_PENDING".into());}
 if operation=="RESUME_GOAL"&&(meta.pointer("/thread/status/type").and_then(Value::as_str)==Some("active")||a.active_turn_id(&g.thread_id).is_some()) {return Err("GOAL_TURN_ALREADY_RUNNING".into());}
 Ok(g)
}
pub(crate) fn validate(core:&RouterCore,input:&GoalInput,operation:&str)->Result<(),String>{
 let mut s=core.session.lock().map_err(|_|"GOAL_SESSION_UNAVAILABLE")?;checked(core,&mut s,input,operation).map(|_|())
}
pub(crate) fn apply(core:&RouterCore,id:&str,input:GoalInput,operation:&str,allow_usage_resume:bool)->Result<GoalControlReceipt,String>{
 let hash=format!("{:x}",Sha256::digest(serde_json::to_vec(&json!([operation,input])).map_err(|_|"GOAL_ARGUMENTS_INVALID")?));
 let old=core.store.prepare_goal_control(&input.target.thread_id,id,&hash)?;
 if old.status!="READY"{return Ok(old);}
 let mut s=core.session.lock().map_err(|_|"GOAL_SESSION_UNAVAILABLE")?;
 let prepared:Result<MobileCodexGoal,String>=(||{
  let current=checked(core,&mut s,&input,operation)?;
  if operation=="RESUME_GOAL"&&current.status=="usageLimited"&&!allow_usage_resume{return Err("GOAL_LIMIT_REQUIRES_OWNER_ANSWER".into());}
  // Resume acquires an already paused/stopped native thread. Never start a chat,
  // recreate an objective, replay a handoff or change budget/settings.
  if operation=="RESUME_GOAL"{
   let context=core.store.codex_chat_context(&input.target.thread_id)?;
   let a=s.adapter.as_mut().ok_or("GOAL_BACKEND_UNAVAILABLE")?;
   // Unlike ordinary handoff acquisition, this explicit operation is allowed
   // to attach to the already verified paused/blocked/usageLimited Goal.
   // Those states cannot automatically continue before the reviewed status set.
   let resumed=a.request_with_timeout("thread/resume",json!({"threadId":input.target.thread_id,"excludeTurns":true}),crate::codex::adapter::WRITE_READINESS_TIMEOUT)?;
   if resumed.pointer("/thread/id").and_then(Value::as_str)!=Some(&input.target.thread_id)||resumed.pointer("/thread/cwd").and_then(Value::as_str)!=Some(&context.cwd){return Err("GOAL_TARGET_CHANGED".into());}
   s.ready_threads.insert(input.target.thread_id.clone());
  }
  let rechecked=checked(core,&mut s,&input,operation)?;
  if rechecked.fingerprint!=current.fingerprint{return Err("GOAL_CHANGED_REFRESH".into());}Ok(current)
 })();
 let before=match prepared{Ok(g)=>g,Err(code)=>{let fixed=if code.starts_with("GOAL_")||code.starts_with("REPLY_"){code.clone()}else{"GOAL_PRECHECK_FAILED".into()};core.store.finish_goal_control(&input.target.thread_id,id,"REJECTED",None,Some(&fixed))?;return Err(fixed);}};
 core.store.claim_goal_control(&input.target.thread_id,id)?;
 let changed:Result<Value,String>=(||{
  let status=transition(&before.status,operation)?;
  let response=s.adapter.as_mut().ok_or("GOAL_BACKEND_UNAVAILABLE")?.set_goal_status_reviewed(&input.target.thread_id,status)?;
  let after=crate::host_application::codex_goal_from_response(&input.target.thread_id,&response)?.ok_or("GOAL_CHANGE_NOT_CONFIRMED")?;
  if after.status!=status||after.objective!=before.objective||after.created_at!=before.created_at||after.token_budget!=before.token_budget||after.tokens_used.zip(before.tokens_used).is_some_and(|(a,b)|a<b)||after.time_used_seconds.zip(before.time_used_seconds).is_some_and(|(a,b)|a<b){return Err("GOAL_CHANGE_NOT_CONFIRMED".into());}
  serde_json::to_value(after).map_err(|_|"GOAL_CHANGE_NOT_CONFIRMED".into())
 })();
 match changed{Ok(g)=>core.store.finish_goal_control(&input.target.thread_id,id,"APPLIED",Some(&g),None)?,Err(_)=>{core.store.finish_goal_control(&input.target.thread_id,id,"UNKNOWN",None,Some("GOAL_CHANGE_NOT_CONFIRMED"))?;return Err("GOAL_CONTROL_UNCERTAIN_CHECK_RECEIPT".into());}}
 core.store.goal_control_receipt(&input.target.thread_id,id)?.ok_or("GOAL_CONTROL_RECEIPT_UNAVAILABLE".into())
}

#[cfg(test)]#[path="goal_control_tests.rs"]mod tests;
