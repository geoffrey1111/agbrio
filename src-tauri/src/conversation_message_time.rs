//! Native message time only. Poll clocks and local unsent drafts never affect recency.
use crate::persistence::RouterStore;
pub(crate) fn codex(store:&RouterStore,adapter:&crate::codex::adapter::CodexAdapter,id:&str,cwd:&str){
 if let Some(at)=adapter.public_message_timestamp(id,cwd).and_then(crate::claude_quota::parse_utc_ms).filter(|t|*t>0){let _=store.note_conversation_message_at(id,cwd,at);}
}
pub(crate) fn claude(id:&str,cwd:&str)->Option<i64>{
 let page=crate::claude_transcript::latest(id)?;
 if !page.cwd.as_deref().is_some_and(|root|router_core::claude_code::same_root(root,cwd)){return None;}
 last_public_at(&page)
}
fn last_public_at(page:&crate::claude_transcript::Page)->Option<i64>{page.turns.iter().flat_map(|t|t.messages.iter()).filter(|m|m.kind=="PUBLIC").max_by_key(|m|m.order).map(|m|m.at).filter(|at|*at>0)}
#[cfg(test)]mod tests{
 use super::*;
 #[test]#[ignore="Explicit read-only installed-clock audit; never sends, pairs or changes provider state"]
 fn installed_claude_message_times_match_native_public_projection(){
  let root=std::path::PathBuf::from(std::env::var_os("USERPROFILE").unwrap()).join(".claude/projects");
  let before=crate::claude_transcript::TEST_PROJECTS.lock().unwrap().replace(root);
  let db=std::path::PathBuf::from(std::env::var_os("LOCALAPPDATA").unwrap()).join("AIWorkRouter/data/router.db");
  use rusqlite::OptionalExtension;
  let c=rusqlite::Connection::open_with_flags(db,rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap();
  let rows:Vec<(String,String)>=c.prepare("SELECT thread_id,cwd FROM codex_watches WHERE enabled=1 AND thread_id LIKE 'claude_code:%'").unwrap().query_map([],|r|Ok((r.get(0)?,r.get(1)?))).unwrap().collect::<Result<_,_>>().unwrap();assert!(!rows.is_empty());
  for (id,cwd) in &rows{
   let mut exact=false;
   for _ in 0..3{
    let at=claude(id.strip_prefix("claude_code:").unwrap(),cwd);
    let stored:Option<i64>=c.query_row("SELECT message_at FROM conversation_message_times WHERE thread_id=?1 AND cwd=?2",rusqlite::params![id,cwd],|r|r.get(0)).optional().unwrap();
    if at.is_some()&&stored==at{exact=true;break;}std::thread::sleep(std::time::Duration::from_secs(3));
   }assert!(exact,"stored clock must match exact native public projection after bounded observation catch-up");
  }
  *crate::claude_transcript::TEST_PROJECTS.lock().unwrap()=before;
  println!("INSTALLED_CLAUDE_MESSAGE_TIME_READ_ONLY_PASS sources={}",rows.len());
 }
 #[test]fn collaboration_does_not_bump_the_parent_conversation(){
  use crate::claude_transcript::{Message,Turn,Page};
  let message=|kind,at,order|Message{id:format!("{order}"),role:"assistant",text:"fixture".into(),kind,card:None,at,order};
  let mut page=Page{turns:vec![Turn{id:"turn".into(),messages:vec![message("PUBLIC",100,10),message("COLLABORATION",999,20)]}],..Default::default()};
  assert_eq!(last_public_at(&page),Some(100));page.turns[0].messages.push(message("PUBLIC",90,30));assert_eq!(last_public_at(&page),Some(90));
 }
}
