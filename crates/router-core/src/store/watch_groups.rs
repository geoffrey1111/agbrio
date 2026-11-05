//! Owner-local watch removal, never Pi/task cancellation or native deletion.
use super::*;

fn children(c:&Connection,parent:&str)->Result<Vec<String>,String>{
 let mut q=c.prepare("SELECT w.thread_id FROM pi_tasks p JOIN codex_watches w ON w.thread_id='pi:'||p.native_session_id JOIN codex_watches parent ON parent.thread_id=?1 LEFT JOIN management_members m ON m.thread_id=w.thread_id LEFT JOIN management_zones z ON z.id=m.zone_id WHERE w.thread_id!=?1 AND ((m.thread_id IS NOT NULL AND z.manager_id=?1) OR (m.thread_id IS NULL AND p.parent_cwd=parent.cwd AND CASE p.parent_provider WHEN 'CLAUDE_CODE' THEN 'claude_code:'||p.parent_external_id WHEN 'CODEX' THEN p.parent_external_id END=?1))").map_err(db_error)?;
 let rows=q.query_map([parent],|r|r.get(0)).map_err(db_error)?.collect::<Result<Vec<String>,_>>().map_err(db_error)?;Ok(rows)
}
pub(super) fn change(c:&Connection,parent:&str,removed:bool)->Result<(),String>{
 let prior:bool=c.query_row("SELECT EXISTS(SELECT 1 FROM watch_removed_items WHERE kind='WATCH' AND id=?1)",[parent],|r|r.get(0)).map_err(db_error)?;
 if prior==removed{return Ok(())}
 let current=children(c,parent)?;
 if removed{
  for child in current{
   let changed=c.execute("INSERT OR IGNORE INTO watch_removed_items(kind,id,was_enabled,removed_at) SELECT 'WATCH',thread_id,enabled,?2 FROM codex_watches WHERE thread_id=?1",params![child,now()]).map_err(db_error)?;
   if changed>0{
    c.execute("INSERT OR REPLACE INTO watch_group_removals(parent_id,child_id) VALUES(?1,?2)",params![parent,child]).map_err(db_error)?;
    c.execute("UPDATE codex_watches SET enabled=0,generation=generation+1 WHERE thread_id=?1",[child]).map_err(db_error)?;
   }
  }
 }else{
  let saved={let mut q=c.prepare("SELECT child_id FROM watch_group_removals WHERE parent_id=?1").map_err(db_error)?;let rows=q.query_map([parent],|r|r.get(0)).map_err(db_error)?.collect::<Result<Vec<String>,_>>().map_err(db_error)?;rows};
  for child in saved.into_iter().filter(|id|current.contains(id)){
   let enabled:Option<bool>=c.query_row("SELECT was_enabled FROM watch_removed_items WHERE kind='WATCH' AND id=?1",[&child],|r|r.get(0)).optional().map_err(db_error)?;
   if let Some(enabled)=enabled{
    let count:i64=c.query_row("SELECT COUNT(*) FROM codex_watches WHERE enabled=1 AND thread_id!=?1",[&child],|r|r.get(0)).map_err(db_error)?;
    if enabled&&count>=super::codex_watch::MAX_ACTIVE_WATCHES{return Err("WATCH_LIMIT_REACHED".into())}
    c.execute("UPDATE codex_watches SET enabled=?2,generation=generation+1,checked_at=0 WHERE thread_id=?1",params![child,enabled]).map_err(db_error)?;
    c.execute("DELETE FROM watch_removed_items WHERE kind='WATCH' AND id=?1",[child]).map_err(db_error)?;
   }
  }
  c.execute("DELETE FROM watch_group_removals WHERE parent_id=?1",[parent]).map_err(db_error)?;
 }
 Ok(())
}

