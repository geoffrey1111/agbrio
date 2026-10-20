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
 assert_eq!(crate::role_bridge::send(&core,&h.id).err().unwrap(),"CLAUDE_WRITES_OWNER_ONLY");
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

#[test] fn every_unarchived_desktop_conversation_is_selectable_before_it_reports() {
 let temp=tempfile::tempdir().unwrap();
 let store=Arc::new(crate::RouterStore::open_at(temp.path().join("router.db")).unwrap());
 let core=RouterCore{chatgpt:Arc::new(Default::default()),session:Arc::new(Mutex::new(Session::default())),completed_chatgpt_responses:Arc::new(Mutex::new(HashMap::new())),store:store.clone()};
 let pair=store.create_claude_connection().unwrap();
 let live="10000000-0000-4000-8000-000000000001";let quiet="10000000-0000-4000-8000-000000000003";let archived="10000000-0000-4000-8000-000000000004";
 ingest(&core,&pair.token,&event(temp.path(),live,"SESSION_START",1)).unwrap();
 let cwd=serde_json::to_string(&temp.path().to_string_lossy()).unwrap();
 let app=temp.path().join("app");
 for (account,id,title,archived) in [("one",quiet,"没上报的对话",false),("two",archived,"已归档",true)] {
  let d=app.join(format!("claude-code-sessions/{account}/org"));std::fs::create_dir_all(&d).unwrap();
  std::fs::write(d.join(format!("local_{id}.json")),format!("{{\"sessionId\":\"local_{id}\",\"cliSessionId\":\"{id}\",\"cwd\":{cwd},\"lastActivityAt\":5,\"isArchived\":{archived},\"title\":\"{title}\"}}")).unwrap();
 }
 *crate::claude_code_desktop::TEST_ROOTS.lock().unwrap()=vec![app.clone()];
 let catalog=catalog(&core).unwrap();
 let ids:Vec<_>=catalog.sessions.iter().map(|s|(s.session_id.as_str(),s.state.as_str())).collect();
 assert!(ids.contains(&(live,"SESSION_START")));assert!(ids.contains(&(quiet,"DESKTOP")));assert!(!ids.iter().any(|(id,_)|*id==archived));
 // Bind and watch the quiet conversation by its exact native identity.
 let project=store.create_project("fictional".into(),None).unwrap();let w=store.create_workstream(&project.id,"fictional".into()).unwrap();
 let side=|id:&str|RoleBindingInput{provider:"CLAUDE_CODE".into(),external_id:id.into(),label:"x".into(),cwd:None};
 let bound=crate::role_bridge::bind(&core,&w.id,w.binding_revision,side(live),side(quiet)).unwrap();
 assert_eq!(bound.execution.unwrap().cwd.as_deref(),Some(temp.path().to_string_lossy().as_ref()));
 assert_eq!(crate::role_bridge::read(&core,&w.id,"EXECUTION").unwrap().replies.len(),0);
 assert!(crate::role_bridge::bind(&core,&w.id,w.binding_revision+1,side(live),side(archived)).is_err());
 let key=format!("claude_code:{quiet}");crate::claude_chat::enable(&core,&key).unwrap();
 let state=crate::claude_chat::state(&core,&key).unwrap();assert_eq!(state["inputReady"],false);assert!(state["watch"]["label"].is_string());
 *crate::claude_code_desktop::TEST_ROOTS.lock().unwrap()=vec![];
}
#[test] fn owner_relay_into_claude_rides_the_plugin_heartbeat_at_most_once() {
 let temp=tempfile::tempdir().unwrap();let db=temp.path().join("router.db");
 let store=Arc::new(crate::RouterStore::open_at(&db).unwrap());
 let core=RouterCore{chatgpt:Arc::new(Default::default()),session:Arc::new(Mutex::new(Session::default())),completed_chatgpt_responses:Arc::new(Mutex::new(HashMap::new())),store:store.clone()};
 let pair=store.create_claude_connection().unwrap();
 let a="10000000-0000-4000-8000-000000000001";let b="10000000-0000-4000-8000-000000000002";
 for session in [a,b] {ingest(&core,&pair.token,&event(temp.path(),session,"SESSION_START",1)).unwrap();}
 let project=store.create_project("fictional".into(),None).unwrap();let w=store.create_workstream(&project.id,"fictional".into()).unwrap();
 let side=|session:&str|RoleBindingInput{provider:"CLAUDE_CODE".into(),external_id:session.into(),label:"fictional".into(),cwd:None};
 crate::role_bridge::bind(&core,&w.id,w.binding_revision,side(a),side(b)).unwrap();
 let at=||std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis() as i64;
 let finish=|session:&str,seq:i64,text:&str|{let turn=uuid::Uuid::new_v4().to_string();let mut r=event(temp.path(),session,"RUNNING",seq);r.turn_id=Some(turn.clone());ingest(&core,&pair.token,&r).unwrap();let mut c=event(temp.path(),session,"COMPLETE",seq+1);c.turn_id=Some(turn);c.text=Some(text.into());c.completed_at=Some(at());ingest(&core,&pair.token,&c).unwrap();};
 finish(a,2,"指令一");
 let relay=|text:&str|{let state=crate::role_bridge::read(&core,&w.id,"DECISION").unwrap();let reply=state.replies.last().unwrap().id.clone();let h=crate::role_bridge::prepare(&core,&w.id,"DECISION",&reply,text,&[]).unwrap();crate::role_bridge::approve(&core,&h.id,&h.payload_hash).unwrap();h};
 let h=relay("指令一");
 // MCP stays closed; the owner path refuses while the plugin has not checked in.
 assert_eq!(crate::role_bridge::send(&core,&h.id).err().unwrap(),"CLAUDE_WRITES_OWNER_ONLY");
 assert!(crate::role_bridge::send_owner(&core,&h.id).err().unwrap().contains("CLAUDE_INPUT_PLUGIN_NOT_READY"));
 assert_eq!(store.role_handoff(&h.id).unwrap().status,"APPROVED");
 let beat=|seq:i64|event(temp.path(),b,"HEARTBEAT",seq);
 ingest(&core,&pair.token,&beat(2)).unwrap();store.offer_claude_manual(&pair.token,&beat(2)).unwrap();
 crate::role_bridge::send_owner(&core,&h.id).unwrap();assert_eq!(store.role_handoff(&h.id).unwrap().status,"SENDING");
 // Destination is not idle yet (SESSION_START): nothing is offered.
 assert!(store.offer_claude_manual(&pair.token,&beat(3)).unwrap().is_none());
 finish(b,3,"执行端空闲");
 let cmd=store.offer_claude_manual(&pair.token,&beat(5)).unwrap().unwrap();
 assert_eq!((cmd.request_id.as_str(),cmd.session_id.as_str(),cmd.text.as_str()),(h.id.as_str(),b,"指令一"));
 assert!(store.offer_claude_manual(&pair.token,&beat(6)).unwrap().is_none()); // never offered twice
 assert_eq!(uuid::Uuid::parse_str(&cmd.request_id).unwrap().get_version_num(),4); // plugin fence requires a native UUID
 let receipt=|id:&str,status:&str,sha:&str|router_core::store::claude_manual::ManualReceipt{request_id:id.into(),session_id:b.into(),instance_id:format!("20000000-0000-4000-8000-{}",&b[24..]),cwd:temp.path().to_string_lossy().into(),status:status.into(),message_id:(status=="SENT").then(||uuid::Uuid::new_v4().to_string()),text_sha256:sha.into()};
 assert!(store.accept_claude_manual_receipt(&pair.token,&receipt(&h.id,"SENT",&"0".repeat(64))).is_err());
 let sent=receipt(&h.id,"SENT",&cmd.text_sha256);store.accept_claude_manual_receipt(&pair.token,&sent).unwrap();store.accept_claude_manual_receipt(&pair.token,&sent).unwrap();
 assert_eq!(store.role_handoff(&h.id).unwrap().status,"SENT");
 assert!(store.accept_claude_manual_receipt(&pair.token,&receipt(&h.id,"NOT_SENT",&cmd.text_sha256)).is_err());
 // A plugin NOT_SENT is a proven pre-write failure.
 finish(a,4,"指令二");let h2=relay("指令二");crate::role_bridge::send_owner(&core,&h2.id).unwrap();
 let cmd2=store.offer_claude_manual(&pair.token,&beat(7)).unwrap().unwrap();
 store.accept_claude_manual_receipt(&pair.token,&receipt(&h2.id,"NOT_SENT",&cmd2.text_sha256)).unwrap();
 assert_eq!(store.role_handoff(&h2.id).unwrap().status,"FAILED");
 // An offer without any receipt becomes UNKNOWN after 30s and is never re-offered.
 finish(a,6,"指令三");let h3=relay("指令三");crate::role_bridge::send_owner(&core,&h3.id).unwrap();
 store.offer_claude_manual(&pair.token,&beat(8)).unwrap().unwrap();
 rusqlite::Connection::open(&db).unwrap().execute("UPDATE claude_bridge_commands SET offered_at=offered_at-31000 WHERE handoff_id=?1",[&h3.id]).unwrap();
 assert_eq!(store.expire_claude_manual_offers().unwrap(),1);let unknown=store.role_handoff(&h3.id).unwrap();assert_eq!((unknown.status.as_str(),unknown.error_code.as_deref()),("SENDING",Some("CLAUDE_RECEIPT_NOT_OBSERVED")));assert_eq!(store.expire_claude_manual_offers().unwrap(),0);
 assert!(store.offer_claude_manual(&pair.token,&beat(9)).unwrap().is_none());
 assert!(core.session.lock().unwrap().adapter.is_none());
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
  // Claude 2.1.299+: --plugin-dir hides the installed identity; only the inline one takes it.
  let args=if args[0]=="--plugin-dir"{assert_eq!(args[4],"agbrio-bridge@inline");&args[2..]}else{assert_eq!(args[2],"agbrio-bridge@agbrio-observation");args};
  assert_eq!(args[0],"plugin");assert_eq!(args[1],"configure");
  let identity=&args[2];
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
