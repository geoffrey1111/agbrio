//! Windows-local maintenance. No TCP listener, browser or provider control.
use crate::{app_control,host_application::RouterCore,HostRuntime};
use serde::Deserialize;
use serde_json::{json,Value};
use std::collections::HashMap;

#[derive(Clone,PartialEq,Eq,Deserialize)]
#[serde(rename_all="camelCase",deny_unknown_fields)]
struct Request { api_version:u32,request_id:String,command:String,expected_instance:Option<String>,#[serde(default)]folder:Option<String>,#[serde(default)]notice_binding:Option<router_core::store::codex_notices::Binding> }

fn dispatch(core:&RouterCore,host:&HostRuntime,bytes:&[u8],seen:&mut HashMap<String,(Request,Value)>)->(Value,bool) {
    let failure=|code:&str|(json!({"apiVersion":1,"errorCode":code}),false);
    if bytes.len()>8192 {return failure("APP_CONTROL_REQUEST_TOO_LARGE");}
    let Ok(r)=serde_json::from_slice::<Request>(bytes)else{return failure("APP_CONTROL_REQUEST_INVALID")};
    if r.api_version!=1||!uuid::Uuid::parse_str(&r.request_id).is_ok_and(|id|id.to_string()==r.request_id){return failure("APP_CONTROL_REQUEST_INVALID");}
    if let Some((prior,reply))=seen.get(&r.request_id){return if prior==&r{(reply.clone(),false)}else{failure("APP_CONTROL_REPLAY_CONFLICT")};}
    if r.expected_instance.as_deref().is_some_and(|s|s!=host.instance_id()){return failure("APP_CONTROL_INSTANCE_CHANGED");}
    let (data,shutdown)=match r.command.as_str(){
        "STATUS"=>(json!({"state":"RUNNING","instanceId":host.instance_id(),"pid":std::process::id(),"appVersion":app_control::version()}),false),
        "CAPABILITIES"=>(app_control::capabilities("OWNER"),false),
        "DIAGNOSTICS"=>match app_control::diagnostics(core,Some(host)){Ok(v)=>(v,false),Err(_)=>return failure("APP_DIAGNOSTICS_STORE_UNAVAILABLE")},
        "PUBLISH_BOARD" if r.expected_instance.as_deref()==Some(host.instance_id())=>{
            if seen.len()>=128{return failure("APP_CONTROL_REQUEST_LIMIT");}
            match r.folder.as_deref().ok_or_else(||"PUBLISHED_BOARD_FOLDER_REQUIRED".to_string()).and_then(crate::published_boards::publish){Ok(v)=>(json!(v),false),Err(code)=>return failure(&code)}
        },
        "PUBLISH_BOARD"=>return failure("APP_CONTROL_EXPECTED_INSTANCE_REQUIRED"),
        "REGISTER_CODEX_NOTICES" if r.expected_instance.as_deref()==Some(host.instance_id())=>{
            if seen.len()>=128{return failure("APP_CONTROL_REQUEST_LIMIT");}
            let Some(binding)=r.notice_binding.as_ref()else{return failure("CLAUDE_NOTICE_BINDING_INVALID")};
            match core.store.register_codex_notice(binding){Ok(v)=>{if binding.mode=="PI"{if let Err(code)=core.store.observe_pi_manager_notice_target(Some(&binding.thread_id)){return failure(&code);}}(json!(v),false)},Err(code)=>return failure(&code)}
        },
        "REGISTER_CODEX_NOTICES"=>return failure("APP_CONTROL_EXPECTED_INSTANCE_REQUIRED"),
        "SHUTDOWN" if r.expected_instance.as_deref()==Some(host.instance_id())=>(json!({"state":"SHUTTING_DOWN","instanceId":host.instance_id(),"pid":std::process::id()}),true),
        "SHUTDOWN"=>return failure("APP_CONTROL_EXPECTED_INSTANCE_REQUIRED"),
        _=>return failure("APP_CONTROL_COMMAND_UNSUPPORTED")
    };
    let reply=json!({"apiVersion":1,"requestId":r.request_id,"data":data});
    // Never evict an id and permit a reused id to change meaning in one Host epoch.
    if shutdown||matches!(r.command.as_str(),"PUBLISH_BOARD"|"REGISTER_CODEX_NOTICES") {
        if seen.len()>=128{return failure("APP_CONTROL_REQUEST_LIMIT");}
        seen.insert(r.request_id.clone(),(r,reply.clone()));
    }
    (reply,shutdown)
}

#[cfg(windows)]
pub(crate) fn process_owner_sid(process:windows::Win32::Foundation::HANDLE)->Result<String,String>{pipe::sid_for_process(process)}

#[cfg(windows)]
mod pipe {
    use super::*;
    use sha2::{Digest,Sha256};
    use tokio::{io::{AsyncReadExt,AsyncWriteExt},net::windows::named_pipe::{NamedPipeServer,ServerOptions}};
    use windows::{core::{PCWSTR,PWSTR},Win32::{Foundation::{CloseHandle,HANDLE,HLOCAL,LocalFree},Security::{Authorization::{ConvertSidToStringSidW,ConvertStringSecurityDescriptorToSecurityDescriptorW},GetTokenInformation,TokenUser,TOKEN_QUERY,TOKEN_USER,PSECURITY_DESCRIPTOR,SECURITY_ATTRIBUTES},System::Threading::{GetCurrentProcess,OpenProcessToken}}};

