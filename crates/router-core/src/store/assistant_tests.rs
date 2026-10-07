use super::*;
fn fixture()->(tempfile::TempDir,RouterStore,String,AssistantGrant,HandoffHistoryItem){
    let dir=tempfile::tempdir().unwrap();let s=RouterStore::open_at(dir.path().join("test.db")).unwrap();
    let p=s.create_project("qa".into(),None).unwrap();let w=s.create_workstream(&p.id,"Bridge".into()).unwrap();
    let side=|native:&str|role_bridge::RoleBindingInput{provider:"CODEX".into(),external_id:native.into(),label:native.into(),cwd:Some(dir.path().to_string_lossy().into())};
    let b=s.bind_role_bridge(&w.id,w.binding_revision,side("thread-source"),side("thread-target")).unwrap();
    s.record_reply_observation(&w.id,&b.decision.unwrap().endpoint.id,Some("turn:one"),"Continue using the agreed plan",None).unwrap();
    let obs=s.reply_observations_for_workstream(&w.id).unwrap().remove(0);
    let g=s.create_assistant_grant(GrantInput{workstream_id:w.id.clone(),source_role:"DECISION".into(),binding_revision:b.binding_revision,label:"Dot".into(),rules:vec![BriefRule{id:"continue".into(),text:"Continue the agreed plan; ask me about changes".into()}],expires_at:now()+86400000}).unwrap();
    let h=s.prepare_role_handoff(&w.id,"DECISION",&obs.id,"exact bytes\n").unwrap();s.register_assistant_draft(&g.id,&h.id,&obs.id).unwrap();
    (dir,s,w.id,g,h)
}
fn approval(h:&HandoffHistoryItem)->ApprovalInput{ApprovalInput{handoff_id:h.id.clone(),expected_hash:h.payload_hash.clone(),rule_id:Some("continue".into()),decision_id:None,assessment:"The result matches the owner's continuation rule".into()}}
#[test]fn concurrent_prepare_registration_has_one_owner_and_no_sendable_loser(){
    let(d,s,w,g,_)=fixture();let obs=s.reply_observations_for_workstream(&w).unwrap().remove(0);
    let a=s.prepare_role_handoff(&w,"DECISION",&obs.id,"first racing preparation").unwrap();
    let b=s.prepare_role_handoff(&w,"DECISION",&obs.id,"second racing preparation").unwrap();
    let barrier=std::sync::Arc::new(std::sync::Barrier::new(2));
    let handles=[a.clone(),b.clone()].map(|h|{let path=d.path().join("test.db");let gid=g.id.clone();let oid=obs.id.clone();let barrier=barrier.clone();std::thread::spawn(move||{
        let store=RouterStore::open_at(path).unwrap();barrier.wait();let result=store.register_assistant_prepare(&gid,&h.id,&oid,"same-request","same-request-hash");(h,result)
    })});
    let results=handles.map(|v|v.join().unwrap());assert_eq!(results.iter().filter(|(_,r)|r.is_ok()).count(),1);
    let winner=&results.iter().find(|(_,r)|r.is_ok()).unwrap().0;let loser=&results.iter().find(|(_,r)|r.is_err()).unwrap().0;
    assert_eq!(s.assistant_prepare_receipt(&g.id,"same-request","same-request-hash").unwrap().unwrap().id,winner.id);
    assert!(s.require_assistant_draft_scope(&g.id,&loser.id,&loser.payload_hash).is_err());
    assert!(s.approve_assistant_handoff(&g.id,approval(loser)).is_err());
    assert!(s.assistant_prepare_receipt(&g.id,"same-request","changed-hash").is_err());
    s.approve_assistant_handoff(&g.id,approval(winner)).unwrap();s.claim_role_handoff(&winner.id).unwrap();
}
#[test]fn covered_handoff_persists_provenance_and_never_claims_twice(){
    let(d,s,_,g,h)=fixture();s.approve_assistant_handoff(&g.id,approval(&h)).unwrap();s.require_assistant_send(&g.id,&h.id,&h.payload_hash).unwrap();
    let token=s.issue_assistant_access(&g.id,"https://qa.invalid/mcp",now()+60000).unwrap();
    assert!(s.authenticate_assistant(&token,"https://other.invalid/mcp").is_err());
    drop(s);let s=RouterStore::open_at(d.path().join("test.db")).unwrap();assert_eq!(s.authenticate_assistant(&token,"https://qa.invalid/mcp").unwrap().id,g.id);
    s.claim_role_handoff(&h.id).unwrap();assert!(s.claim_role_handoff(&h.id).is_err());
}
#[test]fn pending_decision_blocks_rule_bypass_then_one_off_answer_resumes(){
    let(_,s,_,g,h)=fixture();let d=s.ask_assistant_decision(&g.id,&h.id,&h.payload_hash,"A or B?").unwrap();
    assert_eq!(d.id,s.ask_assistant_decision(&g.id,&h.id,&h.payload_hash,"A or B?").unwrap().id);
    assert!(s.approve_assistant_handoff(&g.id,approval(&h)).unwrap_err().contains("PENDING"));
    s.answer_assistant_decision(&g.id,&d.id,&h.payload_hash,"A","assistant-chat/user-message-42").unwrap();
    let mut a=approval(&h);a.rule_id=None;a.decision_id=Some(d.id.clone());s.approve_assistant_handoff(&g.id,a).unwrap();
    assert_eq!(s.assistant_grant(&g.id).unwrap().rules[0].text,g.rules[0].text);
    assert!(s.answer_assistant_decision(&g.id,&d.id,&h.payload_hash,"B","later").is_err());
}
#[test]fn revocation_between_approval_and_physical_claim_stops_send(){
    let(_,s,_,g,h)=fixture();s.approve_assistant_handoff(&g.id,approval(&h)).unwrap();s.revoke_assistant_grant(&g.id).unwrap();
    assert!(s.claim_role_handoff(&h.id).unwrap_err().contains("REVOKED"));assert_eq!(s.role_handoff(&h.id).unwrap().status,"APPROVED");
}
#[test]fn newer_source_invalidates_an_approved_delegate_handoff(){
    let(_,s,w,g,h)=fixture();s.approve_assistant_handoff(&g.id,approval(&h)).unwrap();
    let source=s.role_bridge(&w).unwrap().decision.unwrap().endpoint.id;s.record_reply_observation(&w,&source,Some("turn:two"),"changed result",None).unwrap();
    assert!(s.claim_role_handoff(&h.id).unwrap_err().contains("SOURCE_CHANGED"));
}
#[test]fn edit_invalidates_pending_answer_and_old_payload_approval(){
    let(_,s,_,g,h)=fixture();let d=s.ask_assistant_decision(&g.id,&h.id,&h.payload_hash,"Choose").unwrap();
    let edited=s.edit_role_handoff(&h.id,&h.payload_hash,"different bytes").unwrap();
    assert!(s.answer_assistant_decision(&g.id,&d.id,&edited.payload_hash,"yes","reply").is_err());assert!(s.approve_assistant_handoff(&g.id,approval(&h)).is_err());
}
#[test]fn unknown_rule_and_wrong_grant_cannot_approve(){
    let(_,s,w,g,h)=fixture();let mut a=approval(&h);a.rule_id=Some("invented".into());assert!(s.approve_assistant_handoff(&g.id,a).is_err());
    let other=s.create_assistant_grant(GrantInput{workstream_id:w,source_role:"EXECUTION".into(),binding_revision:g.binding_revision,label:"other".into(),rules:g.rules,expires_at:now()+60000}).unwrap();
    assert!(s.approve_assistant_handoff(&other.id,approval(&h)).is_err());
}
#[test]fn two_way_grant_can_review_the_reverse_reply_but_still_pins_the_same_bridge(){
    let(_,s,w,g,_)=fixture();let b=s.role_bridge(&w).unwrap();
    let grant=s.create_assistant_grant(GrantInput{workstream_id:w.clone(),source_role:"BOTH".into(),binding_revision:b.binding_revision,label:"Dot two-way".into(),rules:g.rules,expires_at:now()+60000}).unwrap();
    s.record_reply_observation(&w,&b.execution.unwrap().endpoint.id,Some("reverse-result"),"Return for management review",None).unwrap();
    let obs=s.reply_observations_for_workstream(&w).unwrap().into_iter().find(|o|o.assistant_identity.as_deref()==Some("reverse-result")).unwrap();
    let h=s.prepare_role_handoff(&w,"EXECUTION",&obs.id,"Return for review").unwrap();s.register_assistant_draft(&grant.id,&h.id,&obs.id).unwrap();s.approve_assistant_handoff(&grant.id,approval(&h)).unwrap();s.claim_role_handoff(&h.id).unwrap();
}
