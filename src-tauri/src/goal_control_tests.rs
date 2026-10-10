use super::*;
use crate::host_application::{Session,read_codex_goal_from_session};
use router_core::store::role_bridge::RoleBindingInput;
use std::{sync::{Arc,Mutex},fs};

// Real Host/store/adapter RPC path, fictional bounded native peer. It records
// every physical request and never contacts a provider or existing user thread.
fn fixture(status:&str,mode:&str)->(tempfile::TempDir,RouterCore,String){
 let d=tempfile::tempdir().unwrap();let store=Arc::new(crate::RouterStore::open_at(d.path().join("router.db")).unwrap());
 let p=store.create_project("Goal fixture".into(),None).unwrap();let w=store.create_workstream(&p.id,"fixture".into()).unwrap();
 let side=|id:&str|RoleBindingInput{provider:"CODEX".into(),external_id:id.into(),label:"fictional".into(),cwd:Some(d.path().to_string_lossy().into())};
 store.bind_role_bridge(&w.id,0,side("goal-source-fixture"),side("goal-target-fixture")).unwrap();
 let script=r#"const fs=require('node:fs'),rl=require('node:readline').createInterface({input:process.stdin});const root=process.argv[1],mode=process.argv[3];let goal={threadId:'goal-target-fixture',objective:'Keep the same approved objective',status:process.argv[2],tokenBudget:null,tokensUsed:43,timeUsedSeconds:21,createdAt:1,updatedAt:2};rl.on('line',line=>{const r=JSON.parse(line);if(r.id===undefined)return;fs.appendFileSync(root+'/rpc.jsonl',JSON.stringify(r)+'\n');let v={};if(r.method==='thread/read'||r.method==='thread/resume')v={thread:{id:r.params.threadId,cwd:root,status:{type:mode==='BUSY'?'active':'idle'}}};else if(r.method==='thread/goal/get')v={goal};else if(r.method==='thread/goal/set'){if(JSON.stringify(Object.keys(r.params).sort())!==JSON.stringify(['origin','status','threadId'])||r.params.origin!=='user'||r.params.threadId!=='goal-target-fixture')throw Error('Goal identity or status-only contract violated');goal={...goal,status:r.params.status,updatedAt:goal.updatedAt+1};v={goal:mode==='WRONG_GOAL'?{...goal,objective:'wrong objective'}:goal};}else if(r.method==='turn/start')v={turn:{id:'fixture-supplement-turn'}};else if(r.method==='thread/turns/list')v={data:[],nextCursor:null};else if(r.method!=='initialize'){process.stdout.write(JSON.stringify({id:r.id,error:{code:-32600,message:'unexpected fixture method'}})+'\n');return;}process.stdout.write(JSON.stringify({id:r.id,result:v})+'\n');});"#;
 let mut command=std::process::Command::new("node");command.args(["-e",script]).arg(d.path()).arg(status).arg(mode);
 let mut adapter=crate::codex::adapter::CodexAdapter::start_shared_validation(command,Arc::new(|_|{})).unwrap();adapter.initialize().unwrap();
 let core=RouterCore{store,session:Arc::new(Mutex::new(Session{adapter:Some(adapter),..Default::default()})),chatgpt:Arc::default(),completed_chatgpt_responses:Arc::default()};(d,core,w.id)
}
fn input(core:&RouterCore)->GoalInput{let goal=read_codex_goal_from_session(&mut core.session.lock().unwrap(),"goal-target-fixture").unwrap().unwrap();GoalInput{target:target(core,"goal-target-fixture").unwrap(),expected_goal_fingerprint:goal.fingerprint}}
fn sets(d:&tempfile::TempDir)->Vec<Value>{fs::read_to_string(d.path().join("rpc.jsonl")).unwrap().lines().map(|s|serde_json::from_str::<Value>(s).unwrap()).filter(|r|r["method"]=="thread/goal/set").collect()}
#[test]fn resumes_same_stopped_goal_once_and_never_recreates_or_sends_chat(){
 for status in ["paused","blocked"]{
  let(d,c,_)=fixture(status,"IDLE");let input=input(&c);let id=uuid::Uuid::new_v4().to_string();let r=apply(&c,&id,input.clone(),"RESUME_GOAL",false).unwrap();
  assert_eq!(r.status,"APPLIED");let g=r.result.unwrap();assert_eq!(g["status"],"active");assert_eq!(g["objective"],"Keep the same approved objective");assert!(g["tokenBudget"].is_null());assert_eq!(g["tokensUsed"],43);assert_eq!(g["timeUsedSeconds"],21);
  assert_eq!(apply(&c,&id,input,"RESUME_GOAL",false).unwrap().status,"APPLIED");assert_eq!(sets(&d).len(),1);
  assert!(!fs::read_to_string(d.path().join("rpc.jsonl")).unwrap().contains("turn/start"));
 }
}
#[test]fn native_pause_is_status_only_and_budget_or_complete_goals_cannot_resume(){
 let(d,c,_)=fixture("active","BUSY");let r=apply(&c,&uuid::Uuid::new_v4().to_string(),input(&c),"PAUSE_GOAL",false).unwrap();assert_eq!(r.result.unwrap()["status"],"paused");assert_eq!(sets(&d).len(),1);
 for status in ["budgetLimited","complete"]{let(d,c,_)=fixture(status,"IDLE");assert!(apply(&c,&uuid::Uuid::new_v4().to_string(),input(&c),"RESUME_GOAL",true).is_err());assert!(sets(&d).is_empty());}
}
#[test]fn fingerprint_rebind_pending_and_live_turn_guards_fail_before_any_native_mutation(){
 let(d,c,w)=fixture("paused","IDLE");let mut stale=input(&c);stale.expected_goal_fingerprint="b".repeat(64);assert!(apply(&c,&uuid::Uuid::new_v4().to_string(),stale,"RESUME_GOAL",false).is_err());
 let old=input(&c);let b=c.store.role_bridge(&w).unwrap();let side=|id:&str|RoleBindingInput{provider:"CODEX".into(),external_id:id.into(),label:"fictional".into(),cwd:Some(d.path().to_string_lossy().into())};c.store.bind_role_bridge(&w,b.binding_revision,side("other-source"),side("goal-target-fixture")).unwrap();assert!(apply(&c,&uuid::Uuid::new_v4().to_string(),old,"RESUME_GOAL",false).is_err());assert!(sets(&d).is_empty());
 let(d,c,_)=fixture("paused","BUSY");assert!(apply(&c,&uuid::Uuid::new_v4().to_string(),input(&c),"RESUME_GOAL",false).is_err());assert!(sets(&d).is_empty());
 let(d,c,_)=fixture("paused","IDLE");let g=c.store.connect_assistant_instance(router_core::store::assistant::AssistantConnectionInput{label:"fixture".into(),expires_at:std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis() as i64+600000}).unwrap();let action=c.store.prepare_assistant_action(&g.id,router_core::store::assistant_actions::ActionInput{request_id:"pending".into(),operation:"RESUME_GOAL".into(),input:json!({"target":{"threadId":"goal-target-fixture"}})}).unwrap();c.store.ask_assistant_action(&g.id,&action.id,&action.payload_hash,"Owner must decide").unwrap();assert!(apply(&c,&uuid::Uuid::new_v4().to_string(),input(&c),"RESUME_GOAL",false).is_err());assert!(sets(&d).is_empty());
}
#[test]fn an_unconfirmed_native_response_keeps_original_receipt_and_blocks_another_request(){
 let(d,c,_)=fixture("paused","WRONG_GOAL");let input=input(&c);let id=uuid::Uuid::new_v4().to_string();assert!(apply(&c,&id,input.clone(),"RESUME_GOAL",false).is_err());assert_eq!(c.store.goal_control_receipt("goal-target-fixture",&id).unwrap().unwrap().status,"UNKNOWN");assert_eq!(apply(&c,&id,input.clone(),"RESUME_GOAL",false).unwrap().status,"UNKNOWN");assert!(apply(&c,&uuid::Uuid::new_v4().to_string(),input,"RESUME_GOAL",false).is_err());assert_eq!(sets(&d).len(),1);
}
#[test]fn usage_limited_assistant_resume_requires_real_saved_owner_answer(){
 let(d,c,_)=fixture("usageLimited","IDLE");assert!(apply(&c,&uuid::Uuid::new_v4().to_string(),input(&c),"RESUME_GOAL",false).is_err());assert!(sets(&d).is_empty());
 let(d,c,_)=fixture("usageLimited","IDLE");let g=c.store.connect_assistant_instance(router_core::store::assistant::AssistantConnectionInput{label:"fixture".into(),expires_at:std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis() as i64+600000}).unwrap();let i=input(&c);
 let call=|name:&str,args|crate::assistant_mcp::call(&c,&g.id,name,args).unwrap();
 let a=call("agbrio_prepare_action",json!({"requestId":"owner-reviewed","operation":"RESUME_GOAL","input":i}));let id=a["id"].as_str().unwrap();let hash=a["payloadHash"].as_str().unwrap();
 call("agbrio_ask_action_decision",json!({"actionId":id,"expectedHash":hash,"question":"Continue same Goal after native usage limit clears?"}));
 call("agbrio_answer_action",json!({"actionId":id,"expectedHash":hash,"answer":"Continue the original Goal without changing its budget","answerReference":"actual fixture owner message"}));
 let r=call("agbrio_execute_action",json!({"actionId":id,"expectedHash":hash,"ruleId":null,"useOwnerAnswer":true,"assessment":"Actual fixture owner approved this exact Goal continuation."}));assert_eq!(r["status"],"APPLIED");assert_eq!(r["result"]["status"],"APPLIED");assert_eq!(sets(&d).len(),1);
}

