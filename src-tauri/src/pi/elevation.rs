//! One-shot Windows runas relay. It never accepts caller-selected executables.
use serde::{Deserialize,Serialize};
use sha2::{Digest,Sha256};
use std::{collections::BTreeMap,fs,io::{BufRead,BufReader,Read,Write},net::{Shutdown,TcpListener,TcpStream},path::{Path,PathBuf},process::{Child,Command},time::{Duration,Instant}};

#[derive(Clone,Debug,Serialize,Deserialize)]
#[serde(rename_all="camelCase")]
pub(super) struct Identity {pub pid:u32,pub owner_sid:String,pub administrator:bool,pub created_at:String}

#[cfg(windows)]
fn inspect(handle:windows::Win32::Foundation::HANDLE,pid:u32)->Result<Identity,String>{
 use windows::Win32::{Foundation::{FILETIME,CloseHandle,HANDLE},Security::{GetTokenInformation,TokenElevation,TOKEN_ELEVATION,TOKEN_QUERY},System::Threading::{OpenProcessToken,GetProcessTimes}};
 let mut token=HANDLE::default();let mut info=TOKEN_ELEVATION::default();let mut size=0;
 unsafe{OpenProcessToken(handle,TOKEN_QUERY,&mut token).map_err(|_|"PI_TOKEN_UNCONFIRMED")?;}
 let checked=unsafe{GetTokenInformation(token,TokenElevation,Some((&mut info as *mut TOKEN_ELEVATION).cast()),std::mem::size_of_val(&info)as u32,&mut size)};
 unsafe{let _=CloseHandle(token);}checked.map_err(|_|"PI_TOKEN_UNCONFIRMED")?;
 let mut created=FILETIME::default();let mut exited=FILETIME::default();let mut kernel=FILETIME::default();let mut user=FILETIME::default();
 unsafe{GetProcessTimes(handle,&mut created,&mut exited,&mut kernel,&mut user).map_err(|_|"PI_PROCESS_IDENTITY_UNCONFIRMED")?;}
 let sid=crate::local_control::process_owner_sid(handle).map_err(|_|"PI_OWNER_UNAVAILABLE")?;
 Ok(Identity{pid,owner_sid:sid,administrator:info.TokenIsElevated!=0,created_at:((created.dwHighDateTime as u64)<<32|created.dwLowDateTime as u64).to_string()})
}
#[cfg(windows)]
pub(super) fn identity(child:Option<&Child>)->Result<Identity,String>{
 use std::os::windows::io::AsRawHandle;
 use windows::Win32::{Foundation::HANDLE,System::Threading::GetCurrentProcess};
 inspect(child.map(|c|HANDLE(c.as_raw_handle())).unwrap_or_else(||unsafe{GetCurrentProcess()}),child.map(Child::id).unwrap_or_else(std::process::id))
}
#[cfg(windows)]
pub(super) fn identity_pid(pid:u32)->Result<Identity,String>{
 use windows::Win32::{Foundation::CloseHandle,System::Threading::{OpenProcess,PROCESS_QUERY_LIMITED_INFORMATION}};
 let handle=unsafe{OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION,false,pid)}.map_err(|_|"PI_TOKEN_UNCONFIRMED")?;
 let result=inspect(handle,pid);unsafe{let _=CloseHandle(handle);}result
}
#[cfg(not(windows))]
pub(super) fn identity(_:Option<&Child>)->Result<Identity,String>{Err("PI_PLATFORM_UNVERIFIED".into())}
#[cfg(not(windows))]
pub(super) fn identity_pid(_:u32)->Result<Identity,String>{Err("PI_PLATFORM_UNVERIFIED".into())}

