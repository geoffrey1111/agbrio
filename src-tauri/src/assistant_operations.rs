//! Global application operations reuse existing services; no shell/credentials.
use crate::host_application::RouterCore;
use router_core::store::role_bridge::RoleBindingInput;
use serde::Deserialize;
use serde_json::{json,Value};
#[derive(Deserialize)]
#[serde(tag="operation",rename_all="SCREAMING_SNAKE_CASE",rename_all_fields="camelCase",deny_unknown_fields)]
pub(crate) enum Operation{
 CreateBridge{name:String},
 RenameBridge{workstream_id:String,binding_revision:i64,name:String},
 BindBridge{workstream_id:String,binding_revision:i64,decision:RoleBindingInput,execution:RoleBindingInput},
 BridgeLifecycle{workstream_id:String,binding_revision:i64,lifecycle:String},
 PinBridge{workstream_id:String,binding_revision:i64,pinned:bool},
 EnableWatch{thread_id:String},PauseWatch{thread_id:String,generation:i64},
 RemoveWatchItem{kind:String,id:String,removed:bool,generation:Option<i64>},
 MarkNotificationRead{sequence:i64},
 SendChat{thread_id:String,generation:i64,source_sequence:Option<i64>,expected_turn_id:Option<String>,mode:String,text:String,options:crate::watch_chat::ReplyOptions},
 StopChat{thread_id:String,turn_id:String},
 RespondChat{thread_id:String,request_id:String,input:crate::host_application::MobileCodexResponseInput},
}
pub(crate) fn decode(operation:&str,input:Value)->Result<Operation,String>{
 let mut obj=input.as_object().cloned().ok_or("ASSISTANT_ARGUMENTS_INVALID")?;
 if obj.contains_key("operation"){return Err("ASSISTANT_ARGUMENTS_INVALID".into());}obj.insert("operation".into(),json!(operation));
 serde_json::from_value(Value::Object(obj)).map_err(|_|"ASSISTANT_OPERATION_OR_ARGUMENTS_INVALID".into())
}
fn bridge(core:&RouterCore,id:&str,revision:i64)->Result<(),String>{
 let w=core.store.snapshot()?.workstreams.into_iter().find(|w|w.id==id).ok_or("ASSISTANT_BRIDGE_UNAVAILABLE")?;
 if w.binding_revision!=revision{return Err("ASSISTANT_BRIDGE_CHANGED_OR_REMOVED".into());}Ok(())
}
fn watch(core:&RouterCore,id:&str,generation:i64)->Result<(),String>{
 if !core.store.codex_watches()?.iter().any(|w|w.thread_id==id&&w.generation==generation){return Err("REPLY_TARGET_CHANGED_REFRESH".into());}Ok(())
}
pub(crate) fn validate(core:&RouterCore,op:&Operation)->Result<(),String>{
 match op{
  Operation::CreateBridge{name}=>if name.trim().is_empty()||name.chars().count()>100{return Err("BRIDGE_NAME_INVALID".into());},
  Operation::RenameBridge{workstream_id,binding_revision,name}=>{bridge(core,workstream_id,*binding_revision)?;if name.trim().is_empty()||name.chars().count()>100{return Err("BRIDGE_NAME_INVALID".into());}},
  Operation::BindBridge{workstream_id,binding_revision,..}|Operation::PinBridge{workstream_id,binding_revision,..}=>bridge(core,workstream_id,*binding_revision)?,
  Operation::BridgeLifecycle{workstream_id,binding_revision,lifecycle}=>{bridge(core,workstream_id,*binding_revision)?;if !matches!(lifecycle.as_str(),"ACTIVE"|"TRASHED"){return Err("ASSISTANT_LIFECYCLE_INVALID".into());}},
  Operation::PauseWatch{thread_id,generation}|Operation::SendChat{thread_id,generation,..}=>watch(core,thread_id,*generation)?,
  Operation::RemoveWatchItem{kind,id,generation,removed}=>{if kind=="WATCH"{let expected=generation.ok_or("ASSISTANT_WATCH_GENERATION_REQUIRED")?;if *removed{watch(core,id,expected)?;}else if core.store.registered_codex_watch(id)?.generation!=expected{return Err("REPLY_TARGET_CHANGED_REFRESH".into());}}else if kind=="EVENT"{core.store.codex_watch_event(id.parse().map_err(|_|"WATCH_SEQUENCE_INVALID")?)?;}else{return Err("WATCH_REMOVAL_INVALID".into());}},
  Operation::MarkNotificationRead{sequence}=>{core.store.codex_watch_event(*sequence)?;},
  Operation::EnableWatch{thread_id}|Operation::StopChat{thread_id,..}|Operation::RespondChat{thread_id,..}=>if thread_id.trim().is_empty()||thread_id.len()>256{return Err("ASSISTANT_ARGUMENTS_INVALID".into());},
 }Ok(())
}
pub(crate) fn execute(core:&RouterCore,aid:&str,op:Operation)->Result<Value,String>{
 match op{
  Operation::CreateBridge{name}=>Ok(json!({"workstreamId":crate::role_bridge::create(core,&name)?})),
  Operation::RenameBridge{workstream_id,name,..}=>serde_json::to_value(core.store.rename_workstream(&workstream_id,&name)?).map_err(|_|"ASSISTANT_RESPONSE_UNAVAILABLE".into()),
  Operation::BindBridge{workstream_id,binding_revision,decision,execution}=>serde_json::to_value(crate::role_bridge::bind(core,&workstream_id,binding_revision,decision,execution)?).map_err(|_|"ASSISTANT_RESPONSE_UNAVAILABLE".into()),
  Operation::BridgeLifecycle{workstream_id,lifecycle,..}=>serde_json::to_value(if lifecycle=="TRASHED"{core.store.trash_workstream(&workstream_id)?}else{core.store.restore_workstream(&workstream_id)?}).map_err(|_|"ASSISTANT_RESPONSE_UNAVAILABLE".into()),
  Operation::PinBridge{workstream_id,pinned,..}=>{core.store.set_workstream_pinned(&workstream_id,pinned)?;Ok(json!({"ok":true}))},
  Operation::EnableWatch{thread_id}=>{crate::codex_watch::enable(core,&thread_id)?;Ok(json!({"ok":true}))},
  Operation::PauseWatch{thread_id,..}=>{core.store.pause_codex_watch(&thread_id)?;Ok(json!({"ok":true}))},
  Operation::RemoveWatchItem{kind,id,removed,..}=>{core.store.set_watch_item_removed(&kind,&id,removed)?;Ok(json!({"ok":true}))},
  Operation::MarkNotificationRead{sequence}=>{core.store.mark_codex_watch_event_seen(sequence)?;Ok(json!({"ok":true}))},
  Operation::SendChat{thread_id,generation,source_sequence,expected_turn_id,mode,text,options}=>crate::watch_chat::command(core,crate::watch_chat::ChatCommand::Send{thread_id,id:aid.into(),generation,source_sequence,expected_turn_id,mode,text,options}),
  Operation::StopChat{thread_id,turn_id}=>crate::watch_chat::command(core,crate::watch_chat::ChatCommand::Stop{thread_id,turn_id}),
  Operation::RespondChat{thread_id,request_id,input}=>crate::watch_chat::command(core,crate::watch_chat::ChatCommand::Respond{thread_id,request_id,input}),
 }
}
