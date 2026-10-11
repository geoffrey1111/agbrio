use super::*;
use crate::store::assistant::{InstanceGrantInput,BriefRule,GrantInput};
fn grant(s:&RouterStore)->String{s.create_assistant_instance_grant(InstanceGrantInput{label:"fixture assistant".into(),rules:vec![BriefRule{id:"routine".into(),text:"fixture routine".into()}],expires_at:now()+600000}).unwrap().id}
fn fixture()->(tempfile::TempDir,RouterStore,String,String){
 let d=tempfile::tempdir().unwrap();let s=RouterStore::open_at(d.path().join("report.db")).unwrap();let gid=grant(&s);let p=s.create_project("Example".into(),None).unwrap();let w=s.create_workstream(&p.id,"Example Bridge".into()).unwrap();
 let side=|name:&str|role_bridge::RoleBindingInput{provider:"CODEX".into(),external_id:name.into(),label:name.into(),cwd:Some(d.path().to_string_lossy().into())};s.bind_role_bridge(&w.id,0,side("example-source"),side("example-target")).unwrap();(d,s,gid,w.id)
}
fn q(w:&str)->ReportQuery{ReportQuery{workstream_ids:vec![w.into()],window_start:20,window_end:40,cursor:None,limit:Some(2)}}
fn add(s:&RouterStore,w:&str,id:&str,status:&str,sent:Option<i64>,actor:Option<&str>){
 let b=s.role_bridge(w).unwrap();let src=b.decision.unwrap().endpoint;let dst=b.execution.unwrap().endpoint;
 s.with_connection(|c|{c.execute("INSERT INTO handoffs(id,workstream_id,source_endpoint_id,destination_endpoint_id,direction,source_response_identity,original_text,approved_text,status,payload_hash,created_at,sent_at) VALUES(?1,?2,?3,?4,'CODEX_TO_CODEX',?1,'fixture','fixture',?5,'fixture-hash',25,?6)",params![id,w,src.id,dst.id,status,sent]).map_err(db_error)?;c.execute("INSERT INTO role_handoff_details(handoff_id,binding_revision,source_role) VALUES(?1,1,'DECISION')",[id]).map_err(db_error)?;if let Some(gid)=actor{c.execute("INSERT INTO assistant_sent_handoffs(handoff_id,grant_id,recorded_at) VALUES(?1,?2,30)",params![id,gid]).map_err(db_error)?;}Ok(())}).unwrap();
}
#[test]fn receipt_windows_pages_scope_digest_and_unknown_actor(){
 let(_d,s,g,w)=fixture();let other=grant(&s);
 add(&s,&w,"at-start","SENT",Some(20),Some(&g));add(&s,&w,"at-end","SENT",Some(40),Some(&g));add(&s,&w,"other","SENT",Some(21),Some(&other));add(&s,&w,"unattributed","SENT",Some(22),None);
 for (id,state) in [("failed","FAILED"),("unknown","READY"),("approved","APPROVED"),("sending","SENDING")]{add(&s,&w,id,state,None,None);}
 let before=serde_json::to_value(s.snapshot().unwrap()).unwrap();let first=s.assistant_report(&g,q(&w)).unwrap();assert_eq!(first["totals"]["handoffsThisGrant"],1);assert_eq!(first["totals"]["handoffsOtherGrant"],1);assert_eq!(first["totals"]["handoffsUnattributed"],1);assert_eq!(first["totalRecords"],8);assert_eq!(first["attributionComplete"],false);assert_eq!(first["pageComplete"],false);
 let mut all=first["records"].as_array().unwrap().clone();let mut page=first.clone();
 while let Some(cursor)=page["nextCursor"].as_str(){let mut query=q(&w);query.cursor=Some(cursor.into());page=s.assistant_report(&g,query).unwrap();all.extend(page["records"].as_array().unwrap().clone());}
 assert_eq!(all.len(),8);assert_eq!(all.iter().map(|r|r["handoffId"].as_str().unwrap()).collect::<std::collections::HashSet<_>>().len(),8);assert_eq!(serde_json::to_value(s.snapshot().unwrap()).unwrap(),before);
 let mut different=q(&w);different.window_start=21;different.cursor=first["nextCursor"].as_str().map(str::to_owned);assert_eq!(s.assistant_report(&g,different).unwrap_err(),"REPORT_CURSOR_SCOPE_CHANGED");
 let mut query=q(&w);query.cursor=first["nextCursor"].as_str().map(str::to_owned);assert_eq!(s.assistant_report(&other,query.clone()).unwrap_err(),"REPORT_CURSOR_SCOPE_CHANGED");add(&s,&w,"new","SENT",Some(30),Some(&g));assert_eq!(s.assistant_report(&g,query).unwrap_err(),"REPORT_LEDGER_CHANGED_RESTART_READ");
 let legacy=s.create_assistant_grant(GrantInput{workstream_id:w.clone(),source_role:"BOTH".into(),binding_revision:1,label:"legacy".into(),rules:vec![BriefRule{id:"ok".into(),text:"ok".into()}],expires_at:now()+600000}).unwrap();assert!(s.assistant_report(&legacy.id,q(&w)).is_err());s.revoke_assistant_grant(&g).unwrap();assert!(s.assistant_report(&g,q(&w)).is_err());
}
#[test]fn sent_before_preparation_window_still_counts_and_coordination_is_separate(){
 let(_d,s,g,w)=fixture();add(&s,&w,"old-prepared","SENT",Some(20),Some(&g));
 let context=s.codex_chat_context("example-target").unwrap();
 s.with_connection(|c|{c.execute("UPDATE handoffs SET created_at=1 WHERE id='old-prepared'",[]).map_err(db_error)?;c.execute("INSERT INTO assistant_actions(id,grant_id,request_id,payload_hash,operation,input_json,status,created_at,updated_at) VALUES('coord',?1,'coord-request','hash','SEND_CHAT','{}','APPLIED',1,30)",[&g]).map_err(db_error)?;c.execute("INSERT INTO watch_replies(id,thread_id,cwd,generation,mode,text,options_json,payload_hash,status,created_at,updated_at) VALUES('coord','example-target',?1,?2,'SEND','fixture','{}','hash','SENT',1,30)",params![context.cwd,context.generation]).map_err(db_error)?;Ok(())}).unwrap();
 let report=s.assistant_report(&g,q(&w)).unwrap();assert_eq!(report["totals"]["handoffsThisGrant"],1);assert_eq!(report["totals"]["coordinationThisGrant"],1);assert_eq!(report["totalRecords"],2);
 let mut invalid=q(&w);invalid.workstream_ids.clear();assert!(s.assistant_report(&g,invalid).is_err());let mut invalid=q(&w);invalid.cursor=Some("{}".into());assert!(s.assistant_report(&g,invalid).is_err());
}
