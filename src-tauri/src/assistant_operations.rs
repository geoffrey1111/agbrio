//! Global application operations reuse existing services; no shell/credentials.
use crate::host_application::RouterCore;
use router_core::store::role_bridge::RoleBindingInput;
use serde::Deserialize;
use serde_json::{json,Value};
#[derive(Deserialize)]
#[serde(tag="operation",rename_all="SCREAMING_SNAKE_CASE",rename_all_fields="camelCase",deny_unknown_fields)]
pub(crate) enum Operation{
 CreateChat{#[serde(default="codex_provider")]provider:String,directory:String,label:Option<String>},
 CreateBridge{name:String},
 RenameBridge{workstream_id:String,binding_revision:i64,name:String},
 BindBridge{workstream_id:String,binding_revision:i64,decision:RoleBindingInput,execution:RoleBindingInput},
 BridgeLifecycle{workstream_id:String,binding_revision:i64,lifecycle:String},
 PinBridge{workstream_id:String,binding_revision:i64,pinned:bool},
 EnableWatch{thread_id:String},PauseWatch{thread_id:String,generation:i64},
 RemoveWatchItem{kind:String,id:String,removed:bool,generation:Option<i64>},
 MarkNotificationRead{sequence:i64},
 SendChat{thread_id:String,generation:i64,source_sequence:Option<i64>,expected_turn_id:Option<String>,#[serde(default="automatic_mode")]mode:String,#[serde(default)]queue_requested:bool,text:String,#[serde(default)]options:crate::watch_chat::ReplyOptions},
 PauseGoal{target:crate::goal_control::GoalTarget,expected_goal_fingerprint:String},
 ResumeGoal{target:crate::goal_control::GoalTarget,expected_goal_fingerprint:String},
 StopChat{thread_id:String,turn_id:String},
 RespondChat{thread_id:String,request_id:String,input:crate::host_application::MobileCodexResponseInput},
 AcknowledgeUnknownChat{thread_id:String,reply_id:String,generation:i64},
}
fn codex_provider()->String{"CODEX".into()}
fn automatic_mode()->String{"AUTO".into()}
fn create_directory(value:&str)->Result<String,String>{
 if value.trim().is_empty()||value.len()>4096||value.starts_with("\\\\")||value.contains('\0'){return Err("ASSISTANT_CREATE_DIRECTORY_INVALID".into());}
 let path=std::path::Path::new(value);
 if !path.is_absolute()||!path.is_dir(){return Err("ASSISTANT_CREATE_DIRECTORY_INVALID".into());}
 let canonical=std::fs::canonicalize(path).map_err(|_|"ASSISTANT_CREATE_DIRECTORY_UNAVAILABLE")?;
 let value=canonical.to_string_lossy();let value=value.strip_prefix("\\\\?\\").unwrap_or(&value);
 if value.starts_with("UNC\\")||value.starts_with("\\\\"){return Err("ASSISTANT_CREATE_DIRECTORY_INVALID".into());}Ok(value.into())
}
pub(crate) fn normalize(input:&mut router_core::store::assistant_actions::ActionInput)->Result<(),String>{
 let op=decode(&input.operation,input.input.clone())?;
 match op {
  Operation::CreateChat{directory,..}=>input.input["directory"]=json!(create_directory(&directory)?),
  Operation::SendChat{mode,queue_requested,..}=>{
   if !queue_requested&&matches!(mode.as_str(),"AUTO"|"SEND"|"QUEUE"){input.input["mode"]=json!("AUTO");}
  },_=>{}
 }Ok(())
}
pub(crate) fn decode(operation:&str,input:Value)->Result<Operation,String>{
 if operation=="RESPOND_CHAT"{validate_response_shape(&input)?;}
 let mut obj=input.as_object().cloned().ok_or("ASSISTANT_ARGUMENTS_INVALID")?;
 if obj.contains_key("operation"){return Err("ASSISTANT_ARGUMENTS_INVALID".into());}obj.insert("operation".into(),json!(operation));
 serde_json::from_value(Value::Object(obj)).map_err(|_|"ASSISTANT_OPERATION_OR_ARGUMENTS_INVALID".into())
}
fn validate_response_shape(value:&Value)->Result<(),String>{
 let fail=|field:&str,expected:&str|format!("ASSISTANT_ARGUMENTS_INVALID: RESPOND_CHAT.{field} {expected}; use requestId/revision and questions[].id from read_chat; answers is a question-ID to answer-string map");
 let outer=value.as_object().ok_or_else(||fail("input","must be an object"))?;
 for key in outer.keys(){if !matches!(key.as_str(),"threadId"|"requestId"|"input"){return Err(fail("input","has an unknown field; allowed: threadId, requestId, input"));}}
 for key in ["threadId","requestId"]{if value[key].as_str().is_none_or(|s|s.trim().is_empty()||s.len()>256){return Err(fail(key,"must be a nonempty string"));}}
 let input=value["input"].as_object().ok_or_else(||fail("input","must be an object with revision and answers or decision"))?;
 for key in input.keys(){if !matches!(key.as_str(),"revision"|"answers"|"decision"){return Err(fail("input","has an unknown field; allowed: revision, answers, decision"));}}
 if input.get("revision").and_then(Value::as_u64).is_none_or(|n|n==0){return Err(fail("input.revision","must be the positive integer revision from read_chat"));}
 let answers=input.get("answers").filter(|v|!v.is_null());let decision=input.get("decision").filter(|v|!v.is_null());
 if answers.is_some()==decision.is_some(){return Err(fail("input","requires exactly one of answers or decision"));}
 if let Some(answers)=answers{if answers.as_object().is_none_or(|a|a.is_empty()||a.len()>12||a.values().any(|v|v.as_str().is_none_or(|s|s.trim().is_empty()||s.len()>16000))){return Err(fail("input.answers","must be a nonempty object mapping exact question IDs to nonempty answer strings"));}}
 if decision.is_some_and(|v|v.as_str().is_none_or(|s|s.trim().is_empty())){return Err(fail("input.decision","must be a decision string offered by the request"));}Ok(())
}
#[cfg(test)]
mod scope_tests {
 use super::*;
 #[test]fn respond_chat_errors_identify_fields_without_echoing_answer_text(){
  let valid=json!({"threadId":"thread","requestId":"call","input":{"revision":1,"answers":{"[\"request_user_input_async\",\"call\",0]":"A"}}});assert!(decode("RESPOND_CHAT",valid.clone()).is_ok());
  for (value,field) in [(json!({"threadId":"thread","requestId":"call","answers":{"q":"secret-answer"}}),"input"),(json!({"threadId":"thread","requestId":"call","input":{"answers":{"q":"secret-answer"}}}),"input.revision"),(json!({"threadId":"thread","requestId":"call","input":{"revision":1,"answers":["secret-answer"]}}),"input.answers"),(json!({"threadId":"thread","requestId":"call","input":{"revision":1,"answers":{"q":["secret-answer"]}}}),"input.answers")]{let error=decode("RESPOND_CHAT",value).err().unwrap();assert!(error.contains(field),"{error}");assert!(!error.contains("secret-answer"));}
  let mut both=valid;both["input"]["decision"]=json!("accept");assert!(decode("RESPOND_CHAT",both).is_err());
  assert!(decode("RESPOND_CHAT",json!({"threadId":"thread","requestId":"call","input":{"revision":1,"decision":"accept"}})).is_ok());
 }
 #[test]
 fn global_bridge_operation_checks_exact_project_independently_of_selection(){
  let dir=tempfile::tempdir().unwrap();let store=std::sync::Arc::new(crate::RouterStore::open_at(dir.path().join("test.db")).unwrap());
  let first=store.create_project("first".into(),None).unwrap();let a=store.create_workstream(&first.id,"a".into()).unwrap();
  let second=store.create_project("second".into(),None).unwrap();store.create_workstream(&second.id,"b".into()).unwrap();
  let core=RouterCore{store:store.clone(),chatgpt:std::sync::Arc::default(),session:std::sync::Arc::new(std::sync::Mutex::new(crate::host_application::Session::default())),completed_chatgpt_responses:std::sync::Arc::default()};
  let operation=Operation::RenameBridge{workstream_id:a.id.clone(),binding_revision:a.binding_revision,name:"new".into()};
  assert!(validate(&core,&operation).is_ok());
  assert!(validate(&core,&Operation::RenameBridge{workstream_id:a.id.clone(),binding_revision:a.binding_revision+1,name:"wrong".into()}).is_err());
  store.trash_workstream(&a.id).unwrap();assert!(validate(&core,&operation).is_err());
 }
}
fn bridge(core:&RouterCore,id:&str,revision:i64)->Result<(),String>{
 let w=core.store.snapshot_for_workstream(id)?.workstreams.into_iter().find(|w|w.id==id).ok_or("ASSISTANT_BRIDGE_UNAVAILABLE")?;
 if w.binding_revision!=revision||w.trashed_at.is_some()||w.archived_at.is_some(){return Err("ASSISTANT_BRIDGE_CHANGED_OR_REMOVED".into());}Ok(())
}
fn watch(core:&RouterCore,id:&str,generation:i64)->Result<(),String>{
 if !core.store.codex_watches()?.iter().any(|w|w.thread_id==id&&w.generation==generation){return Err("REPLY_TARGET_CHANGED_REFRESH".into());}Ok(())
}
fn chat_context(core:&RouterCore,id:&str,generation:i64)->Result<(),String>{if core.store.codex_chat_context(id)?.generation!=generation{return Err("REPLY_TARGET_CHANGED_REFRESH".into());}Ok(())}
pub(crate) fn validate(core:&RouterCore,op:&Operation)->Result<(),String>{
 match op{
  Operation::CreateChat{provider,directory,label}=>{if provider!="CODEX"{return Err("ASSISTANT_CREATE_PROVIDER_UNSUPPORTED".into());}if create_directory(directory)?!=*directory{return Err("ASSISTANT_CREATE_DIRECTORY_CHANGED".into());}if label.as_ref().is_some_and(|s|s.trim().is_empty()||s.chars().count()>120){return Err("WATCH_LABEL_INVALID".into());}},
  Operation::CreateBridge{name}=>if name.trim().is_empty()||name.chars().count()>100{return Err("BRIDGE_NAME_INVALID".into());},
  Operation::RenameBridge{workstream_id,binding_revision,name}=>{bridge(core,workstream_id,*binding_revision)?;if name.trim().is_empty()||name.chars().count()>100{return Err("BRIDGE_NAME_INVALID".into());}},
  Operation::BindBridge{workstream_id,binding_revision,..}|Operation::PinBridge{workstream_id,binding_revision,..}=>bridge(core,workstream_id,*binding_revision)?,
  Operation::BridgeLifecycle{workstream_id,binding_revision,lifecycle}=>{bridge(core,workstream_id,*binding_revision)?;if !matches!(lifecycle.as_str(),"ACTIVE"|"TRASHED"){return Err("ASSISTANT_LIFECYCLE_INVALID".into());}},
  Operation::PauseGoal{target,expected_goal_fingerprint}|Operation::ResumeGoal{target,expected_goal_fingerprint}=>crate::goal_control::validate(core,&crate::goal_control::GoalInput{target:target.clone(),expected_goal_fingerprint:expected_goal_fingerprint.clone()},if matches!(op,Operation::PauseGoal{..}){"PAUSE_GOAL"}else{"RESUME_GOAL"})?,
  Operation::PauseWatch{thread_id,generation}=>watch(core,thread_id,*generation)?,
  Operation::SendChat{thread_id,generation,mode,queue_requested,..}=>{if thread_id.starts_with(router_core::store::claude_manual::PREFIX){return Err("CLAUDE_MCP_WRITE_DISABLED".into());}if !matches!(mode.as_str(),"AUTO"|"SEND"|"QUEUE"|"STEER")||(*queue_requested&&mode!="QUEUE"){return Err("REPLY_INPUT_INVALID".into());}chat_context(core,thread_id,*generation)?;},
  Operation::AcknowledgeUnknownChat{thread_id,reply_id,generation}=>{chat_context(core,thread_id,*generation)?;let r=core.store.watch_reply(reply_id)?.ok_or("REPLY_NOT_FOUND")?;if r.thread_id!=*thread_id||r.status!="UNKNOWN"{return Err("ASSISTANT_UNKNOWN_RECEIPT_REQUIRED".into());}},
  Operation::RemoveWatchItem{kind,id,generation,removed}=>{if kind=="WATCH"{let expected=generation.ok_or("ASSISTANT_WATCH_GENERATION_REQUIRED")?;if *removed{watch(core,id,expected)?;}else if core.store.registered_codex_watch(id)?.generation!=expected{return Err("REPLY_TARGET_CHANGED_REFRESH".into());}}else if kind=="EVENT"{core.store.codex_watch_event(id.parse().map_err(|_|"WATCH_SEQUENCE_INVALID")?)?;}else{return Err("WATCH_REMOVAL_INVALID".into());}},
  Operation::MarkNotificationRead{sequence}=>{core.store.codex_watch_event(*sequence)?;},
  Operation::RespondChat{thread_id,request_id,input}=>{crate::watch_chat::assistant_response_command(core,thread_id,request_id,input)?;},
  Operation::EnableWatch{thread_id}|Operation::StopChat{thread_id,..}=>if thread_id.trim().is_empty()||thread_id.len()>256{return Err("ASSISTANT_ARGUMENTS_INVALID".into());},
 }Ok(())
}
pub(crate) fn execute(core:&RouterCore,aid:&str,op:Operation)->Result<Value,String>{
 match op{
  Operation::CreateChat{directory,label,..}=>{
   let directory=create_directory(&directory)?;
   let created=core.start_mobile_unprojected_codex_thread(Some(directory.clone()))?;
   if uuid::Uuid::parse_str(&created.thread.id).is_err(){return Err("ASSISTANT_CREATE_IDENTITY_INVALID_CHECK_RECEIPT".into());}
   core.store.checkpoint_assistant_created_chat(aid,&json!({"provider":"CODEX","threadId":created.thread.id,"directory":directory,"firstMessageSent":false,"registration":"PENDING","automaticRetry":false}))?;
   // Retain the created identity before any subsequent observation can fail.
   // Empty native threads are session-only until the first actual owner task.
   core.store.enable_codex_watch(&created.thread.id,label.as_deref().or(created.thread.name.as_deref()).unwrap_or("新 Codex 对话"),&directory,&router_core::store::codex_watch::WatchSnapshot{state:"IDLE".into(),turn_id:None,item_id:None,text:String::new()})?;
   let w=core.store.codex_chat_context(&created.thread.id)?;
   Ok(json!({"provider":"CODEX","threadId":created.thread.id,"directory":directory,"watch":w,"firstMessageSent":false,"durability":"SESSION_ONLY_UNTIL_FIRST_TURN","nextAction":"SEND_CHAT","automaticRetry":false}))
  },
  Operation::CreateBridge{name}=>Ok(json!({"workstreamId":crate::role_bridge::create(core,&name)?})),
  Operation::RenameBridge{workstream_id,name,..}=>serde_json::to_value(core.store.rename_workstream(&workstream_id,&name)?).map_err(|_|"ASSISTANT_RESPONSE_UNAVAILABLE".into()),
  Operation::BindBridge{workstream_id,binding_revision,decision,execution}=>serde_json::to_value(crate::role_bridge::bind(core,&workstream_id,binding_revision,decision,execution)?).map_err(|_|"ASSISTANT_RESPONSE_UNAVAILABLE".into()),
  Operation::BridgeLifecycle{workstream_id,lifecycle,..}=>serde_json::to_value(if lifecycle=="TRASHED"{core.store.trash_workstream(&workstream_id)?}else{core.store.restore_workstream(&workstream_id)?}).map_err(|_|"ASSISTANT_RESPONSE_UNAVAILABLE".into()),
  Operation::PinBridge{workstream_id,pinned,..}=>{core.store.set_workstream_pinned(&workstream_id,pinned)?;Ok(json!({"ok":true}))},
  Operation::EnableWatch{thread_id}=>{crate::codex_watch::enable(core,&thread_id)?;Ok(json!({"ok":true}))},
  Operation::PauseWatch{thread_id,..}=>{core.store.pause_codex_watch(&thread_id)?;Ok(json!({"ok":true}))},
  Operation::RemoveWatchItem{kind,id,removed,..}=>{core.store.set_watch_item_removed(&kind,&id,removed)?;Ok(json!({"ok":true}))},
  Operation::MarkNotificationRead{sequence}=>{core.store.mark_codex_watch_event_seen(sequence)?;Ok(json!({"ok":true}))},
  Operation::SendChat{thread_id,generation,source_sequence,expected_turn_id,mode,queue_requested,text,options}=>{
   let (mode,expected_turn_id)=if mode=="STEER"||(mode=="QUEUE"&&queue_requested){(mode,expected_turn_id)}else{
    crate::watch_chat::automatic_reply_target(core,&thread_id,generation,expected_turn_id)?
   };
   crate::watch_chat::command(core,crate::watch_chat::ChatCommand::Send{thread_id,id:aid.into(),generation,source_sequence,expected_turn_id,mode,queue_requested,text,options})
  },
  Operation::PauseGoal{target,expected_goal_fingerprint}=>{let r=crate::goal_control::apply(core,aid,crate::goal_control::GoalInput{target,expected_goal_fingerprint},"PAUSE_GOAL",false)?;if r.status!="APPLIED"{return Err("GOAL_CONTROL_UNCERTAIN_CHECK_RECEIPT".into());}serde_json::to_value(r).map_err(|_|"ASSISTANT_RESPONSE_UNAVAILABLE".into())},
  Operation::ResumeGoal{target,expected_goal_fingerprint}=>{let owner=core.store.assistant_action_has_owner_answer(aid)?;let r=crate::goal_control::apply(core,aid,crate::goal_control::GoalInput{target,expected_goal_fingerprint},"RESUME_GOAL",owner)?;if r.status!="APPLIED"{return Err("GOAL_CONTROL_UNCERTAIN_CHECK_RECEIPT".into());}serde_json::to_value(r).map_err(|_|"ASSISTANT_RESPONSE_UNAVAILABLE".into())},
  Operation::StopChat{thread_id,turn_id}=>crate::watch_chat::command(core,crate::watch_chat::ChatCommand::Stop{thread_id,turn_id}),
  Operation::RespondChat{thread_id,request_id,input}=>{let command=crate::watch_chat::assistant_response_command(core,&thread_id,&request_id,&input)?;crate::watch_chat::command(core,command)},
  Operation::AcknowledgeUnknownChat{thread_id,reply_id,..}=>crate::watch_chat::command(core,crate::watch_chat::ChatCommand::AcknowledgeUnknown{thread_id,id:reply_id,confirmed:true}),
 }
}
