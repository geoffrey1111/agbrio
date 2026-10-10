use super::*;
use crate::host_application::{RouterCore,Session};
use router_core::store::{RouterStore,assistant::{GrantInput,BriefRule},role_bridge::RoleBindingInput};
use serde_json::{json,Value};
use std::time::Instant;
#[cfg(windows)]
#[path="assistant_events_http_tests.rs"]
mod events_contract;
fn now_ms()->i64{clock() as i64*1000}
#[test]fn instance_operations_manage_seen_inbox_restore_watches_and_send_original_chat_once(){
 let(_dir,core,old_gid,_)=fixture();let old=core.store.assistant_grant(&old_gid).unwrap();let g=core.store.create_assistant_instance_grant(router_core::store::assistant::InstanceGrantInput{label:"App manager QA".into(),rules:old.rules,expires_at:now_ms()+600000}).unwrap();
 let mut index=0;let mut apply=|operation:&str,input:Value|{index+=1;let a=crate::assistant_mcp::call(&core,&g.id,"agbrio_prepare_action",json!({"requestId":format!("op-{index}"),"operation":operation,"input":input})).unwrap();let args=json!({"actionId":a["id"],"expectedHash":a["payloadHash"],"ruleId":"continue","useOwnerAnswer":false,"assessment":"Covered disposable operation"});let result=crate::assistant_mcp::call(&core,&g.id,"agbrio_execute_action",args.clone()).unwrap();assert_eq!(result["status"],"APPLIED");assert_eq!(crate::assistant_mcp::call(&core,&g.id,"agbrio_execute_action",args).unwrap(),result);result};
 apply("ENABLE_WATCH",json!({"threadId":"qa-watch-original"}));let watch=core.store.codex_watches().unwrap().remove(0);
 for turn in ["demo-one","demo-two"]{core.store.record_codex_watch(&watch.thread_id,watch.generation,&router_core::store::codex_watch::WatchSnapshot{state:"RESULT_READY".into(),turn_id:Some(turn.into()),item_id:Some("item".into()),text:turn.into()}).unwrap();}
 let first=core.store.codex_watch_feed(0).unwrap().events[0].sequence;apply("MARK_NOTIFICATION_READ",json!({"sequence":first}));let feed=core.store.codex_watch_feed(0).unwrap();assert_eq!(feed.events.len(),2);assert_eq!(feed.events.iter().filter(|e|e.seen_at.is_some()).count(),1);
 apply("REMOVE_WATCH_ITEM",json!({"kind":"WATCH","id":watch.thread_id,"generation":watch.generation,"removed":true}));assert!(core.store.codex_watches().unwrap().is_empty());let removed=core.store.removed_watch_items().unwrap();let generation=removed[0]["generation"].as_i64().unwrap();
 apply("REMOVE_WATCH_ITEM",json!({"kind":"WATCH","id":watch.thread_id,"generation":generation,"removed":false}));let active=core.store.codex_watches().unwrap().remove(0);assert!(active.enabled);
 let read=crate::assistant_mcp::call(&core,&g.id,"agbrio_read_chat",json!({"threadId":watch.thread_id})).unwrap();assert!(read["history"]["messages"].as_array().unwrap().is_empty());
 let result=apply("SEND_CHAT",json!({"threadId":watch.thread_id,"generation":active.generation,"sourceSequence":null,"expectedTurnId":null,"mode":"SEND","text":"Demo follow-up in the original conversation","options":{"model":null,"effort":null,"attachments":[]}}));assert_eq!(result["result"]["status"],"SENT");assert_eq!(core.store.watch_replies(&watch.thread_id).unwrap().len(),1);
}
#[test]fn instance_mcp_routes_multiple_and_future_bridges_and_applies_a_global_operation_once(){
 let(_dir,core,old_gid,first_obs)=fixture();let old=core.store.assistant_grant(&old_gid).unwrap();
 let g=core.store.create_assistant_instance_grant(router_core::store::assistant::InstanceGrantInput{label:"QA global".into(),rules:old.rules.clone(),expires_at:now_ms()+600000}).unwrap();
 assert!(crate::assistant_mcp::call(&core,&old_gid,"agbrio_read_app",json!({})).is_err());
 let list=|gid:&str|crate::assistant_mcp::rpc(&core,gid,json!({"jsonrpc":"2.0","id":1,"method":"tools/list"}));
 assert_eq!(list(&old_gid)["result"]["tools"].as_array().unwrap().len(),8);assert_eq!(list(&g.id)["result"]["tools"].as_array().unwrap().len(),16);
 let call=|name:&str,args:Value|crate::assistant_mcp::call(&core,&g.id,name,args).unwrap();
 let created=call("agbrio_prepare_action",json!({"requestId":"create-demo-b","operation":"CREATE_BRIDGE","input":{"name":"Demo B"}}));
 let execute=json!({"actionId":created["id"],"expectedHash":created["payloadHash"],"ruleId":"continue","useOwnerAnswer":false,"assessment":"Routine setup under owner brief"});
 let applied=call("agbrio_execute_action",execute.clone());assert_eq!(applied["status"],"APPLIED");let second=applied["result"]["workstreamId"].as_str().unwrap();
 assert_eq!(call("agbrio_execute_action",execute),applied);assert_eq!(core.store.snapshot().unwrap().workstreams.len(),2);
 let bind=call("agbrio_prepare_action",json!({"requestId":"bind-demo-b","operation":"BIND_BRIDGE","input":{"workstreamId":second,"bindingRevision":0,"decision":{"provider":"CODEX","externalId":"qa-b-source","label":"B source","cwd":null},"execution":{"provider":"CODEX","externalId":"qa-b-target","label":"B target","cwd":null}}}));
 assert_eq!(call("agbrio_execute_action",json!({"actionId":bind["id"],"expectedHash":bind["payloadHash"],"ruleId":"continue","useOwnerAnswer":false,"assessment":"Exact B identities selected by owner brief"}))["status"],"APPLIED");
 let b=core.store.role_bridge(second).unwrap();let source=b.decision.unwrap().endpoint;core.store.record_reply_observation(second,&source.id,Some("codex:second:item"),"Demo B exact result",None).unwrap();let second_obs=core.store.reply_observations_for_workstream(second).unwrap().remove(0);
 assert!(crate::assistant_mcp::call(&core,&g.id,"agbrio_prepare_handoff",json!({"workstreamId":second,"bindingRevision":1,"requestId":"wrong-source","observationId":first_obs,"role":"DECISION","text":"Wrong Bridge","attachmentIds":[]})).is_err());
 for (index,wid,obs) in [(0,old.workstream_id.as_str(),first_obs.as_str()),(1,second,second_obs.id.as_str())]{
  let ready=call("agbrio_prepare_handoff",json!({"workstreamId":wid,"bindingRevision":1,"requestId":format!("send-{index}"),"observationId":obs,"role":"DECISION","text":format!("Demo handoff {index}"),"attachmentIds":[]}));
  let result=call("agbrio_confirm_and_send",json!({"handoffId":ready["id"],"expectedHash":ready["payloadHash"],"ruleId":"continue","decisionId":null,"assessment":"Routine exact handoff under the owner brief"}));assert_eq!(result["status"],"SENT");
  assert!(crate::assistant_mcp::call(&core,&g.id,"agbrio_confirm_and_send",json!({"handoffId":ready["id"],"expectedHash":ready["payloadHash"],"ruleId":"continue","decisionId":null,"assessment":"Repeat"})).is_err());
 }
 assert_eq!(call("agbrio_read_app",json!({}))["bridges"].as_array().unwrap().len(),2);
 core.store.revoke_assistant_grant(&g.id).unwrap();assert!(crate::assistant_mcp::call(&core,&g.id,"agbrio_read_app",json!({})).is_err());
}
fn fixture()->(tempfile::TempDir,RouterCore,String,String){
 let dir=tempfile::tempdir().unwrap();let store=Arc::new(RouterStore::open_at(dir.path().join("router.db")).unwrap());
 let p=store.create_project("MCP QA".into(),None).unwrap();let w=store.create_workstream(&p.id,"Delegation QA".into()).unwrap();
 let side=|id:&str|RoleBindingInput{provider:"CODEX".into(),external_id:id.into(),label:id.into(),cwd:Some(dir.path().to_string_lossy().into())};
 let bindings=store.bind_role_bridge(&w.id,w.binding_revision,side("qa-source-thread"),side("qa-target-thread")).unwrap();
 let src=bindings.decision.unwrap().endpoint;store.record_reply_observation(&w.id,&src.id,Some("codex:qa-turn:qa-item"),"Owner-approved plan: continue. No change of scope.",None).unwrap();
 let observation=store.reply_observations_for_workstream(&w.id).unwrap()[0].id.clone();
 let g=store.create_assistant_grant(GrantInput{workstream_id:w.id,source_role:"DECISION".into(),binding_revision:bindings.binding_revision,label:"QA assistant".into(),rules:vec![BriefRule{id:"continue".into(),text:"Forward the approved plan; ask me about any change.".into()}],expires_at:now_ms()+86400000}).unwrap();
 let core=RouterCore{store,chatgpt:Arc::default(),session:Arc::new(Mutex::new(Session::default())),completed_chatgpt_responses:Arc::default()};
 let path=dir.path().join("fixture-native.mjs");std::fs::write(&path,r#"
import readline from 'node:readline';
import fs from 'node:fs';
const cwd=process.argv[2];
readline.createInterface({input:process.stdin}).on('line',line=>{
 const q=JSON.parse(line);if(q.id===undefined)return;
 if(q.method==='turn/start'||q.method==='turn/steer')fs.appendFileSync(cwd+'/native-writes.jsonl',JSON.stringify({method:q.method,threadId:q.params.threadId})+'\n');
 const thread={id:q.params?.threadId,cwd,status:{type:'idle'},turns:[]};
 const result=q.method==='initialize'?{userAgent:'Agbrio offline fixture'}:q.method==='thread/read'||q.method==='thread/resume'?{thread,model:'fixture',reasoningEffort:'low'}:q.method==='thread/turns/list'?{data:[],nextCursor:null}:q.method==='thread/goal/get'?{goal:null}:q.method==='turn/start'?{turn:{id:'fixture-turn-'+q.params.threadId,status:'inProgress'}}:{};
 process.stdout.write(JSON.stringify({jsonrpc:'2.0',id:q.id,result})+'\n');
});
"#).unwrap();
 let mut command=std::process::Command::new("node");command.arg(path).arg(dir.path());
 #[cfg(windows)]{use std::os::windows::process::CommandExt;command.creation_flags(0x08000000);}
 let mut adapter=crate::codex::adapter::CodexAdapter::start_shared_validation(command,Arc::new(|_|{})).unwrap();adapter.initialize().unwrap();core.session.lock().unwrap().adapter=Some(adapter);
 (dir,core,g.id,observation)
}
fn config(dir:&std::path::Path)->MobileHttpConfig{MobileHttpConfig{port:0,allowed_host:"assistant.fixture.invalid".into(),allowed_origin:"https://assistant.fixture.invalid".into(),access_issuer:String::new(),access_audience:String::new(),access_jwks_url:String::new(),static_dir:dir.into()}}
#[test]
fn discovery_diagnostics_distinguish_real_protocol_requests_and_preserve_oauth_scope(){
 let(d,core,legacy,_)=fixture();let host=crate::HostRuntime::default();let runtime=tokio::runtime::Runtime::new().unwrap();
 let g=core.store.connect_assistant_instance(router_core::store::assistant::AssistantConnectionInput{label:"Discovery QA".into(),expires_at:now_ms()+86400000}).unwrap();
 let resource="https://assistant.fixture.invalid/mcp";
 let token=core.store.issue_assistant_access(&g.id,resource,g.expires_at).unwrap();
 let legacy_grant=core.store.assistant_grant(&legacy).unwrap();let legacy_token=core.store.issue_assistant_access(&legacy,resource,legacy_grant.expires_at).unwrap();
 runtime.block_on(async{
  let handle=start_with_web_auth(core.clone(),config(d.path()),host.clone(),Arc::new(crate::web_auth::WebAuth::open(None).unwrap())).await.unwrap();
  let base=format!("http://{}/mcp",handle.address);let c=client();
  let request=|method:&str,params:Value|c.post(&base).header("host","assistant.fixture.invalid").header("accept","application/json, text/event-stream").json(&json!({"jsonrpc":"2.0","id":1,"method":method,"params":params}));
  assert_eq!(request("server/discover",json!({})).send().await.unwrap().status(),StatusCode::UNAUTHORIZED);
  let discovery:Value=request("server/discover",json!({})).bearer_auth(&token).send().await.unwrap().json().await.unwrap();
  assert!(discovery.pointer("/result/capabilities/events").is_some());assert_eq!(discovery["result"]["serverInfo"]["version"],crate::assistant_mcp::SERVER_VERSION);
  assert_eq!(request("events/list",json!({"private":"never-log-this-body"})).bearer_auth(&token).send().await.unwrap().status(),StatusCode::BAD_REQUEST);
  let events:Value=request("events/list",json!({})).header("mcp-protocol-version","2026-07-28").bearer_auth(&token).send().await.unwrap().json().await.unwrap();
  assert_eq!(events["result"]["events"].as_array().unwrap().len(),2);
  let old:Value=request("initialize",json!({"protocolVersion":"2025-11-25"})).bearer_auth(&token).send().await.unwrap().json().await.unwrap();assert!(old.pointer("/result/capabilities/events").is_none());
  let read=|access:&str|request("tools/call",json!({"name":"agbrio_read_app","arguments":{}})).bearer_auth(access);
  let app:Value=read(&token).send().await.unwrap().json().await.unwrap();let trace=&app["result"]["structuredContent"]["mcpDiscovery"];let rows=trace["recentRequests"].as_array().unwrap();
  assert_eq!(rows.len(),5);assert_eq!(rows[0]["httpStatus"],401);assert_eq!(rows[0]["authenticated"],false);
  assert_eq!(rows[1]["advertisedEvents"],true);assert_eq!(rows[2]["httpStatus"],400);assert_eq!(rows[2]["headerVersion"],"MISSING");
  assert_eq!(rows[3]["eventCount"],2);assert_eq!(rows[3]["headerVersion"],"2026-07-28");assert_eq!(rows[4]["advertisedEvents"],false);
  assert!(rows.iter().all(|r|r["finishedAt"].is_number()));assert!(!trace.to_string().contains(&token));assert!(!trace.to_string().contains("never-log-this-body"));
  let content:Value=serde_json::from_str(app["result"]["content"][0]["text"].as_str().unwrap()).unwrap();assert_eq!(content["mcpDiscovery"],*trace);
  let denied:Value=read(&legacy_token).send().await.unwrap().json().await.unwrap();assert_eq!(denied["result"]["isError"],true);assert!(denied.pointer("/result/structuredContent/mcpDiscovery").is_none());
  core.store.revoke_assistant_grant(&g.id).unwrap();assert_eq!(read(&token).send().await.unwrap().status(),StatusCode::UNAUTHORIZED);
  handle.shutdown.send(()).unwrap();handle.task.await.unwrap();
 });
}
fn client()->reqwest::Client{reqwest::Client::builder().no_proxy().redirect(reqwest::redirect::Policy::none()).build().unwrap()}
#[test]fn two_paired_clients_share_atomic_read_ack_and_watch_removal(){
 let(d,core,_,_)=fixture();let host=crate::HostRuntime::default();let runtime=tokio::runtime::Runtime::new().unwrap();
 let snap=|text:&str|router_core::store::codex_watch::WatchSnapshot{state:"RESULT_READY".into(),turn_id:Some(text.into()),item_id:Some("item".into()),text:text.into()};
 for thread in ["qa-a","qa-b"]{core.store.enable_codex_watch(thread,thread,d.path().to_str().unwrap(),&snap("baseline")).unwrap();}
 for (thread,text) in [("qa-a","old a"),("qa-b","other b"),("qa-a","selected a"),("qa-a","newer a")]{core.store.record_codex_watch(thread,1,&snap(text)).unwrap();}
 let selected=core.store.codex_watch_feed(0).unwrap().events.into_iter().find(|e|e.snapshot.text=="selected a").unwrap().sequence;
 runtime.block_on(async{
  let web=Arc::new(crate::web_auth::WebAuth::open(None).unwrap());let code=web.issue().unwrap();let a=web.exchange(&code.code,"QA phone",false).unwrap();let code=web.issue().unwrap();let b=web.exchange(&code.code,"QA desktop",false).unwrap();
  let handle=start_with_web_auth(core.clone(),config(d.path()),host.clone(),web).await.unwrap();let base=format!("http://{}/v1/mobile/codex-watches",handle.address);let c=client();
  let post=|token:&str,origin:&str|c.post(&base).header("host","assistant.fixture.invalid").header("origin",origin).header("cookie",format!("{}={token}",crate::web_auth::COOKIE));
  let oldest=core.store.codex_watch_feed(0).unwrap().events[0].sequence;
  assert_eq!(post(&a,"https://assistant.fixture.invalid").json(&json!({"action":"MARK_SEEN","sequence":oldest})).send().await.unwrap().status(),StatusCode::OK);
  let seen:Value=c.get(format!("{base}/events?after=0")).header("host","assistant.fixture.invalid").header("cookie",format!("{}={b}",crate::web_auth::COOKIE)).send().await.unwrap().json().await.unwrap();
  assert_eq!(seen["events"].as_array().unwrap().len(),4);assert!(seen["events"].as_array().unwrap().iter().find(|e|e["sequence"]==oldest).unwrap()["seenAt"].is_number());
  assert_eq!(seen["events"].as_array().unwrap().iter().filter(|e|e["seenAt"].is_number()).count(),1);
  assert_eq!(post(&a,"https://wrong.invalid").json(&json!({"action":"MARK_READ","sequence":selected})).send().await.unwrap().status(),StatusCode::FORBIDDEN);
  assert_eq!(post(&a,"https://assistant.fixture.invalid").json(&json!({"action":"MARK_READ","sequence":selected})).send().await.unwrap().status(),StatusCode::OK);
  let get=|suffix:&str|c.get(format!("{base}{suffix}")).header("host","assistant.fixture.invalid").header("cookie",format!("{}={b}",crate::web_auth::COOKIE));
  let feed:Value=get("/events?after=0").send().await.unwrap().json().await.unwrap();let events=feed["events"].as_array().unwrap();assert_eq!(events.len(),2);assert!(events.iter().any(|e|e["snapshot"]["text"]=="other b"));assert!(events.iter().any(|e|e["snapshot"]["text"]=="newer a"));
  assert_eq!(get(&format!("/events/{selected}")).send().await.unwrap().json::<Value>().await.unwrap()["snapshot"]["text"],"selected a");
  assert_eq!(post(&a,"https://assistant.fixture.invalid").json(&json!({"action":"REMOVE","kind":"WATCH","id":"qa-a","removed":true})).send().await.unwrap().status(),StatusCode::OK);
  let watches:Value=get("").send().await.unwrap().json().await.unwrap();assert_eq!(watches.as_array().unwrap().len(),1);assert_eq!(watches[0]["threadId"],"qa-b");
  let _=handle.shutdown.send(());handle.task.await.unwrap();
 });
}
async fn rpc(client:&reqwest::Client,base:&str,token:&str,name:&str,args:Value)->Value{
 let r=client.post(format!("{base}/mcp")).header("host","assistant.fixture.invalid").header("accept","application/json, text/event-stream").bearer_auth(token)
 .json(&json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":name,"arguments":args}})).send().await.unwrap();assert_eq!(r.status(),StatusCode::OK);r.json().await.unwrap()
}
#[test]
fn oauth_pkce_owner_consent_resource_binding_and_revocation_over_real_http(){
 let(d,core,gid,_)=fixture();let host=crate::HostRuntime::default();let runtime=tokio::runtime::Runtime::new().unwrap();runtime.block_on(async { let web=Arc::new(crate::web_auth::WebAuth::open(None).unwrap());
 let code=web.issue().unwrap();let owner=web.exchange(&code.code,"QA owner",false).unwrap();let cookie=format!("{}={owner}",crate::web_auth::COOKIE);
 let handle=start_with_web_auth(core.clone(),config(d.path()),host.clone(),web).await.unwrap();let base=format!("http://{}",handle.address);let c=client();
 let request=|path:&str|c.post(format!("{base}{path}")).header("host","assistant.fixture.invalid");
 let init=json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":"QA","version":"1"}}});
 let unauth=request("/mcp").json(&init).send().await.unwrap();assert_eq!(unauth.status(),StatusCode::UNAUTHORIZED);assert!(unauth.headers()["www-authenticate"].to_str().unwrap().contains("oauth-protected-resource/mcp"));
 assert_eq!(request("/mcp").header("cookie",&cookie).json(&init).send().await.unwrap().status(),StatusCode::UNAUTHORIZED);
 let client_meta:Value=request("/oauth/register").json(&json!({"client_name":"QA Dot","redirect_uris":["https://client.fixture.invalid/callback"],"token_endpoint_auth_method":"none","grant_types":["authorization_code","refresh_token"],"response_types":["code"]})).send().await.unwrap().json().await.unwrap();let cid=client_meta["client_id"].as_str().unwrap();
 let verifier="a".repeat(64);let challenge=Base64UrlUnpadded::encode_string(&Sha256::digest(verifier.as_bytes()));let res="https://assistant.fixture.invalid/mcp";
 let mut auth_url=url::Url::parse(&format!("{base}/oauth/authorize")).unwrap();auth_url.query_pairs_mut().extend_pairs([("client_id",cid),("redirect_uri","https://client.fixture.invalid/callback"),("response_type","code"),("code_challenge",&challenge),("code_challenge_method","S256"),("state","exact-state"),("resource",res),("scope",SCOPE)]);
 let auth=c.get(auth_url.clone()).header("host","assistant.fixture.invalid").send().await.unwrap();assert_eq!(auth.status(),StatusCode::SEE_OTHER);
 let next=auth.headers()["location"].to_str().unwrap();let page=c.get(format!("{base}{next}")).header("host","assistant.fixture.invalid").send().await.unwrap();assert!(page.headers().contains_key("content-security-policy"));
 let intent=url::Url::parse(&format!("{base}{next}")).unwrap().query_pairs().find(|(k,_)|k=="request").unwrap().1.to_string();
 assert_eq!(request("/v1/mobile/assistant/consent").header("origin",res.trim_end_matches("/mcp")).json(&json!({"request":intent,"grantId":gid,"allow":true})).send().await.unwrap().status(),StatusCode::UNAUTHORIZED);
 let approve:Value=request("/v1/mobile/assistant/consent").header("cookie",&cookie).header("origin","https://assistant.fixture.invalid").json(&json!({"request":intent,"grantId":gid,"allow":true})).send().await.unwrap().json().await.unwrap();
 let redirect=url::Url::parse(approve["redirect"].as_str().unwrap()).unwrap();assert_eq!(redirect.query_pairs().find(|(k,_)|k=="state").unwrap().1,"exact-state");
 let code=redirect.query_pairs().find(|(k,_)|k=="code").unwrap().1.to_string();
 let exchange=|verifier:&str,resource:&str|request("/oauth/token").form(&[("grant_type","authorization_code"),("client_id",cid),("redirect_uri","https://client.fixture.invalid/callback"),("code",code.as_str()),("code_verifier",verifier),("resource",resource)]);
 assert_eq!(exchange(&"b".repeat(64),res).send().await.unwrap().status(),StatusCode::BAD_REQUEST);
 assert_eq!(exchange(&verifier,"https://different.invalid/mcp").send().await.unwrap().status(),StatusCode::BAD_REQUEST);
 let credentials:Value=exchange(&verifier,res).send().await.unwrap().json().await.unwrap();let token=credentials["access_token"].as_str().unwrap();
 assert_eq!(exchange(&verifier,res).send().await.unwrap().status(),StatusCode::BAD_REQUEST);
 let initialized:Value=request("/mcp").bearer_auth(token).header("accept","application/json, text/event-stream").json(&init).send().await.unwrap().json().await.unwrap();assert_eq!(initialized["result"]["serverInfo"]["name"],"Agbrio Agent Bridge");
 let listed:Value=request("/mcp").bearer_auth(token).header("accept","application/json, text/event-stream").json(&json!({"jsonrpc":"2.0","id":2,"method":"tools/list"})).send().await.unwrap().json().await.unwrap();assert_eq!(listed["result"]["tools"].as_array().unwrap().len(),8);
 assert_eq!(request("/v1/mobile/assistant/grants").bearer_auth(token).send().await.unwrap().status(),StatusCode::METHOD_NOT_ALLOWED);
 assert_eq!(c.get(format!("{base}/v1/mobile/assistant/grants")).header("host","assistant.fixture.invalid").bearer_auth(token).send().await.unwrap().status(),StatusCode::UNAUTHORIZED);
 assert_eq!(request("/mcp").bearer_auth(token).header("origin","https://evil.invalid").json(&init).send().await.unwrap().status(),StatusCode::FORBIDDEN);
 core.store.revoke_assistant_grant(&gid).unwrap();assert_eq!(request("/mcp").bearer_auth(token).json(&init).send().await.unwrap().status(),StatusCode::UNAUTHORIZED);
 handle.shutdown.send(()).unwrap();handle.task.await.unwrap();});
}
#[test]
fn decision_gap_answer_and_send_error_retain_exact_receipt_no_false_delivery(){
 let(d,core,gid,observation)=fixture();let host=crate::HostRuntime::default();let runtime=tokio::runtime::Runtime::new().unwrap();runtime.block_on(async {let token=core.store.issue_assistant_access(&gid,"https://assistant.fixture.invalid/mcp",now_ms()+60000).unwrap();
 let handle=start(core.clone(),config(d.path()),host.clone()).await.unwrap();let base=format!("http://{}",handle.address);let c=client();
 let source=rpc(&c,&base,&token,"agbrio_read_source",json!({"observationId":observation})).await;assert_eq!(source["result"]["isError"],false,"{source}");
 let args=json!({"requestId":"request-1","observationId":observation,"text":"exact instruction\n","attachmentIds":[]});
 let prepared=rpc(&c,&base,&token,"agbrio_prepare_handoff",args.clone()).await;assert_eq!(prepared["result"]["isError"],false);let h=&prepared["result"]["structuredContent"];let hid=h["id"].as_str().unwrap();let hash=h["payloadHash"].as_str().unwrap();
 let again=rpc(&c,&base,&token,"agbrio_prepare_handoff",args).await;assert_eq!(again["result"]["structuredContent"]["id"],hid);
 let asked=rpc(&c,&base,&token,"agbrio_request_decision",json!({"handoffId":hid,"expectedHash":hash,"question":"Choose A or B"})).await;let did=asked["result"]["structuredContent"]["id"].as_str().unwrap();
 let blocked=rpc(&c,&base,&token,"agbrio_confirm_and_send",json!({"handoffId":hid,"expectedHash":hash,"ruleId":"continue","decisionId":null,"assessment":"try rule"})).await;assert_eq!(blocked["result"]["isError"],true);assert_eq!(core.store.role_handoff(hid).unwrap().status,"READY");
 let answered=rpc(&c,&base,&token,"agbrio_record_answer",json!({"decisionId":did,"expectedHash":hash,"answer":"Choose A","answerReference":"dot-chat/user-reply-12"})).await;assert_eq!(answered["result"]["isError"],false);
 core.session.lock().unwrap().adapter.take().unwrap().shutdown();
 let send=rpc(&c,&base,&token,"agbrio_confirm_and_send",json!({"handoffId":hid,"expectedHash":hash,"ruleId":null,"decisionId":did,"assessment":"Follow the actual owner's choice A"})).await;assert_eq!(send["result"]["isError"],true);
 let receipt=rpc(&c,&base,&token,"agbrio_receipt",json!({"handoffId":hid,"expectedHash":hash})).await;assert_eq!(receipt["result"]["structuredContent"]["handoff"]["status"],"APPROVED");
 assert_eq!(core.store.assistant_grant(&gid).unwrap().rules[0].id,"continue");
 handle.shutdown.send(()).unwrap();handle.task.await.unwrap();});
}
#[test]fn redirect_registration_rejects_active_schemes_and_ambiguous_uris(){
 for uri in ["javascript:alert(1)","file:///x","https://user:pass@example.test/callback","https://example.test/#fragment","http://remote.test/callback"]{assert!(!redirect_allowed(uri));}
 assert!(redirect_allowed("https://example.test/callback"));assert!(redirect_allowed("http://127.0.0.1:3456/callback"));
}
#[test]fn codex_registration_refresh_request_is_negotiated_without_claiming_refresh_support(){
 let(d,core,_,_)=fixture();let host=crate::HostRuntime::default();let runtime=tokio::runtime::Runtime::new().unwrap();runtime.block_on(async{
  let web=Arc::new(crate::web_auth::WebAuth::open(None).unwrap());let handle=start_with_web_auth(core.clone(),config(d.path()),host.clone(),web).await.unwrap();let base=format!("http://{}",handle.address);let c=client();
  for grants in [json!(["authorization_code","refresh_token"]),json!(["refresh_token","authorization_code"]),json!(["authorization_code"])]{
   let r=c.post(format!("{base}/oauth/register")).header("host","assistant.fixture.invalid").json(&json!({"client_name":"Codex fixture","redirect_uris":["http://127.0.0.1:4567/callback"],"grant_types":grants,"response_types":["code"],"token_endpoint_auth_method":"none","scope":"agbrio:instance","application_type":"native"})).send().await.unwrap();assert_eq!(r.status(),StatusCode::CREATED);let v:Value=r.json().await.unwrap();assert!(v["client_id"].is_string());assert_eq!(v["grant_types"],json!(["authorization_code"]));assert_eq!(v["token_endpoint_auth_method"],"none");assert!(v.get("client_secret").is_none());
  }
  let metadata:Value=c.get(format!("{base}/.well-known/oauth-authorization-server")).header("host","assistant.fixture.invalid").send().await.unwrap().json().await.unwrap();assert_eq!(metadata["grant_types_supported"],json!(["authorization_code"]));
  for grants in [json!([]),json!(["refresh_token"]),json!(["authorization_code","client_credentials"]),json!(["authorization_code","implicit"]),json!(["authorization_code","authorization_code"])]{
   let r=c.post(format!("{base}/oauth/register")).header("host","assistant.fixture.invalid").json(&json!({"client_name":"Invalid grants fixture","redirect_uris":["https://client.fixture.invalid/callback"],"grant_types":grants,"token_endpoint_auth_method":"none"})).send().await.unwrap();assert_eq!(r.status(),StatusCode::BAD_REQUEST);
  }
  let r=c.post(format!("{base}/oauth/token")).header("host","assistant.fixture.invalid").form(&[("grant_type","refresh_token"),("client_id","fixture-client"),("redirect_uri","https://client.fixture.invalid/callback"),("code","not-a-code"),("code_verifier","not-a-verifier"),("resource","https://assistant.fixture.invalid/mcp")]).send().await.unwrap();assert_eq!(r.status(),StatusCode::BAD_REQUEST);
  handle.shutdown.send(()).unwrap();handle.task.await.unwrap();
 });
}
#[test]fn registered_client_survives_restart_and_corrupt_optional_registry_fails_closed(){
 let d=tempfile::tempdir().unwrap();let path=d.path().join("clients.json");let mut auth=AssistantOAuth::open(path.clone());auth.clients.insert("exact-client".into(),Client{name:"QA".into(),redirects:vec!["https://client.invalid/callback".into()],expires:clock()+3600});auth.save().unwrap();
 assert!(AssistantOAuth::open(path.clone()).clients.contains_key("exact-client"));std::fs::write(&path,b"broken").unwrap();assert!(AssistantOAuth::open(path).fault);
}

