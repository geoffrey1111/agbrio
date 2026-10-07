//! Explicit disposable provider gate; uses the existing shared Gateway only.
use super::*;
use crate::codex::adapter::CodexAdapter;
use crate::host_application::{exact_codex_completed_event_result,start_turn,Session};
use router_core::{events::NullEventSink,store::{RouterStore,assistant::{GrantInput,BriefRule}}};
use std::{sync::{Arc,Mutex},collections::HashMap,time::{Instant,Duration}};
#[test]
#[ignore="read-only completion metadata from existing archived disposable QA chats; zero model turns"]
fn real_bridge_reply_completion_read_only(){
 assert_eq!(std::env::var("AGBRIO_RECENCY_READ_GATE").as_deref(),Ok("1"));
 let path=std::path::PathBuf::from(std::env::var("AGBRIO_RECENCY_QA_PROOF").unwrap());let proof:Value=serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();assert_eq!(proof["qaThreadsArchived"],true);
 let mut adapter=CodexAdapter::start(Arc::new(NullEventSink),Arc::new(|_|{})).unwrap();adapter.initialize().unwrap();assert!(adapter.is_shared());
 let mut times=Vec::new();for key in ["sourceThread","targetThread"]{
  let id=proof[key].as_str().unwrap();let read=crate::host_application::read_latest_codex_reply(&mut adapter,id).unwrap();let reply=read.reply.unwrap();assert_eq!(reply.thread.id,id);times.push(reply.completed_at);
 }
 adapter.shutdown();let result=json!({"existingDisposableThreadsOnly":true,"modelTurns":0,"nativeCompletedAt":times,"allCompletionTimesAvailable":times.iter().all(Option::is_some),"sharedGateway":true});
 let root=std::path::Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap().join("runtime/bridge-notification");std::fs::write(root.join("real-timing-proof.json"),serde_json::to_vec_pretty(&result).unwrap()).unwrap();
}
#[test]
#[ignore="explicit two-turn lowest-cost MCP/Codex shared gateway handoff proof"]
fn real_assistant_mcp_shared_gateway_roundtrip(){
 assert_eq!(std::env::var("AGBRIO_REAL_ASSISTANT_GATE").as_deref(),Ok("1"));
 assert!(crate::shared_codex::transport_command().unwrap().is_some(),"Require the existing shared Gateway; never start another native backend");
 let root=std::path::Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap().join("runtime/assistant-mcp-validation");std::fs::create_dir_all(&root).unwrap();let dir=root.join(format!("real-{}",uuid::Uuid::new_v4()));std::fs::create_dir(&dir).unwrap();
 let source_root=dir.join("source");let target_root=dir.join("target");std::fs::create_dir(&source_root).unwrap();std::fs::create_dir(&target_root).unwrap();
 let events:Arc<Mutex<HashMap<String,(String,String,String)>>>=Arc::default();let seen=events.clone();
 let listener=Arc::new(move |event:&Value|{if event["method"]!="turn/completed"{return;}let Some(turn)=event.pointer("/params/turn/id").and_then(Value::as_str)else{return};let Some(thread)=event.pointer("/params/threadId").or_else(||event.pointer("/params/turn/threadId")).and_then(Value::as_str)else{return};if let Ok((item,text))=exact_codex_completed_event_result(thread,turn,event){seen.lock().unwrap().insert(turn.into(),(thread.into(),item,text));}});
 let mut adapter=CodexAdapter::start(Arc::new(NullEventSink),listener).unwrap();adapter.initialize().unwrap();assert!(adapter.is_shared());
 let mut ids=Vec::new();for cwd in [&source_root,&target_root]{let started=adapter.request("thread/start",json!({"cwd":cwd,"model":"gpt-6-luna","config":{"model_reasoning_effort":"low"},"approvalPolicy":"never","sandbox":"read-only"})).unwrap();assert_eq!(started["model"],"gpt-6-luna");assert_eq!(started["reasoningEffort"],"low");ids.push(started["thread"]["id"].as_str().unwrap().to_owned());}
 let store=Arc::new(RouterStore::open_at(dir.join("router.db")).unwrap());let core=RouterCore{store:store.clone(),chatgpt:Arc::default(),session:Arc::new(Mutex::new(Session::default())),completed_chatgpt_responses:Arc::default()};
 {let mut s=core.session.lock().unwrap();s.ready_threads.extend(ids.iter().cloned());s.adapter=Some(adapter);}
 let p=store.create_project("Disposable MCP validation".into(),None).unwrap();let w=store.create_workstream(&p.id,"MCP delegated handoff QA".into()).unwrap();
 let side=|id:&str|router_core::store::role_bridge::RoleBindingInput{provider:"CODEX".into(),external_id:id.into(),label:"Disposable QA".into(),cwd:None};let bindings=crate::role_bridge::bind(&core,&w.id,w.binding_revision,side(&ids[0]),side(&ids[1])).unwrap();
 let material=source_root.join("material.txt");let bytes=b"Agbrio selected material\nExact UTF-8 bytes.\n";std::fs::write(&material,bytes).unwrap();
 let prompt=format!("Reply with exactly one line, no tools and no other text: ARTIFACT: {}",material.display());
 let turn={let mut s=core.session.lock().unwrap();start_turn(s.adapter.as_mut().unwrap(),&ids[0],&prompt).unwrap().turn_id};
 let wait=|turn:&str|{let deadline=Instant::now()+Duration::from_secs(150);loop{if let Some(v)=events.lock().unwrap().get(turn).cloned(){return v;}assert!(Instant::now()<deadline,"Exact original turn timed out. Never resend.");std::thread::sleep(Duration::from_millis(200));}};let original=wait(&turn);
 let state=crate::role_bridge::read(&core,&w.id,"DECISION").unwrap();let obs=state.replies.iter().find(|r|r.endpoint_id==bindings.decision.as_ref().unwrap().endpoint.id&&r.text==original.2).unwrap();
 let grant=store.create_assistant_grant(GrantInput{workstream_id:w.id.clone(),source_role:"DECISION".into(),binding_revision:bindings.binding_revision,label:"MCP QA delegate".into(),rules:vec![BriefRule{id:"qa-marker".into(),text:"Forward only the exact QA marker instruction and selected material to this disposable target.".into()}],expires_at:std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis() as i64+600000}).unwrap();
 let files=call(&core,&grant.id,"agbrio_read_source",json!({"observationId":obs.id})).unwrap();assert_eq!(files["attachments"].as_array().unwrap().len(),1);let attachment=files["attachments"][0]["id"].as_str().unwrap();
 let prepared=call(&core,&grant.id,"agbrio_prepare_handoff",json!({"requestId":"live-marker-1","observationId":obs.id,"text":"Return exactly AGBRIO_MCP_TARGET_OK; do not run tools.","attachmentIds":[attachment]})).unwrap();let hid=prepared["id"].as_str().unwrap();let hash=prepared["payloadHash"].as_str().unwrap();
 let approve=json!({"handoffId":hid,"expectedHash":hash,"ruleId":"qa-marker","decisionId":null,"assessment":"Matches the owner-authorized disposable marker rule; one exact material file."});
 let sent=call(&core,&grant.id,"agbrio_confirm_and_send",approve.clone()).unwrap();assert_eq!(sent["status"],"SENT");assert!(call(&core,&grant.id,"agbrio_confirm_and_send",approve).is_err());
 let run=store.provider_runs_for_workstream(&w.id).unwrap().into_iter().find(|r|r.origin_handoff_id.as_deref()==Some(hid)).unwrap();let received=wait(run.external_run_id.as_deref().unwrap());assert_eq!(received.0,ids[1]);assert_eq!(received.2.trim(),"AGBRIO_MCP_TARGET_OK");
 assert_eq!(std::fs::read(target_root.join(".aiwr/incoming").join(hid).join("material.txt")).unwrap(),bytes);
 let receipt=call(&core,&grant.id,"agbrio_receipt",json!({"handoffId":hid,"expectedHash":hash})).unwrap();assert_eq!(receipt["handoff"]["status"],"SENT");
 store.revoke_assistant_grant(&grant.id).unwrap();
 {let mut s=core.session.lock().unwrap();for id in &ids{s.adapter.as_mut().unwrap().request("thread/archive",json!({"threadId":id})).unwrap();}s.adapter.take().unwrap().shutdown();}
 std::fs::write(dir.join("proof.json"),serde_json::to_vec_pretty(&json!({"status":"AUTOMATED_VALIDATION_PASS","sharedGateway":true,"sourceThread":ids[0],"targetThread":ids[1],"handoff":hid,"payloadHash":hash,"targetResult":received.2,"selectedMaterialBytes":bytes.len(),"duplicateRejected":true,"qaThreadsArchived":true,"physicalTurns":2,"model":"gpt-6-luna","reasoning":"low","notDotE2E":true})).unwrap()).unwrap();
 println!("MCP_SHARED_GATEWAY_PROOF={}",dir.join("proof.json").display());
}
