use super::*;
#[test]fn provider_completion_is_exact_durable_and_does_not_rewrite_receipt_time(){
 let d=tempfile::tempdir().unwrap();let path=d.path().join("completion.db");let store=RouterStore::open_at(&path).unwrap();
 let p=store.create_project("time QA".into(),None).unwrap();let w=store.create_workstream(&p.id,"time QA".into()).unwrap();
 let input=|id:&str|role_bridge::RoleBindingInput{provider:"CODEX".into(),external_id:id.into(),label:id.into(),cwd:Some(d.path().to_string_lossy().into())};
 let b=store.bind_role_bridge(&w.id,w.binding_revision,input("thread-a"),input("thread-b")).unwrap();
 let endpoint=b.decision.unwrap().endpoint.id;
 let r=store.record_reply_observation(&w.id,&endpoint,Some("exact-message"),"fixture",None).unwrap().unwrap();
 assert!(store.note_reply_completion(&w.id,&endpoint,"exact-message",Some(1791331200000)).unwrap());
 assert!(!store.note_reply_completion("other",&endpoint,"exact-message",Some(1791331200000)).unwrap());
 assert!(store.note_reply_completion(&w.id,&endpoint,"exact-message",Some(1791331200001)).is_err());
 store.note_reply_completion(&w.id,&endpoint,"exact-message",None).unwrap();drop(store);
 let store=RouterStore::open_at(path).unwrap();let after=store.reply_observations_for_workstream(&w.id).unwrap().remove(0);
 assert_eq!(after.completed_at,Some(1791331200000));assert_eq!(after.observed_at,r.observed_at);assert!(after.completion_checked);
}
