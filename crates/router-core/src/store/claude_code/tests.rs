use super::*;
fn legacy_store(path:&Path)->RouterStore {
    let mut connection=Connection::open(path).unwrap();
    connection.execute_batch("PRAGMA foreign_keys=ON").unwrap();
    run_migrations(&mut connection).unwrap();run_normal_feature_migrations(&mut connection).unwrap();
    role_bridge::migrate(&mut connection,path).unwrap();endpoint_claims::migrate(&mut connection,path).unwrap();
    bridge_notifications::migrate(&mut connection).unwrap();goal_control::migrate(&connection).unwrap();assistant_processed::migrate(&connection).unwrap();
    RouterStore{connection:Mutex::new(connection),_preview_profile:None}
}
fn event(root:&Path,kind:&str,sequence:i64)->Event {
    Event {event_id:id(),session_id:"10000000-0000-4000-8000-000000000001".into(),instance_id:"20000000-0000-4000-8000-000000000001".into(),sequence,cwd:root.to_string_lossy().into(),version:"2.1.286".into(),kind:kind.into(),turn_id:None,text:None,completed_at:None}
}
#[test] fn private_credentials_dedupe_order_restart_and_quota_boundary() {
    let temp=tempfile::tempdir().unwrap();let path=temp.path().join("router.db");
    let store=RouterStore::open_at(&path).unwrap();let pairing=store.create_claude_connection().unwrap();
    let start=event(temp.path(),"SESSION_START",1);
    assert!(store.ingest_claude_event("bad",&start).is_err());
    assert!(store.ingest_claude_event(&pairing.token,&start).unwrap());
    assert!(!store.ingest_claude_event(&pairing.token,&start).unwrap());
    let mut conflict=start.clone();conflict.kind="RUNNING".into();conflict.turn_id=Some(id());
    assert_eq!(store.ingest_claude_event(&pairing.token,&conflict).unwrap_err(),"CLAUDE_EVENT_REPLAY_CONFLICT");
    let mut complete=event(temp.path(),"COMPLETE",3);
    complete.turn_id=Some(id());complete.text=Some("完整回复\n\n```text\n保持原文\n```".into());complete.completed_at=Some(now());
    let mut active=event(temp.path(),"RUNNING",2);active.turn_id=complete.turn_id.clone();store.ingest_claude_event(&pairing.token,&active).unwrap();
    assert!(store.ingest_claude_event(&pairing.token,&complete).unwrap());
    assert_eq!(store.claude_session(&start.session_id).unwrap().latest_reply.unwrap().text,complete.text.clone().unwrap());
    drop(store);let store=RouterStore::open_at(&path).unwrap();
    assert!(!store.ingest_claude_event(&pairing.token,&complete).unwrap());
    let mut reordered=event(temp.path(),"RUNNING",1);reordered.turn_id=complete.turn_id.clone();
    assert_eq!(store.ingest_claude_event(&pairing.token,&reordered).unwrap_err(),"CLAUDE_EVENT_OUT_OF_ORDER");
    let mut next=event(temp.path(),"RUNNING",4);next.turn_id=Some(id());
    store.ingest_claude_event(&pairing.token,&next).unwrap();
    assert!(store.claude_session(&start.session_id).unwrap().latest_reply.is_none());
    assert!(!pairing.writes_enabled);assert!(!store.claude_catalog().unwrap().writes_enabled);
    store.revoke_claude_connection(&pairing.connection_id).unwrap();
    assert!(store.claude_catalog().unwrap().sessions.is_empty());
    assert!(store.claude_session(&start.session_id).is_err());
    assert!(store.ingest_claude_event(&pairing.token,&event(temp.path(),"HEARTBEAT",5)).is_err());
}
#[test] fn connection_root_instance_and_completion_fences() {
    let temp=tempfile::tempdir().unwrap();let other=tempfile::tempdir().unwrap();
    let store=RouterStore::open_at(temp.path().join("router.db")).unwrap();
    let a=store.create_claude_connection().unwrap();let b=store.create_claude_connection().unwrap();
    let start=event(temp.path(),"SESSION_START",1);store.ingest_claude_event(&a.token,&start).unwrap();
    assert_eq!(store.ingest_claude_event(&b.token,&event(temp.path(),"HEARTBEAT",2)).unwrap_err(),"CLAUDE_SESSION_ALREADY_OWNED");
    let mut changed=event(other.path(),"RUNNING",2);changed.turn_id=Some(id());
    assert_eq!(store.ingest_claude_event(&a.token,&changed).unwrap_err(),"CLAUDE_ROOT_CHANGED");
    changed=event(temp.path(),"SESSION_START",1);changed.instance_id=id();
    assert_eq!(store.ingest_claude_event(&a.token,&changed).unwrap_err(),"CLAUDE_SESSION_INSTANCE_CHANGED");
    let incomplete=event(temp.path(),"COMPLETE",2);
    assert_eq!(store.ingest_claude_event(&a.token,&incomplete).unwrap_err(),"CLAUDE_COMPLETE_REPLY_UNPROVEN");
    store.with_connection(|c|{c.execute("UPDATE claude_sessions SET lease_until=0",[]).map_err(db_error)?;Ok(())}).unwrap();
    store.ingest_claude_event(&a.token,&changed).unwrap();
    assert!(store.ingest_claude_event(&a.token,&event(temp.path(),"HEARTBEAT",3)).is_err());
    store.with_connection(|c|{c.execute("UPDATE claude_connections SET expires_at=0",[]).map_err(db_error)?;Ok(())}).unwrap();
    assert!(store.claude_catalog().unwrap().sessions.is_empty());
    let replacement=store.create_claude_connection().unwrap();
    let mut recovered=event(temp.path(),"SESSION_START",1);recovered.instance_id=id();
    store.ingest_claude_event(&replacement.token,&recovered).unwrap();
    assert_eq!(store.claude_catalog().unwrap().sessions.len(),1);
    assert!(store.ingest_claude_event(&a.token,&event(temp.path(),"HEARTBEAT",5)).is_err());
}
#[test] fn provider_migration_preserves_existing_history_and_foreign_keys() {
    let temp=tempfile::tempdir().unwrap();let path=temp.path().join("router.db");
    let store=legacy_store(&path);
    let project=store.create_project("fixture".into(),None).unwrap();let w=store.create_workstream(&project.id,"fixture".into()).unwrap();
    store.bind_role_bridge(&w.id,0,role_bridge::RoleBindingInput{provider:"CODEX".into(),external_id:"existing-thread-a".into(),label:"A".into(),cwd:Some(temp.path().to_string_lossy().into())},role_bridge::RoleBindingInput{provider:"CODEX".into(),external_id:"existing-thread-b".into(),label:"B".into(),cwd:Some(temp.path().to_string_lossy().into())}).unwrap();
    let bridge=store.role_bridge(&w.id).unwrap();
    let observation=store.record_reply_observation(&w.id,&bridge.decision.unwrap().endpoint.id,Some("codex:fictional-turn:fictional-message"),"old private fixture",None).unwrap().unwrap();
    let handoff=store.prepare_role_handoff(&w.id,"DECISION",&observation.id,"keep bytes\n\n```text\nfixture\n```").unwrap();
    store.approve_role_handoff(&handoff.id).unwrap();store.claim_role_handoff(&handoff.id).unwrap();
    let snapshot=serde_json::to_value(store.snapshot().unwrap()).unwrap();
    let schema=store.with_connection(|c|{
        let mut s=c.prepare("SELECT name,sql FROM sqlite_master WHERE type IN ('index','trigger') AND sql IS NOT NULL ORDER BY name").unwrap();
        let rows=s.query_map([],|r|Ok((r.get::<_,String>(0)?,r.get::<_,String>(1)?))).unwrap().collect::<Result<Vec<_>,_>>().unwrap();Ok(rows)
    }).unwrap();drop(store);
    let store=RouterStore::open_at(&path).unwrap();
    assert_eq!(serde_json::to_value(store.snapshot().unwrap()).unwrap(),snapshot);
    assert_eq!(store.role_handoff(&handoff.id).unwrap().status,"SENDING");
    let after=store.with_connection(|c|{
        let mut s=c.prepare("SELECT name,sql FROM sqlite_master WHERE type IN ('index','trigger') AND sql IS NOT NULL ORDER BY name").unwrap();
        let rows=s.query_map([],|r|Ok((r.get::<_,String>(0)?,r.get::<_,String>(1)?))).unwrap().collect::<Result<Vec<_>,_>>().unwrap();Ok(rows)
    }).unwrap();assert_eq!(schema,after);
    store.with_connection(|c| {assert_eq!(c.query_row("PRAGMA integrity_check",[],|r|r.get::<_,String>(0)).unwrap(),"ok");assert!(!c.prepare("PRAGMA foreign_key_check").unwrap().exists([]).unwrap());assert_eq!(c.query_row("PRAGMA foreign_keys",[],|r|r.get::<_,i64>(0)).unwrap(),1);Ok(())}).unwrap();
}
#[test] fn failed_migration_rolls_back_schema_and_restores_foreign_key_enforcement() {
    let temp=tempfile::tempdir().unwrap();let path=temp.path().join("router.db");let store=legacy_store(&path);
    store.with_connection(|c|{
        let before:String=c.query_row("SELECT sql FROM sqlite_master WHERE name='endpoints'",[],|r|r.get(0)).unwrap();
        c.execute_batch("PRAGMA foreign_keys=OFF; INSERT INTO workstreams(id,project_id,name,status,created_at,updated_at) VALUES('orphan-fixture','missing-fixture','fixture','ACTIVE',1,1); PRAGMA foreign_keys=ON;").unwrap();
        assert_eq!(migrate(c,&path).unwrap_err(),"CLAUDE_MIGRATION_FOREIGN_KEY_INVALID");
        assert_eq!(c.query_row("SELECT sql FROM sqlite_master WHERE name='endpoints'",[],|r|r.get::<_,String>(0)).unwrap(),before);
        assert_eq!(c.query_row("PRAGMA foreign_keys",[],|r|r.get::<_,i64>(0)).unwrap(),1);
        assert!(!c.query_row("SELECT EXISTS(SELECT 1 FROM router_feature_migrations WHERE key='claude-code-adapter-v1')",[],|r|r.get::<_,bool>(0)).unwrap());
        Ok(())
    }).unwrap();
}
