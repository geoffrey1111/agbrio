use super::*;
use assistant::{BriefRule,InstanceGrantInput,GrantInput};
use assistant_actions::{ActionInput,ActionApproval};
fn rules()->Vec<BriefRule>{vec![BriefRule{id:"routine".into(),text:"Carry out routine approved operations; ask about changes".into()}]}
#[test]fn revoking_instance_authority_blocks_a_delayed_direct_reply_at_its_physical_claim(){
 let d=tempfile::tempdir().unwrap();let s=RouterStore::open_at(d.path().join("test.db")).unwrap();let g=global(&s);
 let snapshot=codex_watch::WatchSnapshot{state:"IDLE".into(),turn_id:None,item_id:None,text:String::new()};s.enable_codex_watch("demo-thread","Demo","D:\\demo",&snapshot).unwrap();
 let a=s.prepare_assistant_action(&g.id,ActionInput{request_id:"direct".into(),operation:"SEND_CHAT".into(),input:serde_json::json!({"threadId":"demo-thread","text":"Demo message"})}).unwrap();
 s.claim_assistant_action(&g.id,ActionApproval{action_id:a.id.clone(),expected_hash:a.payload_hash.clone(),rule_id:Some("routine".into()),use_owner_answer:false,assessment:"Covered follow-up".into()}).unwrap();
 s.prepare_watch_reply(&watch_reply::WatchReply{id:a.id.clone(),thread_id:"demo-thread".into(),cwd:"D:\\demo".into(),generation:1,source_sequence:None,expected_turn_id:None,mode:"QUEUE".into(),text:"Demo message".into(),options:serde_json::json!({"attachments":[]}),payload_hash:String::new(),status:String::new(),turn_id:None,error_code:None,created_at:0,updated_at:0}).unwrap();
 s.finish_assistant_action(&a.id,Ok(serde_json::json!({"status":"QUEUED"}))).unwrap();s.revoke_assistant_grant(&g.id).unwrap();
 assert!(s.claim_watch_reply(&a.id).unwrap_err().contains("REVOKED"));assert_eq!(s.watch_reply(&a.id).unwrap().unwrap().status,"QUEUED");
}
#[test]fn old_grant_token_draft_answer_and_approval_survive_the_table_rebuild(){
 let d=tempfile::tempdir().unwrap();let path=d.path().join("migration.db");let s=RouterStore::open_at(&path).unwrap();let p=s.create_project("Demo".into(),None).unwrap();let w=bridge(&s,&p.id,"Migration");
 let b=s.role_bridge(&w).unwrap();let source=b.decision.unwrap().endpoint;s.record_reply_observation(&w,&source.id,Some("original"),"Original bytes",None).unwrap();let obs=s.reply_observations_for_workstream(&w).unwrap().remove(0);
 let g=s.create_assistant_grant(GrantInput{workstream_id:w.clone(),source_role:"DECISION".into(),binding_revision:1,label:"Old owner grant".into(),rules:rules(),expires_at:now()+600000}).unwrap();let h=s.prepare_role_handoff(&w,"DECISION",&obs.id,"Original payload").unwrap();s.register_assistant_draft(&g.id,&h.id,&obs.id).unwrap();
 let decision=s.ask_assistant_decision(&g.id,&h.id,&h.payload_hash,"Continue?").unwrap();s.answer_assistant_decision(&g.id,&decision.id,&h.payload_hash,"Continue","user-message-original").unwrap();s.approve_assistant_handoff(&g.id,assistant::ApprovalInput{handoff_id:h.id.clone(),expected_hash:h.payload_hash.clone(),rule_id:None,decision_id:Some(decision.id),assessment:"Owner answered".into()}).unwrap();let before=s.assistant_approval(&h.id).unwrap();let token=s.issue_assistant_access(&g.id,"https://demo.invalid/mcp",now()+60000).unwrap();
 // Reproduce the actual pre-instance schema, retaining every dependency byte.
 s.with_connection(|c|{let tx=c.transaction_with_behavior(TransactionBehavior::Immediate).map_err(db_error)?;let names=["assistant_grants","assistant_drafts","assistant_decisions","assistant_approvals","assistant_access"];
  for n in names{tx.execute_batch(&format!("CREATE TEMP TABLE _old_{n} AS SELECT * FROM {n};")).map_err(db_error)?;}
  tx.execute_batch("DROP TABLE assistant_actions;").map_err(db_error)?;for n in names.into_iter().rev(){tx.execute_batch(&format!("DROP TABLE {n};")).map_err(db_error)?;}
  tx.execute_batch(include_str!("../../migrations/normal/012_assistant_delegation.sql")).map_err(db_error)?;
  for n in names{let cols=if n=="assistant_grants"{"id,workstream_id,source_role,binding_revision,label,brief_json,created_at,expires_at,revoked_at"}else{"*"};tx.execute_batch(&format!("INSERT INTO {n} SELECT {cols} FROM _old_{n};DROP TABLE _old_{n};")).map_err(db_error)?;}
  tx.execute("DELETE FROM router_feature_migrations WHERE key='assistant-instance-v1'",[]).map_err(db_error)?;tx.commit().map_err(db_error)
 }).unwrap();drop(s);
 let s=RouterStore::open_at(path).unwrap();assert_eq!(s.authenticate_assistant(&token,"https://demo.invalid/mcp").unwrap().scope,"BRIDGE");assert_eq!(s.assistant_approval(&h.id).unwrap(),before);assert_eq!(s.assistant_decisions(&g.id).unwrap()[0].answer_reference.as_deref(),Some("user-message-original"));s.require_assistant_send(&g.id,&h.id,&h.payload_hash).unwrap();
 s.with_connection(|c|{assert!(!c.prepare("PRAGMA foreign_key_check").unwrap().query([]).unwrap().next().unwrap().is_some());Ok(())}).unwrap();assert_eq!(global(&s).scope,"INSTANCE");
}
fn global(s:&RouterStore)->assistant::AssistantGrant{s.create_assistant_instance_grant(InstanceGrantInput{label:"QA assistant".into(),rules:rules(),expires_at:now()+600000}).unwrap()}
fn bridge(s:&RouterStore,p:&str,name:&str)->String{
 let w=s.create_workstream(p,name.into()).unwrap();let side=|suffix:&str|role_bridge::RoleBindingInput{provider:"CODEX".into(),external_id:format!("demo-{name}-{suffix}"),label:suffix.into(),cwd:Some("D:\\demo".into())};
 s.bind_role_bridge(&w.id,0,side("d"),side("e")).unwrap();w.id
}
#[test]fn global_authority_includes_future_bridges_while_old_token_stays_scoped(){
 let d=tempfile::tempdir().unwrap();let s=RouterStore::open_at(d.path().join("test.db")).unwrap();let g=global(&s);
 assert_eq!(g.scope,"INSTANCE");assert!(g.workstream_id.is_empty());
 let p=s.create_project("Demo".into(),None).unwrap();let a=bridge(&s,&p.id,"A");let b=bridge(&s,&p.id,"B");
 for id in [&a,&b]{assert!(s.require_assistant_bridge(&g.id,id,Some(1)).is_ok());}
 let old=s.create_assistant_grant(GrantInput{workstream_id:a.clone(),source_role:"BOTH".into(),binding_revision:1,label:"Old".into(),rules:rules(),expires_at:now()+600000}).unwrap();
 let token=s.issue_assistant_access(&old.id,"https://demo.invalid/mcp",now()+60000).unwrap();assert_eq!(s.authenticate_assistant(&token,"https://demo.invalid/mcp").unwrap().scope,"BRIDGE");
 assert!(s.require_assistant_bridge(&old.id,&b,None).is_err());assert!(s.require_assistant_bridge(&g.id,&b,Some(2)).is_err());
 s.trash_workstream(&a).unwrap();assert!(s.require_assistant_bridge(&g.id,&a,None).is_err());assert!(s.require_assistant_bridge(&g.id,&b,None).is_ok());assert!(s.assistant_grant(&old.id).is_err());
 let token=s.issue_assistant_access(&g.id,"https://demo.invalid/mcp",now()+60000).unwrap();s.revoke_assistant_grant(&g.id).unwrap();assert!(s.authenticate_assistant(&token,"https://demo.invalid/mcp").is_err());
}
#[test]fn global_handoffs_keep_exact_recipient_and_revocation_inside_claim(){
 let d=tempfile::tempdir().unwrap();let s=RouterStore::open_at(d.path().join("test.db")).unwrap();let p=s.create_project("Demo".into(),None).unwrap();let g=global(&s);let mut hs=vec![];
 for name in ["A","B"]{let w=bridge(&s,&p.id,name);let b=s.role_bridge(&w).unwrap();let src=b.decision.unwrap().endpoint;
  s.record_reply_observation(&w,&src.id,Some("demo-result"),"Continue demo",None).unwrap();let obs=s.reply_observations_for_workstream(&w).unwrap().remove(0);
  let h=s.prepare_role_handoff(&w,"DECISION",&obs.id,"Exact demo instructions").unwrap();s.register_assistant_draft(&g.id,&h.id,&obs.id).unwrap();
  s.approve_assistant_handoff(&g.id,assistant::ApprovalInput{handoff_id:h.id.clone(),expected_hash:h.payload_hash.clone(),rule_id:Some("routine".into()),decision_id:None,assessment:"Covered by the owner brief".into()}).unwrap();hs.push(h);
 }
 assert_ne!(hs[0].destination_endpoint.id,hs[1].destination_endpoint.id);s.claim_role_handoff(&hs[0].id).unwrap();assert!(s.claim_role_handoff(&hs[0].id).is_err());
 s.revoke_assistant_grant(&g.id).unwrap();assert!(s.claim_role_handoff(&hs[1].id).unwrap_err().contains("REVOKED"));assert_eq!(s.role_handoff(&hs[1].id).unwrap().status,"APPROVED");
}
#[test]fn global_action_question_and_retry_do_not_bypass_the_owner_or_repeat(){
 let d=tempfile::tempdir().unwrap();let s=RouterStore::open_at(d.path().join("test.db")).unwrap();let g=global(&s);
 let input=ActionInput{request_id:"one-operation".into(),operation:"CREATE_BRIDGE".into(),input:serde_json::json!({"name":"Demo"})};
 let a=s.prepare_assistant_action(&g.id,input.clone()).unwrap();assert_eq!(s.prepare_assistant_action(&g.id,input).unwrap().id,a.id);
 assert!(s.prepare_assistant_action(&g.id,ActionInput{request_id:"one-operation".into(),operation:"CREATE_BRIDGE".into(),input:serde_json::json!({"name":"Changed"})}).is_err());
 let approval=||ActionApproval{action_id:a.id.clone(),expected_hash:a.payload_hash.clone(),rule_id:Some("routine".into()),use_owner_answer:false,assessment:"Routine operation".into()};
 s.ask_assistant_action(&g.id,&a.id,&a.payload_hash,"What should the new Bridge be called?").unwrap();assert!(s.claim_assistant_action(&g.id,approval()).is_err());
 assert!(s.answer_assistant_action(&g.id,&a.id,"wrong hash","Demo","user-message-1").is_err());s.answer_assistant_action(&g.id,&a.id,&a.payload_hash,"Demo","user-message-1").unwrap();
 let mut answer=approval();answer.rule_id=None;answer.use_owner_answer=true;s.claim_assistant_action(&g.id,answer).unwrap();assert!(s.claim_assistant_action(&g.id,approval()).is_err());
 s.finish_assistant_action(&a.id,Ok(serde_json::json!({"workstreamId":"demo-id"}))).unwrap();assert_eq!(s.assistant_action(&g.id,&a.id,None).unwrap().status,"APPLIED");
 assert!(s.finish_assistant_action(&a.id,Ok(serde_json::json!({}))).is_err());
 drop(s);let s=RouterStore::open_at(d.path().join("test.db")).unwrap();assert_eq!(s.assistant_action(&g.id,&a.id,None).unwrap().answer_reference.as_deref(),Some("user-message-1"));
}
#[test]fn seen_notifications_are_exact_persistent_and_do_not_delete_or_pause(){
 let d=tempfile::tempdir().unwrap();let path=d.path().join("test.db");let s=RouterStore::open_at(&path).unwrap();let snap=|id:&str|codex_watch::WatchSnapshot{state:"RESULT_READY".into(),turn_id:Some(id.into()),item_id:Some("item".into()),text:id.into()};
 s.enable_codex_watch("qa","Demo","D:\\demo",&snap("baseline")).unwrap();s.record_codex_watch("qa",1,&snap("one")).unwrap();s.record_codex_watch("qa",1,&snap("two")).unwrap();let events=s.codex_watch_feed(0).unwrap().events;
 s.mark_codex_watch_event_seen(events[0].sequence).unwrap();let at=s.codex_watch_event(events[0].sequence).unwrap().seen_at;assert!(at.is_some());s.mark_codex_watch_event_seen(events[0].sequence).unwrap();assert_eq!(s.codex_watch_event(events[0].sequence).unwrap().seen_at,at);
 assert!(s.codex_watch_event(events[1].sequence).unwrap().seen_at.is_none());assert_eq!(s.codex_watch_feed(0).unwrap().events.len(),2);assert!(s.codex_watches().unwrap()[0].enabled);
 drop(s);let s=RouterStore::open_at(path).unwrap();assert_eq!(s.codex_watch_event(events[0].sequence).unwrap().seen_at,at);
}
