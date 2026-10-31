//! Owner-local elevated cohort. Remote task creation cannot request Windows consent.
use super::elevation::{identity,Identity};
use serde_json::{json,Value};
use sha2::{Digest,Sha256};
use std::{fs,io::{BufRead,BufReader,Read,Write},net::{TcpListener,TcpStream,Shutdown},path::{Path,PathBuf},process::{Command,Stdio},sync::{Mutex,atomic::{AtomicBool,Ordering}},time::{Duration,Instant}};
struct Cohort{socket:TcpStream,identity:Identity,_locks:Vec<fs::File>}
impl Drop for Cohort{fn drop(&mut self){let _=self.socket.shutdown(Shutdown::Both);}}
static CURRENT:Mutex<Option<Cohort>>=Mutex::new(None);
static PREPARING:AtomicBool=AtomicBool::new(false);
fn exchange(socket:&mut TcpStream,value:Value)->Result<Value,String>{
 socket.set_read_timeout(Some(Duration::from_secs(3))).map_err(|_|"PI_ADMIN_BACKEND_DISCONNECTED")?;
 socket.set_write_timeout(Some(Duration::from_secs(3))).map_err(|_|"PI_ADMIN_BACKEND_DISCONNECTED")?;
 writeln!(socket,"{}",value).map_err(|_|"PI_ADMIN_BACKEND_DISCONNECTED")?;
 let mut text=String::new();let mut read=BufReader::with_capacity(1,socket);
 read.by_ref().take(8192).read_line(&mut text).map_err(|_|"PI_ADMIN_BACKEND_DISCONNECTED")?;
 serde_json::from_str(&text).map_err(|_|"PI_ADMIN_BACKEND_DISCONNECTED".into())
}
pub(super) fn status()->Value{
 if let Ok(owner)=identity(None){if owner.administrator{return json!({"ready":true,"mode":"VERIFIED_HOST_TOKEN","process":owner});}}
 if PREPARING.load(Ordering::Acquire){return json!({"ready":false,"preparing":true});}
 let Ok(mut guard)=CURRENT.try_lock()else{return json!({"ready":false,"busy":true});};
 if let Some(cohort)=guard.as_mut(){if exchange(&mut cohort.socket,json!({"operation":"PING"})).is_ok_and(|v|v["ready"]==true){return json!({"ready":true,"mode":"OWNER_PREPARED_COHORT","process":cohort.identity});}}
 *guard=None;json!({"ready":false,"mode":"NOT_PREPARED"})
}
pub(super) fn require_ready()->Result<(),String>{if status()["ready"]==true{Ok(())}else{Err("PI_ADMIN_BACKEND_PREPARE_ON_DESKTOP".into())}}
pub(super) fn shutdown(){if let Ok(mut guard)=CURRENT.lock(){*guard=None;}}
pub(super) fn launch(request:&Path,digest:&str)->Result<(),String>{
 let mut guard=CURRENT.lock().map_err(|_|"PI_ADMIN_BACKEND_UNAVAILABLE")?;
 let cohort=guard.as_mut().ok_or("PI_ADMIN_BACKEND_PREPARE_ON_DESKTOP")?;
 match exchange(&mut cohort.socket,json!({"operation":"LAUNCH","request":super::session::plain(request),"digest":digest})){
  Ok(v)if v["accepted"]==true=>Ok(()),_=>{*guard=None;Err("PI_ADMIN_BACKEND_DISCONNECTED".into())}
 }
}
pub(super) fn prepare()->Result<Value,String>{
 if status()["ready"]==true{return Ok(status());}
 if PREPARING.compare_exchange(false,true,Ordering::AcqRel,Ordering::Acquire).is_err(){return Err("PI_ADMIN_BACKEND_PREPARING".into());}
 struct Reset;impl Drop for Reset{fn drop(&mut self){PREPARING.store(false,Ordering::Release);}}
 let _reset=Reset;let cohort=prepare_native()?;
 *CURRENT.lock().map_err(|_|"PI_ADMIN_BACKEND_UNAVAILABLE")?=Some(cohort);
 Ok(json!({"ready":true,"mode":"OWNER_PREPARED_COHORT"}))
}
#[cfg(windows)]
fn prepare_native()->Result<Cohort,String>{
 use std::os::windows::{fs::OpenOptionsExt,process::CommandExt};
 let owner=identity(None)?;let task_helper=super::elevation::script()?;
 let helper=task_helper.with_file_name("cohort.ps1");
 if fs::read(&helper).map_err(|_|"PI_ELEVATION_HELPER_UNAVAILABLE")?!=include_bytes!("../../../tools/pi-adapter/cohort.ps1"){return Err("PI_ELEVATION_HELPER_CHANGED".into());}
 let base=PathBuf::from(std::env::var_os("LOCALAPPDATA").ok_or("PI_OWNER_UNAVAILABLE")?).join("AIWorkRouter/data/pi-elevation");
 fs::create_dir_all(&base).map_err(|_|"PI_ELEVATION_STORAGE_UNAVAILABLE")?;super::permissions::canonical_directory(&super::session::plain(&base))?;
 let directory=base.join(uuid::Uuid::new_v4().to_string());fs::create_dir(&directory).map_err(|_|"PI_ELEVATION_STORAGE_UNAVAILABLE")?;
 let listener=TcpListener::bind(("127.0.0.1",0)).map_err(|_|"PI_ELEVATION_CHANNEL_UNAVAILABLE")?;listener.set_nonblocking(true).map_err(|_|"PI_ELEVATION_CHANNEL_UNAVAILABLE")?;
 let image=std::env::current_exe().map_err(|_|"PI_OWNER_UNAVAILABLE")?;
 let nonce=format!("{}{}",uuid::Uuid::new_v4().simple(),uuid::Uuid::new_v4().simple());
 let bytes=serde_json::to_vec(&json!({"ownerSid":owner.owner_sid,"callerPid":owner.pid,"callerCreatedAt":owner.created_at,"callerImage":super::session::plain(&image),"callerHash":format!("{:x}",Sha256::digest(fs::read(&image).map_err(|_|"PI_OWNER_UNAVAILABLE")?)),"createdAt":std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_millis()as u64,"port":listener.local_addr().map_err(|_|"PI_ELEVATION_CHANNEL_UNAVAILABLE")?.port(),"nonce":nonce,"helperHash":format!("{:x}",Sha256::digest(include_bytes!("../../../tools/pi-adapter/elevated.ps1")))})).map_err(|_|"PI_ELEVATION_REQUEST_INVALID")?;
 let digest=format!("{:x}",Sha256::digest(&bytes));let request=directory.join("request.json");fs::write(&request,bytes).map_err(|_|"PI_ELEVATION_STORAGE_UNAVAILABLE")?;
 let launcher=directory.join("launch.ps1");
 fs::write(&launcher,r#"param($Shell,$Helper,$Request,$Digest) try { $quote={param($v) if($v.Contains('"')){throw 'PI_ARGUMENT_INVALID'};'"'+$v+'"'}; $launchArguments='-NoProfile -NonInteractive -ExecutionPolicy Bypass -File '+(& $quote $Helper)+' -Request '+(& $quote $Request)+' -Digest '+$Digest; Start-Process -FilePath $Shell -Verb RunAs -ArgumentList $launchArguments -WindowStyle Hidden | Out-Null; exit 0 } catch { exit 1223 }"#).map_err(|_|"PI_ELEVATION_STORAGE_UNAVAILABLE")?;
 let locks=[&request,&helper,&task_helper].iter().map(|p|fs::OpenOptions::new().read(true).share_mode(1).open(p).map_err(|_|"PI_ELEVATION_HELPER_CHANGED".to_string())).collect::<Result<Vec<_>,_>>()?;
 let shell=PathBuf::from(std::env::var_os("ProgramFiles").ok_or("PI_POWERSHELL_7_REQUIRED")?).join("PowerShell/7/pwsh.exe");if !shell.is_file(){return Err("PI_POWERSHELL_7_REQUIRED".into());}
 let mut command=Command::new(&shell);command.env_clear();
 for key in ["SystemRoot","WINDIR","USERPROFILE","APPDATA","LOCALAPPDATA","ProgramFiles","ProgramData","PATH","PATHEXT","ComSpec","TEMP","TMP"]{if let Some(value)=std::env::var_os(key){command.env(key,value);}}
 let mut child=command.args(["-NoProfile","-NonInteractive","-ExecutionPolicy","Bypass","-File"]).arg(&launcher).arg(&shell).arg(&helper).arg(&request).arg(&digest).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).creation_flags(0x08000000).spawn().map_err(|_|"PI_ELEVATION_NOT_COMPLETED")?;
 let until=Instant::now()+Duration::from_secs(600);
 while Instant::now()<until{
  if child.try_wait().map_err(|_|"PI_ELEVATION_NOT_COMPLETED")?.is_some_and(|s|!s.success()){return Err("PI_UAC_CANCELLED".into());}
  if directory.join("failed.json").exists(){return Err("PI_ADMIN_BACKEND_FAILED".into());}
  if let Ok((mut socket,_))=listener.accept(){
   socket.set_nonblocking(false).map_err(|_|"PI_ELEVATION_CHANNEL_UNAVAILABLE")?;
   socket.set_read_timeout(Some(Duration::from_secs(3))).map_err(|_|"PI_ELEVATION_CHANNEL_UNAVAILABLE")?;
   let mut text=String::new();let mut reader=BufReader::with_capacity(1,&mut socket);if reader.by_ref().take(8192).read_line(&mut text).is_err(){continue;}
   let Ok(proof)=serde_json::from_str::<Value>(&text)else{continue;};
   if proof["nonce"]!=nonce||proof["requestHash"]!=digest||proof["ownerSid"]!=owner.owner_sid||proof["administrator"]!=true{continue;}
   let pid=proof["pid"].as_u64().filter(|v|*v>0&&*v<=u32::MAX as u64).ok_or("PI_TOKEN_UNCONFIRMED")?as u32;
   let actual=super::elevation::identity_pid(pid)?;if !actual.administrator||actual.owner_sid!=owner.owner_sid||proof["createdAt"]!=actual.created_at{return Err("PI_TOKEN_UNCONFIRMED".into());}
   return Ok(Cohort{socket,identity:actual,_locks:locks});
  }
  std::thread::sleep(Duration::from_millis(40));
 }
 let _=child.kill();let _=child.wait();Err("PI_ELEVATION_NOT_COMPLETED".into())
}
#[cfg(not(windows))]
fn prepare_native()->Result<Cohort,String>{Err("PI_PLATFORM_UNVERIFIED".into())}

