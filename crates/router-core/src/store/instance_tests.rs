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
  for n in names{let cols=if n=="assistant_grants"{"id,workstream_id,source_role,binding_revision,label,brief_json,created_at,expires_at,revoked_at"}else if n=="assistant_drafts"{"handoff_id,grant_id,observation_id,request_id,request_hash"}else{"*"};tx.execute_batch(&format!("INSERT INTO {n} SELECT {cols} FROM _old_{n};DROP TABLE _old_{n};")).map_err(db_error)?;}
  tx.execute("DELETE FROM router_feature_migrations WHERE key IN ('assistant-instance-v1','assistant-conversation-review-v1')",[]).map_err(db_error)?;tx.commit().map_err(db_error)
 }).unwrap();drop(s);
 let s=RouterStore::open_at(path).unwrap();assert_eq!(s.authenticate_assistant(&token,"https://demo.invalid/mcp").unwrap().approval_mode,"BRIEF_RULES");assert_eq!(s.assistant_approval(&h.id).unwrap(),before);assert_eq!(s.assistant_decisions(&g.id).unwrap()[0].answer_reference.as_deref(),Some("user-message-original"));s.require_assistant_send(&g.id,&h.id,&h.payload_hash).unwrap();
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

fn connected(s:&RouterStore)->assistant::AssistantGrant{
 s.connect_assistant_instance(assistant::AssistantConnectionInput{label:"Dot QA".into(),expires_at:now()+600000}).unwrap()
}
fn review_draft(s:&RouterStore,g:&str,name:&str)->HandoffHistoryItem{
 let p=s.create_project(format!("Demo {name}"),None).unwrap();let w=bridge(s,&p.id,name);let src=s.role_bridge(&w).unwrap().decision.unwrap().endpoint;
 s.record_reply_observation(&w,&src.id,Some("result"),"Review finished; continue the approved reward display",None).unwrap();let o=s.reply_observations_for_workstream(&w).unwrap().remove(0);
 let h=s.prepare_role_handoff(&w,"DECISION",&o.id,"Continue reward display").unwrap();s.register_assistant_draft(g,&h.id,&o.id).unwrap();h
}
fn reviewed(h:&HandoffHistoryItem)->assistant::ApprovalInput{assistant::ApprovalInput{handoff_id:h.id.clone(),expected_hash:h.payload_hash.clone(),rule_id:None,decision_id:None,assessment:"Source confirms review completed; owner conversation permits routine continuation".into()}}
#[test]fn conversation_review_needs_no_brief_but_preserves_exact_send_and_legacy_limits(){
 let d=tempfile::tempdir().unwrap();let s=RouterStore::open_at(d.path().join("test.db")).unwrap();let g=connected(&s);assert!(g.rules.is_empty());assert_eq!(g.approval_mode,"CONVERSATION_REVIEW");assert_eq!(g.scope,"INSTANCE");
 let h=review_draft(&s,&g.id,"A");let mut wrong=reviewed(&h);wrong.expected_hash="wrong".into();assert!(s.approve_assistant_handoff(&g.id,wrong).is_err());
 let mut empty=reviewed(&h);empty.assessment=" ".into();assert!(s.approve_assistant_handoff(&g.id,empty).is_err());
 s.approve_assistant_handoff(&g.id,reviewed(&h)).unwrap();assert_eq!(s.assistant_approval(&h.id).unwrap().unwrap()["basis"],"ASSISTANT_REVIEW");s.claim_role_handoff(&h.id).unwrap();assert!(s.claim_role_handoff(&h.id).is_err());
 let revoked=review_draft(&s,&g.id,"B");s.approve_assistant_handoff(&g.id,reviewed(&revoked)).unwrap();s.revoke_assistant_grant(&g.id).unwrap();assert!(s.claim_role_handoff(&revoked.id).is_err());
 let old=global(&s);assert_eq!(old.approval_mode,"BRIEF_RULES");let legacy=review_draft(&s,&old.id,"C");assert!(s.approve_assistant_handoff(&old.id,reviewed(&legacy)).unwrap_err().contains("BASIS_REQUIRED"));
 let fresh=connected(&s);let stale=review_draft(&s,&fresh.id,"D");let source=stale.endpoint_source().unwrap();s.record_reply_observation(&stale.workstream_id,&source.id,Some("new result"),"Owner must choose a new direction",None).unwrap();assert!(s.approve_assistant_handoff(&fresh.id,reviewed(&stale)).unwrap_err().contains("SOURCE_CHANGED"));
}
#[test]fn conversation_review_cannot_bypass_a_pending_or_answered_owner_question(){
 let d=tempfile::tempdir().unwrap();let s=RouterStore::open_at(d.path().join("test.db")).unwrap();let g=connected(&s);let h=review_draft(&s,&g.id,"Question");
 let q=s.ask_assistant_decision(&g.id,&h.id,&h.payload_hash,"Which reward layout do you want?").unwrap();assert!(s.approve_assistant_handoff(&g.id,reviewed(&h)).unwrap_err().contains("DECISION_PENDING"));
 s.answer_assistant_decision(&g.id,&q.id,&h.payload_hash,"Use compact layout","dot-user-message-1").unwrap();assert!(s.approve_assistant_handoff(&g.id,reviewed(&h)).unwrap_err().contains("ANSWER_REQUIRED"));
 let mut approved=reviewed(&h);approved.decision_id=Some(q.id);s.approve_assistant_handoff(&g.id,approved).unwrap();assert_eq!(s.assistant_approval(&h.id).unwrap().unwrap()["basis"],"ASSISTANT_ATTESTED_OWNER_ANSWER");
}
#[test]fn conversation_action_review_is_durable_single_attempt_and_owner_answers_stay_mandatory(){
 let d=tempfile::tempdir().unwrap();let path=d.path().join("test.db");let s=RouterStore::open_at(&path).unwrap();let g=connected(&s);
 let make=|request:&str|s.prepare_assistant_action(&g.id,ActionInput{request_id:request.into(),operation:"CREATE_BRIDGE".into(),input:serde_json::json!({"name":"Demo"})}).unwrap();
 let approve=|a:&assistant_actions::AssistantAction|ActionApproval{action_id:a.id.clone(),expected_hash:a.payload_hash.clone(),rule_id:None,use_owner_answer:false,assessment:"Owner requested organizing this Bridge in the assistant chat".into()};
 let a=make("routine");s.claim_assistant_action(&g.id,approve(&a)).unwrap();assert!(s.claim_assistant_action(&g.id,approve(&a)).is_err());s.finish_assistant_action(&a.id,Ok(serde_json::json!({"id":"demo"}))).unwrap();
 let q=make("question");s.ask_assistant_action(&g.id,&q.id,&q.payload_hash,"Which conversation should manage?").unwrap();assert!(s.claim_assistant_action(&g.id,approve(&q)).is_err());s.answer_assistant_action(&g.id,&q.id,&q.payload_hash,"Demo manager","dot-user-message-2").unwrap();assert!(s.claim_assistant_action(&g.id,approve(&q)).is_err());let mut answer=approve(&q);answer.use_owner_answer=true;s.claim_assistant_action(&g.id,answer).unwrap();
 drop(s);let s=RouterStore::open_at(path).unwrap();let a=s.assistant_action(&g.id,&a.id,None).unwrap();assert_eq!(a.basis.as_deref(),Some("ASSISTANT_REVIEW"));assert_eq!(a.status,"APPLIED");assert!(s.claim_assistant_action(&g.id,approve(&a)).is_err());s.revoke_assistant_grant(&g.id).unwrap();assert!(s.assistant_action(&g.id,&a.id,None).is_err());
}

