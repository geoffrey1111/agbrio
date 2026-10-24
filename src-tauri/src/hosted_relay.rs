//! Optional hosted entrance. Credentials never leave owner IPC/status DTOs.
use crate::{HostRuntime,RouterCore,mobile_connection::{self,ConnectionInput}};
use serde::{Deserialize,Serialize};
use std::{fs,path::PathBuf,process::{Command,Stdio},sync::{Mutex,OnceLock},time::Duration};
static ASSETS:OnceLock<(PathBuf,PathBuf)>=OnceLock::new();
static CHANGE:Mutex<()>=Mutex::new(());
#[derive(Clone,Serialize,Deserialize)]#[serde(rename_all="camelCase")]
struct Saved {device_key:String,#[serde(default)] lease:Option<Lease>,#[serde(default)] enabled:bool,#[serde(default)] prior:Option<Vec<u8>>}
#[derive(Clone,Serialize,Deserialize)]#[serde(rename_all="camelCase")]
struct Lease{tenant_id:String,origin:String,expires_at:u64,host_token:String,#[serde(default)] kind:Option<String>,#[serde(default)] server_now:Option<u64>,#[serde(default)] received_at:Option<u64>}
#[derive(Serialize)]#[serde(rename_all="camelCase")]
pub(crate) struct Status{pub available:bool,pub enabled:bool,pub state:String,pub origin:Option<String>,pub expires_at:Option<u64>,pub kind:Option<String>,pub server_now:u64,pub trial_available:bool}
#[derive(Deserialize)]#[serde(deny_unknown_fields)]pub(crate) struct RedeemInput{pub code:String}
pub(crate) fn configure_assets(source:PathBuf,node:PathBuf){fn normal(p:PathBuf)->PathBuf{let s=p.to_string_lossy();if let Some(d)=s.strip_prefix(r"\\?\"){if d.as_bytes().get(1)==Some(&b':'){return PathBuf::from(d);}}p}let _=ASSETS.set((normal(source),normal(node)));}
fn path()->Result<PathBuf,String>{Ok(crate::mobile_http::mobile_runtime_config_path()?.with_file_name("hosted-connection.json"))}
fn read()->Result<Option<Saved>,String>{let p=path()?;if !p.exists(){return Ok(None);}serde_json::from_slice(&fs::read(p).map_err(|_|"HOSTED_STORAGE_UNAVAILABLE")?).map(Some).map_err(|_|"HOSTED_STORAGE_INVALID".into())}
fn save(s:&Saved)->Result<(),String>{let p=path()?;fs::create_dir_all(p.parent().unwrap()).map_err(|_|"HOSTED_STORAGE_UNAVAILABLE")?;let tmp=p.with_extension("pending");fs::write(&tmp,serde_json::to_vec(s).map_err(|_|"HOSTED_STORAGE_INVALID")?).map_err(|_|"HOSTED_STORAGE_UNAVAILABLE")?;fs::rename(tmp,p).map_err(|_|"HOSTED_STORAGE_UNAVAILABLE".into())}
fn control()->Result<String,String>{let v=option_env!("AGBRIO_HOSTED_CONTROL_ORIGIN").ok_or("HOSTED_SERVICE_UNCONFIGURED")?;mobile_connection::validate(&ConnectionInput{method:"CUSTOM_HTTPS".into(),origin:v.into()}).map(|v|v.0)}
fn validate_lease(l:&Lease)->Result<(),String>{let u=url::Url::parse(&l.origin).map_err(|_|"HOSTED_LEASE_INVALID")?;if u.scheme()!="https"||u.username()!=""||u.password().is_some()||u.path()!="/"||u.query().is_some()||u.fragment().is_some()||l.tenant_id.len()!=32||!l.tenant_id.bytes().all(|b|b.is_ascii_hexdigit()&& !b.is_ascii_uppercase())||!u.host_str().is_some_and(|h|h.starts_with(&format!("ag-{}.",l.tenant_id)))||l.host_token.len()!=64||!l.host_token.bytes().all(|b|b.is_ascii_hexdigit()&& !b.is_ascii_uppercase()) {return Err("HOSTED_LEASE_INVALID".into());}Ok(())}
pub(crate) fn status(host:&HostRuntime)->Status{
 let saved=read().ok().flatten();let l=saved.as_ref().and_then(|s|s.lease.as_ref());let enabled=saved.as_ref().is_some_and(|s|s.enabled)&&host.mobile_configuration().is_some_and(|c|l.is_some_and(|l|l.origin==c.allowed_origin));
 let expired=l.is_some_and(|l|l.expires_at<=lease_now(l));let state=if !enabled{"SELF_HOSTED"}else if expired{"EXPIRED"}else{
  let v=path().ok().and_then(|p|fs::read(p.with_file_name("hosted-status.json")).ok()).and_then(|b|serde_json::from_slice::<serde_json::Value>(&b).ok());
  if v.as_ref().is_some_and(|v|l.is_some_and(|l|v["tenantId"]==l.tenant_id)&&v["state"]=="REVOKED"){"REVOKED"}else if v.as_ref().is_some_and(|v|l.is_some_and(|l|v["tenantId"]==l.tenant_id)&&v["state"]=="EXPIRED"){"EXPIRED"}else if v.is_some_and(|v|v["state"]=="READY"&&l.is_some_and(|l|v["tenantId"]==l.tenant_id)&&host.hosted_child.lock().is_ok_and(|mut p|p.as_mut().is_some_and(|p|p.try_wait().is_ok_and(|v|v.is_none())))){"READY"}else{"CONNECTING"}
 };Status{available:control().is_ok(),enabled,state:state.into(),origin:l.map(|l|l.origin.clone()),expires_at:l.map(|l|l.expires_at),kind:l.map(|l|l.kind.clone().unwrap_or_else(||"CODE".into())),server_now:l.map_or_else(now_ms,lease_now),trial_available:cfg!(windows)&&control().is_ok()}
}
fn now_ms()->u64{std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_millis() as u64}
fn lease_now(l:&Lease)->u64{match(l.server_now,l.received_at){(Some(server),Some(received))=>server.saturating_add(now_ms().saturating_sub(received)),_=>now_ms()}}
fn connector_binding_matches(bytes:&[u8],lease:&Lease,port:u16)->bool{
 serde_json::from_slice::<serde_json::Value>(bytes).is_ok_and(|v|
  v["port"].as_u64()==Some(port as u64)&&v["origin"]==lease.origin&&v["tenantId"]==lease.tenant_id&&v["hostToken"]==lease.host_token)
}
pub(crate) fn stop(host:&HostRuntime){if let Ok(mut p)=host.hosted_child.lock(){if let Some(mut p)=p.take(){let _=p.kill();let _=p.wait();}}}
fn spawn(host:&HostRuntime,s:&Saved)->Result<(),String>{
 let l=s.lease.as_ref().ok_or("HOSTED_LEASE_MISSING")?;validate_lease(l)?;let (source,node)=ASSETS.get().ok_or("HOSTED_RUNTIME_UNAVAILABLE")?;let address=host.mobile_address().ok_or("MOBILE_LISTENER_UNAVAILABLE")?;
 let config=path()?.with_file_name("hosted-connector.json");let status=path()?.with_file_name("hosted-status.json");
 let mut p=host.hosted_child.lock().map_err(|_|"HOSTED_BUSY")?;
 if p.as_mut().is_some_and(|p|p.try_wait().is_ok_and(|state|state.is_none())){
  if fs::read(&config).is_ok_and(|bytes|connector_binding_matches(&bytes,l,address.port())){return Ok(());}
  // A recovered listener can get another port. Rebind only our owned relay;
  // never leave a live WebSocket forwarding forever to the previous listener.
  if let Some(mut previous)=p.take(){let _=previous.kill();let _=previous.wait();}
 }
 let _=fs::remove_file(&status);
 fs::write(&config,serde_json::to_vec(&serde_json::json!({"origin":l.origin,"hostToken":l.host_token,"tenantId":l.tenant_id,"port":address.port(),"statusPath":status})).unwrap()).map_err(|_|"HOSTED_STORAGE_UNAVAILABLE")?;
 let mut command=Command::new(node);command.arg(source.join("connector.mjs")).arg(config).env("NODE_USE_SYSTEM_CA","1").stdin(Stdio::piped()).stdout(Stdio::null()).stderr(Stdio::null());
 #[cfg(windows)]{use std::os::windows::process::CommandExt;command.creation_flags(0x08000000);}
 *p=Some(command.spawn().map_err(|_|"HOSTED_RUNTIME_UNAVAILABLE")?);Ok(())
}
pub(crate) fn ensure(host:&HostRuntime){let Ok(_lock)=CHANGE.try_lock()else{return;};let Ok(Some(s))=read()else{return;};if !s.enabled||s.lease.as_ref().is_none_or(|l|l.expires_at<=lease_now(l)) {stop(host);return;}if host.mobile_configuration().is_some_and(|c|s.lease.as_ref().is_some_and(|l|l.origin==c.allowed_origin)){let _=spawn(host,&s);}else{stop(host);}}
fn identity()->Result<Saved,String>{let saved=read()?.unwrap_or(Saved{device_key:format!("{}{}",uuid::Uuid::new_v4().simple(),uuid::Uuid::new_v4().simple()),lease:None,enabled:false,prior:None});save(&saved)?;Ok(saved)}
fn client()->Result<reqwest::blocking::Client,String>{reqwest::blocking::Client::builder().timeout(Duration::from_secs(20)).redirect(reqwest::redirect::Policy::none()).build().map_err(|_|"HOSTED_SERVICE_UNAVAILABLE".into())}
fn request_lease(client:&reqwest::blocking::Client,control:&str,path:&str,input:serde_json::Value)->Result<Lease,String>{
 let response=client.post(format!("{control}{path}")).json(&input).send().map_err(|_|"HOSTED_REDEEM_UNCERTAIN")?;
 if !response.status().is_success(){let status=response.status().as_u16();let body:serde_json::Value=response.json().unwrap_or_default();let code=body["error"].as_str().unwrap_or("");return Err(match code{
 "TRIAL_ALREADY_USED"|"TRIAL_NOT_ELIGIBLE"=>"HOSTED_TRIAL_ALREADY_USED",
 "TRIAL_SETUP_EXPIRED"=>"HOSTED_TRIAL_SETUP_EXPIRED",
 "TRIAL_LIMIT_REACHED"|"TRIAL_CAPACITY_REACHED"=>"HOSTED_TRIAL_CAPACITY",
 "TRIAL_UNAVAILABLE"=>"HOSTED_TRIAL_UNAVAILABLE",
 _=>match status{409=>"HOSTED_CODE_ALREADY_USED",403=>"HOSTED_CODE_UNAVAILABLE",503=>"HOSTED_SERVICE_NOT_READY",_=>"HOSTED_CODE_INVALID"}}.into());}
 let mut lease:Lease=response.json().map_err(|_|"HOSTED_LEASE_INVALID")?;lease.received_at=Some(now_ms());validate_lease(&lease)?;
 if lease.kind.as_deref().is_some_and(|k|!matches!(k,"CODE"|"TRIAL"|"TRIAL_PENDING")){return Err("HOSTED_LEASE_INVALID".into());}
 if lease.expires_at<=lease_now(&lease){return Err(if lease.kind.as_deref()==Some("TRIAL"){"HOSTED_TRIAL_EXPIRED"}else{"HOSTED_CODE_EXPIRED"}.into());}Ok(lease)
}
fn activate(core:&RouterCore,host:&HostRuntime,mut saved:Saved,lease:Lease)->Result<Status,String>{
 let before=saved.clone();if saved.prior.is_none(){saved.prior=fs::read(crate::mobile_http::mobile_runtime_config_path()?).ok();}
 saved.lease=Some(lease.clone());saved.enabled=true;save(&saved)?;
 if let Err(e)=spawn(host,&saved){save(&before)?;ensure_after_failure(host,&before);return Err(e);}
 let mut result=Err("HOSTED_CONNECTION_FAILED".to_string());for _ in 0..4{result=mobile_connection::configure(core,host,ConnectionInput{method:"HOSTED".into(),origin:lease.origin.clone()});if result.is_ok(){break;}std::thread::sleep(Duration::from_secs(1));}
 if let Err(error)=result{stop(host);save(&before)?;ensure_after_failure(host,&before);return Err(error);}Ok(status(host))
}
pub(crate) fn redeem(core:&RouterCore,host:&HostRuntime,input:RedeemInput)->Result<Status,String>{
 let _lock=CHANGE.lock().map_err(|_|"HOSTED_BUSY")?;let control=control()?;
 let code=input.code.trim();if code.len()>80||!code.starts_with("AGB"){return Err("HOSTED_CODE_INVALID".into());}
 let saved=identity()?;let lease=request_lease(&client()?,&control,"/v1/redeem",serde_json::json!({"code":code,"deviceKey":saved.device_key}))?;activate(core,host,saved,lease)
}
fn hash_trial_device(raw:&str)->Result<String,String>{
 use sha2::{Digest,Sha256};let normalized=raw.trim().to_ascii_lowercase();
 if normalized.len()<16||normalized.len()>128{return Err("HOSTED_DEVICE_UNAVAILABLE".into());}
 Ok(format!("{:x}",Sha256::digest(format!("agbrio-hosted-trial-v1\0{normalized}").as_bytes())))
}
fn trial_device_id()->Result<String,String>{
 #[cfg(windows)]{use winreg::{RegKey,enums::{HKEY_LOCAL_MACHINE,KEY_READ,KEY_WOW64_64KEY}};
 let key=RegKey::predef(HKEY_LOCAL_MACHINE).open_subkey_with_flags("SOFTWARE\\Microsoft\\Cryptography",KEY_READ|KEY_WOW64_64KEY).map_err(|_|"HOSTED_DEVICE_UNAVAILABLE")?;
 let raw:String=key.get_value("MachineGuid").map_err(|_|"HOSTED_DEVICE_UNAVAILABLE")?;hash_trial_device(&raw)}
 #[cfg(not(windows))]{Err("HOSTED_DEVICE_UNAVAILABLE".into())}
}
pub(crate) fn start_trial(core:&RouterCore,host:&HostRuntime)->Result<Status,String>{start_trial_for_device(core,host,trial_device_id()?)}
fn start_trial_for_device(core:&RouterCore,host:&HostRuntime,trial_id:String)->Result<Status,String>{
 let _lock=CHANGE.lock().map_err(|_|"HOSTED_BUSY")?;let control=control()?;let saved=identity()?;
 if saved.lease.as_ref().is_some_and(|l|l.kind.as_deref()!=Some("TRIAL")){return Err("HOSTED_TRIAL_ALREADY_USED".into());}
 let client=client()?;let payload=serde_json::json!({"deviceKey":saved.device_key,"trialDeviceId":trial_id});
 let mut lease=request_lease(&client,&control,"/v1/trial",payload.clone())?;
 if lease.kind.as_deref()==Some("TRIAL_PENDING"){
  let mut pending=saved.clone();pending.lease=Some(lease.clone());pending.enabled=true;
  stop(host);if let Err(error)=spawn(host,&pending){ensure_after_failure(host,&saved);return Err(error);}
  let input=ConnectionInput{method:"HOSTED".into(),origin:lease.origin.clone()};
  let mut probe=Err("HOSTED_CONNECTION_FAILED".to_string());for _ in 0..4{probe=mobile_connection::verify(host,&input);if probe.is_ok(){break;}std::thread::sleep(Duration::from_secs(1));}
  if let Err(error)=probe{stop(host);ensure_after_failure(host,&saved);return Err(error);}
  let confirmed=request_lease(&client,&control,"/v1/trial/confirm",payload);
  match confirmed{Ok(value)if value.kind.as_deref()==Some("TRIAL")&&value.tenant_id==lease.tenant_id=>lease=value,Ok(_)=>{stop(host);ensure_after_failure(host,&saved);return Err("HOSTED_LEASE_INVALID".into());},Err(error)=>{stop(host);ensure_after_failure(host,&saved);return Err(error);}}
 }
 if lease.kind.as_deref()!=Some("TRIAL"){return Err("HOSTED_LEASE_INVALID".into());}
 activate(core,host,saved,lease)
}
fn ensure_after_failure(host:&HostRuntime,s:&Saved){if s.enabled{let _=spawn(host,s);}}
#[cfg(test)]mod tests{use super::*;#[test]fn listener_recovery_invalidates_only_the_changed_connector_binding(){let l=Lease{tenant_id:"a".repeat(32),origin:format!("https://ag-{}.example.invalid","a".repeat(32)),expires_at:u64::MAX,host_token:"b".repeat(64),kind:None,server_now:None,received_at:None};let bytes=serde_json::to_vec(&serde_json::json!({"port":47114,"origin":l.origin,"tenantId":l.tenant_id,"hostToken":l.host_token})).unwrap();assert!(connector_binding_matches(&bytes,&l,47114));assert!(!connector_binding_matches(&bytes,&l,47115));assert!(!connector_binding_matches(&bytes,&Lease{host_token:"c".repeat(64),..l.clone()},47114));assert!(!connector_binding_matches(b"{}",&l,47114));}#[test]fn lease_identity_never_accepts_path_credentials_or_foreign_tenant(){let l=Lease{tenant_id:"a".repeat(32),origin:format!("https://ag-{}.example.invalid","a".repeat(32)),expires_at:u64::MAX,host_token:"b".repeat(64),kind:None,server_now:None,received_at:None};assert!(validate_lease(&l).is_ok());for origin in ["https://other.example.invalid","https://user@ag-aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa.example.invalid","https://ag-aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa.example.invalid/path","http://ag-aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa.example.invalid"]{assert!(validate_lease(&Lease{origin:origin.into(),..l.clone()}).is_err());}}}

#[cfg(test)]
#[test]
#[ignore="owner-approved isolated profile + deployed hosted relay, no provider operations"]
fn public_disposable_host_fixture(){
 use std::sync::{Arc,Mutex};
 let root=PathBuf::from(std::env::var_os("AGBRIO_RELAY_FIXTURE").expect("explicit fixture directory"));
 let profile=PathBuf::from(std::env::var_os("LOCALAPPDATA").unwrap());assert!(profile.starts_with(&root));
 let input:serde_json::Value=serde_json::from_slice(&fs::read(root.join("input.json")).unwrap()).unwrap();
 let code=input["code"].as_str().unwrap_or("").to_owned();let label=input["label"].as_str().unwrap();
 let core=RouterCore{chatgpt:Arc::default(),store:Arc::new(crate::RouterStore::open_at(root.join("router.db")).unwrap()),session:Arc::new(Mutex::new(crate::Session::default())),completed_chatgpt_responses:Arc::default()};
 if core.store.snapshot().unwrap().projects.is_empty(){crate::role_bridge::create(&core,&format!("Demo {label}")).unwrap();}
 let host=HostRuntime::default();let auth=Arc::new(crate::web_auth::WebAuth::open(Some(root.join("web-auth.json"))).unwrap());*host.test_web_auth.lock().unwrap()=Some(auth.clone());
 let source=PathBuf::from(env!("CARGO_MANIFEST_DIR"));let dist=source.parent().unwrap().join("dist");
 host.ensure_mobile(core.clone(),crate::mobile_http::MobileHttpConfig{port:0,allowed_host:"127.0.0.1:0".into(),allowed_origin:"http://127.0.0.1:0".into(),access_issuer:String::new(),access_audience:String::new(),access_jwks_url:String::new(),static_dir:dist});
 configure_assets(source.join("resources/hosted-relay"),source.join("binaries/browser-executor-node-x86_64-pc-windows-msvc.exe"));
 let result=if input["trial"]==true{start_trial_for_device(&core,&host,input["trialDeviceId"].as_str().unwrap().to_owned())}else{redeem(&core,&host,RedeemInput{code})};if result.is_err(){host.shutdown(&core);}assert!(result.is_ok(),"activation: {:?}",result.err());
 let status=status(&host);let pairing=auth.issue().unwrap();
 fs::write(root.join("ready.json"),serde_json::to_vec(&serde_json::json!({"origin":status.origin,"expiresAt":status.expires_at,"state":status.state,"pairingCode":pairing.code,"port":host.mobile_address().unwrap().port(),"instance":host.instance_id()})).unwrap()).unwrap();
 let deadline=std::time::Instant::now()+Duration::from_secs(1200);
 while std::time::Instant::now()<deadline&&!root.join("stop").exists(){std::thread::sleep(Duration::from_millis(250));ensure(&host);}
 host.shutdown(&core);
}

#[cfg(test)] mod trial_identity_tests{
 use super::*;
 #[test] fn device_hash_is_scoped_normalized_and_contains_no_raw_identifier(){let raw="SYNTHETIC-MACHINE-GUID-20261024";let a=hash_trial_device(raw).unwrap();assert_eq!(a.len(),64);assert_eq!(a,hash_trial_device(&raw.to_ascii_lowercase()).unwrap());assert_ne!(a,hash_trial_device("different-machine-guid").unwrap());assert!(hash_trial_device("").is_err());}
 #[test] fn legacy_leases_remain_code_and_server_clock_offsets_are_projected(){let raw=serde_json::json!({"tenantId":"a".repeat(32),"origin":format!("https://ag-{}.example.invalid","a".repeat(32)),"expiresAt":99,"hostToken":"b".repeat(64)});let mut l:Lease=serde_json::from_value(raw).unwrap();assert!(l.kind.is_none());l.server_now=Some(1000);l.received_at=Some(now_ms());assert!((1000..2000).contains(&lease_now(&l)));}
 #[cfg(windows)] #[test] fn windows_identity_is_stable_across_reads_without_exposing_it(){let a=trial_device_id().unwrap();assert_eq!(a.len(),64);assert_eq!(a,trial_device_id().unwrap());}
}
