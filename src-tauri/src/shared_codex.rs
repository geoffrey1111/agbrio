//! Owner-local native gateway with explicit prepare/activate/disable transactions.
//! Preparing never changes Desktop's persistent endpoint or certificate trust.
use serde::{Serialize,Deserialize};
use std::{path::{Path,PathBuf},sync::{Mutex,OnceLock},process::Command,fs};
use sha2::{Digest,Sha256};
#[derive(Clone)]struct Assets{source:PathBuf,node:PathBuf}
static ASSETS:OnceLock<Assets>=OnceLock::new();static START:Mutex<()>=Mutex::new(());
// Serialize the whole preparation, including native pinning, with the installed
// entry. Locking only the short controller leaves a cross-process staging race.
#[cfg(windows)]struct OwnerMutex(*mut std::ffi::c_void);
#[cfg(windows)]mod owner_mutex_api{
 #[link(name="kernel32")]unsafe extern "system"{
  pub fn CreateMutexW(attributes:*mut std::ffi::c_void,initial_owner:i32,name:*const u16)->*mut std::ffi::c_void;
  pub fn WaitForSingleObject(handle:*mut std::ffi::c_void,milliseconds:u32)->u32;
  pub fn ReleaseMutex(handle:*mut std::ffi::c_void)->i32;
  pub fn CloseHandle(handle:*mut std::ffi::c_void)->i32;
 }
}
#[cfg(windows)]impl OwnerMutex{
 fn acquire()->Result<Self,String>{Self::at(&folder()?)}
 fn at(directory:&Path)->Result<Self,String>{
  let key=directory.to_string_lossy().replace('/',"\\");let hash=format!("{:x}",Sha256::digest(key.as_bytes()));
  let name:Vec<u16>=format!("Local\\Agbrio.SharedCodex.{hash}").encode_utf16().chain(Some(0)).collect();
  let handle=unsafe{owner_mutex_api::CreateMutexW(std::ptr::null_mut(),0,name.as_ptr())};
  if handle.is_null(){return Err("SHARED_START_UNAVAILABLE".into())}
  let wait=unsafe{owner_mutex_api::WaitForSingleObject(handle,0)};
  if wait!=0&&wait!=0x80{unsafe{owner_mutex_api::CloseHandle(handle)};return Err("SHARED_START_UNAVAILABLE".into())}
  Ok(Self(handle))
 }
}
#[cfg(windows)]impl Drop for OwnerMutex{fn drop(&mut self){unsafe{owner_mutex_api::ReleaseMutex(self.0);owner_mutex_api::CloseHandle(self.0);}}}
#[derive(Clone,Serialize,Deserialize)]#[serde(rename_all="camelCase")]
struct Settings{enabled:bool,#[serde(default)]format:u32,#[serde(default)]phase:String,binary:PathBuf,version:String,owner_sid:String,port:u16,instance:String,#[serde(default)]desktop:Option<PathBuf>}
#[derive(Serialize,Deserialize)]#[serde(rename_all="camelCase")]
pub(crate)struct Status{pub enabled:bool,pub ready:bool,pub desktop_connected:bool,pub state:String,pub update_pending:bool,#[serde(default)]pub launch_progress:Option<LaunchProgress>}
#[derive(Serialize,Deserialize)]#[serde(rename_all="camelCase")]
pub(crate)struct LaunchProgress{step:String,state:String,error_code:Option<String>,updated_at:u64}
fn launch_progress()->Option<LaunchProgress>{let v:LaunchProgress=serde_json::from_slice(&fs::read(folder().ok()?.join("launch-progress.json")).ok()?).ok()?;
 matches!(v.step.as_str(),"CHECKING"|"LAUNCHING"|"CONNECTING"|"SHOWING"|"CONNECTED"|"FAILED").then_some(v)
}
fn folder()->Result<PathBuf,String>{Ok(PathBuf::from(std::env::var_os("LOCALAPPDATA").ok_or("SHARED_LOCAL_DATA_UNAVAILABLE")?).join("AIWorkRouter/codex-shared"))}
fn settings()->Result<Option<Settings>,String>{let p=folder()?.join("settings.json");if !p.exists(){return Ok(None)}serde_json::from_slice(&fs::read(p).map_err(|_|"SHARED_SETTINGS_UNAVAILABLE")?).map(Some).map_err(|_|"SHARED_SETTINGS_INVALID".into())}
fn tool_path(path:PathBuf)->PathBuf{
 #[cfg(windows)]{let text=path.to_string_lossy();if let Some(drive)=text.strip_prefix(r"\\?\"){if drive.as_bytes().get(1)==Some(&b':'){return PathBuf::from(drive)}}}
 path
}
pub(crate)fn configure(source:PathBuf,node:PathBuf){let _=ASSETS.set(Assets{source:tool_path(source),node:tool_path(node)});}
#[cfg(windows)]fn hidden(c:&mut Command){use std::os::windows::process::CommandExt;c.creation_flags(0x08000000);}
#[cfg(not(windows))]fn hidden(_: &mut Command){}
fn health(s:&Settings)->Option<serde_json::Value>{if s.format!=2{return None}let client=reqwest::blocking::Client::builder().no_proxy().timeout(std::time::Duration::from_secs(2)).build().ok()?;let v=client.get(format!("http://127.0.0.1:{}/health",s.port)).send().ok()?.json::<serde_json::Value>().ok()?;(v["instance"].as_str()==Some(&s.instance)).then_some(v)}
pub(crate)fn status()->Status{
 let fallback=Status{enabled:false,ready:false,desktop_connected:false,state:"INDEPENDENT".into(),update_pending:false,launch_progress:None};
 let Ok(Some(s))=settings()else{return fallback};if s.phase=="RECOVERY_REQUIRED"{return Status{state:"RECOVERY_REQUIRED".into(),..fallback}}if s.format!=2{return Status{state:if s.enabled{"LEGACY_REPAIR_REQUIRED"}else{"INDEPENDENT"}.into(),..fallback}};
 let h=health(&s);let ready=h.as_ref().is_some_and(|h|h["ready"]==true);let desktop=h.as_ref().is_some_and(|h|h["desktopConnected"]==true);
 Status{enabled:s.enabled,ready,desktop_connected:desktop,state:if s.phase=="RECOVERY_REQUIRED"{"RECOVERY_REQUIRED"}else if s.phase=="PREPARED"&&ready{"SHARED_PREPARED"}else if s.enabled&&desktop{"SHARED_CONNECTED"}else if s.enabled{"SHARED_RECONNECTING"}else{"INDEPENDENT"}.into(),update_pending:runtime_update_pending(),launch_progress:launch_progress()}
}
const SHARED_ASSET_NAMES:[&str;19]=["runtime.mjs","transport.mjs","peer-owner.ps1","desktop-runtime.ps1","desktop-state.ps1","native-runtime.ps1","native-cohort.mjs","backend-recovery.mjs","code-mode-probe.mjs","peer-service.ps1","control.mjs","environment.ps1","owned-processes.ps1","desktop-identity.ps1","launch.ps1","launch-entry.mjs","state-file.mjs","return-normal.ps1","shared-offline-recovery.ps1"];
fn resource_stamp(source:&Path,node:&Path,ws:&Path)->Result<String,String>{
 let mut digest=Sha256::new();
 for name in SHARED_ASSET_NAMES{digest.update(fs::read(source.join(name)).map_err(|_|"SHARED_RUNTIME_ASSET_MISSING")?);}
 digest.update(fs::read(node).map_err(|_|"SHARED_NODE_MISSING")?);
 for name in ["index.js","package.json"]{digest.update(fs::read(ws.join(name)).map_err(|_|"SHARED_WS_MISSING")?);}
 let mut files:Vec<_>=fs::read_dir(ws.join("lib")).map_err(|_|"SHARED_WS_MISSING")?.collect::<Result<_,_>>().map_err(|_|"SHARED_WS_MISSING")?;
 files.sort_by_key(|f|f.file_name());for f in files{let bytes=fs::read(f.path()).map_err(|_|"SHARED_WS_MISSING")?;digest.update(f.file_name().to_string_lossy().as_bytes());digest.update((bytes.len() as u64).to_le_bytes());digest.update(bytes);}Ok(format!("{:x}",digest.finalize()))
}
fn asset_stamp(a:&Assets)->Result<String,String>{resource_stamp(&a.source,&a.node,&a.source.parent().ok_or("SHARED_RUNTIME_ASSET_MISSING")?.join("browser-executor/node_modules/ws"))}
fn cached_resources_match(root:&Path,stamp:&str)->bool{
 fs::read_to_string(root.join("version")).ok().as_deref()==Some(stamp)&&resource_stamp(root,&root.join("node.exe"),&root.join("ws")).ok().as_deref()==Some(stamp)
}
fn live_assets_stamp(source:&Path,node:&Path,ws:&Path)->Option<String>{
 let mut digest=Sha256::new();for name in ["runtime.mjs","transport.mjs","native-cohort.mjs","backend-recovery.mjs","peer-service.ps1"]{digest.update(fs::read(source.join(name)).ok()?);}digest.update(fs::read(node).ok()?);
 for name in ["index.js","package.json"]{digest.update(fs::read(ws.join(name)).ok()?);}
 let mut files:Vec<_>=fs::read_dir(ws.join("lib")).ok()?.collect::<Result<_,_>>().ok()?;files.sort_by_key(|f|f.file_name());
 for file in files{digest.update(file.file_name().to_string_lossy().as_bytes());digest.update(fs::read(file.path()).ok()?);}Some(format!("{:x}",digest.finalize()))
}
fn runtime_update_pending()->bool{
 let Some(a)=ASSETS.get()else{return false};let Some(root)=folder().ok().map(|f|f.join("runtime"))else{return true};
 let Some(ws)=a.source.parent().map(|p|p.join("browser-executor/node_modules/ws"))else{return true};
 // Control/environment scripts execute from the installed package, not the
 // resident supervisor. Updating launch UI must not require ending active turns.
 let expected=live_assets_stamp(&a.source,&a.node,&ws);expected.is_none()||expected!=live_assets_stamp(&root,&root.join("node.exe"),&root.join("ws"))
}
fn same_supervisor_identity(saved:&serde_json::Value,current:&serde_json::Value,expected_image:&str,expected_owner:&str)->bool{
 let normalized=|p:&str|p.replace('/',"\\").trim_start_matches("\\\\?\\").to_ascii_lowercase();
 saved["pid"].as_u64().is_some_and(|id|id>0&&current["pid"].as_u64()==Some(id))&&saved["createdAt"].as_str().is_some_and(|v|!v.is_empty()&&current["createdAt"].as_str()==Some(v))&&current["ownerSid"].as_str()==Some(expected_owner)&&saved["ownerSid"].as_str()==Some(expected_owner)&&current["imagePath"].as_str().is_some_and(|p|normalized(p)==normalized(expected_image))&&saved["imagePath"].as_str().is_some_and(|p|normalized(p)==normalized(expected_image))
}
#[cfg(windows)]fn supervisor_alive()->bool{
 let Ok(folder)=folder()else{return false;};let Some(s)=settings().ok().flatten()else{return false;};let Some(saved)=fs::read(folder.join("ready.json")).ok().and_then(|b|serde_json::from_slice::<serde_json::Value>(&b).ok()).and_then(|v|v.get("supervisorIdentity").cloned())else{return false;};let Some(pid)=saved["pid"].as_u64().filter(|p|*p>0&&*p<=u32::MAX as u64)else{return false;};
 let mut query=Command::new(PathBuf::from(std::env::var_os("WINDIR").unwrap_or_default()).join("System32/WindowsPowerShell/v1.0/powershell.exe"));query.args(["-NoProfile","-NonInteractive","-ExecutionPolicy","Bypass","-File"]).arg(folder.join("runtime/peer-service.ps1")).arg("-QueryPid").arg(pid.to_string());hidden(&mut query);
 let Some(current)=query.output().ok().filter(|v|v.status.success()).and_then(|v|serde_json::from_slice::<serde_json::Value>(&v.stdout).ok())else{return false;};same_supervisor_identity(&saved,&current,&folder.join("runtime/node.exe").to_string_lossy(),&s.owner_sid)
}
#[cfg(not(windows))]fn supervisor_alive()->bool{false}

fn reuse_preparation(health:&serde_json::Value,assets_changed:bool)->Result<bool,String>{
 if health["ready"]!=true{return Ok(false)}
 let active=health["activeTurns"].as_u64().ok_or("SHARED_CONNECTION_UNCERTAIN")?;
 if health["desktopConnected"]==true||active>0{
  if assets_changed{return Err("SHARED_RUNTIME_UPDATE_PENDING".into())}
  return Ok(true)
 }
 Ok(false)
}

fn pin(a:&Assets)->Result<PathBuf,String>{
 let directory=folder()?;let root=directory.join("runtime");
 if fs::symlink_metadata(&root).is_ok_and(|m|m.file_type().is_symlink()){return Err("SHARED_RUNTIME_LINK_REJECTED".into())}
 let stamp=asset_stamp(a)?;
 if cached_resources_match(&root,&stamp){return Ok(root)}
 if supervisor_alive(){return Err("SHARED_RUNTIME_UPDATE_PENDING".into())}
 // Use the same checked staging/promotion/rollback transaction as the installed
 // entry. Do not leave a second, less complete Rust copy implementation.
 let mut command=Command::new(&a.node);
 command.arg(a.source.join("launch-entry.mjs")).arg("--stage-only").arg(&a.source).arg(&directory).arg(&a.node).arg(&stamp);hidden(&mut command);
 let output=command.output().map_err(|_|"SHARED_RUNTIME_STAGE_FAILED")?;
 if !output.status.success(){let code=String::from_utf8_lossy(&output.stderr).trim().to_owned();return Err(if code.starts_with("SHARED_")&&code.len()<128{code}else{"SHARED_RUNTIME_STAGE_FAILED".into()})}
 let value:serde_json::Value=serde_json::from_slice(&output.stdout).map_err(|_|"SHARED_RUNTIME_STAGE_FAILED")?;
 if value["staged"]!=true{return Err("SHARED_RUNTIME_STAGE_FAILED".into())}
 Ok(root)
}
fn control(action:&str,binary:Option<&Path>,version:Option<&str>,desktop:Option<&Path>)->Result<Status,String>{let a=ASSETS.get().ok_or("SHARED_RUNTIME_NOT_CONFIGURED")?;fs::create_dir_all(folder()?).map_err(|_|"SHARED_SETTINGS_UNAVAILABLE")?;let input=folder()?.join("control-request.json");let value=serde_json::json!({"action":action,"directory":folder()?,"scripts":a.source,"binary":binary,"version":version,"desktop":desktop});fs::write(&input,serde_json::to_vec(&value).unwrap()).map_err(|_|"SHARED_SETTINGS_UNAVAILABLE")?;let mut command=Command::new(&a.node);command.arg(a.source.join("control.mjs")).arg(input);hidden(&mut command);let output=command.output().map_err(|_|"SHARED_LIFECYCLE_FAILED")?;if !output.status.success(){let error=String::from_utf8_lossy(&output.stderr).trim().to_string();return Err(if error.starts_with("SHARED_")&&error.len()<128{error}else{"SHARED_LIFECYCLE_FAILED".into()})}serde_json::from_slice(&output.stdout).map_err(|_|"SHARED_LIFECYCLE_FAILED".into())}
pub(crate)fn enabled()->bool{settings().ok().flatten().is_some_and(|s|s.enabled&&s.format==2&&s.phase=="ACTIVE")}
pub(crate)fn transport_command()->Result<Option<Command>,String>{let Some(s)=settings()?else{return Ok(None)};if s.format!=2&&s.enabled{return Err("SHARED_LEGACY_REPAIR_REQUIRED".into())}if s.phase=="RECOVERY_REQUIRED"{return Err("SHARED_ROLLBACK_INCOMPLETE".into())}if !s.enabled{return Ok(None)}if s.phase!="ACTIVE"||!health(&s).is_some_and(|h|h["ready"]==true){return Err("SHARED_RECONNECTING".into())}let root=folder()?.join("runtime");let mut command=Command::new(root.join("node.exe"));command.arg(root.join("transport.mjs")).arg(format!("ws://127.0.0.1:{}/rpc",s.port));hidden(&mut command);Ok(Some(command))}
pub(crate)fn disable()->Result<Status,String>{let _lock=START.lock().map_err(|_|"SHARED_START_UNAVAILABLE")?;#[cfg(windows)]let _owner=OwnerMutex::acquire()?;control("disable",None,None,None)}
pub(crate)fn launch()->Result<Status,String>{let _lock=START.lock().map_err(|_|"SHARED_START_UNAVAILABLE")?;#[cfg(windows)]let _owner=OwnerMutex::acquire()?;control("launch",None,None,None)}
#[cfg(windows)]pub(crate)fn setup()->Result<Status,String>{
 let _lock=START.lock().map_err(|_|"SHARED_START_UNAVAILABLE")?;
 let _owner=OwnerMutex::acquire()?;
 let a=ASSETS.get().ok_or("SHARED_RUNTIME_NOT_CONFIGURED")?;
 // Reuse active work without discovery/pinning. Both entry points must reject
 // ordinary Desktop or uncertain ownership before copying native resources.
 if let Some(s)=settings()?{if let Some(h)=health(&s){if reuse_preparation(&h,runtime_update_pending())?{return Ok(status())}}}
 control("prepare-check",None,None,None)?;
 let mut discovery=Command::new(PathBuf::from(std::env::var_os("WINDIR").ok_or("SHARED_OWNER_UNAVAILABLE")?).join("System32/WindowsPowerShell/v1.0/powershell.exe"));
 discovery.args(["-NoProfile","-NonInteractive","-ExecutionPolicy","Bypass","-File"]).arg(a.source.join("desktop-runtime.ps1")).arg("-Directory").arg(folder()?).arg("-ProbeNode").arg(&a.node);hidden(&mut discovery);
 let output=discovery.output().map_err(|_|"SHARED_DESKTOP_BACKEND_UNRESOLVED")?;
 if !output.status.success(){let text=String::from_utf8_lossy(&output.stderr);let category=text.split(|c:char|!c.is_ascii_uppercase()&&c!='_').find(|s|s.starts_with("SHARED_")).unwrap_or("SHARED_DESKTOP_BACKEND_UNRESOLVED");return Err(category.into())}
 let v:serde_json::Value=serde_json::from_slice(&output.stdout).map_err(|_|"SHARED_DESKTOP_BACKEND_UNRESOLVED")?;
 let binary=PathBuf::from(v["binary"].as_str().ok_or("SHARED_NATIVE_BINARY_UNVERIFIED")?);
 let version=v["version"].as_str().ok_or("SHARED_NATIVE_BINARY_UNVERIFIED")?;
 let desktop=PathBuf::from(v["desktop"].as_str().ok_or("SHARED_DESKTOP_BINARY_UNRESOLVED")?);
 // Candidate verification precedes retiring a live resident. Never restart tasks
 // or choose a second writer to work around an incomplete updated runtime.
 if let Some(s)=settings()?{
  if s.format==2&&!runtime_update_pending()&&health(&s).is_some_and(|h|h["ready"]==true){
   if s.version!=version||s.binary!=binary{return Err("SHARED_RUNTIME_UPDATE_PENDING".into())}
   return Ok(status())
  }
  control("disable",None,None,None)?;
 }
 pin(a)?;control("setup",Some(&binary),Some(version),Some(&desktop))
}
#[cfg(not(windows))]pub(crate)fn setup()->Result<Status,String>{Err("SHARED_WINDOWS_REQUIRED".into())}
#[cfg(test)]mod tests{use super::*;
 #[test]fn declared_assets_cover_every_packaged_shared_resource(){
  let source=Path::new(env!("CARGO_MANIFEST_DIR")).join("resources/codex-shared");
  let mut packaged:Vec<_>=fs::read_dir(source).unwrap().map(|e|e.unwrap().file_name().to_string_lossy().into_owned()).collect();
  let mut declared:Vec<_>=SHARED_ASSET_NAMES.iter().map(|s|s.to_string()).collect();packaged.sort();declared.sort();assert_eq!(packaged,declared);
 }
 #[test]fn cached_version_never_hides_missing_or_changed_runtime_files(){
  let base=Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap().join("runtime/shared-native-compatibility");
  let directory=base.join(format!("stamp-{}",uuid::Uuid::new_v4()));
  struct Fixture(PathBuf,PathBuf);impl Drop for Fixture{fn drop(&mut self){assert_eq!(self.0.parent(),Some(self.1.as_path()));fs::remove_dir_all(&self.0).unwrap();}}
  fs::create_dir_all(directory.join("ws/lib")).unwrap();let _fixture=Fixture(directory.clone(),base);
  for name in SHARED_ASSET_NAMES{fs::write(directory.join(name),name).unwrap();}
  for name in ["node.exe","ws/index.js","ws/package.json","ws/lib/test.js"]{fs::write(directory.join(name),"text fixture").unwrap();}
  let stamp=resource_stamp(&directory,&directory.join("node.exe"),&directory.join("ws")).unwrap();fs::write(directory.join("version"),&stamp).unwrap();
  assert!(cached_resources_match(&directory,&stamp));
  fs::remove_file(directory.join("state-file.mjs")).unwrap();assert!(!cached_resources_match(&directory,&stamp));fs::write(directory.join("state-file.mjs"),"state-file.mjs").unwrap();assert!(cached_resources_match(&directory,&stamp));
  fs::remove_file(directory.join("transport.mjs")).unwrap();assert!(!cached_resources_match(&directory,&stamp));fs::write(directory.join("transport.mjs"),"transport.mjs").unwrap();assert!(cached_resources_match(&directory,&stamp));
  fs::write(directory.join("ws/lib/test.js"),"changed fixture").unwrap();assert!(!cached_resources_match(&directory,&stamp));
  fs::write(directory.join("ws/lib/test.js"),"text fixture").unwrap();assert!(cached_resources_match(&directory,&stamp));fs::rename(directory.join("ws/lib/test.js"),directory.join("ws/lib/renamed.js")).unwrap();assert!(!cached_resources_match(&directory,&stamp));
 }
 #[test]fn preparation_reuses_active_work_without_discovery_and_defers_updates(){
  for h in [serde_json::json!({"ready":true,"desktopConnected":true,"activeTurns":0}),serde_json::json!({"ready":true,"desktopConnected":false,"activeTurns":2})]{assert_eq!(reuse_preparation(&h,false),Ok(true));assert_eq!(reuse_preparation(&h,true),Err("SHARED_RUNTIME_UPDATE_PENDING".into()));}
  assert_eq!(reuse_preparation(&serde_json::json!({"ready":true,"desktopConnected":false,"activeTurns":0}),true),Ok(false));
  assert_eq!(reuse_preparation(&serde_json::json!({"ready":true,"desktopConnected":false}),false),Err("SHARED_CONNECTION_UNCERTAIN".into()));
 }
 #[cfg(windows)]#[test]fn installed_entry_cannot_prepare_while_native_owner_mutex_is_held(){
  let directory=std::env::current_dir().unwrap().join("runtime").join(format!("mutex-{}",uuid::Uuid::new_v4()));
  let guard=OwnerMutex::at(&directory).unwrap();
  let quote=directory.to_string_lossy().replace("'","''");
  let script=format!("$d=[IO.Path]::GetFullPath('{quote}');$s=[Security.Cryptography.SHA256]::Create();$k=([BitConverter]::ToString($s.ComputeHash([Text.Encoding]::UTF8.GetBytes($d)))).Replace('-','').ToLowerInvariant();$s.Dispose();$m=[Threading.Mutex]::new($false,('Local\\Agbrio.SharedCodex.'+$k));$held=$m.WaitOne(0);if($held){{$m.ReleaseMutex();$m.Dispose();exit 0}};$m.Dispose();exit 42");
  let run=||{let mut c=Command::new(PathBuf::from(std::env::var_os("WINDIR").unwrap()).join("System32/WindowsPowerShell/v1.0/powershell.exe"));c.args(["-NoProfile","-NonInteractive","-Command",&script]);hidden(&mut c);c.status().unwrap().code().unwrap()};
  assert_eq!(run(),42);drop(guard);assert_eq!(run(),0);
 }

