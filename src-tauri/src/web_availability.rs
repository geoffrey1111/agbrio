//! Desktop-owned availability watchdog. Reuses one Host and the installed
//! Cloudflared service; never reads credentials or changes tunnel routes.
use serde::Serialize;
use std::io::Read;
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
/// Cloudflared reconnects by itself, and an upstream proxy or network problem is
/// not fixed by a restart: restarting only drops the connections that still work.
/// Restart only after sustained tunnel failure, including an explicit Cloudflare
/// 1033 when local readiness is stale, and at most once an hour.
const TUNNEL_DOWN_SECS:u64=5*60;
const TUNNEL_RESTART_GAP_SECS:u64=60*60;
#[derive(Default)]
struct TunnelPolicy {down_since:Option<u64>,last:Option<u64>}
impl TunnelPolicy {
 fn tick(&mut self,eligible:bool,elapsed:u64)->bool {
  if !eligible {self.down_since=None;return false;}
  let since=*self.down_since.get_or_insert(elapsed);
  if elapsed.saturating_sub(since)<TUNNEL_DOWN_SECS || self.last.is_some_and(|last|elapsed.saturating_sub(last)<TUNNEL_RESTART_GAP_SECS){return false;}
  self.last=Some(elapsed);self.down_since=None;true
 }
}
#[derive(Default)]
struct PublicProbe {ready:bool,tunnel_missing:bool}
fn cloudflare_tunnel_missing(status:u16,server:Option<&str>,body:&str)->bool {
 status==530 && server.is_some_and(|s|s.eq_ignore_ascii_case("cloudflare"))
  && (body.trim().eq_ignore_ascii_case("error code: 1033") || body.contains("Error 1033"))
}
fn inspect_public(response:reqwest::blocking::Response,instance:&str)->PublicProbe {
 let ready=response.status().is_success()&&response.headers().get("x-aiwr-host-instance").and_then(|v|v.to_str().ok())==Some(instance);
 if response.status().as_u16()!=530{return PublicProbe{ready,tunnel_missing:false};}
 let server=response.headers().get("server").and_then(|v|v.to_str().ok()).map(str::to_owned);
 let mut body=String::new();let read=response.take(8192).read_to_string(&mut body).is_ok();
 PublicProbe{ready,tunnel_missing:read&&cloudflare_tunnel_missing(530,server.as_deref(),&body)}
}
fn tunnel_recovery_eligible(local:bool,tunnel:bool,public:&PublicProbe,allowed:bool)->bool {
 local&&!public.ready&&(!tunnel||public.tunnel_missing)&&allowed
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
   let since=Instant::now();let mut policy=TunnelPolicy::default();let mut local_policy=RecoveryPolicy::default();let mut count=0;
   while !guard.stop.load(Ordering::Acquire){
    let config=crate::mobile_http::managed_config(&config);
    host.ensure_mobile(core.clone(),config.clone());
    let local=local_client.get(format!("http://127.0.0.1:{}/v1/mobile/auth/session",config.port)).header("host",&config.allowed_host).send().is_ok_and(|r|r.status().is_success()&&r.headers().get("x-aiwr-host-instance").and_then(|v|v.to_str().ok())==Some(host.instance_id()));
    let legacy=crate::mobile_http::legacy_cloudflare_recovery(&config);
    let tunnel=legacy&&local_client.get("http://127.0.0.1:20241/ready").send().is_ok_and(|r|r.status().is_success());
    let allowed=legacy&&service::can_recover();
    let external_client=if crate::mobile_http::config_method(&config)=="TAILSCALE_SERVE"{&private_client}else{&client};
    let configured=config.allowed_origin.starts_with("https://");
    let public_probe=if configured {external_client.get(format!("{}/v1/mobile/auth/session",config.allowed_origin.trim_end_matches('/'))).header("cache-control","no-cache").send().map(|r|inspect_public(r,host.instance_id())).unwrap_or_default()}else{PublicProbe::default()};
    let public=public_probe.ready;
    let missing=!tunnel||public_probe.tunnel_missing;
    let mut state=if !configured{"CONFIG_REQUIRED"}else if public{"ONLINE"}else if !local{"LOCAL_RECOVERING"}else if legacy&&missing && !allowed{"ADMIN_SETUP_REQUIRED"}else if legacy&&missing{"TUNNEL_RECONNECTING"}else{"PUBLIC_UNREACHABLE"}.to_string();
    if local {local_policy.healthy();}
    if local_policy.tick(!local,since.elapsed().as_secs()) && !guard.stop.load(Ordering::Acquire){host.restart_mobile_listener(core.clone(),config.clone());}
    // A positive /ready can outlive a usable edge connection. Only explicit
    // Cloudflare 1033 overrides it; Access/DNS/origin errors do not.
    if policy.tick(tunnel_recovery_eligible(local,tunnel,&public_probe,allowed),since.elapsed().as_secs()) && !guard.stop.load(Ordering::Acquire){
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
 #[test]fn tunnel_restart_needs_five_minutes_down_and_is_at_most_hourly(){
  let mut p=TunnelPolicy::default();
  for t in (0..300).step_by(15){assert!(!p.tick(true,t));}
  assert!(p.tick(true,300));
  // Still down: no further restart within the hour, however long it stays down.
  for t in (315..3900).step_by(15){assert!(!p.tick(true,t),"t={t}");}
  assert!(p.tick(true,3900));
  // A recovery in between resets the down timer but not the hourly cap.
  let mut q=TunnelPolicy::default();for t in (0..=300).step_by(15){q.tick(true,t);}
  assert!(!q.tick(false,315));for t in (330..630).step_by(15){assert!(!q.tick(true,t));}
  assert!(!q.tick(true,700),"hourly cap");
  // Flapping below five minutes never restarts.
  let mut r=TunnelPolicy::default();for n in 0..40{assert!(!r.tick(n%10!=0,n*15));}
 }
 #[test]fn public_1033_overrides_stale_ready_but_keeps_delay_and_hourly_cap(){
  let listener=std::net::TcpListener::bind("127.0.0.1:0").unwrap();let address=listener.local_addr().unwrap();
  let server=std::thread::spawn(move||{use std::io::Write;let(mut socket,_)=listener.accept().unwrap();socket.set_read_timeout(Some(Duration::from_secs(2))).unwrap();let mut request=[0;2048];let _=socket.read(&mut request).unwrap();let body="error code: 1033";write!(socket,"HTTP/1.1 530 Unknown\r\nServer: cloudflare\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",body.len(),body).unwrap();});
  let response=reqwest::blocking::Client::builder().no_proxy().timeout(Duration::from_secs(2)).build().unwrap().get(format!("http://{address}/v1/mobile/auth/session")).send().unwrap();
  let public=inspect_public(response,"fixture");server.join().unwrap();assert!(!public.ready);assert!(public.tunnel_missing);
  assert!(!tunnel_recovery_eligible(false,true,&public,true));assert!(!tunnel_recovery_eligible(true,true,&public,false));
  let mut policy=TunnelPolicy::default();for t in (0..300).step_by(15){assert!(!policy.tick(tunnel_recovery_eligible(true,true,&public,true),t));}assert!(policy.tick(tunnel_recovery_eligible(true,true,&public,true),300));
  assert!(!policy.tick(true,600));assert!(!policy.tick(true,3899));assert!(policy.tick(true,3900));
 }
 #[test]fn generic_public_failures_never_override_ready_or_a_live_origin(){
  for(status,server,body)in[(403,Some("cloudflare"),"Error 1033"),(502,Some("cloudflare"),"Bad gateway"),(530,Some("cloudflare"),"error code: 1016"),(530,Some("origin"),"error code: 1033"),(530,None,"Error 1033")]{assert!(!cloudflare_tunnel_missing(status,server,body));}
  assert!(!tunnel_recovery_eligible(true,true,&PublicProbe::default(),true));assert!(!tunnel_recovery_eligible(true,false,&PublicProbe{ready:true,tunnel_missing:false},true));
  assert!(cloudflare_tunnel_missing(530,Some("Cloudflare"),"<h1>Error 1033</h1>"));
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
