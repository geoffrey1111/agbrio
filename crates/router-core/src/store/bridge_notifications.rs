//! Notification eligibility and exact-source consumption, independent of transports.
use super::*;

pub(super) fn mark_sent_source_handled(c:&Connection,handoff:&str,at:i64)->Result<(),String>{
    let present:bool=c.query_row("SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='role_handoff_details')",[],|r|r.get(0)).map_err(db_error)?;
    if present {c.execute("UPDATE reply_observations SET read_at=COALESCE(read_at,?2),handled_at=COALESCE(handled_at,?2) WHERE EXISTS(SELECT 1 FROM handoffs h JOIN role_handoff_details d ON d.handoff_id=h.id WHERE h.id=?1 AND h.status='SENT' AND h.workstream_id=reply_observations.workstream_id AND h.source_endpoint_id=reply_observations.endpoint_id AND h.source_response_identity=reply_observations.assistant_identity)",params![handoff,at]).map_err(db_error)?;}
    Ok(())
}

pub(super) fn migrate(c:&mut Connection)->Result<(),String>{
    let key="bridge-sent-source-handled-v1";
    if !c.query_row("SELECT EXISTS(SELECT 1 FROM router_feature_migrations WHERE key=?1)",[key],|r|r.get::<_,bool>(0)).map_err(db_error)?{
    let tx=c.transaction_with_behavior(TransactionBehavior::Immediate).map_err(db_error)?;
    tx.execute("UPDATE reply_observations SET read_at=COALESCE(read_at,(SELECT MIN(h.sent_at) FROM handoffs h JOIN role_handoff_details d ON d.handoff_id=h.id WHERE h.status='SENT' AND h.workstream_id=reply_observations.workstream_id AND h.source_endpoint_id=reply_observations.endpoint_id AND h.source_response_identity=reply_observations.assistant_identity)),handled_at=COALESCE(handled_at,(SELECT MIN(h.sent_at) FROM handoffs h JOIN role_handoff_details d ON d.handoff_id=h.id WHERE h.status='SENT' AND h.workstream_id=reply_observations.workstream_id AND h.source_endpoint_id=reply_observations.endpoint_id AND h.source_response_identity=reply_observations.assistant_identity)) WHERE handled_at IS NULL",[]).map_err(db_error)?;
    tx.execute("INSERT INTO router_feature_migrations(key,applied_at) VALUES(?1,?2)",params![key,now()]).map_err(db_error)?;
    tx.commit().map_err(db_error)?;
    }
    let key="deleted-bridge-watches-paused-v1";
    if !c.query_row("SELECT EXISTS(SELECT 1 FROM router_feature_migrations WHERE key=?1)",[key],|r|r.get::<_,bool>(0)).map_err(db_error)?{
        let tx=c.transaction_with_behavior(TransactionBehavior::Immediate).map_err(db_error)?;
        let ids={let mut q=tx.prepare("SELECT id FROM workstreams WHERE trashed_at IS NOT NULL").map_err(db_error)?;let ids=q.query_map([],|r|r.get::<_,String>(0)).map_err(db_error)?.collect::<Result<Vec<_>,_>>().map_err(db_error)?;ids};
        for id in ids{suspend_bound_watches(&tx,&id,now())?;}
        tx.execute("INSERT INTO router_feature_migrations(key,applied_at) VALUES(?1,?2)",params![key,now()]).map_err(db_error)?;tx.commit().map_err(db_error)?;
    }
    Ok(())
}

pub(super) fn suspend_bound_watches(c:&Connection,workstream:&str,at:i64)->Result<(),String>{
    // Retain the subscription and provider task. Explicit Resume can later
    // establish a fresh standalone baseline; another active Bridge wins.
    c.execute("UPDATE codex_watches SET enabled=0,generation=generation+1 WHERE enabled=1 AND thread_id IN (SELECT external_id FROM endpoints WHERE workstream_id=?1 AND provider='CODEX' AND status='ACTIVE') AND NOT EXISTS(SELECT 1 FROM endpoints e JOIN workstreams w ON w.id=e.workstream_id WHERE e.external_id=codex_watches.thread_id AND e.provider='CODEX' AND e.status='ACTIVE' AND w.trashed_at IS NULL AND w.archived_at IS NULL)",[workstream]).map_err(db_error)?;
    c.execute("UPDATE watch_deliveries SET status='SKIPPED',error_code='BRIDGE_DELETED',updated_at=?2 WHERE status IN ('PENDING','FAILED') AND event_sequence IN (SELECT ev.sequence FROM codex_watch_events ev JOIN codex_watches cw ON cw.thread_id=ev.thread_id WHERE cw.enabled=0 AND ev.thread_id IN (SELECT external_id FROM endpoints WHERE workstream_id=?1 AND provider='CODEX' AND status='ACTIVE'))",params![workstream,at]).map_err(db_error)?;
    Ok(())
}

impl RouterStore {
    pub fn endpoint_notification_enabled(&self,endpoint:&str)->Result<bool,String>{self.with_connection(|c|c.query_row("SELECT EXISTS(SELECT 1 FROM endpoints e JOIN workstreams w ON w.id=e.workstream_id WHERE e.id=?1 AND e.status='ACTIVE' AND w.status='ACTIVE' AND w.trashed_at IS NULL AND w.archived_at IS NULL)",[endpoint],|r|r.get(0)).map_err(db_error))}

}