#[cfg(test)]mod tests{
 use super::*;
 fn watch(s:&RouterStore,id:&str,root:&str){s.enable_codex_watch(id,id,root,&super::super::codex_watch::WatchSnapshot{state:"IDLE".into(),turn_id:None,item_id:None,text:String::new()}).unwrap();}
 fn fixture()->(tempfile::TempDir,RouterStore,String){let d=tempfile::tempdir().unwrap();let s=RouterStore::open_at(d.path().join("router.db")).unwrap();let root=d.path().to_string_lossy().into_owned();watch(&s,"parent",&root);watch(&s,"other",&root);let scope=s.pi_authorize_directory(&root,true,false).unwrap();s.with_connection(|c|{for native in ["one","two"]{c.execute("INSERT INTO pi_tasks(id,create_action_id,creator_grant_id,scope_id,scope_generation,directory,label,parent_provider,parent_external_id,parent_cwd,native_session_id,state,created_at,updated_at) VALUES(?1,?1,'fixture',?2,1,?3,?1,'CODEX','parent',?3,?1,'RUNNING',1,1)",params![native,scope.id,root]).unwrap();}Ok(())}).unwrap();watch(&s,"pi:one",&root);watch(&s,"pi:two",&root);(d,s,root)}
 #[test]fn removal_and_undo_are_atomic_and_never_cancel_tasks(){let(_d,s,_)=fixture();s.set_watch_item_removed("WATCH","pi:two",true).unwrap();s.set_watch_item_removed("WATCH","parent",true).unwrap();assert_eq!(s.codex_watches().unwrap().iter().map(|w|w.thread_id.as_str()).collect::<Vec<_>>(),vec!["other"]);s.set_watch_item_removed("WATCH","parent",true).unwrap();s.set_watch_item_removed("WATCH","parent",false).unwrap();assert!(s.registered_codex_watch("pi:one").unwrap().enabled);assert!(!s.registered_codex_watch("pi:two").unwrap().enabled);assert!(s.pi_tasks().unwrap().iter().all(|t|t.state=="RUNNING"));}
 #[test]fn a_transferred_child_is_not_removed_with_its_original_parent(){let(_d,s,_)=fixture();s.with_connection(|c|{c.execute("INSERT INTO management_zones(id,name,manager_id,created_at,updated_at) VALUES('zone','fixture','other',1,1)",[]).unwrap();c.execute("INSERT INTO management_members(thread_id,zone_id,generation,joined_at) VALUES('pi:one','zone',1,1)",[]).unwrap();Ok(())}).unwrap();s.set_watch_item_removed("WATCH","parent",true).unwrap();assert!(s.registered_codex_watch("pi:one").unwrap().enabled);s.set_watch_item_removed("WATCH","other",true).unwrap();assert!(!s.registered_codex_watch("pi:one").unwrap().enabled);}
 #[test]fn migration_repairs_old_orphans_without_reviving_previously_removed_children(){let(_d,s,_)=fixture();s.with_connection(|c|{c.execute("INSERT INTO watch_removed_items(kind,id,was_enabled,removed_at) VALUES('WATCH','parent',1,9223372036854770000)",[]).unwrap();c.execute("UPDATE codex_watches SET enabled=0 WHERE thread_id='parent'",[]).unwrap();c.execute_batch(include_str!("../../migrations/normal/039_watch_group_removals.sql")).map_err(db_error)?;Ok(())}).unwrap();assert!(!s.registered_codex_watch("pi:one").unwrap().enabled);assert!(!s.registered_codex_watch("pi:two").unwrap().enabled);assert!(s.pi_tasks().unwrap().iter().all(|t|t.state=="RUNNING"));}
 #[test]fn restore_limit_rolls_back_every_child_instead_of_partially_restoring_a_group(){let(_d,s,root)=fixture();s.set_watch_item_removed("WATCH","parent",true).unwrap();for n in 0..126{watch(&s,&format!("independent-{n}"),&root);}assert_eq!(s.set_watch_item_removed("WATCH","parent",false).unwrap_err(),"WATCH_LIMIT_REACHED");for id in ["parent","pi:one","pi:two"]{assert!(!s.registered_codex_watch(id).unwrap().enabled);}assert_eq!(s.codex_watches().unwrap().len(),127);}
}