#[cfg(windows)]#[test]fn native_tool_paths_remove_only_local_drive_extended_prefix(){assert_eq!(tool_path(PathBuf::from(r"\\?\D:\apps\AI Work Router\node.exe")),PathBuf::from(r"D:\apps\AI Work Router\node.exe"));assert_eq!(tool_path(PathBuf::from(r"\\?\UNC\server\share")),PathBuf::from(r"\\?\UNC\server\share"));}#[test]fn recycled_pid_wrong_owner_and_wrong_image_never_match_supervisor(){let saved=serde_json::json!({"pid":42,"createdAt":"134056789123456789","ownerSid":"S-1-5-21-fixture","imagePath":"C:/fixture/node.exe"});assert!(same_supervisor_identity(&saved,&saved,"C:\\fixture\\node.exe","S-1-5-21-fixture"));for(field,value)in[("pid",serde_json::json!(43)),("createdAt",serde_json::json!("134056789123456790")),("ownerSid",serde_json::json!("S-1-other")),("imagePath",serde_json::json!("C:/another/node.exe"))]{let mut current=saved.clone();current[field]=value;assert!(!same_supervisor_identity(&saved,&current,"C:/fixture/node.exe","S-1-5-21-fixture"));}assert!(!same_supervisor_identity(&saved,&serde_json::Value::Null,"C:/fixture/node.exe","S-1-5-21-fixture"));}}
