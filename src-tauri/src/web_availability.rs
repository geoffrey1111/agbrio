//! Desktop-owned availability watchdog. Reuses one Host and the installed
//! Cloudflared service; never reads credentials or changes tunnel routes.
use serde::Serialize;
use std::sync::{atomic::{AtomicBool,Ordering},Arc,Mutex};
use std::time::{Duration,Instant,SystemTime,UNIX_EPOCH};
static SETUP_PENDING:AtomicBool=AtomicBool::new(false);
#[derive(Clone,Serialize)]
#[serde(rename_all="camelCase")]
pub(crate) struct Status {pub state:String,pub local_ready:bool,pub tunnel_ready:bool,pub public_ready:bool,pub recovery_allowed:bool,pub checked_at:u64,pub recovery_count:u32,pub setup_pending:bool}
impl Default for Status {fn default()->Self{Self{state:"CHECKING".into(),local_ready:false,tunnel_ready:false,public_ready:false,recovery_allowed:false,checked_at:0,recovery_count:0,setup_pending:false}}}
#[derive(Default)]
pub(crate) struct Watchdog {status:Mutex<Status>,stop:AtomicBool,wake:AtomicBool,started:AtomicBool,worker:Mutex<Option<std::thread::JoinHandle<()>>>}
#[derive(Default)]
struct RecoveryPolicy {failures:u32,last:Option<u64>,attempts:u32}
impl RecoveryPolicy {
 fn tick(&mut self,eligible:bool,elapsed:u64)->bool {
  if !eligible {self.failures=0;return false;}
  self.failures+=1;
  let delay=(60u64.saturating_mul(1u64<<self.attempts.min(4))).min(600);
  if self.failures<3 || self.last.is_some_and(|last|elapsed.saturating_sub(last)<delay){return false;}
  self.last=Some(elapsed);self.attempts+=1;self.failures=0;true
 }
 fn healthy(&mut self){self.failures=0;self.attempts=0;}
}
impl Watchdog {
 pub(crate) fn status(&self)->Status {let mut status=self.status.lock().map(|s|s.clone()).unwrap_or_default();status.setup_pending=SETUP_PENDING.load(Ordering::Acquire);status}
 pub(crate) fn retry(&self){self.wake.store(true,Ordering::Release);}
 pub(crate) fn stop(&self){self.stop.store(true,Ordering::Release);if let Ok(mut slot)=self.worker.lock(){if let Some(t)=slot.take(){let _=t.join();}}}
 pub(crate) fn start(self:&Arc<Self>,host:crate::HostRuntime,core:crate::RouterCore,config:crate::mobile_http::MobileHttpConfig){
  if self.started.swap(true,Ordering::AcqRel){return;}
  let guard=self.clone();let task=std::thread::spawn(move||{
   let client=match reqwest::blocking::Client::builder().timeout(Duration::from_secs(5)).redirect(reqwest::redirect::Policy::none()).build(){Ok(c)=>c,Err(_)=>return};
   let local_client=match reqwest::blocking::Client::builder().no_proxy().timeout(Duration::from_secs(2)).build(){Ok(c)=>c,Err(_)=>return};
   let private_client=match reqwest::blocking::Client::builder().no_proxy().timeout(Duration::from_secs(5)).redirect(reqwest::redirect::Policy::none()).build(){Ok(c)=>c,Err(_)=>return};
   let since=Instant::now();let mut policy=RecoveryPolicy::default();let mut local_policy=RecoveryPolicy::default();let mut count=0;
   while !guard.stop.load(Ordering::Acquire){
    let config=crate::mobile_http::managed_config(&config);
    host.ensure_mobile(core.clone(),config.clone());
    let local=local_client.get(format!("http://127.0.0.1:{}/v1/mobile/auth/session",config.port)).header("host",&config.allowed_host).send().is_ok_and(|r|r.status().is_success()&&r.headers().get("x-aiwr-host-instance").and_then(|v|v.to_str().ok())==Some(host.instance_id()));
    let legacy=crate::mobile_http::legacy_cloudflare_recovery(&config);
    let tunnel=legacy&&local_client.get("http://127.0.0.1:20241/ready").send().is_ok_and(|r|r.status().is_success());
    let allowed=legacy&&service::can_recover();
    let external_client=if crate::mobile_http::config_method(&config)=="TAILSCALE_SERVE"{&private_client}else{&client};
    let configured=config.allowed_origin.starts_with("https://");
    let public=configured&&external_client.get(format!("{}/v1/mobile/auth/session",config.allowed_origin.trim_end_matches('/'))).header("cache-control","no-cache").send().is_ok_and(|r|r.status().is_success()&&r.headers().get("x-aiwr-host-instance").and_then(|v|v.to_str().ok())==Some(host.instance_id()));
    let mut state=if !configured{"CONFIG_REQUIRED"}else if public{"ONLINE"}else if !local{"LOCAL_RECOVERING"}else if legacy&&!tunnel && !allowed{"ADMIN_SETUP_REQUIRED"}else if legacy&&!tunnel{"TUNNEL_RECONNECTING"}else{"PUBLIC_UNREACHABLE"}.to_string();
    if public {policy.healthy();}
    if local {local_policy.healthy();}
    if local_policy.tick(!local,since.elapsed().as_secs()) && !guard.stop.load(Ordering::Acquire){host.restart_mobile_listener(core.clone(),config.clone());}
    // Public errors while a connector is healthy can be DNS/Access/Cloudflare
    // errors. Never restart the shared service for those or an origin failure.
    if policy.tick(local&&!tunnel&&!public&&allowed,since.elapsed().as_secs()) && !guard.stop.load(Ordering::Acquire){
     state="TUNNEL_RECOVERING".into();if service::recover(&guard.stop).is_ok(){count+=1;}
    }
    if let Ok(mut status)=guard.status.lock(){*status=Status{state,local_ready:local,tunnel_ready:tunnel,public_ready:public,recovery_allowed:allowed,checked_at:SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs(),recovery_count:count,setup_pending:SETUP_PENDING.load(Ordering::Acquire)};}
    for _ in 0..15 {if guard.stop.load(Ordering::Acquire)||guard.wake.swap(false,Ordering::AcqRel){break;}std::thread::sleep(Duration::from_secs(1));}
   }
  });if let Ok(mut slot)=self.worker.lock(){*slot=Some(task);}
 }
}
#[cfg(windows)]
mod service {
 use super::*;use std::ffi::c_void;
 type Handle=*mut c_void;
 #[repr(C)] #[derive(Default)] struct ServiceStatus {kind:u32,state:u32,controls:u32,exit:u32,specific_exit:u32,checkpoint:u32,hint:u32}
 #[repr(C)] #[derive(Default)] struct ProcessStatus {status:ServiceStatus,pid:u32,flags:u32}
 #[link(name="advapi32")] unsafe extern "system" {
  fn OpenSCManagerW(machine:*const u16,database:*const u16,access:u32)->Handle;
  fn OpenServiceW(manager:Handle,name:*const u16,access:u32)->Handle;
  fn CloseServiceHandle(handle:Handle)->i32;
  fn QueryServiceStatusEx(handle:Handle,level:u32,buffer:*mut u8,size:u32,needed:*mut u32)->i32;
  fn ControlService(handle:Handle,control:u32,status:*mut ServiceStatus)->i32;
  fn StartServiceW(handle:Handle,count:u32,args:*const *const u16)->i32;
 }
 struct Owned(Handle);impl Drop for Owned{fn drop(&mut self){unsafe{CloseServiceHandle(self.0);}}}
 fn open()->Result<Owned,()> {unsafe{let manager=Owned(OpenSCManagerW(std::ptr::null(),std::ptr::null(),1));if manager.0.is_null(){return Err(());}let name:Vec<u16>="Cloudflared\0".encode_utf16().collect();let handle=OpenServiceW(manager.0,name.as_ptr(),0x34);if handle.is_null(){Err(())}else{Ok(Owned(handle))}}}
 fn state(handle:&Owned)->Result<u32,()> {let mut s=ProcessStatus::default();let mut needed=0;unsafe{if QueryServiceStatusEx(handle.0,0,(&mut s as *mut ProcessStatus).cast(),std::mem::size_of::<ProcessStatus>() as u32,&mut needed)==0{Err(())}else{Ok(s.status.state)}}}
 pub(super) fn can_recover()->bool{open().is_ok()}
 pub(super) fn recover(stop:&AtomicBool)->Result<(),()> {
  let handle=open()?;let current=state(&handle)?;
  if current==4 {let mut out=ServiceStatus::default();unsafe{if ControlService(handle.0,1,&mut out)==0{return Err(());}}}
  for _ in 0..30 {if stop.load(Ordering::Acquire){return Err(());}match state(&handle)?{1=>{return unsafe{if StartServiceW(handle.0,0,std::ptr::null())!=0{Ok(())}else{Err(())}}},3|4=>std::thread::sleep(Duration::from_secs(1)),_=>return Err(())}}
  Err(())
 }
}
#[cfg(not(windows))] mod service {use super::*;pub(super) fn can_recover()->bool{false}pub(super) fn recover(_: &AtomicBool)->Result<(),()>{Err(())}}
#[cfg(test)] mod tests {
 use super::*;
 #[test]fn loopback_bootstrap_is_local_ready_but_never_public_ready(){
  let d=tempfile::tempdir().unwrap();std::fs::write(d.path().join("index.html"),"fixture").unwrap();let core=crate::RouterCore{store:Arc::new(crate::RouterStore::open_at(d.path().join("db")).unwrap()),chatgpt:Arc::default(),session:Arc::default(),completed_chatgpt_responses:Arc::default()};let host=crate::HostRuntime::default();let mut c=crate::mobile_http::MobileHttpConfig::controlled_host_acceptance(d.path().into());c.port=0;c.access_issuer.clear();c.access_audience.clear();c.access_jwks_url.clear();host.ensure_mobile(core.clone(),c.clone());c.port=host.mobile_address().unwrap().port();c.allowed_origin=format!("http://127.0.0.1:{}",c.port);c.allowed_host=format!("127.0.0.1:{}",c.port);host.ensure_mobile(core.clone(),c.clone());host.web.start(host.clone(),core.clone(),c);let until=Instant::now()+Duration::from_secs(5);while host.web.status().checked_at==0&&Instant::now()<until{std::thread::sleep(Duration::from_millis(50));}let s=host.web.status();host.shutdown(&core);assert!(s.local_ready);assert!(!s.public_ready);assert_eq!(s.state,"CONFIG_REQUIRED");
 }
 #[test]fn transient_failure_and_public_only_failure_never_restart(){let mut p=RecoveryPolicy::default();assert!(!p.tick(true,0));assert!(!p.tick(true,15));assert!(!p.tick(false,30));for n in 0..20{assert!(!p.tick(false,n*15));}}
 #[test]fn recovery_is_bounded_and_healthy_connection_resets_backoff(){let mut p=RecoveryPolicy::default();assert!(!p.tick(true,0));assert!(!p.tick(true,15));assert!(p.tick(true,30));for t in [45,60,90,120]{assert!(!p.tick(true,t));}assert!(p.tick(true,150));p.healthy();assert!(!p.tick(true,165));assert!(!p.tick(true,180));assert!(!p.tick(true,195));assert!(p.tick(true,210));}
}

