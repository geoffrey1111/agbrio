//! Offline MCP regression: no provider connection, messages or real database.
use super::*;
use crate::host_application::Session;
use router_core::store::{RouterStore,assistant::{AssistantConnectionInput,GrantInput},role_bridge::RoleBindingInput};
use std::{sync::{Arc,Mutex},time::{SystemTime,UNIX_EPOCH},collections::BTreeSet};

#[test]
fn instance_read_app_enumerates_all_projects_and_separates_lifecycles_without_selection_writes(){
 let dir=tempfile::tempdir().unwrap();let store=Arc::new(RouterStore::open_at(dir.path().join("fixture.db")).unwrap());
 let core=RouterCore{store:store.clone(),chatgpt:Arc::default(),session:Arc::new(Mutex::new(Session::default())),completed_chatgpt_responses:Arc::default()};
 let expires=SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_millis() as i64+600_000;
 let g=store.connect_assistant_instance(AssistantConnectionInput{label:"Example review assistant".into(),expires_at:expires}).unwrap();
 let pa=store.create_project("Demo A".into(),None).unwrap();let pb=store.create_project("Demo B".into(),None).unwrap();
 let mut sources=vec![];let mut active=vec![];
 for (project,name) in [(&pa.id,"A"),(&pb.id,"B")]{
  let w=store.create_workstream(project,name.into()).unwrap();
  let side=|role:&str|RoleBindingInput{provider:"CODEX".into(),external_id:format!("example-{name}-{role}"),label:role.into(),cwd:Some(dir.path().to_string_lossy().into())};
  let bound=store.bind_role_bridge(&w.id,0,side("manager"),side("executor")).unwrap();
  let endpoint=bound.decision.unwrap().endpoint;store.record_reply_observation(&w.id,&endpoint.id,Some("example-item"),"Example report for review",None).unwrap();
  let observation=store.reply_observations_for_workstream(&w.id).unwrap().remove(0);
  sources.push((w.id.clone(),observation.id));active.push(w.id);
 }
 let archived=store.create_workstream(&pa.id,"Archived example".into()).unwrap();store.archive_workstream(&archived.id).unwrap();
 let trashed=store.create_workstream(&pa.id,"Trashed example".into()).unwrap();store.trash_workstream(&trashed.id).unwrap();
 let archived_trash=store.create_workstream(&pb.id,"Archived then trashed".into()).unwrap();store.archive_workstream(&archived_trash.id).unwrap();store.trash_workstream(&archived_trash.id).unwrap();
 store.select_workspace(&pa.id,Some(&active[0])).unwrap();
 let before=serde_json::to_value(store.snapshot().unwrap()).unwrap();let grant_before=serde_json::to_value(store.assistant_grant(&g.id).unwrap()).unwrap();
 let request=json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"agbrio_read_app","arguments":{}}});
 let reply=rpc(&core,&g.id,request.clone());assert_eq!(reply["result"]["isError"],false);let app=&reply["result"]["structuredContent"];
 let ids=|items:&Value|items.as_array().unwrap().iter().map(|w|w["id"].as_str().unwrap().to_owned()).collect::<BTreeSet<_>>();
 assert_eq!(ids(&app["bridges"]),active.iter().cloned().collect());
 assert_eq!(ids(&app["archivedBridges"]),BTreeSet::from([archived.id.clone()]));
 assert_eq!(ids(&app["trashedBridges"]),BTreeSet::from([trashed.id.clone(),archived_trash.id.clone()]));
 for (key,lifecycle) in [("bridges","ACTIVE"),("archivedBridges","ARCHIVED"),("trashedBridges","TRASHED")]{for w in app[key].as_array().unwrap(){assert_eq!(w["lifecycle"],lifecycle);}}
 for (wid,pid) in [(&active[0],&pa.id),(&active[1],&pb.id)]{let row=app["bridges"].as_array().unwrap().iter().find(|w|w["id"]==*wid).unwrap();assert_eq!(row["projectId"],*pid);assert_eq!(row["bindingRevision"],1);}
 assert_eq!(serde_json::to_value(store.snapshot().unwrap()).unwrap(),before);
 store.select_workspace(&pb.id,Some(&active[1])).unwrap();assert_eq!(rpc(&core,&g.id,request)["result"]["structuredContent"],*app);
 assert_eq!(store.snapshot().unwrap().workstreams.iter().map(|w|w.id.clone()).collect::<BTreeSet<_>>(),BTreeSet::from([active[1].clone(),archived_trash.id.clone()]));
 // Reuse the established offline adapter fixture pattern. Source attachment
 // validation needs exact native metadata, not a real Codex/model connection.
 let script=dir.path().join("read-only-adapter.mjs");std::fs::write(&script,r#"
import readline from 'node:readline';
const cwd=process.argv[2];
readline.createInterface({input:process.stdin}).on('line',line=>{
 const q=JSON.parse(line);if(q.id===undefined)return;
 if(!['initialize','thread/read','thread/resume','thread/turns/list','thread/goal/get'].includes(q.method))throw Error('No writes allowed in read-only fixture');
 const thread={id:q.params?.threadId,cwd,status:{type:'idle'},turns:[]};
 const result=q.method==='initialize'?{userAgent:'Agbrio read-only fixture'}:q.method==='thread/read'||q.method==='thread/resume'?{thread,model:'fixture',reasoningEffort:'low'}:q.method==='thread/turns/list'?{data:[],nextCursor:null}:{goal:null};
 process.stdout.write(JSON.stringify({jsonrpc:'2.0',id:q.id,result})+'\n');
});
"#).unwrap();
 let mut command=std::process::Command::new("node");command.arg(script).arg(dir.path());
 #[cfg(windows)]{use std::os::windows::process::CommandExt;command.creation_flags(0x08000000);}
 let mut adapter=crate::codex::adapter::CodexAdapter::start_shared_validation(command,Arc::new(|_|{})).unwrap();adapter.initialize().unwrap();core.session.lock().unwrap().adapter=Some(adapter);
 for (wid,observation) in sources{call(&core,&g.id,"agbrio_read_bridge",json!({"workstreamId":wid})).unwrap();call(&core,&g.id,"agbrio_read_source",json!({"workstreamId":wid,"observationId":observation,"role":"DECISION"})).unwrap();}
 core.shutdown_owned_adapter();
 assert_eq!(serde_json::to_value(store.assistant_grant(&g.id).unwrap()).unwrap(),grant_before);
 let old=store.create_assistant_grant(GrantInput{workstream_id:active[0].clone(),source_role:"BOTH".into(),binding_revision:1,label:"Legacy example".into(),rules:vec![router_core::store::assistant::BriefRule{id:"example".into(),text:"Review example work".into()}],expires_at:expires}).unwrap();
 assert!(call(&core,&old.id,"agbrio_read_app",json!({})).unwrap_err().contains("INSTANCE_AUTHORITY_REQUIRED"));
 assert!(store.require_assistant_bridge(&old.id,&active[1],Some(1)).is_err());
 assert!(store.require_assistant_bridge(&g.id,&active[1],Some(2)).is_err());
 for id in [&archived.id,&trashed.id,&archived_trash.id]{assert!(store.require_assistant_bridge(&g.id,id,None).is_err());}
 assert_eq!(rpc(&core,&old.id,json!({"jsonrpc":"2.0","id":2,"method":"tools/list"}))["result"]["tools"].as_array().unwrap().len(),8);
 store.revoke_assistant_grant(&g.id).unwrap();assert!(call(&core,&g.id,"agbrio_read_app",json!({})).is_err());
}
