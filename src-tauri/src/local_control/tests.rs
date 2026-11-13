use super::*;
use std::sync::{Arc,Mutex,atomic::{AtomicUsize,Ordering}};
fn fixture()->(tempfile::TempDir,RouterCore,HostRuntime){
    let temp=tempfile::tempdir().unwrap();let store=Arc::new(crate::RouterStore::open_at(temp.path().join("router.db")).unwrap());
    let core=RouterCore{store,session:Arc::new(Mutex::new(Default::default()))};
    (temp,core,HostRuntime::default())
}
fn request(command:&str,expected:Option<&str>)->Vec<u8>{serde_json::to_vec(&json!({"apiVersion":1,"requestId":uuid::Uuid::new_v4().to_string(),"command":command,"expectedInstance":expected})).unwrap()}
#[test]
fn more_than_128_publications_succeed_and_old_replays_keep_the_original_result(){
    let(temp,core,host)=fixture();let mut seen=ReplayStore::new().unwrap();
    let folder=temp.path().join("board");std::fs::create_dir(&folder).unwrap();
    std::fs::write(folder.join("agbrio-board.json"),r#"{"schemaVersion":1,"title":"Fixture","entry":"index.html","files":["index.html"]}"#).unwrap();
    *crate::published_boards::TEST_STORE.lock().unwrap()=Some(temp.path().join("registry.json"));
    struct Reset;impl Drop for Reset{fn drop(&mut self){*crate::published_boards::TEST_STORE.lock().unwrap()=None;}}
    let _reset=Reset;
    let mut originals=Vec::new();
    for n in 0..512{
        std::fs::write(folder.join("index.html"),format!("<h1>Revision {n}</h1>")).unwrap();
        let mut input:Value=serde_json::from_slice(&request("PUBLISH_BOARD",Some(host.instance_id()))).unwrap();input["folder"]=json!(folder);
        let bytes=serde_json::to_vec(&input).unwrap();let(reply,shutdown)=dispatch(&core,&host,&bytes,&mut seen);
        assert!(!shutdown);assert!(reply.get("errorCode").is_none(),"publication {n}: {reply}");
        originals.push((bytes,reply));
    }
    // The sources have changed 511 times: replay must not invoke publish again.
    std::fs::remove_file(folder.join("index.html")).unwrap();
    for(bytes,reply)in &originals{assert_eq!(dispatch(&core,&host,bytes,&mut seen),(reply.clone(),false));}
    let mut changed:Value=serde_json::from_slice(&originals[0].0).unwrap();changed["command"]=json!("REGISTER_CODEX_NOTICES");
    assert_eq!(dispatch(&core,&host,&serde_json::to_vec(&changed).unwrap(),&mut seen).0["errorCode"],"APP_CONTROL_REPLAY_CONFLICT");
    assert!(core.session.lock().unwrap().adapter.is_none());
}

#[test]
fn reserved_unknown_requests_are_fenced_and_disk_receipts_have_a_fixed_page_cache(){
    let(_temp,core,host)=fixture();let mut seen=ReplayStore::new().unwrap();
    let bytes=request("SHUTDOWN",Some(host.instance_id()));let input:Request=serde_json::from_slice(&bytes).unwrap();
    seen.reserve(&input).unwrap();
    assert_eq!(dispatch(&core,&host,&bytes,&mut seen).0["errorCode"],"APP_CONTROL_REQUEST_UNCONFIRMED");
    let mut changed:Value=serde_json::from_slice(&bytes).unwrap();changed["command"]=json!("STATUS");
    assert_eq!(dispatch(&core,&host,&serde_json::to_vec(&changed).unwrap(),&mut seen).0["errorCode"],"APP_CONTROL_REPLAY_CONFLICT");
    assert_eq!(seen.cache_kib(),256);
}
#[test]fn publication_requires_current_host_and_never_becomes_a_remote_path_reader(){let(_temp,core,host)=fixture();let mut seen=ReplayStore::new().unwrap();assert_eq!(dispatch(&core,&host,&request("PUBLISH_BOARD",None),&mut seen).0["errorCode"],"APP_CONTROL_EXPECTED_INSTANCE_REQUIRED");assert_eq!(dispatch(&core,&host,&request("PUBLISH_BOARD",Some("old")),&mut seen).0["errorCode"],"APP_CONTROL_INSTANCE_CHANGED");assert_eq!(dispatch(&core,&host,&request("PUBLISH_BOARD",Some(host.instance_id())),&mut seen).0["errorCode"],"PUBLISHED_BOARD_FOLDER_REQUIRED");assert!(core.session.lock().unwrap().adapter.is_none());}
#[test]fn shutdown_pins_exact_host_and_cannot_replay_or_become_a_shell(){
    let(_temp,core,host)=fixture();let mut seen=ReplayStore::new().unwrap();
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

#[test]
fn replace_zone_manager_discovers_desktop_child_and_moves_routes_atomically() {
    let (temp,core,host)=fixture();
    let mut seen=ReplayStore::new().unwrap();
    let cwd=temp.path().to_string_lossy().into_owned();
    let old=uuid::Uuid::new_v4().to_string();
    let child=uuid::Uuid::new_v4().to_string();
    let alias=format!("local_{}",uuid::Uuid::new_v4());
    let source=uuid::Uuid::new_v4().to_string();
    for (id,label) in [(&old,"Old manager"),(&source,"Executor")] {
        core.store.enable_codex_watch(id,label,&cwd,&router_core::store::codex_watch::WatchSnapshot{
            state:"IDLE".into(),turn_id:None,item_id:None,text:String::new()
        }).unwrap();
    }
    let change=|input:Value|core.store.change_management_zone(&serde_json::from_value(input).unwrap());
    let zone=change(json!({"requestId":uuid::Uuid::new_v4(),"action":"CREATE","name":"Fixture","managerId":old})).unwrap()["zoneId"].as_str().unwrap().to_owned();
    change(json!({"requestId":uuid::Uuid::new_v4(),"action":"ADD","zoneId":zone,"revision":1,"threadId":source,"generation":1})).unwrap();
    let roots=&crate::claude_code_desktop::TEST_ROOTS;
    *roots.lock().unwrap()=vec![temp.path().to_owned()];
    struct Reset;
    impl Drop for Reset{fn drop(&mut self){crate::claude_code_desktop::TEST_ROOTS.lock().unwrap().clear();}}
    let _reset=Reset;
    let dir=temp.path().join("claude-code-sessions/account/project");
    std::fs::create_dir_all(&dir).unwrap();
    let path=dir.join(format!("{alias}.json"));
    std::fs::write(&path,format!("{{\"sessionId\":\"{alias}\",\"cliSessionId\":\"{child}\",\"cwd\":{},\"lastActivityAt\":123,\"title\":\"Child manager\",\"spawnedFrom\":{{\"sessionId\":\"local_parent\"}}}}",json!(cwd))).unwrap();
    let candidate=crate::management_zones::read(&core).unwrap()["candidates"].as_array().unwrap().iter()
        .find(|row|row["localSessionId"]==alias).cloned().expect("fresh unregistered child in picker");
    let manager=format!("claude_code:{child}");
    assert_eq!(candidate["threadId"],manager);
    assert_eq!(candidate["label"],"Child manager");
    assert_eq!(candidate["available"],false);
    let mut input=json!({"apiVersion":1,"requestId":uuid::Uuid::new_v4(),"command":"REPLACE_ZONE_MANAGER",
        "zoneId":zone,"expectedRevision":1,"newManager":{"provider":"CLAUDE_CODE","sessionId":alias}});
    let mut call=|v:&Value|dispatch(&core,&host,&serde_json::to_vec(v).unwrap(),&mut seen).0;
    assert_eq!(call(&input)["errorCode"],"APP_CONTROL_EXPECTED_INSTANCE_REQUIRED");
    input["expectedInstance"]=json!(host.instance_id());
    assert_eq!(call(&input)["errorCode"],"MANAGEMENT_CHANGED_REFRESH");
    assert_eq!(core.store.management_zones().unwrap()["zones"][0]["managerId"],old);
    input["expectedRevision"]=json!(2);
    let receipt=call(&input);
    assert_eq!(receipt["data"]["managerId"],manager,"{receipt}");
    assert_eq!(receipt["data"]["revision"],3);
    // The exact retry returns its durable receipt even if discovery later hides it.
    std::fs::write(dir.join("archived-sessions.idx"),json!({"archived":[alias]}).to_string()).unwrap();
    assert_eq!(call(&input),receipt);
    input["newManager"]["sessionId"]=json!(old);
    assert_eq!(call(&input)["errorCode"],"MANAGEMENT_REQUEST_CONFLICT");
    let view=call(&json!({"apiVersion":1,"requestId":uuid::Uuid::new_v4(),"command":"READ_MANAGEMENT_ZONE","zoneId":zone}));
    let row=&view["data"];
    assert_eq!(row["managerId"],manager);
    assert_eq!(row["revision"],3);
    assert_eq!(row["members"][0]["threadId"],source);
    let routes=row["members"][0]["subscriptions"].as_array().unwrap();
    assert_eq!(routes.len(),1);
    assert_eq!(routes[0]["receiverSid"],child);
    assert_eq!(routes[0]["receiverProvider"],"CLAUDE_CODE");
    assert!(core.store.claude_session(&child).is_err(),"owner selection must not register a publisher");
}

#[test]
fn local_owner_adds_multiple_executors_and_routes_in_one_revision(){
    let(temp,core,host)=fixture();
    let mut seen=ReplayStore::new().unwrap();
    let mut call=|v:&Value|dispatch(&core,&host,&serde_json::to_vec(v).unwrap(),&mut seen).0;
    let cwd=temp.path().to_string_lossy().into_owned();
    let manager=uuid::Uuid::new_v4().to_string();
    let a=uuid::Uuid::new_v4().to_string();
    let b=uuid::Uuid::new_v4().to_string();
    for id in [&manager,&a,&b]{
        core.store.enable_codex_watch(id,"Fixture",&cwd,&router_core::store::codex_watch::WatchSnapshot{
            state:"IDLE".into(),turn_id:None,item_id:None,text:String::new()
        }).unwrap();
    }
    let zone=core.store.change_management_zone(&serde_json::from_value(json!({
        "requestId":uuid::Uuid::new_v4(),"action":"CREATE","name":"Fixture","managerId":manager
    })).unwrap()).unwrap()["zoneId"].as_str().unwrap().to_owned();
    let mut request=json!({"apiVersion":1,"requestId":uuid::Uuid::new_v4(),"command":"ADD_ZONE_EXECUTORS",
        "zoneId":zone,"expectedRevision":1,"executors":[{"threadId":a,"generation":1},{"threadId":b,"generation":2}]});
    assert_eq!(call(&request)["errorCode"],"APP_CONTROL_EXPECTED_INSTANCE_REQUIRED");
    request["expectedInstance"]=json!(host.instance_id());
    assert_eq!(call(&request)["errorCode"],"MANAGEMENT_SOURCE_CHANGED_REFRESH");
    assert_eq!(core.store.management_zones().unwrap()["zones"][0]["members"],json!([]));
    request["executors"][1]["generation"]=json!(1);
    let receipt=call(&request);
    assert_eq!(receipt["data"]["addedThreadIds"],json!([a,b]),"{receipt}");
    assert_eq!(receipt["data"]["revision"],2);
    assert_eq!(call(&request),receipt);
    request["expectedRevision"]=json!(2);
    assert_eq!(call(&request)["errorCode"],"MANAGEMENT_REQUEST_CONFLICT");
    request["requestId"]=json!(uuid::Uuid::new_v4());
    assert_eq!(call(&request)["data"]["alreadyAssignedThreadIds"],json!([a,b]));
    let view=core.store.management_zones().unwrap();
    assert_eq!(view["zones"][0]["revision"],2);
    for member in view["zones"][0]["members"].as_array().unwrap(){
        assert_eq!(member["subscriptions"].as_array().unwrap().len(),1);
        assert_eq!(member["subscriptions"][0]["receiverSid"],manager);
    }
}
