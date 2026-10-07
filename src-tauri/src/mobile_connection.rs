//! Desktop-only setup. Validate a TLS entrance against this exact resident Host
//! before persisting it; configuration probes never authorize data or writes.
use crate::{mobile_http::{self,MobileHttpConfig},HostRuntime,RouterCore};
use serde::{Deserialize,Serialize};
use serde_json::{json,Value};
use std::{path::Path,time::{Duration,Instant},sync::Mutex};
static CONFIGURE:Mutex<()>=Mutex::new(());
pub(crate) struct ProbeGrant{pub nonce:String,pub host:String,pub expires:Instant}
#[derive(Deserialize)]#[serde(deny_unknown_fields)]pub(crate) struct ProbeQuery{pub nonce:String}
#[derive(Clone,Deserialize)]#[serde(deny_unknown_fields)]pub(crate) struct ConnectionInput{pub method:String,pub origin:String}
#[derive(Serialize)]#[serde(rename_all="camelCase")]
pub(crate) struct ConnectionView{pub method:String,pub origin:Option<String>,pub port:u16,pub configured:bool}
pub(crate) fn view(host:&HostRuntime)->Result<ConnectionView,String>{
 let c=host.mobile_configuration().ok_or("MOBILE_LISTENER_UNAVAILABLE")?;let configured=c.allowed_origin.starts_with("https://");
 Ok(ConnectionView{method:mobile_http::config_method(&c),origin:configured.then_some(c.allowed_origin),port:host.mobile_address().map_or(c.port,|a|a.port()),configured})
}
pub(crate) fn validate(input:&ConnectionInput)->Result<(String,String),String>{
 if !matches!(input.method.as_str(),"CLOUDFLARE"|"TAILSCALE_FUNNEL"|"TAILSCALE_SERVE"|"CUSTOM_HTTPS"|"HOSTED"){return Err("MOBILE_METHOD_INVALID".into());}
 let u=url::Url::parse(input.origin.trim()).map_err(|_|"MOBILE_ORIGIN_INVALID")?;
 if u.scheme()!="https"||u.host_str().is_none()||!u.username().is_empty()||u.password().is_some()||u.query().is_some()||u.fragment().is_some()||u.path()!="/"||u.port()==Some(0){return Err("MOBILE_ORIGIN_INVALID".into());}
 if input.method.starts_with("TAILSCALE_")&&!u.host_str().unwrap().ends_with(".ts.net"){return Err("MOBILE_TAILSCALE_ORIGIN_REQUIRED".into());}
 Ok((u.origin().ascii_serialization(),u[url::Position::BeforeHost..url::Position::AfterPort].to_owned()))
}
fn write_atomic(path:&Path,bytes:&[u8])->Result<(),String>{
 if let Some(parent)=path.parent(){std::fs::create_dir_all(parent).map_err(|_|"MOBILE_CONFIG_WRITE_FAILED")?;}
 let pending=path.with_extension("json.pending");std::fs::write(&pending,bytes).map_err(|_|"MOBILE_CONFIG_WRITE_FAILED")?;std::fs::rename(&pending,path).map_err(|_|"MOBILE_CONFIG_WRITE_FAILED".into())
}
pub(crate) fn configure(core:&RouterCore,host:&HostRuntime,input:ConnectionInput)->Result<ConnectionView,String>{
 let path=mobile_http::mobile_runtime_config_path()?;
 let private=input.method=="TAILSCALE_SERVE";
 configure_at(core,host,input,&path,|url,nonce,instance|{
  let builder=reqwest::blocking::Client::builder().timeout(Duration::from_secs(10)).redirect(reqwest::redirect::Policy::none()).user_agent("Agbrio/0.1 Agent-Bridge");let builder=if private{builder.no_proxy()}else{builder};
  let client=builder.build().map_err(|_|"MOBILE_PROBE_FAILED")?;
  let response=client.get(format!("{url}/v1/mobile/connection/probe")).query(&[("nonce",nonce)]).send().map_err(|_|"MOBILE_PROBE_FAILED")?;
  if !response.status().is_success(){return Err("MOBILE_PROBE_FAILED".into());}
  let value:Value=response.json().map_err(|_|"MOBILE_PROBE_FAILED")?;
  if value["nonce"]!=nonce||value["instance"]!=instance{return Err("MOBILE_PROBE_WRONG_HOST".into());}Ok(())
 })?;
 view(host)
}
fn configure_at<F>(core:&RouterCore,host:&HostRuntime,input:ConnectionInput,path:&Path,probe:F)->Result<(),String>
where F:FnOnce(&str,&str,&str)->Result<(),String>{
 let _lock=CONFIGURE.lock().map_err(|_|"MOBILE_CONFIG_BUSY")?;
 let (origin,authority)=validate(&input)?;
 let mut prior=host.mobile_configuration().ok_or("MOBILE_LISTENER_UNAVAILABLE")?;
 if let Some(address)=host.mobile_address(){prior.port=address.port();}
 let mut candidate=prior.clone();candidate.allowed_origin=origin.clone();candidate.allowed_host=authority.clone();
 if input.method!="CLOUDFLARE"||origin!=prior.allowed_origin{candidate.access_issuer.clear();candidate.access_audience.clear();candidate.access_jwks_url.clear();}
 let nonce=uuid::Uuid::new_v4().to_string();
 *host.mobile_probe.lock().map_err(|_|"MOBILE_CONFIG_BUSY")?=Some(ProbeGrant{nonce:nonce.clone(),host:authority,expires:Instant::now()+Duration::from_secs(20)});
 let outcome=probe(&origin,&nonce,host.instance_id());
 *host.mobile_probe.lock().map_err(|_|"MOBILE_CONFIG_BUSY")?=None;
 outcome?; // Failed TLS/identity never touches the saved config or listener.
 let before=std::fs::read(path).ok();let bytes=mobile_http::config_bytes(&candidate,&input.method)?;
 write_atomic(path,&bytes)?;
 host.ensure_mobile(core.clone(),candidate.clone());
 let address=host.mobile_address().ok_or("MOBILE_LISTENER_UNAVAILABLE");
 let ready=address.ok().is_some_and(|address|reqwest::blocking::Client::builder().no_proxy().timeout(Duration::from_secs(3)).build().ok().and_then(|c|c.get(format!("http://{address}/v1/mobile/auth/session")).header("host",&candidate.allowed_host).send().ok()).is_some_and(|r|r.status().is_success()&&r.headers().get("x-aiwr-host-instance").and_then(|v|v.to_str().ok())==Some(host.instance_id())));
 if !ready{
  if std::fs::read(path).ok().as_deref()!=Some(bytes.as_slice()){return Err("MOBILE_CONFIG_CHANGED_RECOVERY_REQUIRED".into());}
  if let Some(before)=before{write_atomic(path,&before)?;}else{std::fs::remove_file(path).map_err(|_|"MOBILE_ROLLBACK_FAILED")?;}
  host.ensure_mobile(core.clone(),prior);
  return Err("MOBILE_CONFIG_ROLLED_BACK".into());
 }
 host.web.retry();Ok(())
}
#[derive(Deserialize)]#[serde(rename_all="camelCase",deny_unknown_fields)]struct SetupRequest{request_id:String,method:String,origin:String}
pub(crate) fn process_setup_request(core:&RouterCore,host:&HostRuntime){
    let Ok(config)=mobile_http::mobile_runtime_config_path()else{return;};let Some(parent)=config.parent()else{return;};let request=parent.join("mobile-setup-request.json");
    if !request.is_file(){return;}
    let processing=parent.join("mobile-setup-request.processing.json");if std::fs::rename(&request,&processing).is_err(){return;}
    let result=parent.join("mobile-setup-result.json");
    let outcome=(||{let b=std::fs::read(&processing).map_err(|_|"MOBILE_SETUP_INVALID")?;if b.len()>4096{return Err("MOBILE_SETUP_INVALID".to_string());}let input:SetupRequest=serde_json::from_slice(&b).map_err(|_|"MOBILE_SETUP_INVALID")?;if uuid::Uuid::parse_str(&input.request_id).is_err(){return Err("MOBILE_SETUP_INVALID".into());}let old:Option<Value>=std::fs::read(&result).ok().and_then(|b|serde_json::from_slice(&b).ok());if old.as_ref().is_some_and(|r|r["requestId"]==input.request_id){return Ok(());}
      write_atomic(&result,&serde_json::to_vec(&json!({"requestId":input.request_id,"status":"VERIFYING"})).unwrap())?;
      let value=match configure(core,host,ConnectionInput{method:input.method,origin:input.origin}){Ok(_)=>json!({"requestId":input.request_id,"status":"READY"}),Err(code)=>json!({"requestId":input.request_id,"status":"FAILED","errorCode":code})};write_atomic(&result,&serde_json::to_vec(&value).unwrap())
    })();
    if let Err(code)=outcome{let _=write_atomic(&result,&serde_json::to_vec(&json!({"status":"FAILED","errorCode":code})).unwrap());}
    // Consumed setup intents are not credentials, chats or user documents.
    let _=std::fs::remove_file(&processing);
}
#[cfg(test)]mod tests{
 use super::*;use std::sync::Arc;
 fn input(method:&str,origin:&str)->ConnectionInput{ConnectionInput{method:method.into(),origin:origin.into()}}
 #[test]#[ignore="explicit disposable fresh profile, public TLS and native handoff fixture only"]
 fn release_fresh_remote_fixture(){
  assert_eq!(std::env::var("AGBRIO_RELEASE_GATE").as_deref(),Ok("1"));
  let root=std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).parent().unwrap().join("runtime/agbrio-release/live");std::fs::create_dir_all(&root).unwrap();
  let case=std::env::var("AGBRIO_RELEASE_CASE").map(std::path::PathBuf::from).unwrap_or_else(|_|root.join(uuid::Uuid::new_v4().to_string()));assert!(case.starts_with(&root));std::fs::create_dir_all(&case).unwrap();let material=case.join("material.md");std::fs::write(&material,"PUBLIC_QA_MATERIAL = release-fixture\n").unwrap();
  let core=RouterCore{store:Arc::new(crate::RouterStore::open_at(case.join("router.db")).unwrap()),chatgpt:Arc::default(),session:Arc::default(),completed_chatgpt_responses:Arc::default()};
  let command=crate::shared_codex::transport_command().unwrap().expect("existing healthy shared connection only");let mut a=crate::codex::adapter::CodexAdapter::start_shared_validation(command,Arc::new(|_|{})).unwrap();a.initialize().unwrap();
  let catalog=a.request("model/list",json!({"limit":100,"cursor":null})).unwrap();let rows=catalog["data"].as_array().unwrap();
  let preferred=["gpt-6-luna","gpt-5.1-codex-mini","gpt-5.4-mini"];
  let model=preferred.iter().find_map(|id|rows.iter().find(|m|m["model"].as_str()==Some(*id)||m["id"].as_str()==Some(*id))).or_else(||rows.iter().find(|m|m["model"].as_str()==Some("gpt-6.1-sol"))).expect("supported low-cost validation model");
  let model=model["model"].as_str().or(model["id"].as_str()).unwrap();
  let start=|a:&mut crate::codex::adapter::CodexAdapter,name:&str|{let v=a.request("thread/start",json!({"cwd":case,"model":model,"approvalPolicy":"never","sandbox":"read-only","config":{"model_reasoning_effort":"low"}})).unwrap();let id=v["thread"]["id"].as_str().unwrap().to_string();let _=a.request("thread/name/set",json!({"threadId":id,"name":name}));id};
  let reused:Option<Value>=std::fs::read(case.join("reuse.json")).ok().and_then(|b|serde_json::from_slice(&b).ok());let old_source=reused.as_ref().and_then(|v|v["ids"][0].as_str());
  let saved:Option<Value>=std::fs::read(case.join("threads.json")).ok().and_then(|b|serde_json::from_slice(&b).ok());let old_target=saved.as_ref().and_then(|v|v["target"].as_str());
  let source=if let Some(id)=old_source{a.request("thread/resume",json!({"threadId":id,"model":model,"config":{"model_reasoning_effort":"low"}})).unwrap();id.to_string()}else{start(&mut a,"Agbrio release QA — manager")};let target=if let Some(id)=old_target{a.request("thread/resume",json!({"threadId":id,"model":model,"config":{"model_reasoning_effort":"low"}})).unwrap();id.to_string()}else{start(&mut a,"Agbrio release QA — executor")};
  std::fs::write(case.join("threads.json"),serde_json::to_vec(&json!({"source":source,"target":target})).unwrap()).unwrap();
  let text=format!("This is a transport QA fixture. Do not use tools. Reply with exactly the following public handoff, preserving the link.\n\nManagement report: material is ready.\n\nSend to the executor:\n```text\nRead only the selected material.md attachment, then reply AGBRIO_RELEASE_REMOTE_OK. Do not change files or access the network.\n```\n\nMaterial: [material.md]({})",material.display());
  if old_source.is_none(){let turn=a.request("turn/start",json!({"threadId":source,"model":model,"effort":"low","input":[{"type":"text","text":text}]})).unwrap();let turn=turn["turn"]["id"].as_str().unwrap();a.mark_turn_started(&source,turn);}
  if old_target.is_none(){let turn=a.request("turn/start",json!({"threadId":target,"model":model,"effort":"low","input":[{"type":"text","text":"Transport QA bootstrap. Do not use tools. Reply READY."}]})).unwrap();a.mark_turn_started(&target,turn["turn"]["id"].as_str().unwrap());}
  let deadline=Instant::now()+Duration::from_secs(90);loop{let r=a.request("thread/read",json!({"threadId":target,"includeTurns":true}));if r.as_ref().ok().and_then(|v|v["thread"]["turns"].as_array()).is_some_and(|turns|turns.iter().any(|t|t["status"]=="completed")){break;}assert!(Instant::now()<deadline,"Bootstrap persistence/terminal timeout; do not resend");std::thread::sleep(Duration::from_millis(500));}
  {let mut session=core.session.lock().unwrap();session.ready_threads.extend([source.clone(),target.clone()]);session.adapter=Some(a);}
  let existing=core.store.snapshot().unwrap().workstreams.iter().find_map(|w|core.store.role_bridge(&w.id).ok().filter(|b|b.decision.as_ref().is_some_and(|s|s.endpoint.external_id==source)&&b.execution.as_ref().is_some_and(|s|s.endpoint.external_id==target)).map(|_|w.id.clone()));
  let work=existing.clone().unwrap_or_else(||crate::role_bridge::create(&core,"Agbrio isolated release handoff").unwrap());let revision=core.store.role_bridge(&work).unwrap().binding_revision;
  let bind=|id:&str,name:&str|router_core::store::role_bridge::RoleBindingInput{provider:"CODEX".into(),external_id:id.into(),label:name.into(),cwd:Some(case.to_string_lossy().into())};if existing.is_none(){crate::role_bridge::bind(&core,&work,revision,bind(&source,"Manager QA"),bind(&target,"Executor QA")).unwrap();}
  let host=HostRuntime::default();let auth=Arc::new(crate::web_auth::WebAuth::open(None).unwrap());*host.test_web_auth.lock().unwrap()=Some(auth.clone());let c=MobileHttpConfig{port:0,allowed_host:"127.0.0.1:0".into(),allowed_origin:"http://127.0.0.1:0".into(),access_issuer:String::new(),access_audience:String::new(),access_jwks_url:String::new(),static_dir:std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).parent().unwrap().join("dist")};host.ensure_mobile(core.clone(),c);let code=auth.issue().unwrap();
  std::fs::write(root.join("ready.json"),serde_json::to_vec(&json!({"case":case,"port":host.mobile_address().unwrap().port(),"code":code.code,"workstreamId":work,"sourceThread":source,"targetThread":target,"model":model})).unwrap()).unwrap();
  for file in ["stop","configured.json"]{let _=std::fs::remove_file(case.join(file));}let deadline=Instant::now()+Duration::from_secs(300);
  while Instant::now()<deadline&&!case.join("stop").exists(){if let Ok(bytes)=std::fs::read(case.join("origin.json")){if let Ok(input)=serde_json::from_slice::<ConnectionInput>(&bytes){let _=std::fs::remove_file(case.join("origin.json"));let path=case.join("mobile-runtime.json");let outcome=configure_at(&core,&host,input,&path,|url,nonce,instance|{let c=reqwest::blocking::Client::builder().timeout(Duration::from_secs(10)).build().map_err(|_|"PROBE_FAILED")?;let r:Value=c.get(format!("{url}/v1/mobile/connection/probe")).query(&[("nonce",nonce)]).send().map_err(|e|if e.is_timeout(){"PROBE_TIMEOUT"}else if e.is_connect(){"PROBE_CONNECT"}else{"PROBE_REQUEST"})?.error_for_status().map_err(|_|"PROBE_HTTP")?.json().map_err(|_|"PROBE_JSON")?;if r["nonce"]==nonce&&r["instance"]==instance{Ok(())}else{Err("PROBE_IDENTITY_MISMATCH".into())}});std::fs::write(case.join("configured.json"),serde_json::to_vec(&json!({"ok":outcome.is_ok(),"error":outcome.err()})).unwrap()).unwrap();}}
   std::thread::sleep(Duration::from_millis(200));
  }host.shutdown(&core);
 }
 #[test]fn validates_https_authority_and_preserves_provider_identity(){
  assert_eq!(validate(&input("CUSTOM_HTTPS","https://Example.test:8443/")).unwrap(),("https://example.test:8443".into(),"example.test:8443".into()));
  for url in ["http://example.test","https://user:pass@example.test","https://example.test/path","https://example.test?token=x","https://example.test#x"]{assert!(validate(&input("CUSTOM_HTTPS",url)).is_err());}
  assert!(validate(&input("TAILSCALE_FUNNEL","https://pc.tail123.ts.net")).is_ok());assert!(validate(&input("TAILSCALE_SERVE","https://example.test")).is_err());assert!(validate(&input("other","https://example.test")).is_err());
 }
 #[test]fn failed_probe_preserves_old_bytes_listener_and_pairing_boundary(){
  let d=tempfile::tempdir().unwrap();std::fs::write(d.path().join("index.html"),"fixture").unwrap();let core=RouterCore{store:Arc::new(crate::RouterStore::open_at(d.path().join("db")).unwrap()),chatgpt:Arc::default(),session:Arc::default(),completed_chatgpt_responses:Arc::default()};let host=HostRuntime::default();let mut c=MobileHttpConfig::controlled_host_acceptance(d.path().into());c.port=0;host.ensure_mobile(core.clone(),c);let address=host.mobile_address();let p=d.path().join("config.json");std::fs::write(&p,b"original exact bytes").unwrap();
  let r=configure_at(&core,&host,input("CUSTOM_HTTPS","https://example.test"),&p,|_,nonce,_|{assert!(host.verify_mobile_probe("example.test",nonce).is_ok());assert!(host.verify_mobile_probe("wrong.test",nonce).is_err());Err("MOBILE_PROBE_FAILED".into())});
  assert!(r.is_err());assert_eq!(std::fs::read(p).unwrap(),b"original exact bytes");assert_eq!(host.mobile_address(),address);assert!(host.mobile_probe.lock().unwrap().is_none());host.shutdown(&core);
 }
 #[test]fn successful_verified_configuration_commits_and_rejects_anonymous_writes(){
  let d=tempfile::tempdir().unwrap();std::fs::write(d.path().join("index.html"),"fixture").unwrap();let core=RouterCore{store:Arc::new(crate::RouterStore::open_at(d.path().join("db")).unwrap()),chatgpt:Arc::default(),session:Arc::default(),completed_chatgpt_responses:Arc::default()};let host=HostRuntime::default();let mut c=MobileHttpConfig::controlled_host_acceptance(d.path().into());c.port=0;host.ensure_mobile(core.clone(),c);let p=d.path().join("config.json");
  configure_at(&core,&host,input("TAILSCALE_FUNNEL","https://pc.tail123.ts.net"),&p,|_,_,_|Ok(())).unwrap();let v:Value=serde_json::from_slice(&std::fs::read(p).unwrap()).unwrap();assert_eq!(v["managed"],true);assert_eq!(v["access_issuer"],"");assert_eq!(v["connection_method"],"TAILSCALE_FUNNEL");
  let response=reqwest::blocking::Client::new().post(format!("http://{}/v1/mobile/bridges",host.mobile_address().unwrap())).header("host","pc.tail123.ts.net").header("origin","https://pc.tail123.ts.net").json(&json!({"name":"must not create"})).send().unwrap();assert_eq!(response.status(),reqwest::StatusCode::UNAUTHORIZED);host.shutdown(&core);
 }
}
