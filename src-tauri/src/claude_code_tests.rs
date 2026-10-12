use crate::claude_code::*;
use crate::host_application::Session;
use router_core::claude_code::Event;
use router_core::store::role_bridge::RoleBindingInput;
use std::{collections::HashMap,sync::{Arc,Mutex}};
fn event(root:&std::path::Path,session:&str,kind:&str,sequence:i64)->Event {
 Event{event_id:uuid::Uuid::new_v4().to_string(),session_id:session.into(),instance_id:format!("20000000-0000-4000-8000-{}",&session[24..]),sequence,cwd:root.to_string_lossy().into(),version:"2.1.286".into(),kind:kind.into(),turn_id:None,text:None,completed_at:None}
}
#[test]fn manual_claude_chat_uses_current_bridge_generation_and_never_enables_mcp_writes(){
 let temp=tempfile::tempdir().unwrap();let store=Arc::new(crate::RouterStore::open_at(temp.path().join("router.db")).unwrap());
 let core=RouterCore{chatgpt:Arc::new(Default::default()),session:Arc::new(Mutex::new(Session::default())),completed_chatgpt_responses:Arc::new(Mutex::new(HashMap::new())),store:store.clone()};
 let pair=store.create_claude_connection().unwrap();let sid="10000000-0000-4000-8000-000000000001";
 ingest(&core,&pair.token,&event(temp.path(),sid,"SESSION_START",1)).unwrap();
 let project=store.create_project("fictional".into(),None).unwrap();let w=store.create_workstream(&project.id,"fictional".into()).unwrap();
 let input=RoleBindingInput{provider:"CLAUDE_CODE".into(),external_id:sid.into(),label:"original Claude".into(),cwd:Some(temp.path().to_string_lossy().into())};
 ingest(&core,&pair.token,&event(temp.path(),"10000000-0000-4000-8000-000000000002","SESSION_START",1)).unwrap();
 crate::role_bridge::bind(&core,&w.id,w.binding_revision,input.clone(),RoleBindingInput{external_id:"10000000-0000-4000-8000-000000000002".into(),..input}).unwrap();
 let turn=uuid::Uuid::new_v4().to_string();let mut running=event(temp.path(),sid,"RUNNING",2);running.turn_id=Some(turn.clone());ingest(&core,&pair.token,&running).unwrap();
 let mut complete=event(temp.path(),sid,"COMPLETE",3);complete.turn_id=Some(turn.clone());complete.text=Some("observed original answer".into());complete.completed_at=Some(std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis() as i64);ingest(&core,&pair.token,&complete).unwrap();
 let key=format!("claude_code:{sid}");let before=crate::claude_chat::state(&core,&key).unwrap();assert_eq!(before["inputReady"],false);let generation=before["watch"]["generation"].as_i64().unwrap();assert!(generation<0);
 let mut beat=event(temp.path(),sid,"HEARTBEAT",4);beat.turn_id=Some(turn.clone());ingest(&core,&pair.token,&beat).unwrap();store.offer_claude_manual(&pair.token,&beat).unwrap();assert_eq!(crate::claude_chat::state(&core,&key).unwrap()["inputReady"],true);
 let command=|generation|crate::watch_chat::ChatCommand::Send{thread_id:key.clone(),id:uuid::Uuid::new_v4().to_string(),generation,source_sequence:None,expected_turn_id:Some(turn.clone()),mode:"SEND".into(),text:"owner supplied actual reply".into(),options:Default::default()};
 assert_eq!(crate::claude_chat::command(&core,command(generation-1)).unwrap_err(),"REPLY_TARGET_CHANGED_REFRESH");
 let r=crate::claude_chat::command(&core,command(generation)).unwrap();assert_eq!(r["status"],"QUEUED");let offered=store.offer_claude_manual(&pair.token,&beat).unwrap().unwrap();assert_eq!(offered.session_id,sid);
 assert!(!router_core::claude_code::WRITES_ENABLED);assert_eq!(router_core::claude_code::require_write_capability().unwrap_err(),"CLAUDE_WRITES_DISABLED_BY_OWNER");
 assert!(crate::claude_chat::command(&core,crate::watch_chat::ChatCommand::Stop{thread_id:key,turn_id:turn}).is_err());
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
 // Resident callback must not depend on the currently selected UI project.
 let other=store.create_project("other selected project".into(),None).unwrap();store.select_workspace(&other.id,None).unwrap();
 assert!(store.snapshot().unwrap().workstreams.iter().all(|row|row.id!=w.id));
 let mut complete=event(temp.path(),a,"COMPLETE",3);complete.turn_id=Some(uuid::Uuid::new_v4().to_string());complete.text=Some("仅模拟验收\n\n```text\n完整指令\n```".into());complete.completed_at=Some(std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis() as i64);
 let mut start=event(temp.path(),a,"RUNNING",2);start.turn_id=complete.turn_id.clone();ingest(&core,&pair.token,&start).unwrap();
 assert!(ingest(&core,&pair.token,&complete).unwrap());assert!(!ingest(&core,&pair.token,&complete).unwrap());
 assert_eq!(store.reply_observations_for_workstream(&w.id).unwrap().len(),1); // before any explicit read/materialize
 assert_eq!(store.snapshot().unwrap().selected_project_id.as_deref(),Some(other.id.as_str()));
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

