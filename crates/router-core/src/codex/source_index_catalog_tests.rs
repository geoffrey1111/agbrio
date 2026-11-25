use super::*;
#[test]
fn catalog_hints_require_exact_native_identity_and_exclude_archived_missing_and_children(){
 let temp=tempfile::tempdir().unwrap();let sessions=temp.path().join("sessions");std::fs::create_dir(&sessions).unwrap();
 let db=rusqlite::Connection::open(temp.path().join("state_5.sqlite")).unwrap();
 db.execute_batch("CREATE TABLE threads(id TEXT,cwd TEXT,rollout_path TEXT,archived INTEGER,source TEXT,updated_at INTEGER)").unwrap();
 let id=|n|format!("10000000-0000-4000-8000-{n:012}");let cwd=temp.path().to_str().unwrap();
 for n in 1..=6 {
  let file=sessions.join(format!("{}.jsonl",id(n)));
  let meta=serde_json::json!({"type":"session_meta","payload":{"id":if n==4{id(99)}else{id(n)},"cwd":if n==6{"D:/different"}else{cwd}}});
  if n!=3{std::fs::write(&file,meta.to_string()).unwrap();}
  db.execute("INSERT INTO threads VALUES(?1,?2,?3,?4,?5,1)",rusqlite::params![id(n),cwd,file.to_str().unwrap(),n==2,if n==5{"exec"}else{"vscode"}]).unwrap();
 }
 let hints=supplemental_at(temp.path(),&Default::default()).unwrap();assert_eq!(hints.len(),1);assert_eq!(hints[0].id,id(1));assert_eq!(hints[0].cwd,cwd);
 assert!(supplemental_at(temp.path(),&[id(1)].into_iter().collect()).unwrap().is_empty());
 assert_eq!(db.query_row("SELECT count(*) FROM threads",[],|r|r.get::<_,i64>(0)).unwrap(),6);
}
#[test]
#[ignore="explicit read-only audit of the owner's native index and metadata; no provider or write"]
fn live_exact_missing_thread_has_valid_index_hint(){
 let hints=supplemental_catalog(&Default::default()).unwrap();
 let expected=std::env::var("AGBRIO_AUDIT_THREAD_ID").expect("set the exact owner-selected thread ID");
 assert!(hints.iter().any(|r|r.id==expected));
 println!("verified native interactive index hints: {}; exact reported thread present",hints.len());
}
