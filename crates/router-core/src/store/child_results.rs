//! Derived read-only subscriptions. Never registers an independently writable child watch.
use super::*;
pub(super) fn eligible(c:&Connection,seq:i64)->Result<Option<bool>,String>{
 let row=c.query_row("SELECT root,cwd,generation FROM child_result_events WHERE sequence=?1",[seq],|r|Ok((r.get::<_,String>(0)?,r.get::<_,String>(1)?,r.get::<_,i64>(2)?))).optional().map_err(db_error)?;
 let Some((root,cwd,generation))=row else{return Ok(None)};
 let active=codex_watch::chat_context(c,&root).is_ok_and(|w|w.enabled&&w.cwd==cwd&&w.generation==generation);
 let enabled:bool=c.query_row("SELECT EXISTS(SELECT 1 FROM child_result_preferences WHERE root=?1 AND cwd=?2 AND generation=?3 AND enabled=1)",params![root,cwd,generation],|r|r.get(0)).map_err(db_error)?;
 Ok(Some(active&&enabled))
}
impl RouterStore {
 pub fn child_result_event_target(&self,seq:i64)->Result<Option<(String,String)>,String>{self.with_connection(|c|c.query_row("SELECT root,child FROM child_result_events WHERE sequence=?1",[seq],|r|Ok((r.get(0)?,r.get(1)?))).optional().map_err(db_error))}
 pub fn child_results_enabled(&self,root:&str)->Result<bool,String>{self.with_connection(|c|{let w=codex_watch::chat_context(c,root)?;c.query_row("SELECT EXISTS(SELECT 1 FROM child_result_preferences WHERE root=?1 AND cwd=?2 AND generation=?3 AND enabled=1)",params![root,w.cwd,w.generation],|r|r.get(0)).map_err(db_error)})}
 pub fn set_child_results(&self,root:&str,generation:i64,enabled:bool,baseline:&[(String,String)])->Result<(),String>{self.with_connection(|c|{
  let tx=c.transaction_with_behavior(TransactionBehavior::Immediate).map_err(db_error)?;let w=codex_watch::chat_context(&tx,root)?;if !w.enabled||w.generation!=generation{return Err("REPLY_TARGET_CHANGED_REFRESH".into())}
  tx.execute("INSERT INTO child_result_preferences VALUES(?1,?2,?3,?4,?5) ON CONFLICT(root) DO UPDATE SET cwd=excluded.cwd,generation=excluded.generation,enabled=excluded.enabled,enabled_at=excluded.enabled_at",params![root,w.cwd,generation,enabled,now()]).map_err(db_error)?;
  tx.execute("DELETE FROM child_result_baselines WHERE root=?1",[root]).map_err(db_error)?;
  for (child,identity) in baseline {tx.execute("INSERT INTO child_result_baselines VALUES(?1,?2,?3)",params![root,child,identity]).map_err(db_error)?;}
  tx.execute("UPDATE watch_deliveries SET status='SKIPPED',error_code='CHILD_SETTING_CHANGED' WHERE status IN ('PENDING','FAILED') AND event_sequence IN(SELECT sequence FROM child_result_events WHERE root=?1)",[root]).map_err(db_error)?;
  tx.commit().map_err(db_error)
 })}
 pub fn record_child_result(&self,root:&str,generation:i64,child:&str,label:&str,snapshot:&codex_watch::WatchSnapshot,result_at:i64)->Result<bool,String>{self.with_connection(|c|{
  let tx=c.transaction_with_behavior(TransactionBehavior::Immediate).map_err(db_error)?;let w=codex_watch::chat_context(&tx,root)?;if !w.enabled||w.generation!=generation{return Ok(false)}
  let enabled:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM child_result_preferences WHERE root=?1 AND cwd=?2 AND generation=?3 AND enabled=1)",params![root,w.cwd,generation],|r|r.get(0)).map_err(db_error)?;if !enabled{return Ok(false)}
  let identity=serde_json::to_string(&(snapshot.state.as_str(),snapshot.turn_id.as_ref(),snapshot.item_id.as_ref())).map_err(|_|"CHILD_RESULT_INVALID")?;
  let previous:Option<String>=tx.query_row("SELECT identity FROM child_result_baselines WHERE root=?1 AND child=?2",params![root,child],|r|r.get(0)).optional().map_err(db_error)?;
  tx.execute("INSERT INTO child_result_baselines VALUES(?1,?2,?3) ON CONFLICT(root,child) DO UPDATE SET identity=excluded.identity",params![root,child,identity]).map_err(db_error)?;
  // Existing descendants are baselined on opt-in. A newly spawned child may
  // finish between polls; only a native completion after opt-in can notify.
  let activated:i64=tx.query_row("SELECT enabled_at FROM child_result_preferences WHERE root=?1",[root],|r|r.get(0)).map_err(db_error)?;
  let emit=previous.map(|old|old!=identity).unwrap_or(result_at>=activated)&&snapshot.state=="RESULT_READY"&&!snapshot.text.is_empty();
  if emit {let mut event_snapshot=snapshot.clone();event_snapshot.item_id=Some(format!("child_result:{}:{}",child,snapshot.item_id.as_deref().unwrap_or("")));let raw=serde_json::to_string(&event_snapshot).map_err(|_|"CHILD_RESULT_INVALID")?;tx.execute("INSERT INTO codex_watch_events(thread_id,label,cwd,snapshot_json,observed_at) VALUES(?1,?2,?3,?4,?5)",params![root,format!("{} · 子对话：{}",w.label,label),w.cwd,raw,now()]).map_err(db_error)?;let seq=tx.last_insert_rowid();tx.execute("INSERT INTO child_result_events VALUES(?1,?2,?3,?4,?5)",params![seq,root,child,w.cwd,generation]).map_err(db_error)?;codex_watch::prune_events(&tx)?;}
  tx.commit().map_err(db_error)?;Ok(emit)
 })}
 pub fn watch_sequence_delivery_enabled(&self,seq:i64,thread:&str)->Result<bool,String>{self.with_connection(|c|{if let Some(enabled)=eligible(c,seq)?{return Ok(enabled)}watch_delivery::event_delivery_enabled(c,thread)})}
}

