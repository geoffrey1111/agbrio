//! Desktop-owner MCP configuration. No prompts, credentials, or provider writes.
use serde::{Deserialize,Serialize,de::{self,DeserializeSeed,MapAccess,Visitor}};
use std::{fs::File,io::Read,path::{Path,PathBuf},process::{Command,Stdio},time::{Duration,Instant}};
#[derive(Clone,Debug,PartialEq)]enum Entry{Absent,Http(String),Other}
#[derive(Default)]struct Probe{entry:Option<Entry>,seen_servers:bool}
struct Servers<'a>(&'a mut Probe);
impl<'de> DeserializeSeed<'de> for Servers<'_>{type Value=();fn deserialize<D:serde::Deserializer<'de>>(self,d:D)->Result<(),D::Error>{d.deserialize_map(self)}}
impl<'de> Visitor<'de> for Servers<'_>{type Value=();fn expecting(&self,f:&mut std::fmt::Formatter)->std::fmt::Result{f.write_str("MCP server map")}fn visit_map<A:MapAccess<'de>>(self,mut a:A)->Result<(),A::Error>{while let Some(key)=a.next_key::<String>()?{if key=="agbrio"{if self.0.entry.is_some(){return Err(de::Error::custom("DUPLICATE_MCP_ENTRY"));}#[derive(Deserialize)]struct Target{#[serde(default)]r#type:Option<String>,#[serde(default)]url:Option<String>}
let t=a.next_value::<Target>()?;self.0.entry=Some(if matches!(t.r#type.as_deref(),Some("http"|"streamable-http")){t.url.map(Entry::Http).unwrap_or(Entry::Other)}else{Entry::Other});continue;}a.next_value::<de::IgnoredAny>()?;}if self.0.entry.is_none(){self.0.entry=Some(Entry::Absent);}Ok(())}}
impl<'de> Visitor<'de> for &mut Probe{type Value=();fn expecting(&self,f:&mut std::fmt::Formatter)->std::fmt::Result{f.write_str("Claude user settings")}fn visit_map<A:MapAccess<'de>>(self,mut a:A)->Result<(),A::Error>{while let Some(key)=a.next_key::<String>()?{if key=="mcpServers"{if self.seen_servers{return Err(de::Error::custom("DUPLICATE_MCP_MAP"));}self.seen_servers=true;a.next_value_seed(Servers(self))?;continue;}a.next_value::<de::IgnoredAny>()?;}if self.entry.is_none(){self.entry=Some(Entry::Absent);}Ok(())}}
fn entry(path:&Path)->Result<Entry,String>{if !path.exists(){return Ok(Entry::Absent);}let file=File::open(path).map_err(|_|"CLAUDE_MCP_CONFIG_UNREADABLE")?;if file.metadata().map_err(|_|"CLAUDE_MCP_CONFIG_UNREADABLE")?.len()>8*1024*1024{return Err("CLAUDE_MCP_CONFIG_UNREADABLE".into());}let mut probe=Probe::default();let mut json=serde_json::Deserializer::from_reader(file.take(8*1024*1024));serde::Deserializer::deserialize_map(&mut json,&mut probe).map_err(|_|"CLAUDE_MCP_CONFIG_UNREADABLE")?;json.end().map_err(|_|"CLAUDE_MCP_CONFIG_UNREADABLE")?;probe.entry.ok_or_else(||"CLAUDE_MCP_CONFIG_UNREADABLE".into())}
fn same_endpoint(a:&str,b:&str)->bool{url::Url::parse(a).ok().zip(url::Url::parse(b).ok()).is_some_and(|(a,b)|a==b)}
trait Admin{fn add(&self,endpoint:&str)->Result<(),String>;}

struct OwnedContext(PathBuf);
impl Drop for OwnedContext{fn drop(&mut self){let _=std::fs::remove_dir(&self.0);}}
fn cli_context()->Result<OwnedContext,String>{
 let local=std::env::var_os("LOCALAPPDATA").map(PathBuf::from).ok_or("CLAUDE_MCP_CONTEXT_UNAVAILABLE")?;
 let base=local.join("AIWorkRouter/onboarding-cli");for parent in base.ancestors(){if parent.join(".mcp.json").exists(){return Err("CLAUDE_MCP_CONTEXT_OVERRIDE_PRESERVED".into());}}
 std::fs::create_dir_all(&base).map_err(|_|"CLAUDE_MCP_CONTEXT_UNAVAILABLE")?;let dir=base.join(uuid::Uuid::new_v4().to_string());std::fs::create_dir(&dir).map_err(|_|"CLAUDE_MCP_CONTEXT_UNAVAILABLE")?;Ok(OwnedContext(dir))
}
struct Cli{exe:PathBuf,cwd:PathBuf}
impl Admin for Cli{fn add(&self,endpoint:&str)->Result<(),String>{
 let mut cmd=Command::new(&self.exe);cmd.args(["mcp","add","--transport","http","--scope","user","agbrio",endpoint]).current_dir(&self.cwd).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null());
 #[cfg(windows)]{use std::os::windows::process::CommandExt;cmd.creation_flags(0x08000000);}
 let mut child=cmd.spawn().map_err(|_|"CLAUDE_MCP_ADMIN_UNAVAILABLE")?;let began=Instant::now();loop{match child.try_wait(){Ok(Some(exit))=>return if exit.success(){Ok(())}else{Err("CLAUDE_MCP_ADD_UNCONFIRMED".into())},Ok(None)if began.elapsed()<Duration::from_secs(20)=>std::thread::sleep(Duration::from_millis(25)),_=>{let _=child.kill();let _=child.wait();return Err("CLAUDE_MCP_ADD_UNCONFIRMED".into());}}}
}}
#[derive(Debug,Serialize)]#[serde(rename_all="camelCase")]pub(crate)struct SetupResult{pub phase:String,pub endpoint:String,pub reused:bool,pub target_session_verified:bool}
fn configure_at(path:&Path,endpoint:&str,admin:&dyn Admin)->Result<SetupResult,String>{
 let good=|reused|SetupResult{phase:"CONFIGURED".into(),endpoint:endpoint.into(),reused,target_session_verified:false};
 match entry(path)?{Entry::Http(existing)if same_endpoint(&existing,endpoint)=>return Ok(good(true)),Entry::Absent=>{},_=>return Err("CLAUDE_MCP_DIFFERENT_SERVER_PRESERVED".into())}
 let outcome=admin.add(endpoint);match entry(path){Ok(Entry::Http(existing))if same_endpoint(&existing,endpoint)=>Ok(good(false)),_=>Err(outcome.err().unwrap_or_else(||"CLAUDE_MCP_ADD_UNCONFIRMED".into()))}
}
fn config_path()->Result<PathBuf,String>{
 let home=std::env::var_os("USERPROFILE").map(PathBuf::from).ok_or("CLAUDE_MCP_CONFIG_UNREADABLE")?;Ok(std::env::var_os("CLAUDE_CONFIG_DIR").map(PathBuf::from).map_or_else(||home.join(".claude.json"),|p|p.join(".claude.json")))
}
/// A new setup uses this computer's own entrance. An existing public entry keeps working and
/// is left alone: changing it would discard the owner's sign-in.
fn setup_endpoint(public:&str,local:&str,path:&Path)->String{if matches!(entry(path),Ok(Entry::Http(existing))if same_endpoint(&existing,public)){public.to_owned()}else{local.to_owned()}}
fn host_endpoint(host:&crate::HostRuntime,path:&Path)->Result<String,String>{
 let configuration=host.mobile_configuration().ok_or("CLAUDE_MCP_ENDPOINT_NOT_READY")?;let public=crate::assistant_connection_endpoint::endpoint(Some(&configuration.allowed_origin),None).ok_or("CLAUDE_MCP_ENDPOINT_NOT_READY")?;
 Ok(setup_endpoint(&public,&crate::assistant_connection_endpoint::local(configuration.port),path))
}
pub(crate)fn configure(host:&crate::HostRuntime)->Result<SetupResult,String>{
 if !cfg!(windows){return Err("PLATFORM_NOT_YET_SUPPORTED".into());}static LOCK:std::sync::Mutex<()>=std::sync::Mutex::new(());let _lock=LOCK.try_lock().map_err(|_|"CLAUDE_MCP_SETUP_BUSY")?;
 let config=config_path()?;let endpoint=host_endpoint(host,&config)?;
 let exe=crate::claude_code_desktop::engine_at(&crate::claude_code_desktop::engine_roots())?;
 let context=cli_context()?;configure_at(&config,&endpoint,&Cli{exe,cwd:context.0.clone()})
}
#[cfg(test)]mod tests{use super::*;use std::cell::Cell;struct Fake<'a>{path:&'a Path,calls:Cell<usize>,fail:bool}impl Admin for Fake<'_>{fn add(&self,url:&str)->Result<(),String>{self.calls.set(self.calls.get()+1);if self.fail{return Err("CLAUDE_MCP_ADD_UNCONFIRMED".into());}std::fs::write(self.path,serde_json::json!({"mcpServers":{"agbrio":{"type":"http","url":url}}}).to_string()).unwrap();Ok(())}}
 #[test]fn adds_absent_exact_endpoint_and_reads_back_without_claiming_a_live_session(){let d=tempfile::tempdir().unwrap();let p=d.path().join("settings");let f=Fake{path:&p,calls:Cell::new(0),fail:false};let result=configure_at(&p,"https://fixture.invalid/mcp",&f).unwrap();assert!(!result.target_session_verified);assert!(!result.reused);assert_eq!(f.calls.get(),1);assert!(configure_at(&p,"https://fixture.invalid/mcp",&f).unwrap().reused);assert_eq!(f.calls.get(),1);}
 #[test]fn preserves_different_or_stdio_named_server(){let d=tempfile::tempdir().unwrap();let p=d.path().join("settings");for server in [serde_json::json!({"type":"http","url":"https://other.invalid/mcp"}),serde_json::json!({"type":"stdio","command":"never-execute"})]{let bytes=serde_json::json!({"private":"do not expose","mcpServers":{"agbrio":server}}).to_string();std::fs::write(&p,&bytes).unwrap();let f=Fake{path:&p,calls:Cell::new(0),fail:false};assert!(configure_at(&p,"https://fixture.invalid/mcp",&f).unwrap_err().contains("PRESERVED"));assert_eq!(f.calls.get(),0);assert_eq!(std::fs::read_to_string(&p).unwrap(),bytes);}}
 #[test]fn ignores_unrelated_values_and_does_not_accept_broken_projection(){let d=tempfile::tempdir().unwrap();let p=d.path().join("settings");std::fs::write(&p,r#"{"private":{"token":"never-return"},"mcpServers":{"other":{"secret":"never-return"},"agbrio":{"type":"http","url":"https://fixture.invalid/mcp","headers":{"secret":"never-return"}}}}"#).unwrap();assert_eq!(entry(&p).unwrap(),Entry::Http("https://fixture.invalid/mcp".into()));std::fs::write(&p,"broken").unwrap();assert!(entry(&p).is_err());}
 #[test]fn duplicate_names_and_broken_trailing_configuration_fail_closed(){let d=tempfile::tempdir().unwrap();let p=d.path().join("settings");for text in [r#"{"mcpServers":{"agbrio":{"type":"http","url":"https://fixture.invalid/mcp"},"agbrio":{"type":"stdio"}}}"#,r#"{"mcpServers":{},"mcpServers":{}}"#,r#"{"mcpServers":{}} trailing"#]{std::fs::write(&p,text).unwrap();assert!(entry(&p).is_err());}}
 #[test]fn a_new_setup_uses_the_local_entrance_and_an_existing_public_entry_stays(){let d=tempfile::tempdir().unwrap();let p=d.path().join("settings");let (public,local)=("https://fixture.invalid/mcp","http://127.0.0.1:47114/mcp");assert_eq!(setup_endpoint(public,local,&p),local);std::fs::write(&p,serde_json::json!({"mcpServers":{"agbrio":{"type":"http","url":public}}}).to_string()).unwrap();assert_eq!(setup_endpoint(public,local,&p),public);std::fs::write(&p,serde_json::json!({"mcpServers":{"agbrio":{"type":"http","url":local}}}).to_string()).unwrap();assert_eq!(setup_endpoint(public,local,&p),local);std::fs::write(&p,serde_json::json!({"mcpServers":{"agbrio":{"type":"http","url":"https://other.invalid/mcp"}}}).to_string()).unwrap();let f=Fake{path:&p,calls:Cell::new(0),fail:false};assert!(configure_at(&p,&setup_endpoint(public,local,&p),&f).unwrap_err().contains("PRESERVED"));assert_eq!(f.calls.get(),0);}
 #[cfg(windows)]#[test]fn the_login_console_reports_the_exit_code_and_refuses_unquotable_paths(){let d=tempfile::tempdir().unwrap();let shell=std::path::PathBuf::from(std::env::var_os("SystemRoot").unwrap()).join("System32/WindowsPowerShell/v1.0/powershell.exe");for (code,expected) in [("0",true),("7",false)]{let status=console_command(&shell,&format!("-NoProfile -Command exit {code}"),d.path()).unwrap().status().unwrap();assert_eq!(status.success(),expected);if !expected{assert_eq!(status.code(),Some(7));}}assert!(console_command(Path::new(r#"C:\a"b\claude.exe"#),"mcp",d.path()).is_err());assert!(console_command(Path::new(r"C:\%PATH%\claude.exe"),"mcp",d.path()).is_err());}
 #[test]fn failed_add_is_not_claimed_as_success(){let d=tempfile::tempdir().unwrap();let p=d.path().join("settings");let f=Fake{path:&p,calls:Cell::new(0),fail:true};assert!(configure_at(&p,"https://fixture.invalid/mcp",&f).is_err());assert_eq!(f.calls.get(),1);assert!(!p.exists());}
}

#[derive(Clone,Default,Serialize)]#[serde(rename_all="camelCase")]pub(crate)struct LoginStatus{pub phase:String,pub endpoint:String,pub target_session_verified:bool}
static LOGIN:std::sync::Mutex<Option<LoginStatus>>=std::sync::Mutex::new(None);
pub(crate)fn login_status()->Result<LoginStatus,String>{Ok(LOGIN.lock().map_err(|_|"CLAUDE_MCP_SETUP_BUSY")?.clone().unwrap_or(LoginStatus{phase:"NOT_STARTED".into(),endpoint:String::new(),target_session_verified:false}))}
fn set_login(phase:&str){if let Ok(mut state)=LOGIN.lock(){if let Some(status)=state.as_mut(){status.phase=phase.into();}}}
/// Claude Code refuses to sign in without a terminal. `start /wait` always gives it a console of
/// its own, whatever handles this windowed process has. On one line cmd would report `start`'s
/// own success, so the exit code is read after the wait, which needs delayed expansion.
#[cfg(windows)]
fn console_command(executable:&Path,arguments:&str,cwd:&Path)->Result<Command,String>{
 use std::os::windows::process::CommandExt;
 let path=executable.to_str().filter(|path|!path.contains(['"','%','!'])).ok_or("CLAUDE_MCP_ADMIN_UNAVAILABLE")?;
 let mut cmd=Command::new(std::env::var_os("ComSpec").unwrap_or_else(||"cmd.exe".into()));
 cmd.raw_arg(format!("/d /v:on /c \"start \"Agbrio\" /wait \"{path}\" {arguments} & exit !errorlevel!\"")).current_dir(cwd).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).creation_flags(0x08000000);
 Ok(cmd)
}
#[cfg(not(windows))]
fn console_command(_:&Path,_:&str,_:&Path)->Result<Command,String>{Err("PLATFORM_NOT_YET_SUPPORTED".into())}
pub(crate)fn start_login(host:&crate::HostRuntime)->Result<SetupResult,String>{
 static START:std::sync::Mutex<()>=std::sync::Mutex::new(());let _start=START.try_lock().map_err(|_|"CLAUDE_MCP_SETUP_BUSY")?;
 let expected=host_endpoint(host,&config_path()?)?;
 let pending=login_status()?;if pending.phase=="AUTHORIZING"{if pending.endpoint!=expected{return Err("CLAUDE_MCP_PREVIOUS_LOGIN_PENDING".into());}return Ok(SetupResult{phase:pending.phase,endpoint:expected,reused:true,target_session_verified:false});}
 let mut configured=configure(host)?;
 {let mut state=LOGIN.lock().map_err(|_|"CLAUDE_MCP_SETUP_BUSY")?;if state.as_ref().is_some_and(|s|s.phase=="AUTHORIZING"){if state.as_ref().is_some_and(|s|s.endpoint!=configured.endpoint){return Err("CLAUDE_MCP_PREVIOUS_LOGIN_PENDING".into());}configured.phase="AUTHORIZING".into();return Ok(configured);}*state=Some(LoginStatus{phase:"AUTHORIZING".into(),endpoint:configured.endpoint.clone(),target_session_verified:false});}
 let executable=match crate::claude_code_desktop::engine_at(&crate::claude_code_desktop::engine_roots()){Ok(path)=>path,Err(error)=>{set_login("MANUAL_AUTH_REQUIRED");return Err(error)}};
 let context=match cli_context(){Ok(context)=>context,Err(error)=>{set_login("MANUAL_AUTH_REQUIRED");return Err(error)}};
 std::thread::spawn(move||{let _context=context;let Ok(mut cmd)=console_command(&executable,"mcp login agbrio",&_context.0)else{set_login("MANUAL_AUTH_REQUIRED");return;};
 let Ok(mut child)=cmd.spawn()else{set_login("MANUAL_AUTH_REQUIRED");return;};let start=Instant::now();loop{match child.try_wait(){Ok(Some(status))=>{set_login(if status.success(){"AUTHORIZED"}else{"MANUAL_AUTH_REQUIRED"});return},Ok(None)if start.elapsed()<Duration::from_secs(300)=>std::thread::sleep(Duration::from_millis(200)),_=>{let _=child.kill();let _=child.wait();set_login("AUTHORIZATION_UNCONFIRMED");return}}}});
 configured.phase="AUTHORIZING".into();Ok(configured)
}