struct Input(TcpStream);
impl Write for Input{fn write(&mut self,b:&[u8])->std::io::Result<usize>{self.0.write(b)}fn flush(&mut self)->std::io::Result<()>{self.0.flush()}}
impl Drop for Input{fn drop(&mut self){let _=self.0.shutdown(Shutdown::Write);}}
pub(super) struct Elevated {socket:TcpStream,directory:PathBuf,hash:String,locks:Vec<fs::File>,pub identity:Identity}
impl Elevated {
 pub fn closed(&self)->bool{fs::read(self.directory.join("closed.json")).ok().and_then(|b|serde_json::from_slice::<serde_json::Value>(&b).ok()).is_some_and(|v|v["closed"]==true&&v["pid"]==self.identity.pid&&v["requestHash"]==self.hash)}
 pub fn stop(&self){let _=self.socket.shutdown(Shutdown::Both);}
}
impl Drop for Elevated{
 fn drop(&mut self){self.stop();self.locks.clear();if self.closed(){let base=PathBuf::from(std::env::var_os("LOCALAPPDATA").unwrap_or_default()).join("AIWorkRouter/data/pi-elevation");if self.directory.parent()==Some(base.as_path())&&uuid::Uuid::parse_str(self.directory.file_name().unwrap_or_default().to_str().unwrap_or("")).is_ok(){let _=fs::remove_dir_all(&self.directory);}}}
}
pub(super) fn script()->Result<PathBuf,String>{
 let exe=std::env::current_exe().map_err(|_|"PI_ELEVATION_HELPER_UNAVAILABLE")?;
 let packaged=exe.parent().ok_or("PI_ELEVATION_HELPER_UNAVAILABLE")?.join("pi-adapter/elevated.ps1");
 let path=if packaged.is_file(){packaged}else{PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../tools/pi-adapter/elevated.ps1")};
 let bytes=fs::read(&path).map_err(|_|"PI_ELEVATION_HELPER_UNAVAILABLE")?;
 if bytes!=include_bytes!("../../../tools/pi-adapter/elevated.ps1"){return Err("PI_ELEVATION_HELPER_CHANGED".into());}Ok(path)
}

#[cfg(windows)]
pub(super) fn start(command:&Command,argv:&[String],cwd:&Path)->Result<(Elevated,Box<dyn Write+Send>,Box<dyn Read+Send>),String>{
 use std::os::windows::fs::OpenOptionsExt;
 super::cohort::require_ready()?;
 let owner=identity(None)?;
 let directory=PathBuf::from(std::env::var_os("LOCALAPPDATA").ok_or("PI_OWNER_UNAVAILABLE")?).join("AIWorkRouter/data/pi-elevation").join(uuid::Uuid::new_v4().to_string());
 fs::create_dir_all(directory.parent().unwrap()).map_err(|_|"PI_ELEVATION_STORAGE_UNAVAILABLE")?;
 super::permissions::canonical_directory(&super::session::plain(directory.parent().unwrap()))?;
 fs::create_dir(&directory).map_err(|_|"PI_ELEVATION_STORAGE_UNAVAILABLE")?;
 let listener=TcpListener::bind(("127.0.0.1",0)).map_err(|_|"PI_ELEVATION_CHANNEL_UNAVAILABLE")?;
 listener.set_nonblocking(true).map_err(|_|"PI_ELEVATION_CHANNEL_UNAVAILABLE")?;
 let nonce=format!("{}{}",uuid::Uuid::new_v4().simple(),uuid::Uuid::new_v4().simple());
 let image=std::env::current_exe().map_err(|_|"PI_OWNER_UNAVAILABLE")?;
 let environment: BTreeMap<_,_>=command.get_envs().filter_map(|(k,v)|v.map(|v|(k.to_string_lossy().into_owned(),v.to_string_lossy().into_owned()))).collect();
 let spec=serde_json::json!({"ownerSid":owner.owner_sid,"callerPid":owner.pid,"callerCreatedAt":owner.created_at,"callerImage":super::session::plain(&image),"callerHash":format!("{:x}",Sha256::digest(fs::read(&image).map_err(|_|"PI_OWNER_UNAVAILABLE")?)),"createdAt":std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_millis()as u64,"node":command.get_program().to_string_lossy(),"argv":argv,"cwd":super::session::plain(cwd),"port":listener.local_addr().map_err(|_|"PI_ELEVATION_CHANNEL_UNAVAILABLE")?.port(),"nonce":nonce,"environment":environment});
 let bytes=serde_json::to_vec(&spec).map_err(|_|"PI_ELEVATION_REQUEST_INVALID")?;let hash=format!("{:x}",Sha256::digest(&bytes));let request=directory.join("request.json");fs::write(&request,bytes).map_err(|_|"PI_ELEVATION_STORAGE_UNAVAILABLE")?;
 let helper=script()?;
 let locks=vec![fs::OpenOptions::new().read(true).share_mode(1).open(&request).map_err(|_|"PI_ELEVATION_REQUEST_CHANGED")?,fs::OpenOptions::new().read(true).share_mode(1).open(&helper).map_err(|_|"PI_ELEVATION_HELPER_CHANGED")?];
 super::cohort::launch(&request,&hash)?;
 let until=Instant::now()+Duration::from_secs(30);
 while Instant::now()<until{
  if directory.join("failed.json").exists(){return Err("PI_ELEVATION_NOT_COMPLETED".into());}
  if let Ok((socket,_))=listener.accept(){
   socket.set_nonblocking(false).map_err(|_|"PI_ELEVATION_CHANNEL_UNAVAILABLE")?;
   socket.set_read_timeout(Some(Duration::from_secs(3))).map_err(|_|"PI_ELEVATION_CHANNEL_UNAVAILABLE")?;
   let mut reader=BufReader::with_capacity(1,socket.try_clone().map_err(|_|"PI_ELEVATION_CHANNEL_UNAVAILABLE")?);let mut line=String::new();
   if reader.by_ref().take(8192).read_line(&mut line).is_err(){continue;}
   let Ok(proof)=serde_json::from_str::<serde_json::Value>(&line)else{continue;};
   if proof["nonce"]!=nonce||proof["requestHash"]!=hash||proof["ownerSid"]!=owner.owner_sid||proof["administrator"]!=true{continue;}
   let pid=proof["pid"].as_u64().filter(|p|*p>0&&*p<=u32::MAX as u64).ok_or("PI_TOKEN_UNCONFIRMED")? as u32;
   socket.set_read_timeout(None).map_err(|_|"PI_ELEVATION_CHANNEL_UNAVAILABLE")?;
   let input=Input(socket.try_clone().map_err(|_|"PI_ELEVATION_CHANNEL_UNAVAILABLE")?);let output=socket.try_clone().map_err(|_|"PI_ELEVATION_CHANNEL_UNAVAILABLE")?;
   let identity=identity_pid(pid)?;if !identity.administrator||identity.owner_sid!=owner.owner_sid||proof["createdAt"]!=identity.created_at{return Err("PI_TOKEN_UNCONFIRMED".into());}
   return Ok((Elevated{socket,directory,hash,locks,identity},Box::new(input),Box::new(output)));
  }
  std::thread::sleep(Duration::from_millis(30));
 }Err("PI_ELEVATION_NOT_COMPLETED".into())
}
#[cfg(not(windows))]
pub(super) fn start(_: &Command,_:&[String],_:&Path)->Result<(Elevated,Box<dyn Write+Send>,Box<dyn Read+Send>),String>{Err("PI_PLATFORM_UNVERIFIED".into())}