#[test]fn codex_combined_scopes_preserve_the_owner_selected_grant_over_real_http(){
 let(d,core,bridge_gid,_)=fixture();
 let instance_gid=core.store.connect_assistant_instance(router_core::store::assistant::AssistantConnectionInput{label:"Whole app scope QA".into(),expires_at:now_ms()+600000}).unwrap().id;
 let host=crate::HostRuntime::default();let runtime=tokio::runtime::Runtime::new().unwrap();
 runtime.block_on(async{
  let web=Arc::new(crate::web_auth::WebAuth::open(None).unwrap());let pairing=web.issue().unwrap();let owner=web.exchange(&pairing.code,"Scope QA owner",false).unwrap();let cookie=format!("{}={owner}",crate::web_auth::COOKIE);
  let handle=start_with_web_auth(core.clone(),config(d.path()),host.clone(),web).await.unwrap();let base=format!("http://{}",handle.address);let c=client();
  let request=|path:&str|c.post(format!("{base}{path}")).header("host","assistant.fixture.invalid");
  let registration=request("/oauth/register").json(&json!({"client_name":"Codex scope QA","redirect_uris":["http://127.0.0.1:4567/callback/fixture"],"token_endpoint_auth_method":"none","grant_types":["authorization_code","refresh_token"],"response_types":["code"]})).send().await.unwrap();assert_eq!(registration.status(),StatusCode::CREATED);
  let registered:Value=registration.json().await.unwrap();let cid=registered["client_id"].as_str().unwrap();let verifier="a".repeat(64);let challenge=Base64UrlUnpadded::encode_string(&Sha256::digest(verifier.as_bytes()));let res="https://assistant.fixture.invalid/mcp";
  let auth_url=|scope:&str|{let mut u=url::Url::parse(&format!("{base}/oauth/authorize")).unwrap();u.query_pairs_mut().extend_pairs([("client_id",cid),("redirect_uri","http://127.0.0.1:4567/callback/fixture"),("response_type","code"),("code_challenge",&challenge),("code_challenge_method","S256"),("state","scope-fixture-state"),("resource",res),("scope",scope)]);u};
  for scope in ["agbrio:handoff agbrio:instance","agbrio:instance agbrio:handoff"]{
   for (gid,expected_scope,expected_tools) in [(&bridge_gid,SCOPE,8),(&instance_gid,INSTANCE_SCOPE,16)]{
    let authorization=c.get(auth_url(scope)).header("host","assistant.fixture.invalid").send().await.unwrap();assert_eq!(authorization.status(),StatusCode::SEE_OTHER,"Combined advertised scopes must reach owner consent");
    let next=authorization.headers()["location"].to_str().unwrap();let intent=url::Url::parse(&format!("{base}{next}")).unwrap().query_pairs().find(|(k,_)|k=="request").unwrap().1.to_string();
    let info=c.get(format!("{base}/v1/mobile/assistant/consent")).header("host","assistant.fixture.invalid").header("cookie",&cookie).query(&[("request",&intent)]).send().await.unwrap();assert_eq!(info.status(),StatusCode::OK);let info:Value=info.json().await.unwrap();assert_eq!(info["grants"].as_array().unwrap().len(),2);
    assert_eq!(request("/v1/mobile/assistant/consent").header("origin","https://assistant.fixture.invalid").json(&json!({"request":intent,"grantId":gid,"allow":true})).send().await.unwrap().status(),StatusCode::UNAUTHORIZED);
    let approved=request("/v1/mobile/assistant/consent").header("cookie",&cookie).header("origin","https://assistant.fixture.invalid").json(&json!({"request":intent,"grantId":gid,"allow":true})).send().await.unwrap();assert_eq!(approved.status(),StatusCode::OK);let approved:Value=approved.json().await.unwrap();let callback=url::Url::parse(approved["redirect"].as_str().unwrap()).unwrap();let code=callback.query_pairs().find(|(k,_)|k=="code").unwrap().1.to_string();assert_eq!(callback.query_pairs().find(|(k,_)|k=="state").unwrap().1,"scope-fixture-state");
    let exchange=|resource:&str,proof:&str|request("/oauth/token").form(&[("grant_type","authorization_code"),("client_id",cid),("redirect_uri","http://127.0.0.1:4567/callback/fixture"),("code",code.as_str()),("code_verifier",proof),("resource",resource)]);
    assert_eq!(exchange("https://other-tenant.fixture.invalid/mcp",&verifier).send().await.unwrap().status(),StatusCode::BAD_REQUEST);assert_eq!(exchange(res,&"b".repeat(64)).send().await.unwrap().status(),StatusCode::BAD_REQUEST);
    let issued=exchange(res,&verifier).send().await.unwrap();assert_eq!(issued.status(),StatusCode::OK);let issued:Value=issued.json().await.unwrap();assert_eq!(issued["scope"],expected_scope);assert!(issued.get("refresh_token").is_none());let token=issued["access_token"].as_str().unwrap();assert_eq!(exchange(res,&verifier).send().await.unwrap().status(),StatusCode::BAD_REQUEST);
    let listed=request("/mcp").bearer_auth(token).header("accept","application/json, text/event-stream").json(&json!({"jsonrpc":"2.0","id":2,"method":"tools/list"})).send().await.unwrap();assert_eq!(listed.status(),StatusCode::OK);let listed:Value=listed.json().await.unwrap();assert_eq!(listed["result"]["tools"].as_array().unwrap().len(),expected_tools);
    if expected_scope==SCOPE{assert_eq!(rpc(&c,&base,token,"agbrio_read_app",json!({})).await["result"]["isError"],true);}
    assert_eq!(c.get(format!("{base}/v1/mobile/assistant/grants")).header("host","assistant.fixture.invalid").bearer_auth(token).send().await.unwrap().status(),StatusCode::UNAUTHORIZED);
   }
  }
  for scope in ["agbrio:instance unknown:scope","agbrio:handoff openid","","agbrio:instance\tagbrio:handoff"]{assert_eq!(c.get(auth_url(scope)).header("host","assistant.fixture.invalid").send().await.unwrap().status(),StatusCode::BAD_REQUEST);}
  handle.shutdown.send(()).unwrap();handle.task.await.unwrap();
 });
}

