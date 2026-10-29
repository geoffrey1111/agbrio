//! Metadata-only task observation and native recipient receipts. Never resumes a task.
use crate::host_application::{RouterCore,Session};
use router_core::store::{codex_notices::{Binding,Command,Question},RouterStore};
use serde_json::{json,Value};use sha2::{Digest,Sha256};
use std::{sync::{Arc,Mutex},time::Duration};
pub(crate) fn goal_key(thread:&str,g:&crate::host_application::MobileCodexGoal)->String{format!("{:x}",Sha256::digest(serde_json::to_vec(&json!([thread,g.objective,g.created_at,g.token_budget])).unwrap_or_default()))}
pub(crate) fn register(core:&RouterCore,token:&str,p:&Command)->Result<Value,String>{
 core.store.codex_notice_auth(token,p)?;
 let thread=p.thread_id.as_deref().ok_or("CLAUDE_NOTICE_TARGET_INVALID")?;let generation=p.generation.ok_or("CLAUDE_NOTICE_TARGET_INVALID")?;let turn=p.turn_id.as_deref().ok_or("CLAUDE_NOTICE_TARGET_INVALID")?;let ttl=p.ttl_seconds.unwrap_or(3600);if !(30..=86400).contains(&ttl){return Err("CLAUDE_NOTICE_TTL_INVALID".into());}
 let w=core.store.codex_chat_context(thread)?;if !w.enabled||w.generation!=generation{return Err("CLAUDE_NOTICE_SOURCE_CHANGED".into());}
 let key=if p.follow_goal==Some(true){let mut s=core.session.lock().map_err(|_|"CLAUDE_NOTICE_SOURCE_BUSY")?;let a=s.adapter.as_mut().ok_or("CLAUDE_NOTICE_SOURCE_UNAVAILABLE")?;let m=a.request_with_timeout("thread/read",json!({"threadId":thread,"includeTurns":false}),Duration::from_secs(3))?;if m.pointer("/thread/id").and_then(Value::as_str)!=Some(thread)||m.pointer("/thread/cwd").and_then(Value::as_str)!=Some(w.cwd.as_str()){return Err("CLAUDE_NOTICE_SOURCE_CHANGED".into());}let response=a.request_with_timeout("thread/goal/get",json!({"threadId":thread}),Duration::from_secs(3))?;let g=crate::host_application::codex_goal_from_response(thread,&response)?.ok_or("CLAUDE_NOTICE_GOAL_REQUIRED")?;if g.created_at.as_ref().and_then(Value::as_i64).is_none()||g.objective.trim().is_empty(){return Err("CLAUDE_NOTICE_GOAL_IDENTITY_INCOMPLETE".into());}Some(goal_key(thread,&g))}else{None};
 let b=Binding{receiver_sid:p.session_id.clone(),receiver_cwd:p.cwd.clone(),thread_id:thread.into(),generation,source_cwd:w.cwd,anchor_turn:turn.into(),mode:if key.is_some(){"GOAL"}else{"TURN"}.into(),goal_key:key,expires_at:crate::local_safety::now()+ttl*1000};let sub=core.store.register_codex_notice(&b)?;
 Ok(json!({"subscription":sub,"wait":{"threadId":thread,"generation":generation,"turnId":sub.binding.anchor_turn,"status":if sub.status=="ACTIVE"{"WAITING"}else{sub.status.as_str()},"followGoal":b.mode=="GOAL","subscriptionId":sub.id,"expiresAt":b.expires_at}}))
}
pub(crate) fn poll(store:&Arc<RouterStore>,session:&Arc<Mutex<Session>>,epoch:u64,cursor:&mut Option<String>){
 let Ok(sources)=store.codex_notice_sources(None)else{return};let ids=crate::observer_batch::next_ids(sources.iter().map(|s|s.binding.thread_id.as_str()),cursor,crate::observer_batch::WATCH_READS_PER_PASS);
 for thread in ids{
  let Ok(w)=store.codex_chat_context(&thread)else{continue};if !w.enabled{continue;}
  let Ok(mut s)=session.try_lock()else{continue};if s.codex_observer_epoch!=epoch{return;}
  let pending:Vec<_>=s.pending_codex_requests.values().filter(|r|r.thread_id==thread&&!r.responded).map(|r|Question{request_id:r.action_id.clone(),turn_id:r.turn_id.clone(),kind:r.method.clone()}).collect();
  let Some(a)=s.adapter.as_mut().filter(|a|!a.is_closed())else{continue};
  let result:Result<(),String>=(||{
   let m=a.request_with_timeout("thread/read",json!({"threadId":thread,"includeTurns":false}),Duration::from_secs(3))?;if m.pointer("/thread/id").and_then(Value::as_str)!=Some(thread.as_str())||m.pointer("/thread/cwd").and_then(Value::as_str)!=Some(w.cwd.as_str()){return Err("CLAUDE_NOTICE_SOURCE_CHANGED".into());}
   a.begin_latest_turn_observation(&thread)?;
   let page=a.request_with_timeout("thread/turns/list",json!({"threadId":thread,"cursor":null,"limit":1,"sortDirection":"desc","itemsView":"notLoaded"}),Duration::from_secs(3))?;
   let latest=page["data"].as_array().and_then(|r|r.first()).ok_or("CLAUDE_NOTICE_NO_TURN")?;let turn=latest["id"].as_str().ok_or("CLAUDE_NOTICE_NO_TURN")?;let status=latest["status"].as_str().unwrap_or("unknown");
   let mut questions=pending;for q in a.read_async_owner_questions(&thread,m.pointer("/thread/path").and_then(Value::as_str)).into_iter().filter(|q|q["responseSent"]==false){if let(Some(id),Some(turn))=(q["requestId"].as_str(),q["turnId"].as_str()){questions.push(Question{request_id:id.into(),turn_id:turn.into(),kind:"USER_INPUT".into()});}}
   let response=a.request_with_timeout("thread/goal/get",json!({"threadId":thread}),Duration::from_secs(3))?;let goal=crate::host_application::codex_goal_from_response(&thread,&response)?;let key=goal.as_ref().map(|g|goal_key(&thread,g));
   store.observe_codex_notices(&thread,w.generation,&w.cwd,turn,status,key.as_deref(),goal.as_ref().map(|g|g.status.as_str()).or(Some("missing")),&questions)
  })();let _=result;
 }
}
pub(crate) fn reconcile(core:&RouterCore){
 let Ok(rows)=core.store.pending_codex_native_receipts()else{return};
 // Native append may have succeeded while a callback/HTTP acknowledgement was lost.
 for(n,s)in rows.into_iter().take(4){if crate::claude_code::observed_session(core,&s.binding.receiver_sid,Some(&s.binding.receiver_cwd)).is_err(){continue;}let Some(path)=crate::claude_transcript::projects_root().and_then(|root|crate::claude_transcript::find(&root,&s.binding.receiver_sid))else{continue};if let Some(id)=crate::claude_transcript::find_plugin_prompt(&path,&s.binding.receiver_sid,&n.text,n.created_at){let _=core.store.reconcile_codex_notice(&n.id,&id);}}
}
/// Compatibility for precise-turn registrations whose UI event was pruned or missed.
pub(crate) fn legacy_probe(core:&RouterCore,token:&str,p:&router_core::store::claude_codex_wait::WaitProbe,reconcile_native:bool)->Result<Value,String>{
 if !reconcile_native{return core.store.claude_codex_wait_probe(token,p);}
 match core.store.claude_codex_wait_probe(token,p){Ok(v)=>Ok(v),Err(code)if code=="CLAUDE_WAIT_TURN_CHANGED"=>{
  let w=core.store.codex_chat_context(&p.thread_id)?;if !w.enabled||w.generation!=p.generation{return Err("CLAUDE_WAIT_GENERATION_CHANGED".into());}
  let mut s=core.session.lock().map_err(|_|"CLAUDE_WAIT_SOURCE_UNAVAILABLE")?;let a=s.adapter.as_mut().ok_or("CLAUDE_WAIT_SOURCE_UNAVAILABLE")?;let status=a.read_exact_turn_status(&p.thread_id,&p.turn_id)?.ok_or("CLAUDE_WAIT_SOURCE_UNAVAILABLE")?;let state=match status.as_str(){"completed"=>"RESULT_READY","failed"=>"FAILED","interrupted"=>"INTERRUPTED",_=>"RUNNING"};Ok(json!({"threadId":p.thread_id,"generation":p.generation,"turnId":p.turn_id,"state":state}))
 },Err(code)=>Err(code)}
}