#[test]fn fingerprint_pins_the_same_goal_contract_without_rejecting_native_accounting_progress(){
 let(_d,c,_)=fixture("active","BUSY");let mut g=read_codex_goal_from_session(&mut c.session.lock().unwrap(),"goal-target-fixture").unwrap().unwrap();let original=fingerprint(&g);
 g.tokens_used=Some(100);g.time_used_seconds=Some(90);g.updated_at=Some(json!(999));assert_eq!(fingerprint(&g),original);
 g.token_budget=Some(1000);assert_ne!(fingerprint(&g),original);g.token_budget=None;
 g.created_at=Some(json!(3));assert_ne!(fingerprint(&g),original);
}

#[test]fn a_genuine_supplement_in_a_loaded_idle_active_goal_chat_uses_one_start_and_keeps_the_goal(){
 let(d,c,_)=fixture("active","IDLE");let context=c.store.codex_chat_context("goal-target-fixture").unwrap();let id=uuid::Uuid::new_v4().to_string();
 let command=crate::watch_chat::ChatCommand::Send{thread_id:context.thread_id.clone(),id:id.clone(),generation:context.generation,source_sequence:None,expected_turn_id:None,mode:"SEND".into(),text:"New authorized supplemental instruction".into(),options:Default::default()};
 let r=crate::watch_chat::command(&c,command).unwrap();assert_eq!(r["status"],"SENT");assert_eq!(r["turnId"],"fixture-supplement-turn");assert!(sets(&d).is_empty());
 let records=fs::read_to_string(d.path().join("rpc.jsonl")).unwrap();assert_eq!(records.lines().filter(|s|s.contains("turn/start")).count(),1);assert!(!records.contains("thread/goal/clear"));
}