#[test]
#[ignore="bounded local-only disposable MCP/browser fixture; no provider account"]
fn assistant_fixture_for_sdk_and_browser(){
 let(d,core,gid,observation)=fixture();let host=crate::HostRuntime::default();let rt=tokio::runtime::Runtime::new().unwrap();
 let socket=std::net::TcpListener::bind("127.0.0.1:0").unwrap();let port=socket.local_addr().unwrap().port();drop(socket);
 let base=format!("http://127.0.0.1:{port}");let mut cfg=config(d.path());cfg.port=port;cfg.allowed_host=format!("127.0.0.1:{port}");cfg.allowed_origin=base.clone();
 let web=Arc::new(crate::web_auth::WebAuth::open(None).unwrap());let pairing=web.issue().unwrap();
 let token=core.store.issue_assistant_access(&gid,&format!("{base}/mcp"),now_ms()+600000).unwrap();
 let handle=rt.block_on(start_with_web_auth(core.clone(),cfg,host.clone(),web)).unwrap();
 let folder=std::path::Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap().join("runtime/assistant-mcp-validation");std::fs::create_dir_all(&folder).unwrap();
 let info=json!({"base":base,"token":token,"pairingCode":pairing.code,"grantId":gid,"observationId":observation,"fixtureOnly":true});
 std::fs::write(folder.join("fixture.json"),serde_json::to_vec(&info).unwrap()).unwrap();
 println!("ASSISTANT_LOCAL_FIXTURE_READY");
 let until=Instant::now()+Duration::from_secs(600);while Instant::now()<until&&!folder.join("stop-fixture").exists(){std::thread::sleep(Duration::from_millis(250));}
 rt.block_on(async{let _=handle.shutdown.send(());handle.task.await.unwrap();});
 core.store.revoke_assistant_grant(&gid).unwrap();std::fs::remove_file(folder.join("fixture.json")).unwrap();
}

