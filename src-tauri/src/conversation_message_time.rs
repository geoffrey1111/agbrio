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
 #[test]fn collaboration_does_not_bump_the_parent_conversation(){
  use crate::claude_transcript::{Message,Turn,Page};
  let message=|kind,at,order|Message{id:format!("{order}"),role:"assistant",text:"fixture".into(),kind,card:None,at,order};
  let mut page=Page{turns:vec![Turn{id:"turn".into(),messages:vec![message("PUBLIC",100,10),message("COLLABORATION",999,20)]}],..Default::default()};
  assert_eq!(last_public_at(&page),Some(100));page.turns[0].messages.push(message("PUBLIC",90,30));assert_eq!(last_public_at(&page),Some(90));
 }
}