#[cfg(test)]mod tests{
 use super::*;use super::super::{role_bridge::RoleBindingInput,codex_watch::WatchSnapshot};
 fn fixture()->(tempfile::TempDir,RouterStore,String){let d=tempfile::tempdir().unwrap();let s=RouterStore::open_at(d.path().join("router.db")).unwrap();let p=s.create_project("fixture".into(),None).unwrap();let w=s.create_workstream(&p.id,"fixture".into()).unwrap();(d,s,w.id)}
 fn bind(s:&RouterStore,w:&str,root:&Path){let input=|id:&str|RoleBindingInput{provider:"CODEX".into(),external_id:id.into(),label:id.into(),cwd:Some(root.to_string_lossy().into_owned())};s.bind_role_bridge(w,s.role_bridge(w).unwrap().binding_revision,input("source-fixture"),input("target-fixture")).unwrap();}
 #[test]fn deleted_bridge_suspends_only_bound_watch_and_cancels_unclaimed_delivery(){
  let(d,s,w)=fixture();let baseline=WatchSnapshot{state:"IDLE".into(),turn_id:None,item_id:None,text:String::new()};s.enable_codex_watch("source-fixture","bound",d.path().to_str().unwrap(),&baseline).unwrap();s.enable_codex_watch("independent-fixture","other",d.path().to_str().unwrap(),&baseline).unwrap();s.set_watch_delivery_channel("EMAIL",true).unwrap();
  let update=WatchSnapshot{state:"RESULT_READY".into(),turn_id:Some("turn".into()),item_id:Some("item".into()),text:"result".into()};s.record_codex_watch("source-fixture",1,&update).unwrap();s.enqueue_watch_deliveries().unwrap();bind(&s,&w,d.path());let endpoint=s.role_bridge(&w).unwrap().decision.unwrap().endpoint;s.trash_workstream(&w).unwrap();
  assert!(!s.endpoint_notification_enabled(&endpoint.id).unwrap());let bound=s.registered_codex_watch("source-fixture").unwrap();assert!(!bound.enabled);assert_eq!(bound.generation,2);assert!(s.registered_codex_watch("independent-fixture").unwrap().enabled);assert!(s.claim_watch_delivery().unwrap().is_none());assert!(s.watch_delivery_history().unwrap().iter().any(|d|d.status=="SKIPPED"&&d.attempts==0));assert!(s.provider_runs_for_workstream(&w).unwrap().is_empty());
  assert!(!s.record_codex_watch("source-fixture",1,&update).unwrap());s.restore_workstream(&w).unwrap();assert!(s.endpoint_notification_enabled(&endpoint.id).unwrap());assert!(!s.registered_codex_watch("source-fixture").unwrap().enabled);
 }
 #[test]fn acknowledged_handoff_consumes_only_its_exact_source_and_backfills_old_sent_rows(){
  let(d,s,w)=fixture();bind(&s,&w,d.path());let src=s.role_bridge(&w).unwrap().decision.unwrap().endpoint;
  let old=s.record_reply_observation(&w,&src.id,Some("chosen-identity"),"chosen",None).unwrap().unwrap();let newer=s.record_reply_observation(&w,&src.id,Some("newer-identity"),"newer",None).unwrap().unwrap();
  let h=s.prepare_role_handoff(&w,"DECISION",&old.id,"chosen").unwrap();s.approve_role_handoff(&h.id).unwrap();s.claim_role_handoff(&h.id).unwrap();assert!(s.reply_observations_for_workstream(&w).unwrap().iter().find(|r|r.id==old.id).unwrap().handled_at.is_none());s.transition_handoff(&h.id,"SENT",None).unwrap();
  let rows=s.reply_observations_for_workstream(&w).unwrap();assert!(rows.iter().find(|r|r.id==old.id).unwrap().handled_at.is_some());assert!(rows.iter().find(|r|r.id==newer.id).unwrap().handled_at.is_none());
  s.with_connection(|c|{c.execute("UPDATE reply_observations SET handled_at=NULL,read_at=NULL WHERE id=?1",[&old.id]).map_err(db_error)?;c.execute("DELETE FROM router_feature_migrations WHERE key='bridge-sent-source-handled-v1'",[]).map_err(db_error)?;Ok(())}).unwrap();drop(s);let reopened=RouterStore::open_at(d.path().join("router.db")).unwrap();let rows=reopened.reply_observations_for_workstream(&w).unwrap();assert!(rows.iter().find(|r|r.id==old.id).unwrap().handled_at.is_some());assert!(rows.iter().find(|r|r.id==newer.id).unwrap().handled_at.is_none());
 }
 #[test]fn existing_deleted_watch_is_backfilled_once_and_manual_resume_remains_possible(){
  let(d,s,w)=fixture();let baseline=WatchSnapshot{state:"IDLE".into(),turn_id:None,item_id:None,text:String::new()};s.enable_codex_watch("source-fixture","bound",d.path().to_str().unwrap(),&baseline).unwrap();bind(&s,&w,d.path());s.with_connection(|c|{c.execute("UPDATE workstreams SET trashed_at=1 WHERE id=?1",[&w]).map_err(db_error)?;c.execute("DELETE FROM router_feature_migrations WHERE key='deleted-bridge-watches-paused-v1'",[]).map_err(db_error)?;Ok(())}).unwrap();drop(s);
  let s=RouterStore::open_at(d.path().join("router.db")).unwrap();assert!(!s.registered_codex_watch("source-fixture").unwrap().enabled);s.enable_codex_watch("source-fixture","bound",d.path().to_str().unwrap(),&baseline).unwrap();drop(s);let s=RouterStore::open_at(d.path().join("router.db")).unwrap();assert!(s.registered_codex_watch("source-fixture").unwrap().enabled);
 }

}
