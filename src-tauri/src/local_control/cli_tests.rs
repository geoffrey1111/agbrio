use super::*;
use std::sync::{Arc,Mutex};
use tokio::{io::{AsyncBufReadExt,AsyncWriteExt,BufReader},net::windows::named_pipe::ClientOptions};
async fn raw(name:&str,command:&str,expected:Option<&str>)->Value{
    let mut stream=None;for _ in 0..100{if let Ok(c)=ClientOptions::new().open(name){stream=Some(c);break}tokio::time::sleep(std::time::Duration::from_millis(10)).await;}
    let mut c=BufReader::new(stream.expect("isolated listener"));let input=json!({"apiVersion":1,"requestId":uuid::Uuid::new_v4().to_string(),"command":command,"expectedInstance":expected});
    c.get_mut().write_all(input.to_string().as_bytes()).await.unwrap();c.get_mut().write_all(b"\n").await.unwrap();let mut line=String::new();c.read_line(&mut line).await.unwrap();c.get_mut().write_all(b"ACK\n").await.unwrap();let mut eof=String::new();let _=c.read_line(&mut eof).await;serde_json::from_str(&line).unwrap()
}
#[test]
#[ignore="explicit PowerShell client against isolated current-user pipe; refuses an existing control owner"]
fn powershell_client_reads_the_real_pipe_without_ui_or_credentials(){
    let temp=tempfile::tempdir().unwrap();let store=Arc::new(crate::RouterStore::open_at(temp.path().join("router.db")).unwrap());
    let core=RouterCore{store,chatgpt:Arc::default(),session:Arc::new(Mutex::new(Default::default())),completed_chatgpt_responses:Arc::new(Mutex::new(Default::default()))};
    let host=HostRuntime::default();let runtime=tokio::runtime::Runtime::new().unwrap();let(name,sid)=pipe::endpoint().unwrap();
    runtime.block_on(async{
        let owned=host.clone();let address=name.clone();let server=tokio::spawn(pipe::serve(core,owned,address,sid,||{}));
        assert_eq!(raw(&name,"STATUS",None).await["data"]["instanceId"],host.instance_id(),"Existing production pipe: no maintenance write performed");
        let script=std::path::Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap().join("tools/agbrio-control/control.ps1");
        for command in ["Status","Capabilities","Diagnostics"]{
            let output=std::process::Command::new("pwsh").args(["-NoProfile","-File"]).arg(&script).args(["-Command",command]).output().unwrap();
            assert!(output.status.success(),"{command}: {} {}",String::from_utf8_lossy(&output.stdout),String::from_utf8_lossy(&output.stderr));
            let value:Value=serde_json::from_slice(&output.stdout).unwrap();
            match command{"Status"=>assert_eq!(value["instanceId"],host.instance_id()),"Capabilities"=>assert_eq!(value["tools"].as_array().unwrap().len(),19),_=>assert_eq!(value["providerProbePerformed"],false)}
        }
        let output=std::process::Command::new("pwsh").args(["-NoProfile","-File"]).arg(&script).args(["-Command","Status","-ExpectedInstance","10000000-0000-4000-8000-000000000001"]).output().unwrap();assert!(!output.status.success());assert_eq!(serde_json::from_slice::<Value>(&output.stdout).unwrap()["errorCode"],"APP_CONTROL_INSTANCE_CHANGED");
        let _=raw(&name,"SHUTDOWN",Some(host.instance_id())).await;server.await.unwrap().unwrap();
    });
}