#[test]
fn instance_oauth_cannot_downgrade_or_elevate_an_old_bridge_token(){
 let(d,core,old_gid,observation)=fixture();let old=core.store.assistant_grant(&old_gid).unwrap();let gid=core.store.connect_assistant_instance(router_core::store::assistant::AssistantConnectionInput{label:"Whole app QA".into(),expires_at:now_ms()+600000}).unwrap().id;let host=crate::HostRuntime::default();let runtime=tokio::runtime::Runtime::new().unwrap();runtime.block_on(async { let web=Arc::new(crate::web_auth::WebAuth::open(None).unwrap());
 let code=web.issue().unwrap();let owner=web.exchange(&code.code,"QA owner",false).unwrap();let cookie=format!("{}={owner}",crate::web_auth::COOKIE);
 let handle=start_with_web_auth(core.clone(),config(d.path()),host.clone(),web).await.unwrap();let base=format!("http://{}",handle.address);let c=client();
 let request=|path:&str|c.post(format!("{base}{path}")).header("host","assistant.fixture.invalid");
 let init=json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":"QA","version":"1"}}});
 let unauth=request("/mcp").json(&init).send().await.unwrap();assert_eq!(unauth.status(),StatusCode::UNAUTHORIZED);assert!(unauth.headers()["www-authenticate"].to_str().unwrap().contains("oauth-protected-resource/mcp"));
 assert_eq!(request("/mcp").header("cookie",&cookie).json(&init).send().await.unwrap().status(),StatusCode::UNAUTHORIZED);
 let client_meta:Value=request("/oauth/register").json(&json!({"client_name":"QA Dot","redirect_uris":["https://client.fixture.invalid/callback"],"token_endpoint_auth_method":"none","grant_types":["authorization_code","refresh_token"],"response_types":["code"]})).send().await.unwrap().json().await.unwrap();let cid=client_meta["client_id"].as_str().unwrap();
 let verifier="a".repeat(64);let challenge=Base64UrlUnpadded::encode_string(&Sha256::digest(verifier.as_bytes()));let res="https://assistant.fixture.invalid/mcp";
 let mut auth_url=url::Url::parse(&format!("{base}/oauth/authorize")).unwrap();auth_url.query_pairs_mut().extend_pairs([("client_id",cid),("redirect_uri","https://client.fixture.invalid/callback"),("response_type","code"),("code_challenge",&challenge),("code_challenge_method","S256"),("state","exact-state"),("resource",res),("scope",INSTANCE_SCOPE)]);
 let auth=c.get(auth_url.clone()).header("host","assistant.fixture.invalid").send().await.unwrap();assert_eq!(auth.status(),StatusCode::SEE_OTHER);
 let next=auth.headers()["location"].to_str().unwrap();let page=c.get(format!("{base}{next}")).header("host","assistant.fixture.invalid").send().await.unwrap();assert!(page.headers().contains_key("content-security-policy"));
 let intent=url::Url::parse(&format!("{base}{next}")).unwrap().query_pairs().find(|(k,_)|k=="request").unwrap().1.to_string();
 assert_eq!(request("/v1/mobile/assistant/consent").header("origin",res.trim_end_matches("/mcp")).json(&json!({"request":intent,"grantId":gid,"allow":true})).send().await.unwrap().status(),StatusCode::UNAUTHORIZED);
 assert_eq!(request("/v1/mobile/assistant/consent").header("cookie",&cookie).header("origin","https://assistant.fixture.invalid").json(&json!({"request":intent,"grantId":old_gid,"allow":true})).send().await.unwrap().status(),StatusCode::BAD_REQUEST);
 let approve:Value=request("/v1/mobile/assistant/consent").header("cookie",&cookie).header("origin","https://assistant.fixture.invalid").json(&json!({"request":intent,"grantId":gid,"allow":true})).send().await.unwrap().json().await.unwrap();
 let redirect=url::Url::parse(approve["redirect"].as_str().unwrap()).unwrap();assert_eq!(redirect.query_pairs().find(|(k,_)|k=="state").unwrap().1,"exact-state");
 let code=redirect.query_pairs().find(|(k,_)|k=="code").unwrap().1.to_string();
 let exchange=|verifier:&str,resource:&str|request("/oauth/token").form(&[("grant_type","authorization_code"),("client_id",cid),("redirect_uri","https://client.fixture.invalid/callback"),("code",code.as_str()),("code_verifier",verifier),("resource",resource)]);
 assert_eq!(exchange(&"b".repeat(64),res).send().await.unwrap().status(),StatusCode::BAD_REQUEST);
 assert_eq!(exchange(&verifier,"https://different.invalid/mcp").send().await.unwrap().status(),StatusCode::BAD_REQUEST);
 let credentials:Value=exchange(&verifier,res).send().await.unwrap().json().await.unwrap();assert_eq!(credentials["scope"],INSTANCE_SCOPE);let token=credentials["access_token"].as_str().unwrap();
 assert_eq!(exchange(&verifier,res).send().await.unwrap().status(),StatusCode::BAD_REQUEST);
 let initialized:Value=request("/mcp").bearer_auth(token).header("accept","application/json, text/event-stream").json(&init).send().await.unwrap().json().await.unwrap();assert_eq!(initialized["result"]["serverInfo"]["name"],"Agbrio Agent Bridge");
 let listed:Value=request("/mcp").bearer_auth(token).header("accept","application/json, text/event-stream").json(&json!({"jsonrpc":"2.0","id":2,"method":"tools/list"})).send().await.unwrap().json().await.unwrap();assert_eq!(listed["result"]["tools"].as_array().unwrap().len(),16);
 assert_eq!(request("/v1/mobile/assistant/grants").bearer_auth(token).send().await.unwrap().status(),StatusCode::METHOD_NOT_ALLOWED);
 assert_eq!(c.get(format!("{base}/v1/mobile/assistant/grants")).header("host","assistant.fixture.invalid").bearer_auth(token).send().await.unwrap().status(),StatusCode::UNAUTHORIZED);
 assert_eq!(request("/mcp").bearer_auth(token).header("origin","https://evil.invalid").json(&init).send().await.unwrap().status(),StatusCode::FORBIDDEN);
 // Exercise review-mode handoff and a whole-app action through actual HTTP,
 // OAuth/PKCE and the isolated provider adapter, never the owner's conversations.
 let app=rpc(&c,&base,token,"agbrio_read_app",json!({})).await;assert_eq!(app["result"]["structuredContent"]["grant"]["approvalMode"],"CONVERSATION_REVIEW");
 let draft=rpc(&c,&base,token,"agbrio_prepare_handoff",json!({"requestId":"conversation-send","workstreamId":old.workstream_id,"bindingRevision":1,"observationId":observation,"role":"DECISION","text":"Continue the reviewed reward display","attachmentIds":[]})).await;
 assert_eq!(draft["result"]["isError"],false,"{draft}");let h=&draft["result"]["structuredContent"];
 let send=json!({"handoffId":h["id"],"expectedHash":h["payloadHash"],"ruleId":null,"decisionId":null,"assessment":"Owner conversation permits forwarding this reviewed continuation"});
 let sent=rpc(&c,&base,token,"agbrio_confirm_and_send",send.clone()).await;assert_eq!(sent["result"]["structuredContent"]["status"],"SENT","{sent}");assert_eq!(rpc(&c,&base,token,"agbrio_confirm_and_send",send).await["result"]["isError"],true);
 let receipt=rpc(&c,&base,token,"agbrio_receipt",json!({"handoffId":h["id"],"expectedHash":h["payloadHash"]})).await;assert_eq!(receipt["result"]["structuredContent"]["approval"]["basis"],"ASSISTANT_REVIEW");
 let action=rpc(&c,&base,token,"agbrio_prepare_action",json!({"requestId":"conversation-create","operation":"CREATE_BRIDGE","input":{"name":"Demo conversation review"}})).await;let a=&action["result"]["structuredContent"];
 let execute=json!({"actionId":a["id"],"expectedHash":a["payloadHash"],"ruleId":null,"useOwnerAnswer":false,"assessment":"The owner requested this Bridge in the Dot conversation"});
 let applied=rpc(&c,&base,token,"agbrio_execute_action",execute.clone()).await;assert_eq!(applied["result"]["structuredContent"]["status"],"APPLIED");assert_eq!(rpc(&c,&base,token,"agbrio_execute_action",execute).await,applied);assert_eq!(core.store.snapshot().unwrap().workstreams.len(),2);
 core.store.revoke_assistant_grant(&gid).unwrap();assert_eq!(request("/mcp").bearer_auth(token).json(&init).send().await.unwrap().status(),StatusCode::UNAUTHORIZED);
 handle.shutdown.send(()).unwrap();handle.task.await.unwrap();});
}

