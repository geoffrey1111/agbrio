//! Owner-selected task notice routing. Only fixed metadata enters Codex; all
//! native send guards/receipts remain in WatchChat. No MCP scope is enlarged.
use crate::host_application::{RouterCore,Session};
use router_core::store::RouterStore;
use std::sync::{Arc,Mutex,atomic::{AtomicBool,AtomicUsize,Ordering}};
use sha2::{Digest,Sha256};

pub(crate) fn read(core:&RouterCore)->Result<serde_json::Value,String>{
    let mut view=core.store.management_zones()?;
    let labels:std::collections::HashMap<_,_>=crate::claude_code::catalog(core).map(|catalog|catalog.sessions.into_iter().filter_map(|s|s.title.map(|title|(format!("claude_code:{}",s.session_id),title))).collect()).unwrap_or_default();
    fn decorate(value:&mut serde_json::Value,labels:&std::collections::HashMap<String,String>){
        match value{
            serde_json::Value::Object(object)=>{if let Some(title)=object.get("threadId").and_then(|v|v.as_str()).and_then(|id|labels.get(id)){if object.contains_key("provider"){object.insert("label".into(),serde_json::json!(title));}}for v in object.values_mut(){decorate(v,labels);}},
            serde_json::Value::Array(rows)=>for row in rows{decorate(row,labels);},_=>{},
        }
    }
    decorate(&mut view,&labels);Ok(view)
}

fn request_id(notice:&str,attempt:i64)->String{
    let digest=Sha256::digest(format!("management-metadata-v1:{notice}:{attempt}"));
    let mut bytes=[0;16];bytes.copy_from_slice(&digest[..16]);
    bytes[6]=(bytes[6]&15)|64;bytes[8]=(bytes[8]&63)|128;
    uuid::Uuid::from_bytes(bytes).to_string()
}
fn submit(core:&RouterCore,intent:router_core::store::watch_reply::WatchReply){
    if intent.status=="QUEUED"{let _=crate::watch_chat::management_notice_command(core,&intent.id);}
}
/// Called from the existing Host maintenance clock, independent of whether a
/// Codex adapter is connected. A single bounded worker cannot block resource
/// alerts or multiply native writers while an earlier read is still pending.
pub(crate) fn maintain(core:&RouterCore){
    static RUNNING:AtomicBool=AtomicBool::new(false);
    if core.store.management_active()!=Ok(true)||RUNNING.compare_exchange(false,true,Ordering::AcqRel,Ordering::Acquire).is_err(){return;}
    let store=core.store.clone();let session=core.session.clone();
    struct Reset;impl Drop for Reset{fn drop(&mut self){RUNNING.store(false,Ordering::Release);}}
    if std::thread::Builder::new().name("management-notices".into()).spawn(move||{let _reset=Reset;poll(&store,&session);}).is_err(){RUNNING.store(false,Ordering::Release);}
}
pub(crate) fn poll(store:&Arc<RouterStore>,session:&Arc<Mutex<Session>>){
    let _=store.sync_management_routes();let _=store.observe_management_passive();
    let Ok(rows)=store.management_codex_deliveries()else{return};if rows.is_empty(){return;}
    static NEXT:AtomicUsize=AtomicUsize::new(0);
    let offset=NEXT.fetch_add(4,Ordering::Relaxed)%rows.len();
    let core=RouterCore{store:store.clone(),session:session.clone(),};
    for(n,s,old,attempt)in rows.iter().cycle().skip(offset).take(rows.len().min(4)){
        if n.status=="UNKNOWN"{
            if let Some(reply)=old{
                // QUEUED is a durable proof that native admission has not
                // happened. Resume that same intent/ID; SENDING/UNKNOWN never
                // enters this path, including after a Host restart.
                if let Ok(Some(intent))=store.management_queued_intent(&n.id,reply){submit(&core,intent);}
                let _=store.reconcile_management_codex_notice(&n.id,reply);
            }
            continue;
        }
        let Ok((watch,mode,turn))=crate::watch_chat::resource_notice_preflight(&core,&s.binding.receiver_sid)else{continue};
        if watch.cwd!=s.binding.receiver_cwd{continue;}
        let id=request_id(&n.id,*attempt);
        let options=serde_json::to_value(crate::watch_chat::ReplyOptions::default()).unwrap_or_default();
        let intent=router_core::store::watch_reply::WatchReply{id:id.clone(),thread_id:watch.thread_id,cwd:watch.cwd,generation:watch.generation,source_sequence:None,expected_turn_id:turn,mode,text:n.text.clone(),options,payload_hash:String::new(),status:"QUEUED".into(),turn_id:None,error_code:None,created_at:0,updated_at:0};
        if store.claim_management_codex_notice(&n.id,&intent)!=Ok(true){continue;}
        if let Ok(Some(intent))=store.management_queued_intent(&n.id,&id){submit(&core,intent);}
        let _=store.reconcile_management_codex_notice(&n.id,&id);
        // One new native metadata submission per observer pass. Unknown attempts
        // on any recipient are read back, never resubmitted after a restart.
        break;
    }
}