    fn sid()->Result<String,String>{sid_for_process(unsafe{GetCurrentProcess()})}
    pub(super) fn sid_for_process(process:HANDLE)->Result<String,String>{unsafe{
        let mut token=HANDLE::default();OpenProcessToken(process,TOKEN_QUERY,&mut token).map_err(|_|"APP_CONTROL_IDENTITY_UNAVAILABLE")?;
        let result=(||{
            let mut needed=0;let _=GetTokenInformation(token,TokenUser,None,0,&mut needed);
            if needed==0||needed>65536{return Err("APP_CONTROL_IDENTITY_UNAVAILABLE".into());}
            let mut buffer=vec![0usize;(needed as usize+std::mem::size_of::<usize>()-1)/std::mem::size_of::<usize>()];
            GetTokenInformation(token,TokenUser,Some(buffer.as_mut_ptr().cast()),needed,&mut needed).map_err(|_|"APP_CONTROL_IDENTITY_UNAVAILABLE")?;
            let user=&*buffer.as_ptr().cast::<TOKEN_USER>();let mut text=PWSTR::null();
            ConvertSidToStringSidW(user.User.Sid,&mut text).map_err(|_|"APP_CONTROL_IDENTITY_UNAVAILABLE")?;
            let result=text.to_string().map_err(|_|"APP_CONTROL_IDENTITY_UNAVAILABLE".into());let _=LocalFree(Some(HLOCAL(text.0.cast())));result
        })();let _=CloseHandle(token);result
    }}
    pub(super) fn endpoint()->Result<(String,String),String>{let sid=sid()?;Ok((format!(r"\\.\pipe\agbrio-control-v1-{:x}",Sha256::digest(sid.as_bytes())),sid))}
    fn create(name:&str,sid:&str,first:bool)->Result<NamedPipeServer,String>{unsafe{
        // Protected DACL: only the current user SID, no inherited Everyone ACE.
        let sddl:Vec<u16>=format!("D:P(A;;GA;;;{sid})").encode_utf16().chain(Some(0)).collect();let mut descriptor=PSECURITY_DESCRIPTOR::default();
        ConvertStringSecurityDescriptorToSecurityDescriptorW(PCWSTR(sddl.as_ptr()),1,&mut descriptor,None).map_err(|_|"APP_CONTROL_SECURITY_UNAVAILABLE")?;
        let mut attributes=SECURITY_ATTRIBUTES{nLength:std::mem::size_of::<SECURITY_ATTRIBUTES>() as u32,lpSecurityDescriptor:descriptor.0,bInheritHandle:false.into()};
        let result=ServerOptions::new().first_pipe_instance(first).reject_remote_clients(true).max_instances(2).create_with_security_attributes_raw(name,(&mut attributes as *mut SECURITY_ATTRIBUTES).cast());
        let _=LocalFree(Some(HLOCAL(descriptor.0)));result.map_err(|error|if error.raw_os_error()==Some(231){"APP_CONTROL_PIPE_BUSY".into()}else{"APP_CONTROL_LISTENER_UNAVAILABLE".into()})
    }}
    pub(super) async fn serve(core:RouterCore,host:HostRuntime,name:String,sid:String,on_shutdown:impl FnOnce()+Send+'static)->Result<(),String>{
        let mut listener=create(&name,&sid,true)?;let mut seen=HashMap::new();
        loop {
            listener.connect().await.map_err(|_|"APP_CONTROL_CONNECT_FAILED")?;
            // Keep a distinct listening instance available before serving this
            // client. Reusing a disconnected handle lets a quick next client
            // race the previous connection's close (ERROR_PIPE_NOT_CONNECTED).
            // Windows can retain a closing instance briefly after the Rust
            // handle drops (ERROR_PIPE_BUSY=231). Retry only listener creation,
            // never a received command; keep the two-instance resource bound.
            let deadline=std::time::Instant::now()+std::time::Duration::from_secs(1);
            let next=loop{match create(&name,&sid,false){
                Ok(next)=>break next,
                Err(code) if code=="APP_CONTROL_PIPE_BUSY"&&std::time::Instant::now()<deadline=>tokio::time::sleep(std::time::Duration::from_millis(10)).await,
                Err(code)=>return Err(code)
            }};
            let mut server=std::mem::replace(&mut listener,next);
            let exchange=tokio::time::timeout(std::time::Duration::from_secs(3),async{
                let mut bytes=Vec::new();
                loop{let byte=server.read_u8().await.map_err(|_|())?;if byte==b'\n'{break}bytes.push(byte);if bytes.len()>8192{break}}
                let (reply,shutdown)=dispatch(&core,&host,&bytes,&mut seen);
                let mut output=serde_json::to_vec(&reply).map_err(|_|())?;output.push(b'\n');
                server.write_all(&output).await.map_err(|_|())?;server.flush().await.map_err(|_|())?;
                // DisconnectNamedPipe may discard unread output. An explicit ACK
                // proves the client consumed the reply before disconnect/exit.
                let mut ack=[0u8;4];server.read_exact(&mut ack).await.map_err(|_|())?;
                if &ack!=b"ACK\n"{return Err(())}Ok::<bool,()>(shutdown)
            }).await;
            // Close this instance instead of making its handle reusable while a
            // fast client is already opening the next connection.
            drop(server);
            // If the shutdown reply was lost, do not act; the caller must inspect
            // this exact Host and may not blindly repeat the command.
            if matches!(exchange,Ok(Ok(true))){on_shutdown();return Ok(())}
        }
    }
}

pub(crate) fn start(app:tauri::AppHandle,core:RouterCore,host:HostRuntime){
    #[cfg(windows)]{
        let Ok((name,sid))=pipe::endpoint()else{eprintln!("APP_CONTROL_IDENTITY_UNAVAILABLE");return};
        tauri::async_runtime::spawn(async move{
            let shutdown=move||{let handle=app.clone();let _=app.run_on_main_thread(move||crate::quit_host(&handle));};
            if pipe::serve(core,host,name,sid,shutdown).await.is_err(){eprintln!("APP_CONTROL_LISTENER_UNAVAILABLE");}
        });
    }
    #[cfg(not(windows))]let _=(app,core,host);
}

#[cfg(test)]
mod tests;
#[cfg(all(test,windows))]
mod cli_tests;