struct FakePluginAdmin {
 path:std::path::PathBuf,
 configured:Mutex<HashMap<String,(String,String)>>,
 calls:Mutex<Vec<Vec<String>>>,
 fail_inline:bool,
}
impl crate::claude_code_desktop::PluginAdmin for FakePluginAdmin {
 fn run(&self,args:&[String],input:Option<&[u8]>)->Result<serde_json::Value,String>{
  assert!(!args.iter().any(|a|matches!(a.as_str(),"-p"|"--resume"|"--continue"|"--force"|"--fork-session")));
  self.calls.lock().unwrap().push(args.to_vec());
  if args==["plugin","list","--json"] {return Ok(serde_json::json!([{"id":"agbrio-bridge@agbrio-observation","scope":"user","enabled":true,"installPath":self.path}]));}
  assert_eq!(args[2],"plugin");assert_eq!(args[3],"configure");
  let identity=&args[4];
  if let Some(input)=input {
   if self.fail_inline&&identity=="agbrio-bridge@inline" {return Err("CLAUDE_PLUGIN_ADMIN_REFUSED".into());}
   let value:serde_json::Value=serde_json::from_slice(input).unwrap();
   let token=value["token"].as_str().unwrap();assert_eq!(token.len(),64);assert!(!args.iter().any(|a|a.contains(token)));
   self.configured.lock().unwrap().insert(identity.clone(),(value["hostUrl"].as_str().unwrap().into(),token.into()));
   Ok(serde_json::json!({"saved":["hostUrl","token"],"unconfigured":[]}))
  }else if let Some((host,_))=self.configured.lock().unwrap().get(identity) {
   Ok(serde_json::json!({"pluginId":identity,"configured":["hostUrl","token"],"unconfigured":[],"inputs":{"hostUrl":host,"token":""}}))
  }else{Ok(serde_json::json!({"pluginId":identity,"configured":[],"unconfigured":["hostUrl","token"],"inputs":{"hostUrl":"","token":""}}))}
 }
}
fn connection_fixture(temp:&tempfile::TempDir)->(RouterCore,crate::HostRuntime,std::path::PathBuf,FakePluginAdmin){
 let core=RouterCore{chatgpt:Arc::default(),session:Arc::default(),completed_chatgpt_responses:Arc::default(),store:Arc::new(crate::RouterStore::open_at(temp.path().join("router.db")).unwrap())};
 let host=crate::HostRuntime::default();*host.test_web_auth.lock().unwrap()=Some(Arc::new(crate::web_auth::WebAuth::open(None).unwrap()));
 host.ensure_mobile(core.clone(),crate::mobile_http::MobileHttpConfig{port:0,allowed_host:"127.0.0.1:0".into(),allowed_origin:"http://127.0.0.1:0".into(),access_issuer:String::new(),access_audience:String::new(),access_jwks_url:String::new(),static_dir:temp.path().into()});
 let package=temp.path().join("claude-code-plugin/.claude-plugin");let cache=temp.path().join("cache/agbrio-observation/agbrio-bridge/0.1.2/.claude-plugin");
 let manifest=serde_json::json!({"name":"agbrio-bridge","version":"0.1.2","userConfig":{"token":{"sensitive":true}}}).to_string();
 for folder in [&package,&cache] {std::fs::create_dir_all(folder).unwrap();std::fs::write(folder.join("plugin.json"),&manifest).unwrap();}
 let admin=FakePluginAdmin{path:cache.parent().unwrap().into(),configured:Mutex::default(),calls:Mutex::default(),fail_inline:false};
 (core,host,temp.path().join("cache"),admin)
}
#[test] fn native_connect_configures_both_identities_via_stdin_and_reuses_saved_config(){
 let temp=tempfile::tempdir().unwrap();let(core,host,cache,admin)=connection_fixture(&temp);
 let first=connect_admin(&core,&host,temp.path(),&admin,&cache).unwrap();assert_eq!(first.phase,"WAITING_SESSION");
 assert!(!first.writes_enabled);assert!(core.store.claude_catalog().unwrap().sessions.is_empty());
 assert_eq!(core.store.claude_connections().unwrap().len(),1);
 let saved=admin.configured.lock().unwrap();assert_eq!(saved["agbrio-bridge@inline"],saved["agbrio-bridge@agbrio-observation"]);drop(saved);
 let second=connect_admin(&core,&host,temp.path(),&admin,&cache).unwrap();assert_eq!(second.phase,"WAITING_SESSION");
 assert_eq!(core.store.claude_connections().unwrap().len(),1);assert!(core.session.lock().unwrap().adapter.is_none());host.shutdown(&core);
}
#[test] fn native_connect_reuses_real_registered_publisher_without_admin_or_credentials(){
 let temp=tempfile::tempdir().unwrap();let(core,host,cache,admin)=connection_fixture(&temp);
 let pair=core.store.create_claude_connection().unwrap();ingest(&core,&pair.token,&event(temp.path(),"10000000-0000-4000-8000-000000000001","SESSION_START",1)).unwrap();
 assert_eq!(connect_admin(&core,&host,temp.path(),&admin,&cache).unwrap().phase,"CONNECTED");
 assert!(admin.calls.lock().unwrap().is_empty());assert_eq!(core.store.claude_connections().unwrap().len(),1);host.shutdown(&core);
}
#[test] fn partial_configuration_failure_revokes_only_its_new_connection(){
 let temp=tempfile::tempdir().unwrap();let(core,host,cache,mut admin)=connection_fixture(&temp);admin.fail_inline=true;
 let prior=core.store.create_claude_connection().unwrap();
 assert_eq!(connect_admin(&core,&host,temp.path(),&admin,&cache).err().unwrap(),"CLAUDE_PLUGIN_ADMIN_REFUSED");
 let connections=core.store.claude_connections().unwrap();assert_eq!(connections.len(),2);
 assert!(!connections.iter().find(|c|c.connection_id==prior.connection_id).unwrap().revoked);
 assert!(connections.iter().find(|c|c.connection_id!=prior.connection_id).unwrap().revoked);host.shutdown(&core);
}
#[test] fn native_connect_never_overwrites_other_host_configuration(){
 let temp=tempfile::tempdir().unwrap();let(core,host,cache,admin)=connection_fixture(&temp);
 admin.configured.lock().unwrap().insert("agbrio-bridge@agbrio-observation".into(),("http://127.0.0.1:12345".into(),"not exported".into()));
 assert_eq!(connect_admin(&core,&host,temp.path(),&admin,&cache).err().unwrap(),"CLAUDE_PLUGIN_DIFFERENT_HOST");assert!(core.store.claude_connections().unwrap().is_empty());host.shutdown(&core);
}