#[test]fn bridge_original_reply_uses_binding_context_without_an_independent_watch(){
 let d=tempfile::tempdir().unwrap();let s=RouterStore::open_at(d.path().join("test.db")).unwrap();let p=s.create_project("Demo".into(),None).unwrap();let wid=bridge(&s,&p.id,"Bound");let thread="demo-Bound-d";
 let context=s.codex_chat_context(thread).unwrap();assert!(context.generation<0&&context.generation.abs()<9_007_199_254_740_991);assert!(s.enable_codex_watch(thread,"Demo",&context.cwd,&context.snapshot).unwrap_err().contains("ALREADY_IN_BRIDGE"));assert!(s.codex_watches().unwrap().is_empty());
 let input=watch_reply::WatchReply{id:Uuid::new_v4().to_string(),thread_id:thread.into(),cwd:context.cwd.clone(),generation:context.generation,source_sequence:None,expected_turn_id:None,mode:"SEND".into(),text:"Add a compact reward preview".into(),options:serde_json::json!({"attachments":[]}),payload_hash:String::new(),status:String::new(),turn_id:None,error_code:None,created_at:0,updated_at:0};
 s.prepare_watch_reply(&input).unwrap();assert!(s.codex_watches().unwrap().is_empty());s.claim_watch_reply(&input.id).unwrap();assert!(s.claim_watch_reply(&input.id).is_err());s.finish_watch_reply(&input.id,"SENT",Some("demo-turn"),None).unwrap();
 let mut stale=input;stale.id=Uuid::new_v4().to_string();s.prepare_watch_reply(&stale).unwrap();
 let side=|id:&str|role_bridge::RoleBindingInput{provider:"CODEX".into(),external_id:id.into(),label:id.into(),cwd:Some("D:\\demo".into())};s.bind_role_bridge(&wid,1,side(thread),side("new-execution")).unwrap();
 assert_ne!(s.codex_chat_context(thread).unwrap().generation,stale.generation);assert!(s.claim_watch_reply(&stale.id).unwrap_err().contains("TARGET_CHANGED"));s.trash_workstream(&wid).unwrap();assert!(s.codex_chat_context(thread).is_err());
}
#[test]fn bound_conversations_are_hidden_from_independent_list_without_destroying_user_watch(){
 let d=tempfile::tempdir().unwrap();let s=RouterStore::open_at(d.path().join("test.db")).unwrap();let snapshot=codex_watch::WatchSnapshot{state:"IDLE".into(),turn_id:None,item_id:None,text:String::new()};
 s.enable_codex_watch("demo-Existing-d","Explicit watch","D:\\demo",&snapshot).unwrap();assert_eq!(s.codex_watches().unwrap().len(),1);
 let p=s.create_project("Demo".into(),None).unwrap();let wid=bridge(&s,&p.id,"Existing");assert!(s.codex_watches().unwrap().is_empty());assert!(s.registered_codex_watch("demo-Existing-d").is_ok());s.trash_workstream(&wid).unwrap();assert_eq!(s.codex_watches().unwrap().len(),1);
}