/// Explicit desktop button only. Windows shows one UAC prompt; the resident
/// watchdog never elevates and receives only the existing service's 0x34 rights.
#[cfg(windows)]
pub(crate) fn request_setup(resource_dir:std::path::PathBuf)->Result<(),String>{
 use base64ct::{Base64,Encoding};use std::os::windows::process::CommandExt;
 let script=resource_dir.join("web-availability/setup-service-recovery.ps1");if !script.is_file(){return Err("WEB_RECOVERY_SETUP_MISSING".into());}
 let evidence=std::path::PathBuf::from(std::env::var_os("LOCALAPPDATA").ok_or("WEB_RECOVERY_SETUP_UNAVAILABLE")?).join("AIWorkRouter/data/web-recovery");
 let quote=|p:&std::path::Path|format!("'{}'",p.to_string_lossy().replace('\'',"''"));
 let command=format!(r#"$ErrorActionPreference='Stop';$owner=[Security.Principal.WindowsIdentity]::GetCurrent().User.Value;$script={};$evidence={};New-Item -ItemType Directory -Path $evidence -Force|Out-Null;try{{$setupArgs=@('-NoProfile','-ExecutionPolicy','Bypass','-File',('"'+$script+'"'),'-OwnerSid',$owner,'-EvidenceDirectory',('"'+$evidence+'"'));$p=Start-Process -FilePath $PSHOME\powershell.exe -ArgumentList $setupArgs -Verb RunAs -WindowStyle Hidden -Wait -PassThru;if($p.ExitCode -ne 0){{throw 'Setup did not complete'}}}}catch{{@{{status='ADMIN_SETUP_NOT_COMPLETED'}}|ConvertTo-Json|Set-Content -LiteralPath (Join-Path $evidence 'setup-result.json') -Encoding utf8}}"#,quote(&script),quote(&evidence));
 let bytes:Vec<u8>=command.encode_utf16().flat_map(u16::to_le_bytes).collect();
 let executable=std::path::PathBuf::from(std::env::var_os("WINDIR").ok_or("WEB_RECOVERY_SETUP_UNAVAILABLE")?).join("System32/WindowsPowerShell/v1.0/powershell.exe");
 if SETUP_PENDING.swap(true,Ordering::AcqRel){return Err("WEB_RECOVERY_SETUP_ALREADY_PENDING".into());}
 let child=std::process::Command::new(executable).args(["-NoProfile","-NonInteractive","-EncodedCommand",&Base64::encode_string(&bytes)]).creation_flags(0x08000000).spawn();
 match child {Ok(mut child)=>{std::thread::spawn(move||{let _=child.wait();SETUP_PENDING.store(false,Ordering::Release);});Ok(())},Err(_)=>{SETUP_PENDING.store(false,Ordering::Release);Err("WEB_RECOVERY_SETUP_UNAVAILABLE".into())}}
}
#[cfg(not(windows))]pub(crate) fn request_setup(_:std::path::PathBuf)->Result<(),String>{Err("WEB_RECOVERY_SETUP_UNAVAILABLE".into())}