#[cfg(test)]
mod tests{
 use super::*;
 #[test]
 #[ignore="owned native elevated relay handshake/disconnect; no model task"]
 fn elevated_cohort_native_lifecycle(){
  assert_eq!(std::env::var("AGBRIO_PI_NATIVE_QA").as_deref(),Ok("1"));assert!(identity(None).unwrap().administrator);
  let mut owned=prepare_native().unwrap();let pid=owned.identity.pid;
  assert_eq!(exchange(&mut owned.socket,json!({"operation":"PING"})).unwrap()["ready"],true);
  assert!(super::super::elevation::identity_pid(pid).unwrap().administrator);
  *CURRENT.lock().unwrap()=Some(owned);
  let installation=super::super::installation().unwrap();
  let base=PathBuf::from(std::env::var_os("LOCALAPPDATA").unwrap()).join("AIWorkRouter/data/pi-executor");fs::create_dir_all(&base).unwrap();
  for _ in 0..2{
   let directory=base.join(uuid::Uuid::new_v4().to_string());fs::create_dir(&directory).unwrap();
   let argv=super::super::args(&installation,&directory);let mut command=Command::new(&installation.node);command.env_clear();
   for key in ["SystemRoot","WINDIR","USERPROFILE","HOME","APPDATA","LOCALAPPDATA","ProgramFiles","ProgramData","PATH","PATHEXT","ComSpec","TEMP","TMP"]{if let Some(value)=std::env::var_os(key){command.env(key,value);}}
   let (relay,mut input,output)=super::super::elevation::start(&command,&argv,&directory).unwrap();assert!(relay.identity.administrator);
   input.write_all(b"{\"id\":\"cohort-metadata\",\"type\":\"get_state\"}\n").unwrap();input.flush().unwrap();
   let(tx,rx)=std::sync::mpsc::sync_channel(1);std::thread::spawn(move||{let mut reader=BufReader::new(output);let mut text=String::new();while reader.read_line(&mut text).unwrap_or(0)>0{if serde_json::from_str::<Value>(&text).ok().is_some_and(|v|v["id"]=="cohort-metadata"&&v["success"]==true){let _=tx.send(());break;}text.clear();}});
   rx.recv_timeout(Duration::from_secs(20)).unwrap();drop(input);relay.stop();
   let closed=Instant::now()+Duration::from_secs(10);while !relay.closed(){assert!(Instant::now()<closed,"native task relay close receipt missing");std::thread::sleep(Duration::from_millis(40));}drop(relay);
   assert_eq!(directory.parent(),Some(base.as_path()));fs::remove_dir_all(&directory).unwrap();
  }
  shutdown();
  let until=Instant::now()+Duration::from_secs(10);while super::super::elevation::identity_pid(pid).is_ok(){assert!(Instant::now()<until,"owned cohort survived host disconnect");std::thread::sleep(Duration::from_millis(40));}
 }
}