#[test]
#[ignore="bounded whole-app MCP/SDK/browser fixture; fake provider only"]
fn instance_fixture_for_sdk_and_browser(){
 let(d,core,old_gid,observation)=fixture();let old=core.store.assistant_grant(&old_gid).unwrap();
 let gid=core.store.create_assistant_instance_grant(router_core::store::assistant::InstanceGrantInput{label:"Demo whole app".into(),rules:old.rules,expires_at:now_ms()+600000}).unwrap().id;
 let baseline=router_core::store::codex_watch::WatchSnapshot{state:"RESULT_READY".into(),turn_id:Some("baseline".into()),item_id:Some("item".into()),text:"Demo baseline".into()};
 core.store.enable_codex_watch("demo-notification-thread","Demo notifications",d.path().to_str().unwrap(),&baseline).unwrap();
 for i in 1..=6{core.store.record_codex_watch("demo-notification-thread",1,&router_core::store::codex_watch::WatchSnapshot{state:"RESULT_READY".into(),turn_id:Some(format!("demo-turn-{i}")),item_id:Some("item".into()),text:format!("Demo notification {i}")}).unwrap();}
 let rt=tokio::runtime::Runtime::new().unwrap();let host=crate::HostRuntime::default();let root=std::path::Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();let folder=root.join("runtime/instance-mcp");std::fs::create_dir_all(&folder).unwrap();
 let socket=std::net::TcpListener::bind("127.0.0.1:0").unwrap();let port=socket.local_addr().unwrap().port();drop(socket);let base=format!("http://127.0.0.1:{port}");
 let mut cfg=config(&root.join("dist"));cfg.port=port;cfg.allowed_host=format!("127.0.0.1:{port}");cfg.allowed_origin=base.clone();
 let web=Arc::new(crate::web_auth::WebAuth::open(None).unwrap());let pair=web.issue().unwrap();let owner=web.exchange(&pair.code,"Disposable browser QA",false).unwrap();
 let token=core.store.issue_assistant_access(&gid,&format!("{base}/mcp"),now_ms()+600000).unwrap();
 let handle=rt.block_on(start_with_web_auth(core.clone(),cfg,host.clone(),web)).unwrap();
 let info=json!({"base":base,"token":token,"ownerCookie":owner,"grantId":gid,"legacyGrantId":old_gid,"workstreamId":old.workstream_id,"observationId":observation,"fixtureOnly":true});std::fs::write(folder.join("fixture.json"),info.to_string()).unwrap();println!("INSTANCE_DISPOSABLE_FIXTURE_READY");
 let until=Instant::now()+Duration::from_secs(600);while Instant::now()<until&&!folder.join("stop-fixture").exists(){std::thread::sleep(Duration::from_millis(250));}
 rt.block_on(async{let _=handle.shutdown.send(());handle.task.await.unwrap();});core.store.revoke_assistant_grant(&gid).unwrap();std::fs::remove_file(folder.join("fixture.json")).unwrap();drop(d);
}