#[cfg(test)]mod tests{
    use super::*;
    use serde_json::json;
    #[test]fn codex_manager_receives_only_fixed_metadata_once_through_existing_native_guards(){
        let d=tempfile::tempdir().unwrap();let root=d.path().to_string_lossy().to_string();
        let store=Arc::new(RouterStore::open_at(d.path().join("router.db")).unwrap());
        let manager=uuid::Uuid::new_v4().to_string();let source=uuid::Uuid::new_v4().to_string();
        for(id,label)in [(&manager,"Manager"),(&source,"Executor")]{store.enable_codex_watch(id,label,&root,&router_core::store::codex_watch::WatchSnapshot{state:"RESULT_READY".into(),turn_id:Some("previous".into()),item_id:Some("final".into()),text:"private business text must not be relayed".into()}).unwrap();}
        let create:router_core::store::management_zones::Change=serde_json::from_value(json!({"requestId":uuid::Uuid::new_v4().to_string(),"action":"CREATE","name":"Fixture","managerId":manager})).unwrap();
        let zone=store.change_management_zone(&create).unwrap()["zoneId"].as_str().unwrap().to_string();
        store.change_management_zone(&serde_json::from_value(json!({"requestId":uuid::Uuid::new_v4().to_string(),"action":"ADD","zoneId":zone,"revision":1,"threadId":source,"generation":1})).unwrap()).unwrap();
        store.publish_management_event(&source,1,"source-turn","QUESTION","ACTION_REQUIRED",Some("request-a"),crate::local_safety::now()).unwrap();
        let log=d.path().join("metadata.jsonl");
        let script=r#"const fs=require('node:fs');const [cwd,log]=process.argv.slice(1);require('node:readline').createInterface({input:process.stdin}).on('line',line=>{const q=JSON.parse(line);if(q.id===undefined)return;let result={};if(q.method==='thread/read'||q.method==='thread/resume')result={thread:{id:q.params.threadId,cwd,status:{type:'idle'}}};else if(q.method==='thread/turns/list')result={data:[{id:'previous',status:'completed',itemsView:'notLoaded'}],nextCursor:null};else if(q.method==='thread/items/list')result={data:[{turnId:'previous',item:{id:'final',type:'agentMessage',phase:'final_answer',text:'fixture previous'}}],nextCursor:null};else if(q.method==='thread/goal/get')result={goal:null};else if(q.method==='turn/start'){fs.appendFileSync(log,JSON.stringify(q.params)+'\n');result={turn:{id:'accepted-metadata'}};}else if(q.method!=='initialize')throw Error('unexpected '+q.method);process.stdout.write(JSON.stringify({id:q.id,result})+'\n');});"#;
        let mut command=std::process::Command::new("node");command.args(["-e",script,&root,log.to_str().unwrap()]);
        let mut adapter=crate::codex::adapter::CodexAdapter::start_shared_validation(command,Arc::new(|_|{})).unwrap();adapter.initialize().unwrap();
        let session=Arc::new(Mutex::new(Session{adapter:Some(adapter),..Default::default()}));
        poll(&store,&session);
        let reopened=Arc::new(RouterStore::open_at(d.path().join("router.db")).unwrap());
        poll(&reopened,&session);
        let sent=std::fs::read_to_string(&log).unwrap_or_default();
        assert_eq!(sent.lines().count(),1,"{sent}");
        assert!(sent.contains("request-a"));assert!(sent.contains("metadata only"));assert!(sent.contains(&source));
        assert!(!sent.contains("private business text"));
        let core=RouterCore{store,session,};core.shutdown_owned_adapter();
    }
}
