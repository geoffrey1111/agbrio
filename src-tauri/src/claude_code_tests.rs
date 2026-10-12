use crate::claude_code::*;
use crate::host_application::Session;
use router_core::claude_code::Event;
use router_core::store::role_bridge::RoleBindingInput;
use std::{collections::HashMap,sync::{Arc,Mutex}};
fn event(root:&std::path::Path,session:&str,kind:&str,sequence:i64)->Event {
 Event{event_id:uuid::Uuid::new_v4().to_string(),session_id:session.into(),instance_id:format!("20000000-0000-4000-8000-{}",&session[24..]),sequence,cwd:root.to_string_lossy().into(),version:"2.1.286".into(),kind:kind.into(),turn_id:None,text:None,completed_at:None}
}
#[test] fn claude_fixture_bridge_read_review_and_send_gate_without_any_provider() {
 let temp=tempfile::tempdir().unwrap();
 let store=Arc::new(crate::RouterStore::open_at(temp.path().join("router.db")).unwrap());
 let core=RouterCore{chatgpt:Arc::new(Default::default()),session:Arc::new(Mutex::new(Session::default())),completed_chatgpt_responses:Arc::new(Mutex::new(HashMap::new())),store:store.clone()};
 let pair=store.create_claude_connection().unwrap();
 let a="10000000-0000-4000-8000-000000000001";let b="10000000-0000-4000-8000-000000000002";
 for session in [a,b] {ingest(&core,&pair.token,&event(temp.path(),session,"SESSION_START",1)).unwrap();}
 let project=store.create_project("fictional Claude test".into(),None).unwrap();let w=store.create_workstream(&project.id,"fictional bridge".into()).unwrap();
 let side=|session:&str|RoleBindingInput{provider:"CLAUDE_CODE".into(),external_id:session.into(),label:"fictional".into(),cwd:None};
 let bindings=crate::role_bridge::bind(&core,&w.id,w.binding_revision,side(a),side(b)).unwrap();
 let mut complete=event(temp.path(),a,"COMPLETE",3);complete.turn_id=Some(uuid::Uuid::new_v4().to_string());complete.text=Some("仅模拟验收\n\n```text\n完整指令\n```".into());complete.completed_at=Some(std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis() as i64);
 let mut start=event(temp.path(),a,"RUNNING",2);start.turn_id=complete.turn_id.clone();ingest(&core,&pair.token,&start).unwrap();
 assert!(ingest(&core,&pair.token,&complete).unwrap());assert!(!ingest(&core,&pair.token,&complete).unwrap());
 let state=crate::role_bridge::read(&core,&w.id,"DECISION").unwrap();assert_eq!(state.replies.len(),1);assert_eq!(state.replies[0].text,complete.text.clone().unwrap());
 let activity=crate::role_bridge::directory_activity(&core,&[w.id.clone()]).unwrap();assert_eq!(activity[0].sides[0].state,"COMPLETE");assert!(activity[0].sides[0].result_observation_id.is_some());
 let h=crate::role_bridge::prepare(&core,&w.id,"DECISION",&state.replies[0].id,"完整指令",&[]).unwrap();
 assert_eq!(h.direction,"CLAUDE_CODE_TO_CLAUDE_CODE");
 let approved=crate::role_bridge::approve(&core,&h.id,&h.payload_hash).unwrap();assert_eq!(approved.status,"APPROVED");
 assert_eq!(crate::role_bridge::send(&core,&h.id).err().unwrap(),"CLAUDE_WRITES_DISABLED_BY_OWNER");
 assert_eq!(store.role_handoff(&h.id).unwrap().status,"APPROVED");
 assert_eq!(store.claim_role_handoff(&h.id).unwrap_err(),"CLAUDE_WRITES_DISABLED_BY_OWNER");
 assert_eq!(store.role_handoff(&h.id).unwrap().status,"APPROVED");
 assert!(core.session.lock().unwrap().adapter.is_none());
 let mut running=event(temp.path(),a,"RUNNING",4);running.turn_id=Some(uuid::Uuid::new_v4().to_string());ingest(&core,&pair.token,&running).unwrap();
 let activity=crate::role_bridge::directory_activity(&core,&[w.id.clone()]).unwrap();assert_eq!(activity[0].sides[0].state,"RUNNING");
 assert!(!ingest(&core,&pair.token,&complete).unwrap()); // old duplicate after a newer active turn
 assert_eq!(store.claude_session(a).unwrap().state,"RUNNING");
 assert_eq!(crate::role_bridge::read(&core,&w.id,"DECISION").unwrap().replies.len(),1); // retained prior history, no new complete reply
 store.trash_workstream(&w.id).unwrap();
 let mut later=complete.clone();later.event_id=uuid::Uuid::new_v4().to_string();later.sequence=5;later.turn_id=running.turn_id.clone();
 ingest(&core,&pair.token,&later).unwrap();assert_eq!(store.reply_observations_for_workstream(&w.id).unwrap().len(),1);
 assert!(bindings.explicit_roles);
}

#[test]
fn native_setup_uses_live_listener_not_deployment_parser_or_working_directory(){
 let temp=tempfile::tempdir().unwrap();
 let core=RouterCore{chatgpt:Arc::default(),session:Arc::default(),completed_chatgpt_responses:Arc::default(),store:Arc::new(crate::RouterStore::open_at(temp.path().join("router.db")).unwrap())};
 let host=crate::HostRuntime::default();
 assert!(setup(&core,&host,temp.path()).is_err());assert!(core.store.claude_connections().unwrap().is_empty());
 let plugin=temp.path().join("claude-code-plugin/.claude-plugin");std::fs::create_dir_all(&plugin).unwrap();std::fs::write(plugin.join("plugin.json"),"{}").unwrap();
 assert_eq!(setup(&core,&host,temp.path()).unwrap_err(),"CLAUDE_OBSERVATION_HOST_UNAVAILABLE");assert!(core.store.claude_connections().unwrap().is_empty());
 // No index.html is required by observation setup, and configured port0 is not
 // the returned address: the actual kernel-assigned live listener is authoritative.
 *host.test_web_auth.lock().unwrap()=Some(Arc::new(crate::web_auth::WebAuth::open(None).unwrap()));
 let config=crate::mobile_http::MobileHttpConfig{port:0,allowed_host:"127.0.0.1:0".into(),allowed_origin:"http://127.0.0.1:0".into(),access_issuer:String::new(),access_audience:String::new(),access_jwks_url:String::new(),static_dir:temp.path().into()};
 host.ensure_mobile(core.clone(),config);
 let before=host.mobile_address().unwrap();let result=setup(&core,&host,temp.path()).unwrap();
 assert_eq!(result["hostUrl"],format!("http://{before}"));assert_eq!(result["writesEnabled"],false);assert_eq!(core.store.claude_connections().unwrap().len(),1);assert!(core.session.lock().unwrap().adapter.is_none());
 let event=event(temp.path(),"10000000-0000-4000-8000-000000000001","SESSION_START",1);
 let response=reqwest::blocking::Client::new().post(format!("http://{before}/v1/claude-code/events")).bearer_auth(result["token"].as_str().unwrap()).json(&event).send().unwrap();assert!(response.status().is_success());
 assert_eq!(host.mobile_address(),Some(before));host.shutdown(&core);
}
