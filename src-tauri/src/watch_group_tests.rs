use crate::host_application::RouterCore;
use router_core::store::{RouterStore,codex_watch::WatchSnapshot};
use std::sync::{Arc,Mutex};

#[test]
fn removed_pi_tasks_are_not_resurrected_by_the_directory_fallback(){
 let d=tempfile::tempdir().unwrap();let path=d.path().join("router.db");let store=Arc::new(RouterStore::open_at(&path).unwrap());let root=d.path().to_string_lossy().into_owned();
 let scope=store.pi_authorize_directory(&root,true,false).unwrap();
 let c=rusqlite::Connection::open(&path).unwrap();c.pragma_update(None,"foreign_keys","ON").unwrap();
 c.execute("INSERT INTO pi_tasks(id,create_action_id,creator_grant_id,scope_id,scope_generation,directory,label,parent_provider,parent_external_id,parent_cwd,native_session_id,state,created_at,updated_at) VALUES('fixture','fixture','fixture',?1,1,?2,'fixture','CODEX','parent',?2,'fixture-native','RUNNING',1,1)",rusqlite::params![scope.id,root]).unwrap();drop(c);
 for id in ["parent","pi:fixture-native"]{store.enable_codex_watch(id,id,&root,&WatchSnapshot{state:"IDLE".into(),turn_id:None,item_id:None,text:String::new()}).unwrap();}
 let core=RouterCore{store:store.clone(),session:Arc::new(Mutex::default()),chatgpt:Arc::default(),completed_chatgpt_responses:Arc::default()};
 assert_eq!(crate::claude_chat::list(&core).unwrap().len(),2);
 store.set_watch_item_removed("WATCH","parent",true).unwrap();assert!(crate::claude_chat::list(&core).unwrap().is_empty());
 assert_eq!(store.pi_tasks().unwrap()[0].state,"RUNNING");
 store.set_watch_item_removed("WATCH","parent",false).unwrap();assert_eq!(crate::claude_chat::list(&core).unwrap().len(),2);
 store.set_watch_item_removed("WATCH","pi:fixture-native",true).unwrap();let visible=crate::claude_chat::list(&core).unwrap();assert_eq!(visible.len(),1);assert_eq!(visible[0].watch.thread_id,"parent");
 assert!(!core.session.lock().unwrap().pi.loaded("fixture"));
}