#[test]fn bridge_original_conversation_reply_and_status_do_not_register_an_independent_watch(){
 let(_d,core,gid,_)=fixture();let g=core.store.assistant_grant(&gid).unwrap();let rows=crate::role_bridge::directory_activity(&core,&[g.workstream_id.clone()]).unwrap();assert_eq!(rows.len(),1);assert_eq!(rows[0].unread_count,1);assert_eq!(rows[0].sides.len(),2);assert!(core.store.codex_watches().unwrap().is_empty());
 let view=crate::watch_chat::state(&core,"qa-source-thread").unwrap();assert!(view.watch.generation<0);let args=json!({"action":"SEND","threadId":"qa-source-thread","id":uuid::Uuid::new_v4().to_string(),"generation":view.watch.generation,"sourceSequence":null,"expectedTurnId":null,"mode":"SEND","text":"Demo additional review note","options":{"attachments":[]}});
 let sent=crate::watch_chat::command(&core,serde_json::from_value(args.clone()).unwrap()).unwrap();assert_eq!(sent["status"],"SENT");assert_eq!(crate::watch_chat::command(&core,serde_json::from_value(args).unwrap()).unwrap(),sent);assert!(core.store.codex_watches().unwrap().is_empty());
 assert!(crate::role_bridge::directory_activity(&core,&vec![g.workstream_id;21]).is_err());
}