#[cfg(test)]mod tests{
 use super::*;
 #[test]fn native_metadata_observation_follows_one_goal_without_writing_or_checkpoint_spam(){
  let d=tempfile::tempdir().unwrap();let root=d.path().to_str().unwrap().to_string();let store=Arc::new(RouterStore::open_at(d.path().join("router.db")).unwrap());let pair=store.create_claude_connection().unwrap();let sid=uuid::Uuid::new_v4().to_string();let instance=uuid::Uuid::new_v4().to_string();
  store.ingest_claude_event(&pair.token,&router_core::claude_code::Event{event_id:uuid::Uuid::new_v4().to_string(),session_id:sid.clone(),instance_id:instance.clone(),sequence:1,cwd:root.clone(),version:"2.1.309".into(),kind:"SESSION_START".into(),turn_id:None,text:None,completed_at:None}).unwrap();store.enable_codex_watch("20000000-0000-4000-8000-000000000002","fixture",&root,&router_core::store::codex_watch::WatchSnapshot{state:"RUNNING".into(),turn_id:Some("current-turn".into()),item_id:None,text:String::new()}).unwrap();
  std::fs::write(d.path().join("stage.json"),r#"{"status":"active","turn":"current-turn"}"#).unwrap();
  let script=r#"const fs=require('node:fs'),rl=require('node:readline').createInterface({input:process.stdin});const root=process.argv[1];rl.on('line',l=>{const r=JSON.parse(l);if(r.id===undefined)return;fs.appendFileSync(root+'/rpc.jsonl',JSON.stringify(r)+'\n');const stage=JSON.parse(fs.readFileSync(root+'/stage.json'));let v={};if(r.method==='thread/read')v={thread:{id:r.params.threadId,cwd:root,status:{type:'idle'}}};else if(r.method==='thread/turns/list')v={data:[{id:stage.turn,status:'completed'},...(r.params.limit===4?[{id:'old-anchor',status:'completed'}]:[])],nextCursor:null};else if(r.method==='thread/goal/get')v={goal:{threadId:r.params.threadId,objective:'Never relay this private objective',status:stage.status,createdAt:1,updatedAt:2,tokenBudget:null}};else if(r.method!=='initialize')throw Error('Unexpected write or body read: '+r.method);process.stdout.write(JSON.stringify({id:r.id,result:v})+'\n');});"#;
  let mut cmd=std::process::Command::new("node");cmd.args(["-e",script,&root]);let mut adapter=crate::codex::adapter::CodexAdapter::start_shared_validation(cmd,Arc::new(|_|{})).unwrap();adapter.initialize().unwrap();let raw=adapter.get_goal("20000000-0000-4000-8000-000000000002").unwrap();let goal=crate::host_application::codex_goal_from_response("20000000-0000-4000-8000-000000000002",&raw).unwrap().unwrap();
  // A resident adapter already served another watch: its one-page permit is consumed.
  adapter.begin_latest_turn_observation("another-watch").unwrap();adapter.request("thread/turns/list",json!({"threadId":"another-watch","cursor":null,"limit":1,"sortDirection":"desc","itemsView":"notLoaded"})).unwrap();
  let session=Arc::new(Mutex::new(Session{adapter:Some(adapter),..Default::default()}));let core=RouterCore{store:store.clone(),session:session.clone(),chatgpt:Default::default(),completed_chatgpt_responses:Default::default()};
  let w=store.registered_codex_watch("20000000-0000-4000-8000-000000000002").unwrap();store.register_codex_notice(&Binding{receiver_sid:sid.clone(),receiver_cwd:root.clone(),thread_id:w.thread_id.clone(),generation:w.generation,source_cwd:root.clone(),anchor_turn:"old-anchor".into(),mode:"GOAL".into(),goal_key:Some(goal_key(&w.thread_id,&goal)),expires_at:crate::local_safety::now()+3600000}).unwrap();
  let p:Command=serde_json::from_value(json!({"sessionId":sid,"instanceId":instance,"cwd":root,"action":"SYNC"})).unwrap();poll(&store,&session,0,&mut None);assert!(store.codex_notice_sync(&pair.token,&p).unwrap()["notice"].is_null());
  std::fs::write(d.path().join("stage.json"),r#"{"status":"blocked","turn":"next-turn"}"#).unwrap();poll(&store,&session,0,&mut None);let result=store.codex_notice_sync(&pair.token,&p).unwrap();assert_eq!(result["notice"]["sourceState"],"blocked");assert_eq!(result["notice"]["turnId"],"next-turn");assert!(!result["notice"]["text"].as_str().unwrap().contains("Never relay this private objective"));
  let probe=router_core::store::claude_codex_wait::WaitProbe{session_id:sid,instance_id:instance,cwd:root,thread_id:w.thread_id,generation:w.generation,turn_id:"old-anchor".into()};assert!(legacy_probe(&core,&pair.token,&probe,false).is_err());assert_eq!(legacy_probe(&core,&pair.token,&probe,true).unwrap()["state"],"RESULT_READY");
  let requests=std::fs::read_to_string(d.path().join("rpc.jsonl")).unwrap();assert!(!requests.contains("turn/start"));assert!(!requests.contains("thread/resume"));assert!(!requests.contains("thread/goal/set"));assert!(!requests.contains("thread/items/list"));
 }
}