#[test]fn reviewed_earlier_deliverable_remains_selectable_but_new_source_heads_invalidate_it(){
 let d=tempfile::tempdir().unwrap();let s=RouterStore::open_at(d.path().join("test.db")).unwrap();let g=connected(&s);let p=s.create_project("Demo".into(),None).unwrap();let w=bridge(&s,&p.id,"History");let source=s.role_bridge(&w).unwrap().execution.unwrap().endpoint;
 s.record_reply_observation(&w,&source.id,Some("deliverable"),"Complete checklist and review materials",None).unwrap();let old=s.reply_observations_for_workstream(&w).unwrap().remove(0);
 s.record_reply_observation(&w,&source.id,Some("blocked"),"Waiting; Goal blocked",None).unwrap();let h=s.prepare_role_handoff(&w,"EXECUTION",&old.id,"Forward the complete checklist").unwrap();s.register_assistant_draft(&g.id,&h.id,&old.id).unwrap();
 s.approve_assistant_handoff(&g.id,reviewed(&h)).unwrap();s.require_assistant_send(&g.id,&h.id,&h.payload_hash).unwrap();
 s.record_reply_observation(&w,&source.id,Some("new facts"),"New scope decision needed",None).unwrap();assert!(s.claim_role_handoff(&h.id).unwrap_err().contains("SOURCE_CHANGED"));
}
