use super::*;
use std::sync::{Arc,Mutex,atomic::{AtomicUsize,Ordering}};
fn fixture()->(tempfile::TempDir,RouterCore,HostRuntime){
    let temp=tempfile::tempdir().unwrap();let store=Arc::new(crate::RouterStore::open_at(temp.path().join("router.db")).unwrap());
    let core=RouterCore{store,chatgpt:Arc::default(),session:Arc::new(Mutex::new(Default::default())),completed_chatgpt_responses:Arc::new(Mutex::new(Default::default()))};
    (temp,core,HostRuntime::default())
}
fn request(command:&str,expected:Option<&str>)->Vec<u8>{serde_json::to_vec(&json!({"apiVersion":1,"requestId":uuid::Uuid::new_v4().to_string(),"command":command,"expectedInstance":expected})).unwrap()}
#[test]fn shutdown_pins_exact_host_and_cannot_replay_or_become_a_shell(){
    let(_temp,core,host)=fixture();let mut seen=HashMap::new();
    assert_eq!(dispatch(&core,&host,&request("SHUTDOWN",None),&mut seen).0["errorCode"],"APP_CONTROL_EXPECTED_INSTANCE_REQUIRED");
    assert_eq!(dispatch(&core,&host,&request("SHUTDOWN",Some("old-host")),&mut seen).0["errorCode"],"APP_CONTROL_INSTANCE_CHANGED");
    for op in ["RESTART_PROVIDER","SHELL","SEND_CHAT"]{assert_eq!(dispatch(&core,&host,&request(op,None),&mut seen).0["errorCode"],"APP_CONTROL_COMMAND_UNSUPPORTED");}
    let command=request("SHUTDOWN",Some(host.instance_id()));let(first,stop)=dispatch(&core,&host,&command,&mut seen);assert!(stop);
    let(replayed,stop)=dispatch(&core,&host,&command,&mut seen);assert_eq!(replayed,first);assert!(!stop);
    let mut changed:Value=serde_json::from_slice(&command).unwrap();changed["command"]=json!("STATUS");assert_eq!(dispatch(&core,&host,&serde_json::to_vec(&changed).unwrap(),&mut seen).0["errorCode"],"APP_CONTROL_REPLAY_CONFLICT");
    assert!(core.session.lock().unwrap().adapter.is_none());assert!(core.store.codex_watches().unwrap().is_empty());
}
#[test]fn diagnostic_reads_do_not_disclose_private_rows_or_invent_provider_health(){
    let(_temp,core,host)=fixture();core.store.enable_codex_watch("private-native-id","private-conversation-title","C:/private-path",&router_core::store::codex_watch::WatchSnapshot{state:"RESULT_READY".into(),turn_id:Some("private-turn".into()),item_id:Some("private-item".into()),text:"PRIVATE-CHAT-CONTENT".into()}).unwrap();
    let value=app_control::diagnostics(&core,Some(&host)).unwrap();let text=value.to_string();
    for secret in ["private-native-id","private-conversation-title","C:/private-path","private-turn","PRIVATE-CHAT-CONTENT"]{assert!(!text.contains(secret));}
    assert_eq!(value["providerProbePerformed"],false);assert_eq!(value["watches"]["total"],1);assert_eq!(value["codex"]["connected"],false);
    let lock=core.session.lock().unwrap();assert_eq!(app_control::diagnostics(&core,Some(&host)).unwrap()["codex"]["observation"],"BUSY");drop(lock);
}
#[cfg(windows)]
#[test]fn real_current_user_pipe_requires_ack_and_exact_instance_for_shutdown(){
    use tokio::{io::{AsyncBufReadExt,AsyncWriteExt,BufReader},net::windows::named_pipe::ClientOptions};
    let(_temp,core,host)=fixture();let runtime=tokio::runtime::Runtime::new().unwrap();let(name,sid)=pipe::endpoint().unwrap();let name=format!("{name}-test-{}",uuid::Uuid::new_v4());let calls=Arc::new(AtomicUsize::new(0));
    runtime.block_on(async{
        let count=calls.clone();let owned=host.clone();let addr=name.clone();let server=tokio::spawn(pipe::serve(core,owned,addr,sid,move||{count.fetch_add(1,Ordering::SeqCst);}));
        async fn connect(name:&str)->tokio::net::windows::named_pipe::NamedPipeClient{
            for _ in 0..100{if let Ok(c)=ClientOptions::new().open(name){return c}tokio::time::sleep(std::time::Duration::from_millis(10)).await;}panic!("pipe did not open")
        }
        async fn exchange(name:&str,bytes:&[u8],ack:bool)->Value{
            let mut client=BufReader::new(connect(name).await);
            client.get_mut().write_all(bytes).await.expect("write request");
            client.get_mut().write_all(b"\n").await.expect("write delimiter");
            let mut text=String::new();client.read_line(&mut text).await.expect("read response");
            if ack{client.get_mut().write_all(b"ACK\n").await.expect("ack response");let mut eof=String::new();let _=client.read_line(&mut eof).await;}
            serde_json::from_str(&text).unwrap()
        }
        assert_eq!(exchange(&name,&request("STATUS",None),true).await["data"]["instanceId"],host.instance_id());
        for _ in 0..32{assert_eq!(exchange(&name,&request("STATUS",None),true).await["data"]["instanceId"],host.instance_id());}
        assert_eq!(exchange(&name,&request("SHUTDOWN",Some("wrong-host")),false).await["errorCode"],"APP_CONTROL_INSTANCE_CHANGED");
        let abandoned=request("SHUTDOWN",Some(host.instance_id()));assert_eq!(exchange(&name,&abandoned,false).await["data"]["state"],"SHUTTING_DOWN");
        tokio::time::sleep(std::time::Duration::from_millis(30)).await;assert_eq!(calls.load(Ordering::SeqCst),0);
        let _=exchange(&name,&abandoned,true).await;assert_eq!(calls.load(Ordering::SeqCst),0);
        let _=exchange(&name,&request("SHUTDOWN",Some(host.instance_id())),true).await;
        tokio::time::timeout(std::time::Duration::from_secs(5),server).await.unwrap().unwrap().unwrap();assert_eq!(calls.load(Ordering::SeqCst),1);
    });
}