#[test]
#[ignore="explicit one isolated native Goal control gate; minimum-cost model; no existing user goal writes"]
fn real_shared_native_goal_control_once(){
 assert_eq!(std::env::var("AGBRIO_REAL_GOAL_CONTROL_GATE").as_deref(),Ok("1"));
 let root=std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).parent().unwrap().join("runtime/mcp-events-20261010/goal-native-reference").join(format!("native-goal-{}",uuid::Uuid::new_v4()));fs::create_dir_all(&root).unwrap();
 let command=crate::shared_codex::transport_command().unwrap().expect("Existing shared transport required; never launch/restart a backend");
 let mut a=crate::codex::adapter::CodexAdapter::start_shared_validation(command,Arc::new(|_|{})).unwrap();a.initialize().unwrap();
 let response=a.request("thread/start",json!({"cwd":root.to_string_lossy(),"model":"gpt-6-luna","config":{"model_reasoning_effort":"low"},"approvalPolicy":"never","sandbox":"read-only"})).unwrap();let id=response["thread"]["id"].as_str().unwrap().to_string();let cwd=response["thread"]["cwd"].as_str().unwrap().to_string();
 let objective="Isolated Agbrio native Goal control validation. Do not use tools, files, network or other chats. Reply only AGBRIO_GOAL_CONTROL_NATIVE_ACK. The validation controller will pause this Goal; do not expand the task.";
 let initial=a.request("thread/goal/set",json!({"threadId":id,"objective":objective,"status":"paused","origin":"user"})).unwrap();let g=crate::host_application::codex_goal_from_response(&id,&initial).unwrap().unwrap();assert_eq!(g.status,"paused");assert!(g.token_budget.is_none());
 let store=Arc::new(crate::RouterStore::open_at(root.join("router.db")).unwrap());store.enable_codex_watch(&id,"isolated native Goal gate",&cwd,&router_core::store::codex_watch::WatchSnapshot{state:"IDLE".into(),turn_id:None,item_id:None,text:String::new()}).unwrap();
 let c=RouterCore{store,session:Arc::new(Mutex::new(Session{adapter:Some(a),..Default::default()})),chatgpt:Arc::default(),completed_chatgpt_responses:Arc::default()};
 let before=read_codex_goal_from_session(&mut c.session.lock().unwrap(),&id).unwrap().unwrap();let i=GoalInput{target:target(&c,&id).unwrap(),expected_goal_fingerprint:before.fingerprint.clone()};let request=uuid::Uuid::new_v4().to_string();
 let result=apply(&c,&request,i.clone(),"RESUME_GOAL",true);
 let mut proof=json!({"realNative":true,"minimumCostModel":"gpt-6-luna","reasoning":"low","existingUserGoalsTouched":false,"sharedBackendRestarted":false,"sourceTestRetriggered":false,"newChatSendUsedForResume":false,"nativeGoalCreationForIsolatedValidationOnly":true});
 if let Ok(receipt)=&result{proof["resumeReceipt"]=json!(receipt.status);proof["sameObjective"]=json!(receipt.result.as_ref().is_some_and(|g|g["objective"]==objective));proof["budgetRetained"]=json!(receipt.result.as_ref().is_some_and(|g|g["tokenBudget"].is_null()));proof["sameRequestReplay"]=json!(apply(&c,&request,i,"RESUME_GOAL",true).unwrap().status);}
 // Finish this disposable Goal. No historical handoff is sent, no real Bridge
 // is bound/subscribed, and cleanup never touches another thread.
 {let mut s=c.session.lock().unwrap();let a=s.adapter.as_mut().unwrap();a.clear_goal(&id).unwrap();assert!(a.get_goal(&id).unwrap()["goal"].is_null());}
 proof["isolatedGoalCleared"]=json!(true);fs::write(root.join("native-goal-control-proof.json"),serde_json::to_vec_pretty(&proof).unwrap()).unwrap();fs::write(root.parent().unwrap().join("current-native-goal-control-case.txt"),root.to_string_lossy().as_bytes()).unwrap();
 assert!(result.is_ok(),"Native Goal resume did not pass; isolated Goal cleaned up");assert_eq!(proof["resumeReceipt"],"APPLIED");assert_eq!(proof["sameObjective"],true);assert_eq!(proof["budgetRetained"],true);
}
