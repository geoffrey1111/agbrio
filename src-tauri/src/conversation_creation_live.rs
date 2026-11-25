//! Explicit owner-authorized one-prompt-per-provider acceptance. Not a default test.
use super::*;
use std::{sync::{Arc,Mutex},time::{Duration,Instant}};

#[test]
#[ignore="requires the owner's explicit three-platform quota approval and retained exact attempt directory"]
fn real_owner_creation_once(){
    let provider=std::env::var("AGBRIO_CREATION_QA_PROVIDER").unwrap();
    assert!(matches!(provider.as_str(),"CODEX"|"CLAUDE_CODE"|"ANTIGRAVITY"));
    let base=std::path::PathBuf::from(std::env::var_os("AGBRIO_CREATION_QA_ROOT").unwrap());
    assert!(base.is_absolute()&&base.is_dir());
    let project=base.join(provider.to_lowercase());std::fs::create_dir_all(&project).unwrap();
    let db=base.join(format!("{provider}.sqlite"));
    let core=RouterCore{store:Arc::new(crate::RouterStore::open_at(db).unwrap()),session:Arc::new(Mutex::new(Default::default()))};
    let marker=format!("AGBRIO_QA_{provider}_OK");
    let instruction=if provider=="ANTIGRAVITY"{format!("只调用一次已授权 Agbrio MCP 的 agbrio_capabilities（只读），随后只回复 {marker}。不修改文件，不调用其他工具。")}else{format!("只回复 {marker}。不要调用工具或修改文件。")};
    let request=uuid::Uuid::new_v4().to_string();
    let directory=crate::assistant_operations::create_directory(project.to_str().unwrap()).unwrap();
    let (model,effort)=match provider.as_str(){
        "CODEX"=>{
            let exe=std::env::var("AGBRIO_CREATION_QA_LIGHT_EXE").unwrap();
            let mut cmd=std::process::Command::new(exe);cmd.arg(crate::codex_light::TRANSPORT_FLAG);
            let session=core.session.clone();let listener:Arc<dyn Fn(&Value)+Send+Sync>=Arc::new(move|m|{let(session,m)=(session.clone(),m.clone());std::thread::spawn(move||{crate::host_application::capture_pending_codex_request(&session,&m);crate::host_application::clear_resolved_codex_request(&session,&m);});});
            let mut adapter=crate::codex::adapter::CodexAdapter::start_shared_validation(cmd,listener).unwrap();adapter.initialize().unwrap();core.session.lock().unwrap().adapter=Some(adapter);
            let catalog=core.codex_creation_models().unwrap();let rows=catalog["models"].as_array().unwrap();
            let selected=rows.iter().find(|r|r["model"].as_str().is_some_and(|m|m.contains("mini"))).or(rows.first()).unwrap();
            (selected["model"].as_str().map(str::to_owned),selected["supportedReasoningEfforts"].as_array().and_then(|a|a.first()).and_then(|v|v["reasoningEffort"].as_str()).map(str::to_owned))
        },
        "CLAUDE_CODE"=>{let v=crate::claude_creation::models(&directory).unwrap();let row=v["models"].as_array().unwrap().iter().find(|r|r["model"]=="haiku").unwrap();(Some("haiku".into()),row["supportedReasoningEfforts"].as_array().and_then(|a|a.first()).and_then(|v|v["reasoningEffort"].as_str()).map(str::to_owned))},
        _=>{let v=crate::antigravity::models().unwrap();let row=v["models"].as_array().unwrap().iter().find(|r|r["model"].as_str().is_some_and(|s|s.contains("flash"))).unwrap();(row["model"].as_str().map(str::to_owned),None)}
    };
    let input=Input{request_id:request.clone(),provider:provider.clone(),directory,model,effort,text:instruction};
    // Permanent attempt fence precedes the first possible user-frame offer.
    // Even a failed run is evidence, never permission to run another prompt.
    let guard=base.join(format!("{provider}.attempt.json"));
    use std::io::Write;
    let mut file=std::fs::OpenOptions::new().create_new(true).write(true).open(&guard).expect("original live attempt exists: only read its receipts");
    file.write_all(serde_json::to_string_pretty(&input).unwrap().as_bytes()).unwrap();file.sync_all().unwrap();
    let result=command(&core,Command::Create{input});
    let mut evidence=json!({"provider":provider,"requestId":request,"initial":result.as_ref().ok(),"initialError":result.as_ref().err(),"markerObserved":false,"automaticRetry":false});
    let evidence_path=base.join(format!("{provider}.result.json"));
    std::fs::write(&evidence_path,serde_json::to_vec_pretty(&evidence).unwrap()).unwrap();
    let deadline=Instant::now()+Duration::from_secs(120);
    while Instant::now()<deadline{
        let receipt=core.store.owner_conversation_creation(&request).unwrap();evidence["creation"]=json!(receipt);
        if receipt.status=="UNKNOWN"{break;}
        if let Some(key)=receipt.result.as_ref().and_then(|r|r["watch"]["threadId"].as_str()){
            if let Ok(delivery)=core.store.watch_reply(&request){evidence["delivery"]=json!(delivery);}
            let text=if provider=="ANTIGRAVITY"{core.store.managed_public_history(key,None).ok().map(|v|v.to_string()).unwrap_or_default()}
            else if provider=="CLAUDE_CODE"{key.strip_prefix("claude_code:").and_then(crate::claude_transcript::latest).map(|p|p.turns.into_iter().flat_map(|t|t.messages).filter(|m|m.role=="assistant").map(|m|m.text).collect::<Vec<_>>().join("\n")).unwrap_or_default()}
            else{crate::watch_chat::state(&core,key).ok().map(|v|v.watch.snapshot.text).unwrap_or_default()};
            if text.contains(&marker){evidence["markerObserved"]=json!(true);break;}
        }
        std::fs::write(&evidence_path,serde_json::to_vec_pretty(&evidence).unwrap()).unwrap();std::thread::sleep(Duration::from_millis(500));
    }
    std::fs::write(&evidence_path,serde_json::to_vec_pretty(&evidence).unwrap()).unwrap();
    assert_eq!(evidence["markerObserved"],true,"read retained original receipt; never repeat this prompt");
}