#[cfg(test)]mod tests{
 use super::*;
 fn snap(state:&str,turn:&str)->codex_watch::WatchSnapshot{codex_watch::WatchSnapshot{state:state.into(),turn_id:Some(turn.into()),item_id:Some("item".into()),text:"fictional result".into()}}
 #[test]fn default_off_baseline_future_results_dedupe_disable_and_restart(){
 let d=tempfile::tempdir().unwrap();let path=d.path().join("router.db");let s=RouterStore::open_at(&path).unwrap();s.enable_codex_watch("root","manager",d.path().to_str().unwrap(),&snap("IDLE","root")).unwrap();
 assert!(!s.child_results_enabled("root").unwrap());assert!(!s.record_child_result("root",1,"child","child",&snap("RESULT_READY","old"),now()+1).unwrap());
 let identity=serde_json::to_string(&("RESULT_READY",Some("old"),Some("item"))).unwrap();s.set_child_results("root",1,true,&[("child".into(),identity)]).unwrap();assert!(!s.record_child_result("root",1,"child","child",&snap("RESULT_READY","old"),now()+1).unwrap());
 assert!(!s.record_child_result("root",1,"historical-child","historical",&snap("RESULT_READY","ancient"),0).unwrap());
 s.set_watch_delivery_channel("EMAIL",true).unwrap();assert!(!s.record_child_result("root",1,"child","child",&snap("RUNNING","next"),now()+1).unwrap());assert!(s.record_child_result("root",1,"child","child",&snap("RESULT_READY","next"),now()+1).unwrap());assert!(!s.record_child_result("root",1,"child","child",&snap("RESULT_READY","next"),now()+1).unwrap());s.enqueue_watch_deliveries().unwrap();let(channel,event)=s.claim_watch_delivery().unwrap().unwrap();assert_eq!(channel,"EMAIL");assert_eq!(event.thread_id,"root");assert!(s.watch_sequence_delivery_enabled(event.sequence,"root").unwrap());
 drop(s);let s=RouterStore::open_at(&path).unwrap();assert!(s.child_results_enabled("root").unwrap());assert!(!s.record_child_result("root",1,"child","child",&snap("RESULT_READY","next"),now()+1).unwrap());s.set_child_results("root",1,false,&[]).unwrap();assert!(!s.watch_sequence_delivery_enabled(event.sequence,"root").unwrap());assert!(!s.record_child_result("root",1,"child","child",&snap("RESULT_READY","later"),now()+1).unwrap());
 s.set_child_results("root",1,true,&[]).unwrap();s.pause_codex_watch("root").unwrap();assert!(!s.watch_sequence_delivery_enabled(event.sequence,"root").unwrap());assert!(s.set_child_results("root",1,true,&[]).is_err());
 }
 #[test]fn bound_tree_subscription_is_scoped_to_exact_bridge_revision(){
 let d=tempfile::tempdir().unwrap();let s=RouterStore::open_at(d.path().join("router.db")).unwrap();let p=s.create_project("fixture".into(),None).unwrap();let w=s.create_workstream(&p.id,"fixture".into()).unwrap();
 let side=|id:&str|role_bridge::RoleBindingInput{provider:"CODEX".into(),external_id:id.into(),label:id.into(),cwd:Some(d.path().to_string_lossy().into())};
 s.bind_role_bridge(&w.id,w.binding_revision,side("root-fixture"),side("other-fixture")).unwrap();let context=s.codex_chat_context("root-fixture").unwrap();assert!(context.generation<0);
 s.set_child_results("root-fixture",context.generation,true,&[]).unwrap();assert!(!s.record_child_result("root-fixture",context.generation,"child","child",&snap("RUNNING","t"),now()+1).unwrap());assert!(s.record_child_result("root-fixture",context.generation,"child","child",&snap("RESULT_READY","t"),now()+1).unwrap());let event=s.codex_watch_feed(0).unwrap().events.pop().unwrap();assert!(s.watch_sequence_delivery_enabled(event.sequence,"root-fixture").unwrap());
 let bridge=s.role_bridge(&w.id).unwrap();s.bind_role_bridge(&w.id,bridge.binding_revision,side("root-fixture"),side("replacement")).unwrap();assert!(!s.child_results_enabled("root-fixture").unwrap());assert!(!s.watch_sequence_delivery_enabled(event.sequence,"root-fixture").unwrap());
 }

 #[test]fn migration_preserves_notification_identity_seen_originals_and_delivery_audit(){
 let d=tempfile::tempdir().unwrap();let path=d.path().join("router.db");let s=RouterStore::open_at(&path).unwrap();s.enable_codex_watch("fixture-root","manager",d.path().to_str().unwrap(),&snap("IDLE","root")).unwrap();s.set_watch_delivery_channel("EMAIL",true).unwrap();s.record_codex_watch("fixture-root",1,&snap("RESULT_READY","final")).unwrap();let before=s.codex_watch_feed(0).unwrap().events.pop().unwrap();s.mark_codex_watch_event_seen(before.sequence).unwrap();s.enqueue_watch_deliveries().unwrap();
 s.with_connection(|c|{c.execute_batch("DROP TABLE child_result_events;DROP TABLE child_result_baselines;DROP TABLE child_result_preferences;DELETE FROM router_feature_migrations WHERE key='child-results-v1';").map_err(db_error)}).unwrap();drop(s);
 let s=RouterStore::open_at(&path).unwrap();let after=s.codex_watch_event(before.sequence).unwrap();assert_eq!(before.snapshot,after.snapshot);assert_eq!(before.thread_id,after.thread_id);assert!(after.seen_at.is_some());assert_eq!(s.watch_delivery_history().unwrap().len(),1);s.with_connection(|c|{let failures:i64=c.query_row("SELECT COUNT(*) FROM pragma_foreign_key_check",[],|r|r.get(0)).map_err(db_error)?;assert_eq!(failures,0);Ok(())}).unwrap();
 s.record_codex_watch("fixture-root",1,&snap("RESULT_READY","newer")).unwrap();assert!(s.codex_watch_feed(0).unwrap().next_cursor>before.sequence);
 }

 #[test]fn a_new_child_finishing_between_polls_is_not_missed_and_old_reparented_results_are_not_replayed(){
 let d=tempfile::tempdir().unwrap();let s=RouterStore::open_at(d.path().join("router.db")).unwrap();s.enable_codex_watch("root","manager",d.path().to_str().unwrap(),&snap("IDLE","root")).unwrap();s.set_child_results("root",1,true,&[]).unwrap();
 assert!(!s.record_child_result("root",1,"old-child","old",&snap("RESULT_READY","old"),0).unwrap());
 assert!(s.record_child_result("root",1,"new-child","new",&snap("RESULT_READY","new"),now()+1).unwrap());
 assert!(!s.record_child_result("root",1,"new-child","new",&snap("RESULT_READY","new"),now()+1).unwrap());
 }

 #[test]fn pruning_an_original_cannot_erase_an_inflight_child_delivery_fence(){
 let d=tempfile::tempdir().unwrap();let s=RouterStore::open_at(d.path().join("router.db")).unwrap();s.enable_codex_watch("root","manager",d.path().to_str().unwrap(),&snap("IDLE","root")).unwrap();s.set_child_results("root",1,true,&[]).unwrap();s.record_child_result("root",1,"child","child",&snap("RESULT_READY","new"),now()+1).unwrap();let seq=s.codex_watch_feed(0).unwrap().events[0].sequence;
 s.with_connection(|c|{c.execute("DELETE FROM codex_watch_events WHERE sequence=?1",[seq]).map_err(db_error)?;Ok(())}).unwrap();assert!(s.child_result_event_target(seq).unwrap().is_some());s.set_child_results("root",1,false,&[]).unwrap();assert!(!s.watch_sequence_delivery_enabled(seq,"root").unwrap());
 }

}
